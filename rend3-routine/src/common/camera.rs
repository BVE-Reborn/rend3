/// Specifier representing which camera we're referring to.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum CameraSpecifier {
    Viewport,
    Shadow(u32),
}

impl CameraSpecifier {
    /// Returns `true` if the camera specifier is [`Viewport`].
    ///
    /// [`Viewport`]: CameraIndex::Viewport
    #[must_use]
    pub fn is_viewport(&self) -> bool {
        matches!(self, Self::Viewport)
    }

    /// Returns `true` if the camera specifier is [`Shadow`].
    ///
    /// [`Shadow`]: CameraIndex::Shadow
    #[must_use]
    pub fn is_shadow(&self) -> bool {
        matches!(self, Self::Shadow(..))
    }

    /// Returns a shader compatible index for the camera, using u32::MAX for the viewport camera.
    #[must_use]
    pub fn to_shader_index(&self) -> u32 {
        match *self {
            Self::Viewport => u32::MAX,
            Self::Shadow(index) => {
                assert_ne!(index, u32::MAX, "Shadow camera index cannot be 0xFFFF_FFFF");
                index
            }
        }
    }
}
