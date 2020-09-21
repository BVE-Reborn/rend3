use crate::{renderer::util, RendererOptions};
use wgpu::{Device, Surface, SwapChain};

pub struct RendererGlobalResources {
    pub swapchain: SwapChain,
}
impl RendererGlobalResources {
    pub fn new(device: &Device, surface: &Surface, options: &RendererOptions) -> Self {
        let swapchain = util::create_swapchain(device, surface, options.size, options.vsync);

        Self { swapchain }
    }

    pub fn update(
        &mut self,
        device: &Device,
        surface: &Surface,
        old_options: &RendererOptions,
        new_options: &RendererOptions,
    ) {
        let dirty = determine_dirty(old_options, new_options);

        if dirty.contains(DirtyResources::SWAPCHAIN) {
            self.swapchain = util::create_swapchain(device, surface, new_options.size, new_options.vsync);
        }
    }
}

bitflags::bitflags! {
    struct DirtyResources: u8 {
        const SWAPCHAIN = 0x01;
    }
}

fn determine_dirty(current: &RendererOptions, new: &RendererOptions) -> DirtyResources {
    let mut dirty = DirtyResources::empty();

    if current.size != new.size {
        dirty |= DirtyResources::SWAPCHAIN;
    }

    if current.vsync != current.vsync {
        dirty |= DirtyResources::SWAPCHAIN;
    }

    dirty
}
