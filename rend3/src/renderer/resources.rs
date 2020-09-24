use crate::{
    renderer::{camera::Camera, uniforms::UniformManager, util},
    RendererOptions,
};
use wgpu::{BindGroupLayout, Device, Surface, SwapChain};

pub struct RendererGlobalResources {
    pub swapchain: SwapChain,

    pub camera: Camera,
    pub uniforms: UniformManager,

    pub object_input_bgl: BindGroupLayout,
    pub object_output_bgl: BindGroupLayout,
    pub uniform_bgl: BindGroupLayout,
}
impl RendererGlobalResources {
    pub fn new(device: &Device, surface: &Surface, options: &RendererOptions) -> Self {
        let swapchain = util::create_swapchain(device, surface, options.size, options.vsync);

        let camera = Camera::new(options.size.width as f32 / options.size.height as f32);

        let object_input_bgl = util::create_object_input_bgl(device);
        let object_output_bgl = util::create_object_output_bgl(device);
        let uniform_bgl = util::create_uniform_bgl(device);

        let uniforms = UniformManager::new(device, &uniform_bgl);

        Self {
            swapchain,
            camera,
            uniforms,
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
        if dirty.contains(DirtyResources::CAMERA) {
            self.camera
                .set_aspect_ratio(new_options.size.width as f32 / new_options.size.height as f32);
        }
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

    if current.vsync != current.vsync {
        dirty |= DirtyResources::SWAPCHAIN;
    }

    dirty
}
