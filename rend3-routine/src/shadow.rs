use rend3::types::Material;

/// Trait for all materials that can use the built-in shadow/prepass rendering.
pub trait DepthRenderableMaterial: Material {
    /// If Some render with the given alpha cutout, if None, render without alpha cutout.
    const ALPHA_CUTOUT: Option<AlphaCutoutSpec>;
}

/// How the material should be read for alpha cutouting.
pub struct AlphaCutoutSpec {
    /// Index into the texture array to read the alpha from. Currently _must_ be 0. This will be lifted.
    pub index: usize,
    /// Alpha cutoff. If the alpha value is less, the pixel will be discarded.
    pub cutoff: f32,
}
