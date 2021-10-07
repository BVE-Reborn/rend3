use std::convert::TryFrom;

use glam::UVec2;
use wgpu::{
    Device, Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages, TextureView,
    TextureViewDescriptor,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RenderTextureOptions {
    pub resolution: UVec2,
    pub samples: SampleCount,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum SampleCount {
    One = 1,
    Four = 4,
}

impl Default for SampleCount {
    fn default() -> Self {
        Self::One
    }
}

impl TryFrom<u8> for SampleCount {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            1 => Self::One,
            4 => Self::Four,
            v => return Err(v),
        })
    }
}

pub struct RenderTextures {
    pub color: TextureView,
    pub resolve: Option<TextureView>,
    pub depth: TextureView,
    pub samples: SampleCount,
}
impl RenderTextures {
    pub fn new(device: &Device, options: RenderTextureOptions) -> Self {
        profiling::scope!("RenderTextures::new");
        Self {
            color: create_internal_color_buffer(
                device,
                TextureFormat::Rgba16Float,
                options.resolution,
                options.samples,
            ),
            resolve: if options.samples != SampleCount::One {
                Some(create_internal_color_buffer(
                    device,
                    TextureFormat::Rgba16Float,
                    options.resolution,
                    SampleCount::One,
                ))
            } else {
                None
            },
            depth: create_internal_depth_buffer(device, options),
            samples: options.samples,
        }
    }

    pub fn blit_source_view(&self) -> &TextureView {
        self.resolve.as_ref().unwrap_or(&self.color)
    }
}

fn create_internal_color_buffer(
    device: &Device,
    format: TextureFormat,
    resolution: UVec2,
    samples: SampleCount,
) -> TextureView {
    device
        .create_texture(&TextureDescriptor {
            label: Some("internal renderbuffer"),
            size: Extent3d {
                width: resolution.x,
                height: resolution.y,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: samples as u32,
            dimension: TextureDimension::D2,
            format,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC,
        })
        .create_view(&TextureViewDescriptor::default())
}

fn create_internal_depth_buffer(device: &Device, options: RenderTextureOptions) -> TextureView {
    device
        .create_texture(&TextureDescriptor {
            label: Some("internal depth renderbuffer"),
            size: Extent3d {
                width: options.resolution.x,
                height: options.resolution.y,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: options.samples as u32,
            dimension: TextureDimension::D2,
            format: TextureFormat::Depth32Float,
            usage: TextureUsages::RENDER_ATTACHMENT,
        })
        .create_view(&TextureViewDescriptor::default())
}
