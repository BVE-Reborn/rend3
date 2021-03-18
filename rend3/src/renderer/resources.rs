use crate::{
    bind_merge::BindGroupBuilder,
    datatypes::{Camera, TextureHandle},
    modules::CameraManager,
    RendererMode, RendererOptions,
};
use wgpu::{BindGroupLayout, BindingResource, Device, Sampler, Surface, SwapChain};

pub struct RendererGlobalResources {
    pub swapchain: Option<SwapChain>,

    pub camera: CameraManager,
    pub background_texture: Option<TextureHandle>,
}
impl RendererGlobalResources {
    pub fn new(device: &Device, surface: Option<&Surface>, mode: RendererMode, options: &RendererOptions) -> Self {
        let swapchain = surface.map(|surface| util::create_swapchain(device, surface, options.size, options.vsync));

        let camera = CameraManager::new(Camera::default(), Some(options.aspect_ratio()));

        Self {
            swapchain,
            camera,
            background_texture: None,
        }
    }

    pub fn update(
        &mut self,
        device: &Device,
        surface: Option<&Surface>,
        old_options: &mut RendererOptions,
        new_options: RendererOptions,
    ) {
        let dirty = determine_dirty(old_options, &new_options);

        if dirty.contains(DirtyResources::SWAPCHAIN) {
            self.swapchain =
                surface.map(|surface| util::create_swapchain(device, surface, new_options.size, new_options.vsync));
        }
        if dirty.contains(DirtyResources::CAMERA) {
            self.camera.set_aspect_ratio(Some(new_options.aspect_ratio()));
        }

        *old_options = new_options
    }
}

bitflags::bitflags! {
    struct DirtyResources: u8 {
        const SWAPCHAIN = 0x01;
        const CAMERA = 0x02;
    }
}

fn determine_dirty(current: &RendererOptions, new: &RendererOptions) -> DirtyResources {
    let mut dirty = DirtyResources::empty();

    if current.size != new.size {
        dirty |= DirtyResources::SWAPCHAIN;
        dirty |= DirtyResources::CAMERA;
    }

    if current.vsync != new.vsync {
        dirty |= DirtyResources::SWAPCHAIN;
    }

    dirty
}
