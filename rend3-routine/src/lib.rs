/// PBR Render Routine for rend3.
/// Contains [`PbrMaterial`] and the [`PbrRenderRoutine`] which serve as the default render routines.
///
/// Tries to strike a balance between photorealism and performance.
pub mod base;
pub mod common;
pub mod culling;
pub mod depth;
pub mod pbr {
    mod material;
    mod routine;

    pub use material::*;
    pub use routine::*;
}
pub mod pre_cull;
pub mod shaders;
pub mod skybox;
pub mod tonemapping;
pub mod uniforms;
pub mod vertex;
