use std::{
    borrow::Cow,
    ops::{Index, Range},
    sync::Arc,
};

use parking_lot::{Mutex, MutexGuard, RwLock};
use range_alloc::RangeAllocator;
use rend3_types::{RawMeshHandle, VertexAttributeId, VERTEX_ATTRIBUTE_JOINT_INDICES, VERTEX_ATTRIBUTE_POSITION};
use thiserror::Error;
use wgpu::{Buffer, BufferAddress, BufferDescriptor, BufferUsages, CommandEncoder, Device};

use crate::{
    types::{Mesh, MeshHandle},
    util::{error_scope::AllocationErrorScope, frustum::BoundingSphere, upload::upload_buffer_via_encoder},
};

/// Vertex buffer slot for object indices
/// Note that this slot is only used in the GpuDriven profile.
pub const VERTEX_OBJECT_INDEX_SLOT: u32 = 0;

/// Pre-allocated mesh data. 32MB.
pub const STARTING_MESH_DATA: u64 = 1 << 25;

/// Internal representation of a mesh.
pub struct InternalMesh {
    /// Location in the vertex buffer for each vertex attribute
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

    pub fn get_attribute(&self, attribute: &VertexAttributeId) -> Option<Range<u64>> {
        self.vertex_attribute_ranges
            .iter()
            .find_map(|(id, range)| (*id == *attribute).then_some(range.clone()))
    }
}

#[derive(Debug, Error)]
pub enum MeshCreationError {
    #[error("Tried to grow mesh data buffer to {size}, but allocation failed")]
    BufferAllocationFailed {
        size: u64,
        #[source]
        inner: wgpu::Error,
    },
    #[error("Exceeded maximum mesh data buffer size of {max_buffer_size}")]
    ExceededMaximumBufferSize { max_buffer_size: u32 },
    #[error(
        "Failed to write new mesh data to buffer using write_buffer_with.
         Tried to write {bytes} bytes. Trying to write {}",
        if let Some(attribute) = attribute {
            Cow::Owned(format!("attribute data named \"{}\"", attribute.name()))
        } else {
            Cow::Borrowed("the index data")
        }
    )]
    BufferWriteFailed {
        attribute: Option<VertexAttributeId>,
        bytes: u64,
        #[source]
        inner: wgpu::Error,
    },
}

/// Manages vertex and instance buffers. All buffers are sub-allocated from
/// megabuffers.
pub struct MeshManager {
    buffer: RwLock<Arc<Buffer>>,

    allocator: Mutex<RangeAllocator<u64>>,

    data: Mutex<Vec<Option<InternalMesh>>>,
}

impl MeshManager {
    pub fn new(device: &Device) -> Self {
        profiling::scope!("MeshManager::new");

        let buffer: RwLock<Arc<Buffer>> = RwLock::new(Arc::new(device.create_buffer(&BufferDescriptor {
            label: Some("mesh data buffer"),
            size: STARTING_MESH_DATA as BufferAddress,
            usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST | BufferUsages::INDEX | BufferUsages::STORAGE,
            mapped_at_creation: false,
        })));

        let allocator = Mutex::new(RangeAllocator::new(0..STARTING_MESH_DATA));

        let data = Mutex::new(Vec::new());

        Self {
            buffer,
            allocator,
            data,
        }
    }

