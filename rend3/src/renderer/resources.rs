use crate::{
    renderer::{camera::Camera, util},
    RendererOptions,
};
use wgpu::{BindGroup, BindGroupLayout, Device, Sampler, Surface, SwapChain, Texture, TextureView};

pub struct RendererGlobalResources {
    pub swapchain: SwapChain,

    pub color_texture: Texture,
    pub color_texture_view: TextureView,
    pub depth_texture: Texture,
    pub depth_texture_view: TextureView,
    pub color_bg: BindGroup,

    pub camera: Camera,

    pub blit_bgl: BindGroupLayout,
    pub object_input_bgl: BindGroupLayout,
    pub object_output_bgl: BindGroupLayout,
    pub object_output_noindirect_bgl: BindGroupLayout,
    pub material_bgl: BindGroupLayout,
    pub uniform_bgl: BindGroupLayout,

    pub sampler: Sampler,
}
impl RendererGlobalResources {
    pub fn new(device: &Device, surface: &Surface, options: &RendererOptions) -> Self {
        let swapchain = util::create_swapchain(device, surface, options.size, options.vsync);

        let (color_texture, color_texture_view) =
            util::create_framebuffer_texture(device, options.size, util::FramebufferTextureKind::Color);
        let (depth_texture, depth_texture_view) =
            util::create_framebuffer_texture(device, options.size, util::FramebufferTextureKind::Depth);

        let camera = Camera::new(options.size.width as f32 / options.size.height as f32);

        let blit_bgl = util::create_blit_bgl(device);
        let object_input_bgl = util::create_object_input_bgl(device);
        let object_output_bgl = util::create_object_output_bgl(device);
        let object_output_noindirect_bgl = util::create_object_output_noindirect_bgl(device);
        let material_bgl = util::create_material_bgl(device);
        let uniform_bgl = util::create_uniform_bgl(device);

        let sampler = util::create_sampler(device);

        let color_bg = util::create_blit_bg(device, &blit_bgl, &color_texture_view, &sampler);

        Self {
            swapchain,
            color_texture,
            color_texture_view,
            depth_texture,
            depth_texture_view,
            color_bg,
            camera,
            blit_bgl,
            object_input_bgl,
            object_output_bgl,
            object_output_noindirect_bgl,
            material_bgl,
            uniform_bgl,
            sampler,
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
        if dirty.contains(DirtyResources::FRAMEBUFFER) {
            let (color_texture, color_texture_view) =
                util::create_framebuffer_texture(device, new_options.size, util::FramebufferTextureKind::Color);
            let (depth_texture, depth_texture_view) =
                util::create_framebuffer_texture(device, new_options.size, util::FramebufferTextureKind::Depth);

            self.color_texture = color_texture;
            self.color_texture_view = color_texture_view;
            self.depth_texture = depth_texture;
            self.depth_texture_view = depth_texture_view;
        }
    }
}

bitflags::bitflags! {
    struct DirtyResources: u8 {
        const SWAPCHAIN = 0x01;
        const CAMERA = 0x02;
        const FRAMEBUFFER = 0x04;
    }
}

fn determine_dirty(current: &RendererOptions, new: &RendererOptions) -> DirtyResources {
    let mut dirty = DirtyResources::empty();

    if current.size != new.size {
        dirty |= DirtyResources::SWAPCHAIN;
        dirty |= DirtyResources::CAMERA;
        dirty |= DirtyResources::FRAMEBUFFER;
    }

    if current.vsync != current.vsync {
        dirty |= DirtyResources::SWAPCHAIN;
    }

    dirty
}
