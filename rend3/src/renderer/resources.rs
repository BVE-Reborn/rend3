use crate::{
    datatypes::{Camera, TextureHandle},
    resources::CameraManager,
    util::output::SWAPCHAIN_FORMAT,
    RendererMode, RendererOptions, VSyncMode,
};
use wgpu::{Device, PresentMode, Surface, SwapChain, SwapChainDescriptor, TextureUsage};

pub struct RendererGlobalResources {
    pub swapchain: Option<SwapChain>,

    pub camera: CameraManager,
    pub background_texture: Option<TextureHandle>,
}
impl RendererGlobalResources {
    pub fn new(device: &Device, surface: Option<&Surface>, mode: RendererMode, options: &RendererOptions) -> Self {
        let swapchain = surface.map(|surface| create_swapchain(device, surface, options.size, options.vsync));

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
                surface.map(|surface| create_swapchain(device, surface, new_options.size, new_options.vsync));
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

fn create_swapchain(device: &Device, surface: &Surface, size: [u32; 2], vsync: VSyncMode) -> SwapChain {
    device.create_swap_chain(
        &surface,
        &SwapChainDescriptor {
            width: size[0],
            height: size[1],
            usage: TextureUsage::RENDER_ATTACHMENT,
            format: SWAPCHAIN_FORMAT,
            present_mode: match vsync {
                VSyncMode::On => PresentMode::Fifo,
                VSyncMode::Off => PresentMode::Immediate,
            },
        },
    )
}
