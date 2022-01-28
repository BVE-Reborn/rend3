use crate::{
    managers::{ObjectManager, SkeletonManager},
    types::{Mesh, MeshHandle},
    util::{
        buffer_copier::{VertexBufferCopier, VertexBufferCopierParams},
        frustum::BoundingSphere,
        registry::ResourceRegistry,
    },
};
use glam::{Vec2, Vec3};
use range_alloc::RangeAllocator;
use rend3_types::{RawMeshHandle, RawSkeletonHandle};
use std::{
    mem::size_of,
    ops::Range,
    sync::atomic::{AtomicUsize, Ordering},
};
use wgpu::{
    Buffer, BufferAddress, BufferDescriptor, BufferUsages, CommandEncoder, Device, IndexFormat, Queue, RenderPass,
};

/// Size of a single vertex position.
pub const VERTEX_POSITION_SIZE: usize = size_of::<Vec3>();
/// Size of a single vertex normal.
pub const VERTEX_NORMAL_SIZE: usize = size_of::<Vec3>();
/// Size of a single vertex tangent.
pub const VERTEX_TANGENT_SIZE: usize = size_of::<Vec3>();
/// Size of a single vertex texture coordinate.
pub const VERTEX_UV_SIZE: usize = size_of::<Vec2>();
/// Size of a single vertex color.
pub const VERTEX_COLOR_SIZE: usize = size_of::<[u8; 4]>();
/// Size of a single index.
pub const INDEX_SIZE: usize = size_of::<u32>();
/// Size of a joint index vector
pub const VERTEX_JOINT_INDEX_SIZE: usize = size_of::<[u16; 4]>();
/// Size of a joint weight vector
pub const VERTEX_JOINT_WEIGHT_SIZE: usize = size_of::<[f32; 4]>();

/// Vertex buffer slot for positions
pub const VERTEX_POSITION_SLOT: u32 = 0;
/// Vertex buffer slot for normals
pub const VERTEX_NORMAL_SLOT: u32 = 1;
/// Vertex buffer slot for tangents
pub const VERTEX_TANGENT_SLOT: u32 = 2;
/// Vertex buffer slot for uv0
pub const VERTEX_UV0_SLOT: u32 = 3;
/// Vertex buffer slot for uv1
pub const VERTEX_UV1_SLOT: u32 = 4;
/// Vertex buffer slot for colors
pub const VERTEX_COLOR_SLOT: u32 = 5;
/// Vertex buffer slot for joint indices
pub const VERTEX_JOINT_INDEX_SLOT: u32 = 6;
/// Vertex buffer slot for joint weights
pub const VERTEX_JOINT_WEIGHT_SLOT: u32 = 7;
/// Vertex buffer slot for object indices
/// Note that this slot is only used in the GpuDriven profile.
pub const VERTEX_OBJECT_INDEX_SLOT: u32 = 8;

/// Pre-allocated vertex count in the vertex megabuffers.
pub const STARTING_VERTICES: usize = 1 << 16;
/// Pre-allocated index count in the index megabuffer.
pub const STARTING_INDICES: usize = 1 << 16;

/// Internal representation of a mesh.
pub struct InternalMesh {
    /// Range of values from the [`MeshBuffers`] where vertex data for this mesh
    /// is
    pub vertex_range: Range<usize>,
    /// Range of values from the [`MeshBuffers`] where index data for this mesh
    /// is
    pub index_range: Range<usize>,
    /// The bounding sphere of this mesh. Used for culling.
    pub bounding_sphere: BoundingSphere,
    /// Handles to the skeletons that point to this mesh. Used for internal
    /// bookkeeping
    pub skeletons: Vec<RawSkeletonHandle>,
    /// For skinned meshes, stores the number of joints present in the joint
    /// index buffer
    pub num_joints: u32,
}

impl InternalMesh {
    /// Returns an empty InternalMesh
    fn new_empty() -> Self {
        InternalMesh {
            vertex_range: 0..0,
            index_range: 0..0,
            bounding_sphere: BoundingSphere::from_mesh(&[]),
            skeletons: Vec::new(),
            num_joints: 0,
        }
    }
}

/// Set of megabuffers used by the mesh manager.
pub struct MeshBuffers {
    pub vertex_position: Buffer,
    pub vertex_normal: Buffer,
    pub vertex_tangent: Buffer,
    pub vertex_uv0: Buffer,
    pub vertex_uv1: Buffer,
    pub vertex_color: Buffer,
    pub vertex_joint_index: Buffer,
    pub vertex_joint_weight: Buffer,

    pub index: Buffer,
}

