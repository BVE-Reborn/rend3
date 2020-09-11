use crate::datatypes::{MaterialHandle, MeshHandle, ModelVertex, TextureFormat, TextureHandle};
use smallvec::SmallVec;

pub enum SceneChangeInstruction {
    AddMesh {
        vertices: Vec<ModelVertex>,
        indices: Vec<u32>,
        material_count: u32,
        // TODO: Bones/joints/animation
    },
    AddTexture {
        data: Vec<u8>,
        format: TextureFormat,
        width: u32,
        height: u32,
    },
    AddMaterial {
        // Consider:
        //
        // - albedo and opacity
        // - normal
        // - roughness
        // - specular color
        // - thiccness for leaves
        // - porosity, so I can do things like make things wet when it rains
        // - Maybe subsurface scattering radii? Or some kind of transmission value
        // - Index of Refraction for transparent things
        color: TextureHandle,
        normal: TextureHandle,
        //
    },
    AddObject {
        mesh: MeshHandle,
        materials: SmallVec<[MaterialHandle; 4]>,
    },
}

pub enum GeneralInstruction {}
