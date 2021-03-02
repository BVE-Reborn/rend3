use glam::Vec4;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VSyncMode {
    On,
    Off,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RendererOptions {
    pub vsync: VSyncMode,
    pub size: [u32; 2],
    /// A temporary minimum linear color value used to compensate for the lack of IBL currently. The result of lighting is combined like so `max(lighting, ambient * albedo)`. Set to zero to ignore ambient.
    pub ambient: Vec4,
}
impl RendererOptions {
    pub fn aspect_ratio(&self) -> f32 {
        self.size[0] as f32 / self.size[1] as f32
    }
}
