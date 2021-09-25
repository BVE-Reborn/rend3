use std::sync::Arc;

use glam::Vec4;
use rend3::{
    resources::{DirectionalLightManager, MaterialManager, TextureManager},
    types::{TextureFormat, TextureHandle, TransparencyType},
    ManagerReadyOutput, ModeData, RenderRoutine, Renderer, RendererMode,
};
use wgpu::{
    Color, CommandBuffer, Device, LoadOp, Operations, RenderPassColorAttachment, RenderPassDepthStencilAttachment,
    RenderPassDescriptor, TextureView,
};

pub use utils::*;

pub mod common;
pub mod culling;
pub mod directional;
pub mod forward;
pub mod shaders;
pub mod skybox;
pub mod tonemapping;
pub mod uniforms;
mod utils;
pub mod vertex;

/// Render routine that renders the using PBR materials and gpu based culling.
pub struct PbrRenderRoutine {
    pub interfaces: common::interfaces::ShaderInterfaces,
    pub samplers: common::samplers::Samplers,
    pub cpu_culler: culling::cpu::CpuCuller,
    pub gpu_culler: ModeData<(), culling::gpu::GpuCuller>,
    pub primary_passes: PrimaryPasses,
    pub tonemapping_pass: tonemapping::TonemappingPass,

    pub skybox_texture: Option<TextureHandle>,
    pub ambient: Vec4,

    pub render_textures: RenderTextures,
}

impl PbrRenderRoutine {
    pub fn new(
        renderer: &Renderer,
        render_texture_options: RenderTextureOptions,
        output_format: TextureFormat,
    ) -> Self {
        let interfaces = common::interfaces::ShaderInterfaces::new(&renderer.device);

        let samplers = common::samplers::Samplers::new(&renderer.device, renderer.mode, &interfaces.samplers_bgl);

        let cpu_culler = culling::cpu::CpuCuller::new();
        let gpu_culler = renderer
            .mode
            .into_data(|| (), || culling::gpu::GpuCuller::new(&renderer.device));

        let primary_passes = {
            let d2_textures = renderer.d2_texture_manager.read();
            let directional_light = renderer.directional_light_manager.read();
            let materials = renderer.material_manager.read();
            PrimaryPasses::new(PrimaryPassesNewArgs {
                mode: renderer.mode,
                device: &renderer.device,
                d2_textures: &d2_textures,
                directional_lights: &directional_light,
                materials: &materials,
                interfaces: &interfaces,
                samples: render_texture_options.samples,
            })
        };
        let tonemapping_pass = tonemapping::TonemappingPass::new(tonemapping::TonemappingPassNewArgs {
            device: &renderer.device,
            interfaces: &interfaces,
            output_format,
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
            ambient: Vec4::new(0.0, 0.0, 0.0, 1.0),

            render_textures,
        }
    }

    pub fn set_ambient_color(&mut self, color: Vec4) {
        self.ambient = color;
    }

    pub fn set_background_texture(&mut self, handle: Option<TextureHandle>) {
        self.skybox_texture = handle;
    }

    pub fn resize(&mut self, renderer: &Renderer, options: RenderTextureOptions) {
        let different_sample_count = self.render_textures.samples != options.samples;
        self.render_textures = RenderTextures::new(&renderer.device, options);
        if different_sample_count {
            let d2_textures = renderer.d2_texture_manager.read();
            let directional_light = renderer.directional_light_manager.read();
            let materials = renderer.material_manager.read();
            self.primary_passes = PrimaryPasses::new(PrimaryPassesNewArgs {
                mode: renderer.mode,
                device: &renderer.device,
                d2_textures: &d2_textures,
                directional_lights: &directional_light,
                materials: &materials,
                interfaces: &self.interfaces,
                samples: self.render_textures.samples,
            });
        }
    }
}

impl RenderRoutine<(), &TextureView> for PbrRenderRoutine {
    fn render(
        &mut self,
        renderer: Arc<Renderer>,
        cmd_bufs: flume::Sender<CommandBuffer>,
        ready: ManagerReadyOutput,
        _input: (),
        output: &TextureView,
    ) {
        profiling::scope!("PBR Render Routine");

        let mut encoder = renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("primary encoder"),
        });

        let mut profiler = renderer.profiler.lock();

        let mesh_manager = renderer.mesh_manager.read();
        let directional_light = renderer.directional_light_manager.read();
        let materials = renderer.material_manager.read();
        let d2c_textures = renderer.d2c_texture_manager.read();
        let camera_manager = renderer.camera_manager.read();

        self.primary_passes.skybox_pass.update_skybox(skybox::UpdateSkyboxArgs {
            device: &renderer.device,
            d2c_texture_manager: &d2c_textures,
            interfaces: &self.interfaces,
            new_skybox_handle: self.skybox_texture.clone(),
        });

