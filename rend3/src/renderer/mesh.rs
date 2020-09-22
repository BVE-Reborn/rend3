use crate::{
    datatypes::{Mesh, MeshHandle, ModelVertex},
    registry::ResourceRegistry,
};
use range_alloc::RangeAllocator;
use std::{mem::size_of, ops::Range};
use wgpu::{Buffer, BufferAddress, BufferDescriptor, BufferUsage, Device, Queue};

const VERTEX_SIZE: usize = size_of::<ModelVertex>();
const INDEX_SIZE: usize = size_of::<u32>();

const STARTING_VERTICES: usize = 1 << 16;
const STARTING_INDICES: usize = 1 << 18;

pub struct InternalMesh {
    pub vertex_range: Range<usize>,
    pub index_range: Range<usize>,
    pub material_count: u32,
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
        let vertex_bytes = STARTING_VERTICES * VERTEX_SIZE;
        let index_bytes = STARTING_INDICES * INDEX_SIZE;

        let vertex_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("vertex buffer"),
            size: vertex_bytes as BufferAddress,
            usage: BufferUsage::COPY_SRC | BufferUsage::COPY_DST | BufferUsage::VERTEX,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("index buffer"),
            size: index_bytes as BufferAddress,
            usage: BufferUsage::COPY_SRC | BufferUsage::COPY_DST | BufferUsage::INDEX,
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

    pub fn fill(&mut self, queue: &Queue, handle: MeshHandle, mesh: Mesh) {
        let vertex_range = self
            .vertex_alloc
            .allocate_range(mesh.vertices.len())
            .unwrap_or_else(|_| todo!("Deal with resizing buffers"));
        let index_range = self
            .index_alloc
            .allocate_range(mesh.indices.len())
            .unwrap_or_else(|_| todo!("Deal with resizing buffers"));

        let vertex_base = vertex_range.start;

        queue.write_buffer(
            &self.vertex_buffer,
            (vertex_base * VERTEX_SIZE) as BufferAddress,
            bytemuck::cast_slice(&mesh.vertices),
        );
        queue.write_buffer(
            &self.index_buffer,
            (index_range.start * INDEX_SIZE) as BufferAddress,
            bytemuck::cast_slice(&mesh.indices),
        );

        let mesh = InternalMesh {
            vertex_range,
            index_range,
            material_count: mesh.material_count,
        };

        self.registry.insert(handle.0, mesh);
    }

    pub fn remove(&mut self, handle: MeshHandle) {
        let mesh = self.registry.remove(handle.0).1;

        self.vertex_alloc.free_range(mesh.vertex_range);
        self.index_alloc.free_range(mesh.index_range);
    }

    pub fn internal_data(&self, handle: MeshHandle) -> &InternalMesh {
        self.registry.get(handle.0)
    }
}
