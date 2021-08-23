use std::sync::Arc;

use glam::Vec4;
use rend3::{types::TextureHandle, util::output::OutputFrame, ModeData, RenderRoutine, Renderer};
use wgpu::{
    Color, CommandBuffer, LoadOp, Operations, RenderPassColorAttachment, RenderPassDepthStencilAttachment,
    RenderPassDescriptor,
};

pub use utils::*;

pub mod common;
pub mod culling;
pub mod directional;
pub mod opaque;
pub mod shaders;
pub mod skybox;
pub mod tonemapping;
pub mod uniforms;
mod utils;
pub mod vertex;

pub struct PbrRenderRoutine {
    pub interfaces: common::interfaces::ShaderInterfaces,
    pub samplers: common::samplers::Samplers,
    pub cpu_culler: culling::cpu::CpuCuller,
    pub gpu_culler: ModeData<(), culling::gpu::GpuCuller>,
    pub primary_passes: PrimaryPasses,
    pub tonemapping_pass: tonemapping::TonemappingPass,

    pub skybox_texture: Option<TextureHandle>,

    pub render_textures: RenderTextures,
}

impl PbrRenderRoutine {
    pub fn new(renderer: &Renderer, render_texture_options: RenderTextureOptions) -> Self {
        let device = renderer.device();
        let mode = renderer.mode();
        let interfaces = common::interfaces::ShaderInterfaces::new(device);

        let samplers = common::samplers::Samplers::new(device, mode, &interfaces.samplers_bgl);

        let cpu_culler = culling::cpu::CpuCuller::new();
        let gpu_culler = mode.into_data(|| (), || culling::gpu::GpuCuller::new(device));

        let primary_passes = PrimaryPasses::new(renderer, &interfaces, render_texture_options.samples);
        let tonemapping_pass = tonemapping::TonemappingPass::new(tonemapping::TonemappingPassNewArgs {
            device,
            interfaces: &interfaces,
        });

        let render_textures = RenderTextures::new(&renderer.device, render_texture_options);

        Self {
            interfaces,
            samplers,
            cpu_culler,
            gpu_culler,
            primary_passes,
            tonemapping_pass,

            skybox_texture: None,

            render_textures,
        }
    }

    pub fn set_background_texture(&mut self, handle: Option<TextureHandle>) {
        self.skybox_texture = handle;
    }

    pub fn resize(&mut self, renderer: &Renderer, options: RenderTextureOptions) {
        let different_sample_count = self.render_textures.samples != options.samples;
        self.render_textures = RenderTextures::new(&renderer.device, options);
        if different_sample_count {
            self.primary_passes = PrimaryPasses::new(renderer, &self.interfaces, options.samples);
        }
    }
}

impl RenderRoutine for PbrRenderRoutine {
    fn render(&mut self, renderer: Arc<Renderer>, encoders: &mut Vec<CommandBuffer>, frame: &OutputFrame) {
        let mut encoder = renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("primary encoder"),
        });

        let mesh_manager = renderer.mesh_manager.read();
        let mut directional_light = renderer.directional_light_manager.write();
        let mut materials = renderer.material_manager.write();
        let mut d2_textures = renderer.d2_texture_manager.write();
        let mut d2c_textures = renderer.d2c_texture_manager.write();

        directional_light.ready(&renderer.device, &renderer.queue);
        materials.ready(&renderer.device, &renderer.queue, &d2_textures);
        let d2_texture_output = d2_textures.ready(&renderer.device);
        let _d2c_texture_output = d2c_textures.ready(&renderer.device);
        let objects = renderer.object_manager.read().ready();

        self.primary_passes.skybox_pass.update_skybox(skybox::UpdateSkyboxArgs {
            device: &renderer.device,
            d2c_texture_manager: &d2c_textures,
            interfaces: &self.interfaces,
            new_skybox_handle: self.skybox_texture,
        });

        let culler = self.gpu_culler.as_ref().map_cpu(|_| &self.cpu_culler);

        let culled_lights =
            self.primary_passes
                .shadow_passes
                .cull_shadows(directional::DirectionalShadowPassCullShadowsArgs {
                    device: &renderer.device,
                    encoder: &mut encoder,
                    culler,
                    materials: &materials,
                    interfaces: &self.interfaces,
                    lights: &directional_light,
                    objects: &objects,
                });

        let global_resources = renderer.global_resources.read();

        let culled_objects = self.primary_passes.opaque_pass.cull_opaque(opaque::OpaquePassCullArgs {
            device: &renderer.device,
            encoder: &mut encoder,
            culler,
            materials: &materials,
            interfaces: &self.interfaces,
            camera: &global_resources.camera,
            objects: &objects,
        });

        let d2_texture_output_bg_ref = d2_texture_output.bg.as_ref().map(|_| (), |a| &**a);

        self.primary_passes.shadow_passes.draw_culled_shadows(
            directional::DirectionalShadowPassDrawCulledShadowsArgs {
                encoder: &mut encoder,
                materials: &materials,
                meshes: mesh_manager.buffers(),
                samplers: &self.samplers,
                texture_bg: d2_texture_output_bg_ref,
                culled_lights: &culled_lights,
            },
        );

        let primary_camera_uniform_bg = uniforms::create_shader_uniform(uniforms::CreateShaderUniformArgs {
            device: &renderer.device,
            camera: &global_resources.camera,
            interfaces: &self.interfaces,
            ambient: Vec4::new(0.0, 0.0, 0.0, 1.0),
        });

        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("primary renderpass"),
            color_attachments: &[RenderPassColorAttachment {
                view: &self.render_textures.color,
                resolve_target: self.render_textures.resolve.as_ref(),
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: true,
                },
            }],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &self.render_textures.depth,
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(0.0),
                    store: true,
                }),
                stencil_ops: None,
            }),
        });

        self.primary_passes.opaque_pass.prepass(opaque::OpaquePassPrepassArgs {
            rpass: &mut rpass,
            materials: &materials,
            meshes: mesh_manager.buffers(),
            samplers: &self.samplers,
            texture_bg: d2_texture_output_bg_ref,
            culled_objects: &culled_objects,
        });

        self.primary_passes.skybox_pass.draw_skybox(skybox::SkyboxPassDrawArgs {
            rpass: &mut rpass,
            samplers: &self.samplers,
            shader_uniform_bg: &primary_camera_uniform_bg,
        });

        self.primary_passes.opaque_pass.draw(opaque::OpaquePassDrawArgs {
            rpass: &mut rpass,
            materials: &materials,
            meshes: mesh_manager.buffers(),
            samplers: &self.samplers,
            directional_light_bg: directional_light.get_bg(),
            texture_bg: d2_texture_output_bg_ref,
            shader_uniform_bg: &primary_camera_uniform_bg,
            culled_objects: &culled_objects,
        });

        drop(rpass);

        self.tonemapping_pass.blit(tonemapping::TonemappingPassBlitArgs {
            device: &renderer.device,
            encoder: &mut encoder,
            interfaces: &self.interfaces,
            samplers: &self.samplers,
            source: self.render_textures.blit_source_view(),
            target: frame.as_view(),
        });

        encoders.push(encoder.finish())
    }
}

