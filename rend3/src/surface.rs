use crate::types::{PresentMode, Surface};
use glam::UVec2;
use rend3_types::{TextureFormat, TextureUsages};
use wgpu::{Device, SurfaceConfiguration};

pub fn configure_surface(
    surface: &Surface,
    device: &Device,
    format: TextureFormat,
    size: UVec2,
    present_mode: PresentMode,
) {
    surface.configure(
        device,
        &SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.x,
            height: size.y,
            present_mode,
        },
    )
}