    pub fn add(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        mesh: Mesh,
    ) -> Result<InternalMesh, MeshCreationError> {
        profiling::scope!("MeshManager::add");

        let vertex_count = mesh.vertex_count;
        let index_count = mesh.indices.len();

        if vertex_count == 0 || index_count == 0 {
            return Ok(InternalMesh::new_empty());
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

        let mut allocator_guard = self.allocator.lock();
        let mut vertex_attribute_ranges = Vec::with_capacity(mesh.attributes.len());
        for attribute in &mesh.attributes {
            let range = self.allocate_range_impl(device, encoder, &mut allocator_guard, attribute.bytes())?;
            vertex_attribute_ranges.push((*attribute.id(), range));
        }
        let index_range = self.allocate_range_impl(device, encoder, &mut allocator_guard, index_count as u64 * 4)?;
        drop(allocator_guard);

        let buffer_guard = self.buffer.read();
        for (attribute_data, (attribute, range)) in mesh.attributes.iter().zip(&vertex_attribute_ranges) {
            upload_buffer_via_encoder(
                device,
                encoder,
                &buffer_guard,
                range.start,
                attribute_data.untyped_data(),
            )
            .map_err(|e| MeshCreationError::BufferWriteFailed {
                attribute: Some(*attribute),
                bytes: attribute_data.bytes(),
                inner: e,
            })?;
        }

        let index_write_size = mesh.indices.len() as u64 * 4;
        upload_buffer_via_encoder(
            device,
            encoder,
            &buffer_guard,
            index_range.start,
            bytemuck::cast_slice(&mesh.indices),
        )
        .map_err(|e| MeshCreationError::BufferWriteFailed {
            attribute: None,
            bytes: index_write_size,
            inner: e,
        })?;
        drop(buffer_guard);

        // We can cheat here as we know vertex positions are always the first attribute as they must exist.
        let bounding_sphere = BoundingSphere::from_mesh(
            mesh.attributes
                .first()
                .expect("Meshes first attributes must always exist")
                .typed_data(&VERTEX_ATTRIBUTE_POSITION)
                .expect("Meshes must have positions"),
        );

        Ok(InternalMesh {
            vertex_attribute_ranges,
            vertex_count: mesh.vertex_count as u32,
            index_range,
            required_joint_count,
            bounding_sphere,
        })
    }

    pub fn fill(&self, handle: &MeshHandle, mesh: InternalMesh) {
        profiling::scope!("MeshManager::fill");

        let mut data_guard = self.data.lock();
        if handle.idx >= data_guard.len() {
            data_guard.resize_with(handle.idx + 1, || None);
        }
        data_guard[handle.idx] = Some(mesh);
        drop(data_guard);
    }

    pub fn remove(&self, object_id: RawMeshHandle) {
        let mesh = self.data.lock()[object_id.idx].take().unwrap();

        let mut allocator_guard = self.allocator.lock();
        for (_id, range) in mesh.vertex_attribute_ranges {
            if range.is_empty() {
                continue;
            }
            allocator_guard.free_range(range);
        }
        if mesh.index_range.is_empty() {
            return;
        }
        allocator_guard.free_range(mesh.index_range);
    }

    pub fn evaluate(&self) -> Arc<Buffer> {
        self.buffer.read().clone()
    }

    /// Duplicates a mesh's vertex data so that it can be skinned on the GPU.
    pub fn allocate_range(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        bytes: u64,
    ) -> Result<Range<u64>, MeshCreationError> {
        self.allocate_range_impl(device, encoder, &mut self.allocator.lock(), bytes)
    }

    fn allocate_range_impl(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        allocator: &mut RangeAllocator<u64>,
        bytes: u64,
    ) -> Result<Range<u64>, MeshCreationError> {
        Ok(match allocator.allocate_range(bytes) {
            Ok(range) => range,
            Err(..) => {
                self.reallocate_buffers(device, encoder, allocator, bytes)?;
                allocator.allocate_range(bytes).expect(
                    "Second allocation range should always succeed, as there should always be enough space in the tail of the buffer",
                )
            }
        })
    }

    pub fn free_range(&self, range: Range<u64>) {
        Self::free_range_impl(&mut self.allocator.lock(), range);
    }

    fn free_range_impl(allocator: &mut RangeAllocator<u64>, range: Range<u64>) {
        if range.is_empty() {
            return;
        }
        allocator.free_range(range);
    }

    pub fn lock_internal_data(&self) -> LockedInternalMeshDataArray<'_> {
        LockedInternalMeshDataArray(self.data.lock())
    }

    fn reallocate_buffers(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        allocator: &mut RangeAllocator<u64>,
        needed_bytes: u64,
    ) -> Result<(), MeshCreationError> {
        profiling::scope!("reallocate mesh buffers");

        let current_bytes = allocator.initial_range().end;
        let desired_bytes = current_bytes
            .checked_add(needed_bytes)
            .expect("Using more than 2^64 bytes of mesh data")
            .checked_next_power_of_two()
            .expect("Using more than 2^63 bytes of mesh data");

        let max_buffer_size = device.limits().max_storage_buffer_binding_size;

        let new_bytes = desired_bytes.min(max_buffer_size as u64);

        if new_bytes == current_bytes {
            return Err(MeshCreationError::ExceededMaximumBufferSize { max_buffer_size });
        }

        let scope = AllocationErrorScope::new(device);
        let new_buffer = Arc::new(device.create_buffer(&BufferDescriptor {
            label: Some("mesh data buffer"),
            size: new_bytes,
            usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST | BufferUsages::INDEX | BufferUsages::STORAGE,
            mapped_at_creation: false,
        }));
        scope.end().map_err(|e| MeshCreationError::BufferAllocationFailed {
            size: new_bytes,
            inner: e,
        })?;

        let mut buffer_guard = self.buffer.write();
        encoder.copy_buffer_to_buffer(&buffer_guard, 0, &new_buffer, 0, allocator.initial_range().end);

        *buffer_guard = new_buffer;
        allocator.grow_to(new_bytes);

        Ok(())
    }
}

pub struct LockedInternalMeshDataArray<'a>(MutexGuard<'a, Vec<Option<InternalMesh>>>);

impl<'a> Index<RawMeshHandle> for LockedInternalMeshDataArray<'a> {
    type Output = InternalMesh;

    fn index(&self, handle: RawMeshHandle) -> &Self::Output {
        self.0[handle.idx].as_ref().unwrap()
    }
}
