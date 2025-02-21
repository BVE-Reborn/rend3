use std::{
    mem,
    ops::{Index, Range},
    sync::Arc,
};

use parking_lot::{Mutex, MutexGuard};
use range_alloc::RangeAllocator;
use rend3_types::{RawMeshHandle, VertexAttributeId, VERTEX_ATTRIBUTE_JOINT_INDICES, VERTEX_ATTRIBUTE_POSITION};
use thiserror::Error;
use wgpu::{
    Buffer, BufferAddress, BufferDescriptor, BufferUsages, CommandBuffer, CommandEncoder, CommandEncoderDescriptor,
    Device,
};

use crate::{
    types::{Mesh, MeshHandle},
    util::{error_scope::AllocationErrorScope, frustum::BoundingSphere, sync::WaitGroup, upload::UploadChainer},
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
        self.vertex_attribute_ranges.iter().find_map(|(id, range)| (*id == *attribute).then_some(range.clone()))
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
    #[error("Failed to write new mesh data to buffer. Failed to allocate staging buffer.")]
    BufferWriteFailed {
        #[source]
        inner: wgpu::Error,
    },
}

/// Contains all the state for the mesh buffer.
///
/// As the mesh manager is multithreaded, this needs to be wrapped in a single mutex
/// to make sure a single order of operations happen to all of them.
pub struct BufferState {
    pub buffer: Arc<Buffer>,
    pub allocator: RangeAllocator<u64>,
    pub encoder: CommandEncoder,

    // We need to block submission until all the staging actions are complete
    // and the buffers are no longer mapped.
    pub wait_group: Arc<WaitGroup>,
}

/// Manages vertex and instance buffers. All buffers are sub-allocated from
/// megabuffers.
pub struct MeshManager {
    buffer_state: Mutex<BufferState>,

    data: Mutex<Vec<Option<InternalMesh>>>,
}

