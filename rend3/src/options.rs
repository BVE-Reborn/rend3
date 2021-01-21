#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VSyncMode {
    On,
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendererOptions {
    pub vsync: VSyncMode,
    pub size: [u32; 2],
}
impl RendererOptions {
    pub fn aspect_ratio(&self) -> f32 {
        self.size[0] as f32 / self.size[1] as f32
    }
}
