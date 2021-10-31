use rend3::{managers::TextureManager, types::TextureHandle, util::bind_merge::BindGroupBuilder};
use wgpu::{BindGroup, Device, RenderPass, RenderPipeline};

use crate::common::{interfaces::ShaderInterfaces};

pub struct UpdateSkyboxArgs<'a> {
    pub device: &'a Device,
    pub d2c_texture_manager: &'a TextureManager,

    pub interfaces: &'a ShaderInterfaces,

    pub new_skybox_handle: Option<TextureHandle>,
}

pub struct SkyboxPassDrawArgs<'rpass, 'b> {
    pub rpass: &'b mut RenderPass<'rpass>,

    pub shader_uniform_bg: &'rpass BindGroup,
}

pub struct StoredSkybox {
    pub bg: BindGroup,
    pub handle: TextureHandle,
}

pub struct SkyboxPass {
    pub skybox_pipeline: RenderPipeline,
    pub current_skybox: Option<StoredSkybox>,
}

impl SkyboxPass {
    pub fn new(skybox_pipeline: RenderPipeline) -> Self {
        Self {
            skybox_pipeline,
            current_skybox: None,
        }
    }

    pub fn update_skybox(&mut self, args: UpdateSkyboxArgs<'_>) {
        profiling::scope!("Update Skybox");

        if let Some(handle) = args.new_skybox_handle {
            if self.current_skybox.as_ref().map(|s| &s.handle) == Some(&handle) {
                return;
            }

            let bg = BindGroupBuilder::new()
                .append_texture_view(args.d2c_texture_manager.get_view(handle.get_raw()))
                .build(args.device, Some("skybox"), &args.interfaces.skybox_bgl);

            self.current_skybox = Some(StoredSkybox { bg, handle })
        }
    }

    pub fn draw_skybox<'rpass>(&'rpass self, args: SkyboxPassDrawArgs<'rpass, '_>) {
        if let Some(ref skybox) = self.current_skybox {
            args.rpass.set_pipeline(&self.skybox_pipeline);
            args.rpass.set_bind_group(0, args.shader_uniform_bg, &[]);
            args.rpass.set_bind_group(1, &skybox.bg, &[]);
            args.rpass.draw(0..3, 0..1);
        }
    }
}