        let culler = self.gpu_culler.as_ref().map_cpu(|_| &self.cpu_culler);

        let culled_lights =
            self.primary_passes
                .shadow_passes
                .cull_shadows(directional::DirectionalShadowPassCullShadowsArgs {
                    device: &renderer.device,
                    profiler: &mut profiler,
                    encoder: &mut encoder,
                    culler,
                    materials: &materials,
                    interfaces: &self.interfaces,
                    lights: &directional_light,
                    directional_light_cameras: &ready.directional_light_cameras,
                    objects: &ready.objects,
                });

        profiler.begin_scope("forward culling", &mut encoder, &renderer.device);

        let transparent_culled_objects = self.primary_passes.transparent_pass.cull(forward::ForwardPassCullArgs {
            device: &renderer.device,
            profiler: &mut profiler,
            encoder: &mut encoder,
            culler,
            materials: &materials,
            interfaces: &self.interfaces,
            camera: &camera_manager,
            objects: &ready.objects,
        });

        let cutout_culled_objects = self.primary_passes.cutout_pass.cull(forward::ForwardPassCullArgs {
            device: &renderer.device,
            profiler: &mut profiler,
            encoder: &mut encoder,
            culler,
            materials: &materials,
            interfaces: &self.interfaces,
            camera: &camera_manager,
            objects: &ready.objects,
        });

        let opaque_culled_objects = self.primary_passes.opaque_pass.cull(forward::ForwardPassCullArgs {
            device: &renderer.device,
            profiler: &mut profiler,
            encoder: &mut encoder,
            culler,
            materials: &materials,
            interfaces: &self.interfaces,
            camera: &camera_manager,
            objects: &ready.objects,
        });

        profiler.end_scope(&mut encoder);

        let d2_texture_output_bg_ref = ready.d2_texture.bg.as_ref().map(|_| (), |a| &**a);

        self.primary_passes.shadow_passes.draw_culled_shadows(
            directional::DirectionalShadowPassDrawCulledShadowsArgs {
                device: &renderer.device,
                profiler: &mut profiler,
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
            camera: &camera_manager,
            interfaces: &self.interfaces,
            ambient: self.ambient,
        });

        {
            profiling::scope!("primary renderpass");
            profiler.begin_scope("primary renderpass", &mut encoder, &renderer.device);

            let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: None,
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

            profiler.begin_scope("depth prepass", &mut rpass, &renderer.device);
            self.primary_passes
                .opaque_pass
                .prepass(forward::ForwardPassPrepassArgs {
                    device: &renderer.device,
                    profiler: &mut profiler,
                    rpass: &mut rpass,
                    materials: &materials,
                    meshes: mesh_manager.buffers(),
                    samplers: &self.samplers,
                    texture_bg: d2_texture_output_bg_ref,
                    culled_objects: &opaque_culled_objects,
                });

            self.primary_passes
                .cutout_pass
                .prepass(forward::ForwardPassPrepassArgs {
                    device: &renderer.device,
                    profiler: &mut profiler,
                    rpass: &mut rpass,
                    materials: &materials,
                    meshes: mesh_manager.buffers(),
                    samplers: &self.samplers,
                    texture_bg: d2_texture_output_bg_ref,
                    culled_objects: &cutout_culled_objects,
                });

            profiler.end_scope(&mut rpass);
            profiler.begin_scope("skybox", &mut rpass, &renderer.device);

            self.primary_passes.skybox_pass.draw_skybox(skybox::SkyboxPassDrawArgs {
                rpass: &mut rpass,
                samplers: &self.samplers,
                shader_uniform_bg: &primary_camera_uniform_bg,
            });

            profiler.end_scope(&mut rpass);
            profiler.begin_scope("forward", &mut rpass, &renderer.device);

            self.primary_passes.opaque_pass.draw(forward::ForwardPassDrawArgs {
                device: &renderer.device,
                profiler: &mut profiler,
                rpass: &mut rpass,
                materials: &materials,
                meshes: mesh_manager.buffers(),
                samplers: &self.samplers,
                directional_light_bg: directional_light.get_bg(),
                texture_bg: d2_texture_output_bg_ref,
                shader_uniform_bg: &primary_camera_uniform_bg,
                culled_objects: &opaque_culled_objects,
            });

            self.primary_passes.cutout_pass.draw(forward::ForwardPassDrawArgs {
                device: &renderer.device,
                profiler: &mut profiler,
                rpass: &mut rpass,
                materials: &materials,
                meshes: mesh_manager.buffers(),
                samplers: &self.samplers,
                directional_light_bg: directional_light.get_bg(),
                texture_bg: d2_texture_output_bg_ref,
                shader_uniform_bg: &primary_camera_uniform_bg,
                culled_objects: &cutout_culled_objects,
            });

            self.primary_passes.transparent_pass.draw(forward::ForwardPassDrawArgs {
                device: &renderer.device,
                profiler: &mut profiler,
                rpass: &mut rpass,
                materials: &materials,
                meshes: mesh_manager.buffers(),
                samplers: &self.samplers,
                directional_light_bg: directional_light.get_bg(),
                texture_bg: d2_texture_output_bg_ref,
                shader_uniform_bg: &primary_camera_uniform_bg,
                culled_objects: &transparent_culled_objects,
            });

            profiler.end_scope(&mut rpass);

            drop(rpass);
            profiler.end_scope(&mut encoder);
        }

