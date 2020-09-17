use crate::datatypes::{AffineTransform, MaterialHandle, Object, ObjectHandle};
use crate::registry::ResourceRegistry;
use crate::renderer::mesh::MeshManager;
use smallvec::SmallVec;

#[derive(Debug, Clone)]
struct InternalObject {
    materials: SmallVec<[MaterialHandle; 4]>,
    transform: AffineTransform,
    start_idx: u32,
    count: u32,
    vertex_offset: u32,
}

#[derive(Debug, Copy, Clone)]
#[repr(C, align(16))]
struct ShaderObject {
    start_idx: u32,
    count: u32,
    vertex_offset: u32,
    material_translation_idx: u32,
}

unsafe impl bytemuck::Zeroable for ShaderObject {}
unsafe impl bytemuck::Pod for ShaderObject {}

pub struct ObjectManager {
    registry: ResourceRegistry<InternalObject>,
}
impl ObjectManager {
    pub fn new() -> Self {
        let registry = ResourceRegistry::new();

        Self { registry }
    }

    pub fn allocate(&self) -> ObjectHandle {
        ObjectHandle(self.registry.allocate())
    }

    pub fn fill(&mut self, handle: ObjectHandle, object: Object, mesh_manager: &MeshManager) {
        let mesh = mesh_manager.internal_data(object.mesh);

        assert_eq!(
            object.materials.len(),
            mesh.material_count as usize,
            "Mismatching material and mesh material count. Material: {}, Mesh: {}",
            object.materials.len(),
            mesh.material_count
        );

        let shader_object = InternalObject {
            materials: object.materials,
            transform: object.transform,
            start_idx: mesh.index_range.start as u32,
            count: (mesh.index_range.end - mesh.index_range.start) as u32,
            vertex_offset: mesh.vertex_range.start as u32,
        };

        self.registry.insert(handle.0, shader_object);
    }
}
