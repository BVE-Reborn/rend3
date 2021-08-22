use glam::UVec2;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VSyncMode {
    On,
    Off,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InternalSurfaceOptions {
    pub vsync: VSyncMode,
    pub size: UVec2,
}
impl InternalSurfaceOptions {
    pub fn aspect_ratio(&self) -> f32 {
        self.size.x as f32 / self.size.y as f32
    }
}