impl MeshBuffers {
    pub fn bind<'rpass>(&'rpass self, rpass: &mut RenderPass<'rpass>) {
        rpass.set_vertex_buffer(VERTEX_POSITION_SLOT, self.vertex_position.slice(..));
        rpass.set_vertex_buffer(VERTEX_NORMAL_SLOT, self.vertex_normal.slice(..));
        rpass.set_vertex_buffer(VERTEX_TANGENT_SLOT, self.vertex_tangent.slice(..));
        rpass.set_vertex_buffer(VERTEX_UV0_SLOT, self.vertex_uv0.slice(..));
        rpass.set_vertex_buffer(VERTEX_UV1_SLOT, self.vertex_uv1.slice(..));
        rpass.set_vertex_buffer(VERTEX_COLOR_SLOT, self.vertex_color.slice(..));
        rpass.set_vertex_buffer(VERTEX_JOINT_INDEX_SLOT, self.vertex_joint_index.slice(..));
        rpass.set_vertex_buffer(VERTEX_JOINT_WEIGHT_SLOT, self.vertex_joint_weight.slice(..));
        rpass.set_index_buffer(self.index.slice(..), IndexFormat::Uint32);
    }
}

/// Manages vertex and instance buffers. All buffers are sub-allocated from
/// megabuffers.
pub struct MeshManager {
    buffers: MeshBuffers,

    vertex_alloc: RangeAllocator<usize>,
    index_alloc: RangeAllocator<usize>,

    registry: ResourceRegistry<InternalMesh, Mesh>,

    buffer_copier: VertexBufferCopier,
}

impl MeshManager {
    pub fn new(device: &Device) -> Self {
        profiling::scope!("MeshManager::new");

        let buffers = create_buffers(device, STARTING_VERTICES, STARTING_INDICES);

        let vertex_alloc = RangeAllocator::new(0..STARTING_VERTICES);
        let index_alloc = RangeAllocator::new(0..STARTING_INDICES);

        let registry = ResourceRegistry::new();

        Self {
            buffers,
            vertex_alloc,
            index_alloc,
            registry,
            buffer_copier: VertexBufferCopier::new(device),
        }
    }

