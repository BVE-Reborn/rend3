use crate::{
    bind_merge::BindGroupBuilder,
    datatypes::TextureHandle,
    renderer::{camera::Camera, util, util::SamplerType, RendererMode},
    RendererOptions,
};
use wgpu::{BindGroupLayout, BindingResource, Device, Sampler, Surface, SwapChain};

pub struct RendererGlobalResources {
    pub swapchain: SwapChain,

    pub camera: Camera,
    pub background_texture: Option<TextureHandle>,

    pub prefix_sum_bgl: BindGroupLayout,
    pub object_input_bgl: BindGroupLayout,
    pub object_output_bgl: BindGroupLayout,
    pub pre_cull_bgl: BindGroupLayout,

    pub general_bgl: BindGroupLayout,
    pub object_data_bgl: BindGroupLayout,
    pub material_bgl: BindGroupLayout,
    pub camera_data_bgl: BindGroupLayout,
    pub shadow_texture_bgl: BindGroupLayout,
    pub skybox_bgl: BindGroupLayout,

    pub linear_sampler: Sampler,
    pub shadow_sampler: Sampler,
}
impl RendererGlobalResources {
    pub fn new(device: &Device, surface: &Surface, mode: RendererMode, options: &RendererOptions) -> Self {
        let swapchain = util::create_swapchain(device, surface, options.size, options.vsync);

        let camera = Camera::new_projection(options.size[0] as f32 / options.size[1] as f32);

        let prefix_sum_bgl = util::create_prefix_sum_bgl(device);
        let pre_cull_bgl = util::create_pre_cull_bgl(device);
        let object_input_bgl = util::create_object_input_bgl(device);
        let object_output_bgl = util::create_object_output_bgl(device);

        let general_bgl = util::create_general_bind_group_layout(device);
        let object_data_bgl = util::create_object_data_bgl(device);
        let material_bgl = util::create_material_bgl(device, mode);
        let camera_data_bgl = util::create_camera_data_bgl(device);
        let shadow_texture_bgl = util::create_shadow_texture_bgl(device);
        let skybox_bgl = util::create_skybox_bgl(device);

        let linear_sampler = util::create_sampler(device, SamplerType::Linear);
        let shadow_sampler = util::create_sampler(device, SamplerType::Shadow);

        Self {
            swapchain,
            camera,
            background_texture: None,
            prefix_sum_bgl,
            pre_cull_bgl,
            general_bgl,
            object_input_bgl,
            object_output_bgl,
            object_data_bgl,
            material_bgl,
            camera_data_bgl,
            shadow_texture_bgl,
            skybox_bgl,
            linear_sampler,
            shadow_sampler,
        }
    }

    pub fn update(
        &mut self,
        device: &Device,
        surface: &Surface,
        old_options: &mut RendererOptions,
        new_options: RendererOptions,
    ) {
        let dirty = determine_dirty(old_options, &new_options);

        if dirty.contains(DirtyResources::SWAPCHAIN) {
            self.swapchain = util::create_swapchain(device, surface, new_options.size, new_options.vsync);
        }
        if dirty.contains(DirtyResources::CAMERA) {
            self.camera
                .set_aspect_ratio(new_options.size[0] as f32 / new_options.size[1] as f32);
        }

        *old_options = new_options
    }

    pub fn append_to_bgb<'a>(&'a self, general_bgb: &mut BindGroupBuilder<'a>) {
        general_bgb.append(BindingResource::Sampler(&self.linear_sampler));
        general_bgb.append(BindingResource::Sampler(&self.shadow_sampler));
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