        self.tonemapping_pass.blit(tonemapping::TonemappingPassBlitArgs {
            device: &renderer.device,
            profiler: &mut profiler,
            encoder: &mut encoder,
            interfaces: &self.interfaces,
            samplers: &self.samplers,
            source: self.render_textures.blit_source_view(),
            target: output,
        });

        cmd_bufs.send(encoder.finish()).unwrap();
    }
}

pub struct PrimaryPassesNewArgs<'a> {
    pub mode: RendererMode,
    pub device: &'a Device,

    pub d2_textures: &'a TextureManager,
    pub directional_lights: &'a DirectionalLightManager,
    pub materials: &'a MaterialManager,

    pub interfaces: &'a common::interfaces::ShaderInterfaces,

    pub samples: SampleCount,
}

pub struct PrimaryPasses {
    pub shadow_passes: directional::DirectionalShadowPass,
    pub skybox_pass: skybox::SkyboxPass,
    pub opaque_pass: forward::ForwardPass,
    pub cutout_pass: forward::ForwardPass,
    pub transparent_pass: forward::ForwardPass,
}
impl PrimaryPasses {
    pub fn new(args: PrimaryPassesNewArgs<'_>) -> Self {
        let gpu_d2_texture_bgl = args.mode.into_data(|| (), || args.d2_textures.gpu_bgl());

        let shadow_pipelines =
            common::depth_pass::build_depth_pass_shader(common::depth_pass::BuildDepthPassShaderArgs {
                mode: args.mode,
                device: args.device,
                interfaces: args.interfaces,
                texture_bgl: gpu_d2_texture_bgl,
                materials: args.materials,
                samples: SampleCount::One,
                ty: common::depth_pass::DepthPassType::Shadow,
            });
        let depth_pipelines =
            common::depth_pass::build_depth_pass_shader(common::depth_pass::BuildDepthPassShaderArgs {
                mode: args.mode,
                device: args.device,
                interfaces: args.interfaces,
                texture_bgl: gpu_d2_texture_bgl,
                materials: args.materials,
                samples: args.samples,
                ty: common::depth_pass::DepthPassType::Prepass,
            });
        let skybox_pipeline = common::skybox_pass::build_skybox_shader(common::skybox_pass::BuildSkyboxShaderArgs {
            mode: args.mode,
            device: args.device,
            interfaces: args.interfaces,
            samples: args.samples,
        });
        let forward_pass_args = common::forward_pass::BuildForwardPassShaderArgs {
            mode: args.mode,
            device: args.device,
            interfaces: args.interfaces,
            directional_light_bgl: args.directional_lights.get_bgl(),
            texture_bgl: gpu_d2_texture_bgl,
            materials: args.materials,
            samples: args.samples,
            transparency: TransparencyType::Opaque,
            baking: common::forward_pass::Baking::Disabled,
        };
        let opaque_pipeline = Arc::new(common::forward_pass::build_forward_pass_shader(
            forward_pass_args.clone(),
        ));
        let cutout_pipeline = Arc::new(common::forward_pass::build_forward_pass_shader(
            common::forward_pass::BuildForwardPassShaderArgs {
                transparency: TransparencyType::Cutout,
                ..forward_pass_args.clone()
            },
        ));
        let transparent_pipeline = Arc::new(common::forward_pass::build_forward_pass_shader(
            common::forward_pass::BuildForwardPassShaderArgs {
                transparency: TransparencyType::Blend,
                ..forward_pass_args.clone()
            },
        ));
        Self {
            shadow_passes: directional::DirectionalShadowPass::new(
                Arc::clone(&shadow_pipelines.cutout),
                Arc::clone(&shadow_pipelines.opaque),
            ),
            skybox_pass: skybox::SkyboxPass::new(skybox_pipeline),
            transparent_pass: forward::ForwardPass::new(
                Some(Arc::clone(&depth_pipelines.opaque)),
                transparent_pipeline,
                TransparencyType::Blend,
            ),
            cutout_pass: forward::ForwardPass::new(
                Some(Arc::clone(&depth_pipelines.cutout)),
                cutout_pipeline,
                TransparencyType::Cutout,
            ),
            opaque_pass: forward::ForwardPass::new(
                Some(Arc::clone(&depth_pipelines.opaque)),
                opaque_pipeline,
                TransparencyType::Opaque,
            ),
        }
    }
}
