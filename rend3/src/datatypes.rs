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
    pub position: Vec3, // 00..12
    pub normal: Vec3,   // 12..24
    pub uv: Vec2,       // 24..32
    pub color: [u8; 4], // 32..36
    pub material: u32,  // 36..40
}

unsafe impl bytemuck::Zeroable for ModelVertex {}
unsafe impl bytemuck::Pod for ModelVertex {}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct AffineTransform {
    pub transform: Vec3A,
    pub rotation: Quat,
    pub scale: Vec3A,
}

unsafe impl bytemuck::Zeroable for AffineTransform {}
unsafe impl bytemuck::Pod for AffineTransform {}

pub enum RendererTextureFormat {
    Rgba8Srgb,
    Rgba8Linear,
}

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
pub struct Material {
    pub color: Option<TextureHandle>,
    pub normal: Option<TextureHandle>,
    pub roughness: Option<TextureHandle>,
    pub specular: Option<TextureHandle>,
}
