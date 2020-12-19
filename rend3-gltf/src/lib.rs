pub use rend3::datatypes as dt;

struct LoadedGltfScene {
    meshes: Vec<dt::MeshHandle>,
    materials: Vec<dt::MaterialHandle>,
    textures: Vec<dt::TextureHandle>,
    objects: Vec<dt::ObjectHandle>,
}
