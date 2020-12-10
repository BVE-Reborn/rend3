use crate::{
    datatypes::{Mesh, MeshHandle, ModelVertex},
    registry::ResourceRegistry,
    renderer::{copy::GpuCopy, frustum::BoundingSphere},
};
use range_alloc::RangeAllocator;
use std::{mem::size_of, ops::Range};
use wgpu::{Buffer, BufferAddress, BufferDescriptor, BufferUsage, CommandEncoder, Device, Queue};

const VERTEX_SIZE: usize = size_of::<ModelVertex>();
const INDEX_SIZE: usize = size_of::<u32>();

const STARTING_VERTICES: usize = 1 << 16;
const STARTING_INDICES: usize = 1 << 16;

pub struct InternalMesh {
    pub vertex_range: Range<usize>,
    pub index_range: Range<usize>,
    pub bounding_sphere: BoundingSphere,
}

pub struct MeshManager {
    vertex_buffer: Buffer,
    vertex_count: usize,
    vertex_alloc: RangeAllocator<usize>,

    index_buffer: Buffer,
    index_count: usize,
    index_alloc: RangeAllocator<usize>,

    registry: ResourceRegistry<InternalMesh>,
}

impl MeshManager {
    pub fn new(device: &Device) -> Self {
        span_transfer!(_ -> new_span, INFO, "Creating Mesh Manager");

        let vertex_bytes = STARTING_VERTICES * VERTEX_SIZE;
        let index_bytes = STARTING_INDICES * INDEX_SIZE;

        let vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("vertex buffer"),
            size: vertex_bytes as BufferAddress,
            usage: BufferUsage::COPY_DST | BufferUsage::VERTEX | BufferUsage::STORAGE,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("index buffer"),
            size: index_bytes as BufferAddress,
            usage: BufferUsage::COPY_DST | BufferUsage::INDEX | BufferUsage::STORAGE,
            mapped_at_creation: false,
        });

        let vertex_count = STARTING_VERTICES;
        let index_count = STARTING_INDICES;

        let vertex_alloc = RangeAllocator::new(0..vertex_count);
        let index_alloc = RangeAllocator::new(0..index_count);

        let registry = ResourceRegistry::new();

        Self {
            vertex_buffer,
            vertex_count,
            vertex_alloc,
            index_buffer,
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

        let vertex_count = mesh.vertices.len();
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
            &self.vertex_buffer,
            (vertex_range.start * VERTEX_SIZE) as BufferAddress,
            bytemuck::cast_slice(&mesh.vertices),
        );
        queue.write_buffer(
            &self.index_buffer,
            (index_range.start * INDEX_SIZE) as BufferAddress,
            bytemuck::cast_slice(&mesh.indices),
        );

        let bounding_sphere = BoundingSphere::from_mesh(&mesh.vertices);

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

    pub fn buffers(&self) -> (&Buffer, &Buffer) {
        (&self.vertex_buffer, &self.index_buffer)
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

        let new_vert_bytes = new_vert_count * VERTEX_SIZE;
        let new_index_bytes = new_index_count * INDEX_SIZE;

        let new_vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("vertex buffer"),
            size: new_vert_bytes as BufferAddress,
            usage: BufferUsage::COPY_DST | BufferUsage::VERTEX | BufferUsage::STORAGE,
            mapped_at_creation: false,
        });

        let new_index_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("index buffer"),
            size: new_index_bytes as BufferAddress,
            usage: BufferUsage::COPY_DST | BufferUsage::INDEX | BufferUsage::STORAGE,
            mapped_at_creation: false,
        });

        let mut new_vert_alloc = RangeAllocator::new(0..new_vert_count);
        let mut new_index_alloc = RangeAllocator::new(0..new_index_count);

        let vert_copy_data = gpu_copy.prepare(
            device,
            self.vertex_buffer.slice(..),
            new_vertex_buffer.slice(..),
            "vertex copy",
        );

        let index_copy_data = gpu_copy.prepare(
            device,
            self.index_buffer.slice(..),
            new_index_buffer.slice(..),
            "index copy",
        );

        let mut cpass = encoder.begin_compute_pass();

        for mesh in self.registry.values_mut() {
            let new_vert_range = new_vert_alloc.allocate_range(mesh.vertex_range.len()).unwrap();
            let new_index_range = dbg!(new_index_alloc.allocate_range(mesh.index_range.len()).unwrap());

            let vert_difference = dbg!(new_vert_range.start as isize - mesh.vertex_range.start as isize);

            // Copy verts over to new buffer
            let vert_copy_start = (mesh.vertex_range.start * VERTEX_SIZE) / 4;
            let vert_copy_end = (mesh.vertex_range.end * VERTEX_SIZE) / 4;
            let vert_output = (new_vert_range.start * VERTEX_SIZE) / 4;
            gpu_copy.copy_words(
                &mut cpass,
                &vert_copy_data,
                vert_copy_start as u32..vert_copy_end as u32,
                vert_output as u32,
            );

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

        self.vertex_buffer = new_vertex_buffer;
        self.index_buffer = new_index_buffer;
        self.vertex_count = new_vert_count;
        self.index_count = new_index_count;
        self.vertex_alloc = new_vert_alloc;
        self.index_alloc = new_index_alloc;
    }
}