    pub fn allocate(counter: &AtomicUsize) -> MeshHandle {
        let idx = counter.fetch_add(1, Ordering::Relaxed);

        MeshHandle::new(idx)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn fill(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        object_manager: &mut ObjectManager,
        skeleton_manager: &mut SkeletonManager,
        handle: &MeshHandle,
        mesh: Mesh,
    ) {
        profiling::scope!("MeshManager::fill");

        let vertex_count = mesh.vertex_positions.len();
        let index_count = mesh.indices.len();

        // This value is used later when setting joints, to make sure all indices are
        // in-bounds
        let num_joints = mesh.vertex_joint_indices.iter().flatten().max().map_or(0, |i| i + 1);

        // If vertex_count is 0, index_count _must_ also be 0, as all indices would be
        // out of range.
        if index_count == 0 {
            let mesh = InternalMesh::new_empty();
            self.registry.insert(handle, mesh);
            return;
        }

        let mut vertex_range = self.vertex_alloc.allocate_range(vertex_count).ok();
        let mut index_range = self.index_alloc.allocate_range(index_count).ok();

        let needed = match (&vertex_range, &index_range) {
            (None, Some(_)) => Some((vertex_count, 0)),
            (Some(_), None) => Some((0, index_count)),
            (None, None) => Some((vertex_count, index_count)),
            _ => None,
        };

        if let Some((needed_verts, needed_indices)) = needed {
            self.reallocate_buffers(
                device,
                encoder,
                object_manager,
                skeleton_manager,
                needed_verts as u32,
                needed_indices as u32,
            );
            vertex_range = self.vertex_alloc.allocate_range(vertex_count).ok();
            index_range = self.index_alloc.allocate_range(index_count).ok();
        }

        let vertex_range = vertex_range.unwrap();
        let index_range = index_range.unwrap();

        queue.write_buffer(
            &self.buffers.vertex_position,
            (vertex_range.start * VERTEX_POSITION_SIZE) as BufferAddress,
            bytemuck::cast_slice(&mesh.vertex_positions),
        );
        queue.write_buffer(
            &self.buffers.vertex_tangent,
            (vertex_range.start * VERTEX_TANGENT_SIZE) as BufferAddress,
            bytemuck::cast_slice(&mesh.vertex_tangents),
        );
        queue.write_buffer(
            &self.buffers.vertex_normal,
            (vertex_range.start * VERTEX_NORMAL_SIZE) as BufferAddress,
            bytemuck::cast_slice(&mesh.vertex_normals),
        );
        queue.write_buffer(
            &self.buffers.vertex_uv0,
            (vertex_range.start * VERTEX_UV_SIZE) as BufferAddress,
            bytemuck::cast_slice(&mesh.vertex_uv0),
        );
        queue.write_buffer(
            &self.buffers.vertex_uv1,
            (vertex_range.start * VERTEX_UV_SIZE) as BufferAddress,
            bytemuck::cast_slice(&mesh.vertex_uv1),
        );
        queue.write_buffer(
            &self.buffers.vertex_color,
            (vertex_range.start * VERTEX_COLOR_SIZE) as BufferAddress,
            bytemuck::cast_slice(&mesh.vertex_colors),
        );
        queue.write_buffer(
            &self.buffers.vertex_joint_index,
            (vertex_range.start * VERTEX_JOINT_INDEX_SIZE) as BufferAddress,
            bytemuck::cast_slice(&mesh.vertex_joint_indices),
        );
        queue.write_buffer(
            &self.buffers.vertex_joint_weight,
            (vertex_range.start * VERTEX_JOINT_WEIGHT_SIZE) as BufferAddress,
            bytemuck::cast_slice(&mesh.vertex_joint_weights),
        );
        queue.write_buffer(
            &self.buffers.index,
            (index_range.start * INDEX_SIZE) as BufferAddress,
            bytemuck::cast_slice(&mesh.indices),
        );

        let bounding_sphere = BoundingSphere::from_mesh(&mesh.vertex_positions);

        let mesh = InternalMesh {
            vertex_range,
            index_range,
            bounding_sphere,
            num_joints: num_joints as u32,
            skeletons: Vec::new(),
        };

        self.registry.insert(handle, mesh);
    }

    /// Duplicates a mesh's vertex data so that it can be skinned on the GPU.
    pub fn allocate_skeleton_mesh(
        &mut self,
        device: &Device,
        encoder: &mut CommandEncoder,
        object_manager: &mut ObjectManager,
        skeleton_manager: &mut SkeletonManager,
        mesh_handle: &MeshHandle,
    ) -> Range<usize> {
        // Need to fetch internal data twice, because the returned mesh borrows &self
        let needed_verts = self.internal_data(mesh_handle.get_raw()).vertex_range.len();
        let vertex_range = match self.vertex_alloc.allocate_range(needed_verts) {
            Ok(range) => range,
            Err(_) => {
                self.reallocate_buffers(
                    device,
                    encoder,
                    object_manager,
                    skeleton_manager,
                    needed_verts as u32,
                    0,
                );
                self.vertex_alloc
                    .allocate_range(needed_verts)
                    .expect("We just reallocated")
            }
        };

        let original = self.internal_data(mesh_handle.get_raw());

        // Copies one region of the vertex buffer to another using a compute
        // shader. This is necessary because wgpu's copy_buffer_to_buffer does
        // not allow copies whithin the same buffer.
        self.buffer_copier.execute(
            device,
            encoder,
            [
                &self.buffers.vertex_position,
                &self.buffers.vertex_normal,
                &self.buffers.vertex_tangent,
                &self.buffers.vertex_uv0,
                &self.buffers.vertex_uv1,
                &self.buffers.vertex_color,
                &self.buffers.vertex_joint_index,
                &self.buffers.vertex_joint_weight,
            ],
            VertexBufferCopierParams {
                src_offset: original.vertex_range.start as u32,
                dst_offset: vertex_range.start as u32,
                count: vertex_range.len() as u32,
            },
        );

        vertex_range
    }

    pub fn free_skeleton_mesh(&mut self, vertex_range: Range<usize>) {
        self.vertex_alloc.free_range(vertex_range);
    }

    pub fn buffers(&self) -> &MeshBuffers {
        &self.buffers
    }

    pub fn internal_data(&self, handle: RawMeshHandle) -> &InternalMesh {
        self.registry.get(handle)
    }

    pub fn internal_data_mut(&mut self, handle: RawMeshHandle) -> &mut InternalMesh {
        self.registry.get_mut(handle)
    }

    pub fn ready(&mut self) {
        profiling::scope!("MeshManager::ready");

        let vertex_alloc = &mut self.vertex_alloc;
        let index_alloc = &mut self.index_alloc;
        self.registry.remove_all_dead(|_, _, mesh| {
            if mesh.vertex_range.is_empty() {
                return;
            }
            vertex_alloc.free_range(mesh.vertex_range);
            index_alloc.free_range(mesh.index_range);
        });
    }

    fn reallocate_buffers(
        &mut self,
        device: &Device,
        encoder: &mut CommandEncoder,
        object_manager: &mut ObjectManager,
        skeleton_manager: &mut SkeletonManager,
        needed_verts: u32,
        needed_indices: u32,
    ) {
        profiling::scope!("reallocate mesh buffers");

        let new_vert_count = (self.vertex_count() + needed_verts as usize).next_power_of_two();
        let new_index_count = (self.index_count() + needed_indices as usize).next_power_of_two();

        log::debug!(
            "Recreating vertex buffer from {} to {}",
            self.vertex_count(),
            new_vert_count
        );
        log::debug!(
            "Recreating index buffer from {} to {}",
            self.index_count(),
            new_index_count
        );

        let new_buffers = create_buffers(device, new_vert_count, new_index_count);

        let mut new_vert_alloc = RangeAllocator::new(0..new_vert_count);
        let mut new_index_alloc = RangeAllocator::new(0..new_index_count);

        for mesh in self.registry.values_mut() {
            if mesh.index_range.is_empty() {
                continue;
            }

            let new_vert_range = new_vert_alloc.allocate_range(mesh.vertex_range.len()).unwrap();
            let new_index_range = new_index_alloc.allocate_range(mesh.index_range.len()).unwrap();

            // Copy the vertex data to the new buffer range
            copy_to_new_buffers(
                encoder,
                &self.buffers,
                &new_buffers,
                &mesh.vertex_range,
                &new_vert_range,
            );

            // Copy the skeleton data that was copied from this mesh
            for skeleton_handle in mesh.skeletons.iter() {
                let skeleton = skeleton_manager.internal_data(*skeleton_handle);
                let new_skeleton_vert_range = new_vert_alloc
                    .allocate_range(skeleton.skeleton_vertex_range.len())
                    .unwrap();
                copy_to_new_buffers(
                    encoder,
                    &self.buffers,
                    &new_buffers,
                    &skeleton.skeleton_vertex_range,
                    &new_skeleton_vert_range,
                );

                // Update the cache range data on the skeleton
                skeleton_manager.set_skeleton_range(*skeleton_handle, &new_skeleton_vert_range, &new_vert_range);
            }

            // Copy indices over to new buffer, adjusting their value by the difference
            let index_copy_start = mesh.index_range.start * INDEX_SIZE;
            let index_copy_size = (mesh.index_range.end * INDEX_SIZE) - index_copy_start;
            let index_output = new_index_range.start * INDEX_SIZE;
            encoder.copy_buffer_to_buffer(
                &self.buffers.index,
                index_copy_start as u64,
                &new_buffers.index,
                index_output as u64,
                index_copy_size as u64,
            );

            mesh.vertex_range = new_vert_range;
            mesh.index_range = new_index_range;
        }

        // Need to call this to update the vertex ranges cached inside the objects
        object_manager.fix_objects_after_realloc(self, skeleton_manager);

        self.buffers = new_buffers;
        self.vertex_alloc = new_vert_alloc;
        self.index_alloc = new_index_alloc;
    }

    fn vertex_count(&self) -> usize {
        self.vertex_alloc.initial_range().end
    }

    fn index_count(&self) -> usize {
        self.index_alloc.initial_range().end
    }
}

fn copy_to_new_buffers(
    encoder: &mut CommandEncoder,
    current_buffers: &MeshBuffers,
    new_buffers: &MeshBuffers,
    current_range: &Range<usize>,
    new_range: &Range<usize>,
) {
    copy_vert(
        encoder,
        &current_buffers.vertex_position,
        &new_buffers.vertex_position,
        current_range,
        new_range,
        VERTEX_POSITION_SIZE,
    );
    copy_vert(
        encoder,
        &current_buffers.vertex_normal,
        &new_buffers.vertex_normal,
        current_range,
        new_range,
        VERTEX_NORMAL_SIZE,
    );
    copy_vert(
        encoder,
        &current_buffers.vertex_tangent,
        &new_buffers.vertex_tangent,
        current_range,
        new_range,
        VERTEX_TANGENT_SIZE,
    );
    copy_vert(
        encoder,
        &current_buffers.vertex_uv0,
        &new_buffers.vertex_uv0,
        current_range,
        new_range,
        VERTEX_UV_SIZE,
    );
    copy_vert(
        encoder,
        &current_buffers.vertex_uv1,
        &new_buffers.vertex_uv1,
        current_range,
        new_range,
        VERTEX_UV_SIZE,
    );
    copy_vert(
        encoder,
        &current_buffers.vertex_color,
        &new_buffers.vertex_color,
        current_range,
        new_range,
        VERTEX_COLOR_SIZE,
    );
    copy_vert(
        encoder,
        &current_buffers.vertex_joint_index,
        &new_buffers.vertex_joint_index,
        current_range,
        new_range,
        VERTEX_JOINT_INDEX_SIZE,
    );
    copy_vert(
        encoder,
        &current_buffers.vertex_joint_weight,
        &new_buffers.vertex_joint_weight,
        current_range,
        new_range,
        VERTEX_JOINT_WEIGHT_SIZE,
    );
}

fn copy_vert(
    encoder: &mut CommandEncoder,
    src: &Buffer,
    dst: &Buffer,
    orig_vert_range: &Range<usize>,
    new_vert_range: &Range<usize>,
    size: usize,
) {
    let vert_copy_start = orig_vert_range.start * size;
    let vert_copy_size = (orig_vert_range.end * size) - vert_copy_start;
    let vert_output = new_vert_range.start * size;
    encoder.copy_buffer_to_buffer(
        src,
        vert_copy_start as u64,
        dst,
        vert_output as u64,
        vert_copy_size as u64,
    );
}

fn create_buffers(device: &Device, vertex_count: usize, index_count: usize) -> MeshBuffers {
    profiling::scope!("mesh buffers creation");

    let position_bytes = vertex_count * VERTEX_POSITION_SIZE;
    let normal_bytes = vertex_count * VERTEX_NORMAL_SIZE;
    let tangent_bytes = vertex_count * VERTEX_TANGENT_SIZE;
    let uv_bytes = vertex_count * VERTEX_UV_SIZE;
    let color_bytes = vertex_count * VERTEX_COLOR_SIZE;
    let joint_index_bytes = vertex_count * VERTEX_JOINT_INDEX_SIZE;
    let joint_weight_bytes = vertex_count * VERTEX_JOINT_WEIGHT_SIZE;
    let index_bytes = index_count * INDEX_SIZE;

    let vertex_position = device.create_buffer(&BufferDescriptor {
        label: Some("position vertex buffer"),
        size: position_bytes as BufferAddress,
        usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST | BufferUsages::VERTEX | BufferUsages::STORAGE,
        mapped_at_creation: false,
    });

    let vertex_normal = device.create_buffer(&BufferDescriptor {
        label: Some("normal vertex buffer"),
        size: normal_bytes as BufferAddress,
        usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST | BufferUsages::VERTEX | BufferUsages::STORAGE,
        mapped_at_creation: false,
    });

    let vertex_tangent = device.create_buffer(&BufferDescriptor {
        label: Some("tangent vertex buffer"),
        size: tangent_bytes as BufferAddress,
        usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST | BufferUsages::VERTEX | BufferUsages::STORAGE,
        mapped_at_creation: false,
    });

    let vertex_uv0 = device.create_buffer(&BufferDescriptor {
        label: Some("uv0 vertex buffer"),
        size: uv_bytes as BufferAddress,
        usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST | BufferUsages::VERTEX | BufferUsages::STORAGE,
        mapped_at_creation: false,
    });

    let vertex_uv1 = device.create_buffer(&BufferDescriptor {
        label: Some("uv1 vertex buffer"),
        size: uv_bytes as BufferAddress,
        usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST | BufferUsages::VERTEX | BufferUsages::STORAGE,
        mapped_at_creation: false,
    });

    let vertex_color = device.create_buffer(&BufferDescriptor {
        label: Some("color vertex buffer"),
        size: color_bytes as BufferAddress,
        usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST | BufferUsages::VERTEX | BufferUsages::STORAGE,
        mapped_at_creation: false,
    });

    let vertex_joint_index = device.create_buffer(&BufferDescriptor {
        label: Some("joint index vertex buffer"),
        size: joint_index_bytes as BufferAddress,
        usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST | BufferUsages::VERTEX | BufferUsages::STORAGE,
        mapped_at_creation: false,
    });

    let vertex_joint_weight = device.create_buffer(&BufferDescriptor {
        label: Some("joint weight vertex buffer"),
        size: joint_weight_bytes as BufferAddress,
        usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST | BufferUsages::VERTEX | BufferUsages::STORAGE,
        mapped_at_creation: false,
    });

    let index = device.create_buffer(&BufferDescriptor {
        label: Some("index buffer"),
        size: index_bytes as BufferAddress,
        usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST | BufferUsages::INDEX | BufferUsages::STORAGE,
        mapped_at_creation: false,
    });

    MeshBuffers {
        vertex_position,
        vertex_normal,
        vertex_tangent,
        vertex_uv0,
        vertex_uv1,
        vertex_color,
        vertex_joint_index,
        vertex_joint_weight,
        index,
    }
}
