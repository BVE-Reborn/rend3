use crate::{renderer::util, RendererOptions};
use wgpu::{BindGroupLayout, Device, Surface, SwapChain};

pub struct RendererGlobalResources {
    pub swapchain: SwapChain,
    pub object_input_bgl: BindGroupLayout,
    pub object_output_bgl: BindGroupLayout,
    pub uniform_bgl: BindGroupLayout,
}
impl RendererGlobalResources {
    pub fn new(device: &Device, surface: &Surface, options: &RendererOptions) -> Self {
        let swapchain = util::create_swapchain(device, surface, options.size, options.vsync);

        let object_input_bgl = util::create_object_input_bgl(device);
        let object_output_bgl = util::create_object_output_bgl(device);
        let uniform_bgl = util::create_uniform_bgl(device);

        Self {
            swapchain,
            object_input_bgl,
            object_output_bgl,
            uniform_bgl,
        }
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