pub struct SampleDependantPassesNewArgs<'a> {
    pub renderer: &'a Renderer,
    pub interfaces: &'a common::interfaces::ShaderInterfaces,
}

pub struct PrimaryPasses {
    pub shadow_passes: directional::DirectionalShadowPass,
    pub skybox_pass: skybox::SkyboxPass,
    pub opaque_pass: opaque::OpaquePass,
}
impl PrimaryPasses {
    pub fn new(renderer: &Renderer, interfaces: &common::interfaces::ShaderInterfaces, samples: SampleCount) -> Self {
        let gpu_d2_texture_manager_guard = renderer.mode.into_data(|| (), || renderer.d2_texture_manager.read());
        let gpu_d2_texture_bgl = gpu_d2_texture_manager_guard
            .as_ref()
            .map(|_| (), |guard| guard.gpu_bgl());

        let material_manager = renderer.material_manager.read();
        let directional_light_manager = renderer.directional_light_manager.read();
        let shadow_pipeline = Arc::new(common::depth_pass::build_depth_pass_shader(
            common::depth_pass::BuildDepthPassShaderArgs {
                mode: renderer.mode,
                device: &renderer.device,
                interfaces,
                texture_bgl: gpu_d2_texture_bgl,
                materials: &material_manager,
                samples: SampleCount::One,
                ty: common::depth_pass::DepthPassType::Shadow,
            },
        ));
        let depth_pipeline = Arc::new(common::depth_pass::build_depth_pass_shader(
            common::depth_pass::BuildDepthPassShaderArgs {
                mode: renderer.mode,
                device: &renderer.device,
                interfaces,
                texture_bgl: gpu_d2_texture_bgl,
                materials: &material_manager,
                samples,
                ty: common::depth_pass::DepthPassType::Prepass,
            },
        ));
        let skybox_pipeline = common::skybox_pass::build_skybox_shader(common::skybox_pass::BuildSkyboxShaderArgs {
            mode: renderer.mode,
            device: &renderer.device,
            interfaces,
            samples,
        });
        let opaque_pipeline = Arc::new(common::opaque_pass::build_opaque_pass_shader(
            common::opaque_pass::BuildOpaquePassShaderArgs {
                mode: renderer.mode,
                device: &renderer.device,
                interfaces,
                directional_light_bgl: directional_light_manager.get_bgl(),
                texture_bgl: gpu_d2_texture_bgl,
                materials: &material_manager,
                samples,
            },
        ));
        Self {
            shadow_passes: directional::DirectionalShadowPass::new(Arc::clone(&shadow_pipeline)),
            skybox_pass: skybox::SkyboxPass::new(skybox_pipeline),
            opaque_pass: opaque::OpaquePass::new(Arc::clone(&depth_pipeline), Arc::clone(&opaque_pipeline)),
        }
    }
}
