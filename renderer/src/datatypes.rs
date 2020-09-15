use glam::{Quat, Vec2, Vec3, Vec3A};

macro_rules! declare_handle {
    ($($name:ident),*) => {$(
        #[derive(Debug, Copy, Clone, PartialEq, Eq)]
        pub struct $name(pub(crate) usize);

        impl $name {
            pub fn get(&self) -> usize {
                self.0
            }
        }
    )*};
}

declare_handle!(MeshHandle, TextureHandle, MaterialHandle, ObjectHandle);

// Consider:
//
// Bone weights!!!
// Lightmap UVs
// Spherical harmonics
// Baked light color
// A lot of renderers put the tangent vector in the vertex data, but you can calculate it in the pixel shader ezpz
// Maybe thiccness data for tree branches
// I'd consider putting everything you can into the vertex data structure. Vertex data is just per-vertex data, and a lot of things can be per-vertex
// Then you don't need a million 4K textures
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ModelVertex {
    position: Vec3, // 00..12
    normal: Vec3,   // 12..24
    uv: Vec2,       // 24..32
    color: [u8; 4], // 32..36
    material: u32,  // 36..40
}

unsafe impl bytemuck::Zeroable for ModelVertex {}
unsafe impl bytemuck::Pod for ModelVertex {}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AffineTransform {
    transform: Vec3A,
    rotation: Quat,
    scale: Vec3A,
}

unsafe impl bytemuck::Zeroable for AffineTransform {}
unsafe impl bytemuck::Pod for AffineTransform {}

pub enum RendererTextureFormat {
    Rgba8Srgb,
    Rgba8Linear,
}
