use crate::{
    datatypes::{Mesh, MeshHandle},
    registry::ResourceRegistry,
    renderer::{copy::GpuCopy, frustum::BoundingSphere},
};
use glam::{Vec2, Vec3};
use range_alloc::RangeAllocator;
use std::{mem::size_of, ops::Range};
use wgpu::{
    Buffer, BufferAddress, BufferDescriptor, BufferUsage, CommandEncoder, ComputePassDescriptor, Device, Queue,
};

pub const VERTEX_POSITION_SIZE: usize = size_of::<Vec3>();
pub const VERTEX_NORMAL_SIZE: usize = size_of::<Vec3>();
pub const VERTEX_TANGENT_SIZE: usize = size_of::<Vec3>();
pub const VERTEX_UV_SIZE: usize = size_of::<Vec2>();
pub const VERTEX_COLOR_SIZE: usize = size_of::<[u8; 4]>();
pub const VERTEX_MATERIAL_INDEX_SIZE: usize = size_of::<u32>();
const INDEX_SIZE: usize = size_of::<u32>();

const STARTING_VERTICES: usize = 1 << 16;
const STARTING_INDICES: usize = 1 << 16;

pub struct InternalMesh {
    pub vertex_range: Range<usize>,
    pub index_range: Range<usize>,
    pub bounding_sphere: BoundingSphere,
}

pub struct MeshBuffers {
    pub vertex_position: Buffer,
    pub vertex_normal: Buffer,
    pub vertex_tangent: Buffer,
    pub vertex_uv: Buffer,
    pub vertex_color: Buffer,
    pub vertex_mat_index: Buffer,

    pub index: Buffer,
}

pub struct MeshManager {
    buffers: MeshBuffers,

    vertex_count: usize,
    vertex_alloc: RangeAllocator<usize>,

    index_count: usize,
    index_alloc: RangeAllocator<usize>,

    registry: ResourceRegistry<InternalMesh>,
}

impl MeshManager {
    pub fn new(device: &Device) -> Self {
        span_transfer!(_ -> new_span, INFO, "Creating Mesh Manager");

        let buffers = create_buffers(device, STARTING_VERTICES, STARTING_INDICES);

        let vertex_count = STARTING_VERTICES;
        let index_count = STARTING_INDICES;

        let vertex_alloc = RangeAllocator::new(0..vertex_count);
        let index_alloc = RangeAllocator::new(0..index_count);

        let registry = ResourceRegistry::new();

        Self {
            buffers,
            vertex_count,
            vertex_alloc,
            index_count,
            index_alloc,
            registry,
        }
    }

    pub fn allocate(&self) -> MeshHandle {
        MeshHandle(self.registry.allocate())
    }

    pub fn fill(
        &mut self,
        device: &Device,
        queue: &Queue,
        gpu_copy: &GpuCopy,
        encoder: &mut CommandEncoder,
        handle: MeshHandle,
        mesh: Mesh,
    ) {
        span_transfer!(_ -> fill_span, INFO, "Mesh Manager Fill");

        assert!(mesh.validate());

        let vertex_count = mesh.vertex_positions.len();
        let index_count = mesh.indices.len();

        let mut vertex_range = self.vertex_alloc.allocate_range(vertex_count).ok();
        let mut index_range = self.index_alloc.allocate_range(index_count).ok();

        let needed = match (&vertex_range, &index_range) {
            (None, Some(_)) => Some((vertex_count, 0)),
            (Some(_), None) => Some((0, index_count)),
            (None, None) => Some((vertex_count, index_count)),
            _ => None,
        };

        if let Some((needed_verts, needed_indices)) = needed {
            self.reallocate_buffers(device, encoder, gpu_copy, needed_verts as u32, needed_indices as u32);
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
            &self.buffers.vertex_uv,
            (vertex_range.start * VERTEX_UV_SIZE) as BufferAddress,
            bytemuck::cast_slice(&mesh.vertex_uvs),
        );
        queue.write_buffer(
            &self.buffers.vertex_color,
            (vertex_range.start * VERTEX_COLOR_SIZE) as BufferAddress,
            bytemuck::cast_slice(&mesh.vertex_colors),
        );
        queue.write_buffer(
            &self.buffers.vertex_mat_index,
            (vertex_range.start * VERTEX_MATERIAL_INDEX_SIZE) as BufferAddress,
            bytemuck::cast_slice(&mesh.vertex_material_indices),
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
        };

        self.registry.insert(handle.0, mesh);
    }