impl MeshManager {
    pub fn new(device: &Device) -> Self {
        profiling::scope!("MeshManager::new");

        let buffer = Arc::new(device.create_buffer(&BufferDescriptor {
            label: Some("mesh data buffer"),
            size: STARTING_MESH_DATA as BufferAddress,
            usage: BufferUsages::COPY_SRC | BufferUsages::COPY_DST | BufferUsages::INDEX | BufferUsages::STORAGE,
            mapped_at_creation: false,
        }));

        let allocator = RangeAllocator::new(0..STARTING_MESH_DATA);

        let data = Mutex::new(Vec::new());

        let encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("mesh manager init encoder") });

        Self {
            buffer_state: Mutex::new(BufferState { buffer, allocator, encoder, wait_group: WaitGroup::new() }),
            data,
        }
    }

    pub fn add(&self, device: &Device, mesh: Mesh) -> Result<InternalMesh, MeshCreationError> {
        profiling::scope!("MeshManager::add");

        let vertex_count = mesh.vertex_count;
        let index_count = mesh.indices.len();

        if vertex_count == 0 || index_count == 0 {
            return Ok(InternalMesh::new_empty());
        }

        // This value is used later when setting joints, to make sure all indices are
        // in-bounds with the specified amount of joints.
        let mut required_joint_count = None;
        let joint_indices_attribute =
            mesh.attributes.iter().find_map(|attribute| attribute.typed_data(&VERTEX_ATTRIBUTE_JOINT_INDICES));
        if let Some(joint_indices) = joint_indices_attribute {
            required_joint_count = Some(joint_indices.iter().flatten().max().map_or(0, |v| v + 1));
        }

        let mut vertex_attribute_ranges = Vec::with_capacity(mesh.attributes.len());
        let mut upload = UploadChainer::new();

        // Need to deref to allow split borrows
        let mut buffer_state_guard = self.buffer_state.lock();
        let buffer_state = &mut *buffer_state_guard;

        for attribute in &mesh.attributes {
            let range = self.allocate_range_impl(device, buffer_state, attribute.bytes())?;
            upload.add(range.start, attribute.untyped_data());
            vertex_attribute_ranges.push((*attribute.id(), range));
        }

        let index_range = self.allocate_range_impl(device, buffer_state, index_count as u64 * 4)?;
        upload.add(index_range.start, bytemuck::cast_slice(&mesh.indices));
        upload.create_staging_buffer(device).map_err(|e| MeshCreationError::BufferWriteFailed { inner: e })?;
        upload.encode_upload(&mut buffer_state.encoder, &buffer_state.buffer);

        let staging_guard = buffer_state.wait_group.increment();
        drop(buffer_state_guard);

        // We intentionally write to the internal staging buffer _after_ we drop
        // the mutex, as we merely need to complete this before the next submission.
        upload.stage();
        drop(staging_guard);

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

    pub fn remove(&self, object_id: &RawMeshHandle) {
        let mesh = self.data.lock()[object_id.idx].take().unwrap();

        let mut buffer_state = self.buffer_state.lock();
        for (_id, range) in mesh.vertex_attribute_ranges {
            if range.is_empty() {
                continue;
            }
            buffer_state.allocator.free_range(range);
        }
        if mesh.index_range.is_empty() {
            return;
        }
        buffer_state.allocator.free_range(mesh.index_range);
    }

    pub fn evaluate(&self, device: &Device) -> (Arc<Buffer>, CommandBuffer) {
        let new_encoder =
            device.create_command_encoder(&CommandEncoderDescriptor { label: Some("mesh manager init encoder") });

        let mut buffer_state = self.buffer_state.lock();
        let buffer = buffer_state.buffer.clone();
        let cmd_enc = mem::replace(&mut buffer_state.encoder, new_encoder);
        let wait_group = mem::replace(&mut buffer_state.wait_group, WaitGroup::new());
        drop(buffer_state);

        wait_group.wait();
        (buffer, cmd_enc.finish())
    }

    /// Duplicates a mesh's vertex data so that it can be skinned on the GPU.
    pub fn allocate_range(&self, device: &Device, bytes: u64) -> Result<Range<u64>, MeshCreationError> {
        self.allocate_range_impl(device, &mut self.buffer_state.lock(), bytes)
    }

    fn allocate_range_impl(
        &self,
        device: &Device,
        buffer_state: &mut BufferState,
        bytes: u64,
    ) -> Result<Range<u64>, MeshCreationError> {
        Ok(match buffer_state.allocator.allocate_range(bytes) {
            Ok(range) => range,
            Err(..) => {
                self.reallocate_buffers(device, buffer_state, bytes)?;
                buffer_state.allocator.allocate_range(bytes).expect(
                    "Second allocation range should always succeed, as there should always be enough space in the tail of the buffer",
                )
            }
        })
    }

    pub fn free_range(&self, range: Range<u64>) {
        Self::free_range_impl(&mut self.buffer_state.lock(), range);
    }

    fn free_range_impl(buffer_state: &mut BufferState, range: Range<u64>) {
        if range.is_empty() {
            return;
        }
        buffer_state.allocator.free_range(range);
    }

    pub fn lock_internal_data(&self) -> LockedInternalMeshDataArray<'_> {
        LockedInternalMeshDataArray(self.data.lock())
    }

    fn reallocate_buffers(
        &self,
        device: &Device,
        buffer_state: &mut BufferState,
        needed_bytes: u64,
    ) -> Result<(), MeshCreationError> {
        profiling::scope!("reallocate mesh buffers");

        let current_bytes = buffer_state.allocator.initial_range().end;
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
        scope.end().map_err(|e| MeshCreationError::BufferAllocationFailed { size: new_bytes, inner: e })?;

        buffer_state.encoder.copy_buffer_to_buffer(
            &buffer_state.buffer,
            0,
            &new_buffer,
            0,
            buffer_state.allocator.initial_range().end,
        );

        buffer_state.buffer = new_buffer;
        buffer_state.allocator.grow_to(new_bytes);

        Ok(())
    }
}

pub struct LockedInternalMeshDataArray<'a>(MutexGuard<'a, Vec<Option<InternalMesh>>>);

impl<'a> Index<RawMeshHandle> for LockedInternalMeshDataArray<'a> {
    type Output = InternalMesh;

    fn index(&self, handle: RawMeshHandle) -> &Self::Output {
        self.0[handle.idx].as_ref().expect("Entry in LockedInternalMeshDataArray should not be None")
    }
}
