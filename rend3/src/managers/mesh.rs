use crate::{
    types::{Mesh, MeshHandle},
    util::frustum::BoundingSphere,
};

use range_alloc::RangeAllocator;
use rend3_types::{RawMeshHandle, VertexAttributeId, VERTEX_ATTRIBUTE_JOINT_INDICES, VERTEX_ATTRIBUTE_POSITION};
use std::ops::Range;
use wgpu::{BufferAddress, BufferDescriptor, BufferUsages, CommandEncoder, Device, Queue};

/// Vertex buffer slot for object indices
/// Note that this slot is only used in the GpuDriven profile.
pub const VERTEX_OBJECT_INDEX_SLOT: u32 = 0;

/// Pre-allocated mesh data. 32MB.
pub const STARTING_MESH_DATA: u64 = 1 << 25;

/// Internal representation of a mesh.
pub struct InternalMesh {
    /// Range of values from the [`MeshBuffers`] where vertex data for this mesh
    /// is
    pub vertex_attribute_ranges: Vec<(VertexAttributeId, Range<u64>)>,
    /// Vertex count
    pub vertex_count: u32,
    /// Range in the mesh data buffer where index data for this mesh resides.
    pub index_range: Range<u64>,
    /// For skinned meshes, stores the maximum joint index present in the joint
    /// index buffer. None means it has no joint index buffer.
    pub required_joint_count: Option<u16>,
    /// The bounding sphere of this mesh. Used for culling.
    pub bounding_sphere: BoundingSphere,
}

impl InternalMesh {
    /// Returns an empty InternalMesh
    fn new_empty() -> Self {
        InternalMesh {
            vertex_attribute_ranges: Vec::new(),
            vertex_count: 0,
            index_range: 0..0,
            required_joint_count: None,
            bounding_sphere: BoundingSphere::from_mesh(&[]),
        }
    }

    pub fn get_attribute(&self, attribute: &'static VertexAttributeId) -> Option<Range<u64>> {
        self.vertex_attribute_ranges
            .iter()
            .find_map(|(id, range)| (*id == *attribute).then_some(range.clone()))
    }
}

/// Manages vertex and instance buffers. All buffers are sub-allocated from
/// megabuffers.
pub struct MeshManager {
    buffer: wgpu::Buffer,

    allocator: RangeAllocator<u64>,

    data: Vec<Option<InternalMesh>>,
}

impl MeshManager {
    pub fn new(device: &Device) -> Self {
        profiling::scope!("MeshManager::new");

        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some("mesh data buffer"),
            size: STARTING_MESH_DATA as BufferAddress,
            usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST | BufferUsages::INDEX | BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        let allocator = RangeAllocator::new(0..STARTING_MESH_DATA);

        let data = Vec::new();

        Self {
            buffer,
            allocator,
            data,
        }
    }

    pub fn add(
        &mut self,
        device: &Device,
        queue: &Queue,
        encoder: &mut CommandEncoder,
        handle: &MeshHandle,
        mesh: Mesh,
    ) {
        profiling::scope!("MeshManager::fill");

        let index_count = mesh.indices.len();

        // If vertex_count is 0, index_count _must_ also be 0, as all indices would be
        // out of range.
        if index_count == 0 {
            let mesh = InternalMesh::new_empty();
            self.data[handle.idx] = Some(mesh);
            return;
        }

        // This value is used later when setting joints, to make sure all indices are
        // in-bounds with the specified amount of joints.
        let mut required_joint_count = None;
        let joint_indices_attribute = mesh
            .attributes
            .iter()
            .find_map(|attribute| attribute.typed_data(&VERTEX_ATTRIBUTE_JOINT_INDICES));
        if let Some(joint_indices) = joint_indices_attribute {
            required_joint_count = Some(joint_indices.iter().flatten().max().map_or(0, |v| v + 1));
        }

        let mut vertex_attribute_ranges = Vec::with_capacity(mesh.attributes.len());
        for attribute in &mesh.attributes {
            let range = self.allocate_range(device, encoder, attribute.bytes());
            vertex_attribute_ranges.push((attribute.id(), range));
        }

        let index_range = self.allocate_range(device, encoder, index_count as u64 * 4);

        for (attribute_data, (_, range)) in mesh.attributes.iter().zip(&vertex_attribute_ranges) {
            queue.write_buffer(&self.buffer, range.start as u64, attribute_data.untyped_data());
        }

        // We can cheat here as we know vertex positions are always the first attribute as they must exist.
        let bounding_sphere = BoundingSphere::from_mesh(
            &mesh
                .attributes
                .first()
                .unwrap()
                .typed_data(&VERTEX_ATTRIBUTE_POSITION)
                .unwrap(),
        );

        let mesh = InternalMesh {
            vertex_attribute_ranges,
            vertex_count: mesh.vertex_count,
            index_range,
            required_joint_count,
            bounding_sphere,
        };

        if handle.idx >= self.data.len() {
            self.data.resize_with(handle.idx + 1, || None);
        }
        self.data[handle.idx] = Some(mesh);
    }

    pub fn remove(&mut self, object_id: RawMeshHandle) {
        let mesh = self.data[object_id.idx].take().unwrap();

        for (_id, range) in mesh.vertex_attribute_ranges {
            if range.is_empty() {
                continue;
            }
            self.allocator.free_range(range);
        }
        if mesh.index_range.is_empty() {
            return;
        }
        self.allocator.free_range(mesh.index_range);
    }

    /// Duplicates a mesh's vertex data so that it can be skinned on the GPU.
    pub fn allocate_range(&mut self, device: &Device, encoder: &mut CommandEncoder, bytes: u64) -> Range<u64> {
        match self.allocator.allocate_range(bytes) {
            Ok(range) => range,
            Err(..) => {
                self.reallocate_buffers(device, encoder, bytes);
                self.allocator.allocate_range(bytes).unwrap()
            }
        }
    }

    pub fn free_range(&mut self, range: Range<u64>) {
        if range.is_empty() {
            return;
        }
        self.allocator.free_range(range);
    }

    pub fn internal_data(&self, handle: RawMeshHandle) -> &InternalMesh {
        self.data[handle.idx].as_ref().unwrap()
    }

    pub fn internal_data_mut(&mut self, handle: RawMeshHandle) -> &mut InternalMesh {
        self.data[handle.idx].as_mut().unwrap()
    }

    fn reallocate_buffers(&mut self, device: &Device, encoder: &mut CommandEncoder, needed_bytes: u64) {
        profiling::scope!("reallocate mesh buffers");

        // We subtract one, in case we end up at a power of two after adding.
        let new_bytes = self
            .allocator
            .initial_range()
            .end
            .checked_add(needed_bytes)
            .unwrap()
            .saturating_sub(1)
            .next_power_of_two();

        let new_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("mesh data buffer"),
            size: new_bytes,
            usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST | BufferUsages::INDEX | BufferUsages::STORAGE,
            mapped_at_creation: false,
        });

        encoder.copy_buffer_to_buffer(&self.buffer, 0, &new_buffer, 0, self.allocator.initial_range().end);

        self.buffer = new_buffer;
        self.allocator.grow_to(new_bytes);
    }
}