    pub fn remove(&mut self, handle: MeshHandle) {
        let mesh = self.registry.remove(handle.0).1;

        self.vertex_alloc.free_range(mesh.vertex_range);
        self.index_alloc.free_range(mesh.index_range);
    }

    pub fn buffers(&self) -> &MeshBuffers {
        &self.buffers
    }

    pub fn internal_data(&self, handle: MeshHandle) -> &InternalMesh {
        self.registry.get(handle.0)
    }

    pub fn reallocate_buffers(
        &mut self,
        device: &Device,
        encoder: &mut CommandEncoder,
        gpu_copy: &GpuCopy,
        needed_verts: u32,
        needed_indices: u32,
    ) {
        let new_vert_count = (self.vertex_count + needed_verts as usize).next_power_of_two();
        let new_index_count = (self.index_count + needed_indices as usize).next_power_of_two();

        tracing::debug!(
            "Recreating vertex buffer from {} to {}",
            self.vertex_count,
            new_vert_count
        );
        tracing::debug!(
            "Recreating index buffer from {} to {}",
            self.index_count,
            new_index_count
        );

        let new_buffers = create_buffers(device, new_vert_count, new_index_count);

        let mut new_vert_alloc = RangeAllocator::new(0..new_vert_count);
        let mut new_index_alloc = RangeAllocator::new(0..new_index_count);

        let vertex_position_copy_data = gpu_copy.prepare(
            device,
            &self.buffers.vertex_position,
            &new_buffers.vertex_position,
            "vertex position copy",
        );

        let vertex_normal_copy_data = gpu_copy.prepare(
            device,
            &self.buffers.vertex_normal,
            &new_buffers.vertex_normal,
            "vertex normal copy",
        );

        let vertex_tangent_copy_data = gpu_copy.prepare(
            device,
            &self.buffers.vertex_tangent,
            &new_buffers.vertex_tangent,
            "vertex tangent copy",
        );

        let vertex_uv_copy_data = gpu_copy.prepare(
            device,
            &self.buffers.vertex_uv,
            &new_buffers.vertex_uv,
            "vertex uv copy",
        );

        let vertex_color_copy_data = gpu_copy.prepare(
            device,
            &self.buffers.vertex_color,
            &new_buffers.vertex_color,
            "vertex color copy",
        );

        let vertex_mat_index_copy_data = gpu_copy.prepare(
            device,
            &self.buffers.vertex_mat_index,
            &new_buffers.vertex_mat_index,
            "vertex material index copy",
        );
        let index_copy_data = gpu_copy.prepare(device, &self.buffers.index, &new_buffers.index, "index copy");

        let mut cpass = encoder.begin_compute_pass(&ComputePassDescriptor::default());

        for mesh in self.registry.values_mut() {
            let new_vert_range = new_vert_alloc.allocate_range(mesh.vertex_range.len()).unwrap();
            let new_index_range = new_index_alloc.allocate_range(mesh.index_range.len()).unwrap();

            let vert_difference = new_vert_range.start as isize - mesh.vertex_range.start as isize;

            // TODO: This was once a function but borrowck wasn't happy with me, could I make it happy?
            macro_rules! copy_vert_fn {
                ($data:expr, $size:expr) => {
                    // Copy verts over to new buffer
                    let vert_copy_start = (mesh.vertex_range.start * $size) / 4;
                    let vert_copy_end = (mesh.vertex_range.end * $size) / 4;
                    let vert_output = (new_vert_range.start * $size) / 4;
                    gpu_copy.copy_words(
                        &mut cpass,
                        $data,
                        vert_copy_start as u32..vert_copy_end as u32,
                        vert_output as u32,
                    );
                };
            }

            copy_vert_fn!(&vertex_position_copy_data, VERTEX_POSITION_SIZE);
            copy_vert_fn!(&vertex_normal_copy_data, VERTEX_NORMAL_SIZE);
            copy_vert_fn!(&vertex_tangent_copy_data, VERTEX_TANGENT_SIZE);
            copy_vert_fn!(&vertex_uv_copy_data, VERTEX_UV_SIZE);
            copy_vert_fn!(&vertex_color_copy_data, VERTEX_COLOR_SIZE);
            copy_vert_fn!(&vertex_mat_index_copy_data, VERTEX_MATERIAL_INDEX_SIZE);

            // Copy indices over to new buffer, adjusting their value by the difference
            let index_copy_start = (mesh.index_range.start * INDEX_SIZE) / 4;
            let index_copy_end = (mesh.index_range.end * INDEX_SIZE) / 4;
            let index_output = (new_index_range.start * INDEX_SIZE) / 4;
            gpu_copy.copy_words_with_offset(
                &mut cpass,
                &index_copy_data,
                index_copy_start as u32..index_copy_end as u32,
                index_output as u32,
                vert_difference as i32,
            );

            mesh.vertex_range = new_vert_range;
            mesh.index_range = new_index_range;
        }

        drop(cpass);

        self.buffers = new_buffers;
        self.vertex_count = new_vert_count;
        self.index_count = new_index_count;
        self.vertex_alloc = new_vert_alloc;
        self.index_alloc = new_index_alloc;
    }
}

