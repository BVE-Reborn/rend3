use crate::datatypes::{
    AffineTransform, MaterialHandle, MeshHandle, ModelVertex, ObjectHandle, TextureFormat, TextureHandle,
};
use smallvec::SmallVec;

pub enum SceneChangeInstruction {
    AddMesh {
        vertices: Vec<ModelVertex>,
        indices: Vec<u32>,
        material_count: u32,
        // TODO: Bones/joints/animation
    },
    RemoveMesh {
        object: ObjectHandle,
    },
    AddTexture {
        data: Vec<u8>,
        format: TextureFormat,
        width: u32,
        height: u32,
    },
    RemoveTexture {
        texture: TextureHandle,
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
        color: Option<TextureHandle>,
        normal: Option<TextureHandle>,
        roughness: Option<TextureHandle>,
        specular: Option<TextureHandle>,
    },
    RemoveMaterial {
        material: MaterialHandle,
    },
    AddObject {
        mesh: MeshHandle,
        materials: SmallVec<[MaterialHandle; 4]>,
        transform: AffineTransform,
    },
    SetObjectTransform {
        object: ObjectHandle,
        transform: AffineTransform,
    },
    RemoveObject {
        object: ObjectHandle,
    },
}

pub enum GeneralInstruction {}