fn create_buffers(device: &Device, vertex_count: usize, index_count: usize) -> MeshBuffers {
    let position_bytes = vertex_count * VERTEX_POSITION_SIZE;
    let normal_bytes = vertex_count * VERTEX_NORMAL_SIZE;
    let tangent_bytes = vertex_count * VERTEX_TANGENT_SIZE;
    let uv_bytes = vertex_count * VERTEX_UV_SIZE;
    let color_bytes = vertex_count * VERTEX_COLOR_SIZE;
    let mat_index_bytes = vertex_count * VERTEX_MATERIAL_INDEX_SIZE;
    let index_bytes = index_count * INDEX_SIZE;

    let vertex_position = device.create_buffer(&BufferDescriptor {
        label: Some("position vertex buffer"),
        size: position_bytes as BufferAddress,
        usage: BufferUsage::COPY_DST | BufferUsage::VERTEX | BufferUsage::STORAGE,
        mapped_at_creation: false,
    });

    let vertex_normal = device.create_buffer(&BufferDescriptor {
        label: Some("normal vertex buffer"),
        size: normal_bytes as BufferAddress,
        usage: BufferUsage::COPY_DST | BufferUsage::VERTEX | BufferUsage::STORAGE,
        mapped_at_creation: false,
    });

    let vertex_tangent = device.create_buffer(&BufferDescriptor {
        label: Some("tangent vertex buffer"),
        size: tangent_bytes as BufferAddress,
        usage: BufferUsage::COPY_DST | BufferUsage::VERTEX | BufferUsage::STORAGE,
        mapped_at_creation: false,
    });

    let vertex_uv = device.create_buffer(&BufferDescriptor {
        label: Some("uv vertex buffer"),
        size: uv_bytes as BufferAddress,
        usage: BufferUsage::COPY_DST | BufferUsage::VERTEX | BufferUsage::STORAGE,
        mapped_at_creation: false,
    });

    let vertex_color = device.create_buffer(&BufferDescriptor {
        label: Some("color vertex buffer"),
        size: color_bytes as BufferAddress,
        usage: BufferUsage::COPY_DST | BufferUsage::VERTEX | BufferUsage::STORAGE,
        mapped_at_creation: false,
    });

    let vertex_mat_index = device.create_buffer(&BufferDescriptor {
        label: Some("material index vertex buffer"),
        size: mat_index_bytes as BufferAddress,
        usage: BufferUsage::COPY_DST | BufferUsage::VERTEX | BufferUsage::STORAGE,
        mapped_at_creation: false,
    });

    let index = device.create_buffer(&BufferDescriptor {
        label: Some("index buffer"),
        size: index_bytes as BufferAddress,
        usage: BufferUsage::COPY_DST | BufferUsage::INDEX | BufferUsage::STORAGE,
        mapped_at_creation: false,
    });

    MeshBuffers {
        vertex_position,
        vertex_normal,
        vertex_tangent,
        vertex_uv,
        vertex_color,
        vertex_mat_index,
        index,
    }
}
