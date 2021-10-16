/// PBR Render Routine for rend3.
///
/// Contains [`PbrMaterial`] and the [`PbrRenderRoutine`] which serve as the default render routines.
///
/// Tries to strike a balance between photorealism and performance.
use std::{borrow::Cow, mem, sync::Arc};

use glam::{UVec2, Vec4};
use ordered_float::OrderedFloat;
use parking_lot::Mutex;
use rend3::{
    resources::{DirectionalLightManager, GpuCullingInput, MaterialManager, TextureManager},
    types::{TextureFormat, TextureHandle, TextureUsages},
    BufferTargetDescriptor, ModeData, ReadyData, RenderGraph, RenderGraphNodeBuilder, RenderTargetDescriptor, Renderer,
    RendererMode, SHADOW_DIMENSIONS,
};
use wgpu::{
    BufferUsages, Color, CommandEncoderDescriptor, Device, LoadOp, Operations, RenderPassColorAttachment,
    RenderPassDepthStencilAttachment, RenderPassDescriptor,
};

pub use utils::*;

use crate::material::{PbrMaterial, TransparencyType};

pub mod common;
pub mod culling;
pub mod directional;
pub mod forward;
pub mod material;
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

    pub ambient: Vec4,

    pub render_texture_options: RenderTextureOptions,
}

impl PbrRenderRoutine {
    pub fn new(renderer: &Renderer, render_texture_options: RenderTextureOptions) -> Self {
        profiling::scope!("PbrRenderRoutine::new");

        let interfaces = common::interfaces::ShaderInterfaces::new(&renderer.device);

        let samplers = common::samplers::Samplers::new(&renderer.device, renderer.mode, &interfaces.samplers_bgl);

        let cpu_culler = culling::cpu::CpuCuller::new();
        let gpu_culler = renderer
            .mode
            .into_data(|| (), || culling::gpu::GpuCuller::new(&renderer.device));

        let primary_passes = {
            let d2_textures = renderer.d2_texture_manager.read();
            let directional_light = renderer.directional_light_manager.read();
            let mut materials = renderer.material_manager.write();
            // TODO(material): figure out a better way for zero materials to work
            materials.ensure_archetype::<PbrMaterial>(&renderer.device, renderer.mode);
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

        Self {
            interfaces,
            samplers,
            cpu_culler,
            gpu_culler,
            primary_passes,

            ambient: Vec4::new(0.0, 0.0, 0.0, 1.0),

            render_texture_options,
        }
    }

    pub fn set_ambient_color(&mut self, color: Vec4) {
        self.ambient = color;
    }

    pub fn resize(&mut self, renderer: &Renderer, options: RenderTextureOptions) {
        profiling::scope!("PbrRenderRoutine::resize");
        let different_sample_count = self.render_texture_options.samples != options.samples;
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
                samples: options.samples,
            });
        }
        self.render_texture_options = options;
    }

    pub fn add_pre_cull_to_graph<'node>(&'node self, graph: &mut RenderGraph<'node>, renderer: &Renderer) {
        let object_manager = renderer.object_manager.read();

        let mut declare_pre_cull = |name, transparency: TransparencyType| {
            let mut builder = graph.add_node();
            let handle = builder.add_buffer_output(
                name,
                BufferTargetDescriptor {
                    length: (object_manager.get_objects::<PbrMaterial>(transparency as u64).len()
                        * mem::size_of::<GpuCullingInput>()) as _,
                    usage: BufferUsages::STORAGE,
                    mapped: true,
                },
            );

            builder.build(move |renderer, _prefix_cmd_bufs, cmd_bufs, ready, texture_store| {
                let object_manager = renderer.object_manager.read();
                let camera_manager = renderer.camera_manager.read();

                let objects = object_manager.get_objects::<PbrMaterial>(transparency as u64);
                let count = objects.len();

                let objects = common::sorting::sort_objects(objects, &camera_manager, transparency.to_sorting());
                let buffer = texture_store.get_buffer(handle);
                culling::gpu::build_cull_data(buffer, &objects);
            });
        };

        declare_pre_cull("Opaque Pre-Cull Data", material::TransparencyType::Opaque);
        declare_pre_cull("Cutout Pre-Cull Data", material::TransparencyType::Cutout);
        declare_pre_cull("Blend Pre-Cull Data", material::TransparencyType::Blend);
    }

    pub fn add_shadows_to_graph<'node>(&'node self, graph: &mut RenderGraph<'node>, ready: &ReadyData) {
        for idx in 0..ready.directional_light_cameras.len() {
            for (transparency, pre_cull_name) in [
                (TransparencyType::Opaque, "Opaque Pre-Cull Data"),
                (TransparencyType::Cutout, "Cutout Pre-Cull Data"),
            ] {
                let mut builder = graph.add_node();

                let pre_cull_handle = self
                    .gpu_culler
                    .mode()
                    .into_data(|| (), || builder.add_buffer_input(pre_cull_name));
                let shadow_map_handle = builder.add_shadow_output(idx, SHADOW_DIMENSIONS as usize);

                builder.build(move |renderer, _prefix_cmd_bufs, cmd_bufs, ready, texture_store| {
                    let shadow_map = texture_store.get_shadow(shadow_map_handle);
                    let culling_input_opaque = pre_cull_handle.map_gpu(|handle| texture_store.get_buffer(handle));

                    let mut encoder = renderer
                        .device
                        .create_command_encoder(&CommandEncoderDescriptor::default());

                    let camera_manager = renderer.camera_manager.read();
                    let object_manager = renderer.object_manager.read();
                    let material_manager = renderer.material_manager.read();

                    let count = object_manager.get_objects::<PbrMaterial>(transparency as u64).len();

                    let culled_objects = match self.gpu_culler {
                        ModeData::CPU(_) => self.cpu_culler.cull(culling::cpu::CpuCullerCullArgs {
                            device: &renderer.device,
                            camera: &camera_manager,
                            interfaces: &self.interfaces,
                            objects: &object_manager,
                            transparency,
                        }),
                        ModeData::GPU(gpu_culler) => gpu_culler.cull(culling::gpu::GpuCullerCullArgs {
                            device: &renderer.device,
                            encoder: &mut encoder,
                            interfaces: &self.interfaces,
                            camera: &camera_manager,
                            input_buffer: culling_input_opaque.into_gpu(),
                            input_count: count,
                            transparency,
                        }),
                    };

                    let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
                        label: None,
                        color_attachments: &[],
                        depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                            view: &shadow_map.view,
                            depth_ops: Some(Operations {
                                load: match transparency {
                                    TransparencyType::Opaque => LoadOp::Clear(1.0),
                                    _ => LoadOp::Load,
                                },
                                store: true,
                            }),
                            stencil_ops: None,
                        }),
                    });
                    rpass.set_pipeline(&self.primary_passes.shadow_passes.opaque_pipeline);
                    rpass.set_bind_group(0, &self.samplers.linear_nearest_bg, &[]);
                    rpass.set_bind_group(1, &culled_objects.output_bg, &[]);

                    match culled_objects.calls {
                        ModeData::CPU(ref draws) => {
                            culling::cpu::run(&mut rpass, draws, &self.samplers, 0, &material_manager, 2)
                        }
                        ModeData::GPU(ref data) => {
                            rpass.set_bind_group(2, material_manager.get_bind_group_gpu::<PbrMaterial>(), &[]);
                            rpass.set_bind_group(3, ready.d2_texture.bg.as_gpu(), &[]);
                            culling::gpu::run(&mut rpass, data);
                        }
                    }

                    cmd_bufs.send(encoder.finish()).unwrap();
                });
            }
        }
    }

    pub fn add_prepass_to_graph<'node>(&'node self, graph: &mut RenderGraph<'node>) {
        let mut builder = graph.add_node();

        let hdr_color_handle = builder.add_render_target_output(
            "hdr color",
            RenderTargetDescriptor {
                dim: self.render_texture_options.resolution,
                format: TextureFormat::Rgba16Float,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            },
        );

        let hdr_depth_handle = builder.add_render_target_output(
            "hdr depth",
            RenderTargetDescriptor {
                dim: self.render_texture_options.resolution,
                format: TextureFormat::Depth32Float,
                usage: TextureUsages::RENDER_ATTACHMENT,
            },
        );

        builder.build(move |renderer, _prefix_cmd_bufs, cmd_bufs, ready, texture_store| {
            let hdr_color = texture_store.get_render_target(hdr_color_handle);
            let hdr_depth = texture_store.get_render_target(hdr_depth_handle);
            profiling::scope!("PBR Render Routine");

            let mut encoder = renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("primary encoder"),
            });

            let mut profiler = renderer.profiler.lock();

            let mesh_manager = renderer.mesh_manager.read();
            let directional_light = renderer.directional_light_manager.read();
            let materials = renderer.material_manager.read();
            let camera_manager = renderer.camera_manager.read();
            // TODO(material): let this be read
            let mut object_manager = renderer.object_manager.write();

            let culler = self.gpu_culler.as_ref().map_cpu(|_| &self.cpu_culler);

            let culled_lights =
                self.primary_passes
                    .shadow_passes
                    .cull_shadows(directional::DirectionalShadowPassCullShadowsArgs {
                        device: &renderer.device,
                        profiler: &mut profiler,
                        encoder: &mut encoder,
                        culler,
                        interfaces: &self.interfaces,
                        lights: &directional_light,
                        directional_light_cameras: &ready.directional_light_cameras,
                        objects: &mut object_manager,
                        culling_input_opaque: culling_input_opaque.as_gpu_only_mut(),
                        culling_input_cutout: culling_input_cutout.as_gpu_only_mut(),
                    });

            profiler.begin_scope("forward culling", &mut encoder, &renderer.device);
            let cutout_culled_objects = self.primary_passes.cutout_pass.cull(forward::ForwardPassCullArgs {
                device: &renderer.device,
                profiler: &mut profiler,
                encoder: &mut encoder,
                culler,
                interfaces: &self.interfaces,
                camera: &camera_manager,
                objects: &mut object_manager,
                culling_input: culling_input_cutout.as_gpu_only_mut(),
            });

            let opaque_culled_objects = self.primary_passes.opaque_pass.cull(forward::ForwardPassCullArgs {
                device: &renderer.device,
                profiler: &mut profiler,
                encoder: &mut encoder,
                culler,
                interfaces: &self.interfaces,
                camera: &camera_manager,
                objects: &mut object_manager,
                culling_input: culling_input_opaque.as_gpu_only_mut(),
            });

            profiler.end_scope(&mut encoder);

            let d2_texture_output_bg_ref = ready.d2_texture.bg.as_ref().map(|_| (), |a| &**a);

            // shadows being part of the code renderpass now makes no sense
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

            {
                let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: None,
                    color_attachments: &[RenderPassColorAttachment {
                        view: hdr_color,
                        resolve_target: None,
                        ops: Operations {
                            load: LoadOp::Clear(Color::BLACK),
                            store: true,
                        },
                    }],
                    depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                        view: hdr_depth,
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

                drop(rpass);
            }
            cmd_bufs.send(encoder.finish()).unwrap();
        });
    }

    pub fn add_forward_to_graph<'node>(&'node self, graph: &mut RenderGraph<'node>) {
        let mut builder = graph.add_node();

        let hdr_color_handle = builder.add_render_target_output(
            "hdr color",
            RenderTargetDescriptor {
                dim: self.render_texture_options.resolution,
                format: TextureFormat::Rgba16Float,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            },
        );

        let hdr_depth_handle = builder.add_render_target_output(
            "hdr depth",
            RenderTargetDescriptor {
                dim: self.render_texture_options.resolution,
                format: TextureFormat::Depth32Float,
                usage: TextureUsages::RENDER_ATTACHMENT,
            },
        );

        builder.build(move |renderer, _prefix_cmd_bufs, cmd_bufs, ready, texture_store| {
            let hdr_color = texture_store.get_render_target(hdr_color_handle);
            let hdr_depth = texture_store.get_render_target(hdr_depth_handle);
            profiling::scope!("PBR Render Routine");

            let mut encoder = renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("primary encoder"),
            });

            let mut profiler = renderer.profiler.lock();

            let mesh_manager = renderer.mesh_manager.read();
            let directional_light = renderer.directional_light_manager.read();
            let materials = renderer.material_manager.read();
            let camera_manager = renderer.camera_manager.read();
            // TODO(material): let this be read
            let mut object_manager = renderer.object_manager.write();

            let culler = self.gpu_culler.as_ref().map_cpu(|_| &self.cpu_culler);

            let mut culling_input_opaque = culler.map(
                |_| (),
                |culler| {
                    culler.pre_cull(culling::gpu::GpuCullerPreCullArgs {
                        device: &renderer.device,
                        camera: &camera_manager,
                        objects: &mut object_manager,
                        transparency: TransparencyType::Opaque,
                        sort: None,
                    })
                },
            );
            let mut culling_input_cutout = culler.map(
                |_| (),
                |culler| {
                    culler.pre_cull(culling::gpu::GpuCullerPreCullArgs {
                        device: &renderer.device,
                        camera: &camera_manager,
                        objects: &mut object_manager,
                        transparency: TransparencyType::Cutout,
                        sort: None,
                    })
                },
            );
            let mut culling_input_blend = culler.map(
                |_| (),
                |culler| {
                    culler.pre_cull(culling::gpu::GpuCullerPreCullArgs {
                        device: &renderer.device,
                        camera: &camera_manager,
                        objects: &mut object_manager,
                        transparency: TransparencyType::Blend,
                        sort: Some(culling::Sorting::FrontToBack),
                    })
                },
            );

            profiler.begin_scope("forward culling", &mut encoder, &renderer.device);

            let transparent_culled_objects = self.primary_passes.transparent_pass.cull(forward::ForwardPassCullArgs {
                device: &renderer.device,
                profiler: &mut profiler,
                encoder: &mut encoder,
                culler,
                interfaces: &self.interfaces,
                camera: &camera_manager,
                objects: &mut object_manager,
                culling_input: culling_input_blend.as_gpu_only_mut(),
            });

            let cutout_culled_objects = self.primary_passes.cutout_pass.cull(forward::ForwardPassCullArgs {
                device: &renderer.device,
                profiler: &mut profiler,
                encoder: &mut encoder,
                culler,
                interfaces: &self.interfaces,
                camera: &camera_manager,
                objects: &mut object_manager,
                culling_input: culling_input_cutout.as_gpu_only_mut(),
            });

            let opaque_culled_objects = self.primary_passes.opaque_pass.cull(forward::ForwardPassCullArgs {
                device: &renderer.device,
                profiler: &mut profiler,
                encoder: &mut encoder,
                culler,
                interfaces: &self.interfaces,
                camera: &camera_manager,
                objects: &mut object_manager,
                culling_input: culling_input_opaque.as_gpu_only_mut(),
            });

            profiler.end_scope(&mut encoder);

            let d2_texture_output_bg_ref = ready.d2_texture.bg.as_ref().map(|_| (), |a| &**a);

            let primary_camera_uniform_bg = uniforms::create_shader_uniform(uniforms::CreateShaderUniformArgs {
                device: &renderer.device,
                camera: &camera_manager,
                interfaces: &self.interfaces,
                ambient: self.ambient,
            });

            {
                let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: None,
                    color_attachments: &[RenderPassColorAttachment {
                        view: hdr_color,
                        resolve_target: None,
                        ops: Operations {
                            load: LoadOp::Load,
                            store: true,
                        },
                    }],
                    depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                        view: hdr_depth,
                        depth_ops: Some(Operations {
                            load: LoadOp::Load,
                            store: true,
                        }),
                        stencil_ops: None,
                    }),
                });

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
            }
            cmd_bufs.send(encoder.finish()).unwrap();
        });
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
    pub opaque_pass: forward::ForwardPass,
    pub cutout_pass: forward::ForwardPass,
    pub transparent_pass: forward::ForwardPass,
}
impl PrimaryPasses {
    pub fn new(args: PrimaryPassesNewArgs<'_>) -> Self {
        profiling::scope!("PrimaryPasses::new");

        let gpu_d2_texture_bgl = args.mode.into_data(|| (), || args.d2_textures.gpu_bgl());

        let shadow_pipelines =
            common::depth_pass::build_depth_pass_pipeline(common::depth_pass::BuildDepthPassShaderArgs {
                mode: args.mode,
                device: args.device,
                interfaces: args.interfaces,
                texture_bgl: gpu_d2_texture_bgl,
                materials: args.materials,
                samples: SampleCount::One,
                ty: common::depth_pass::DepthPassType::Shadow,
            });
        let depth_pipelines =
            common::depth_pass::build_depth_pass_pipeline(common::depth_pass::BuildDepthPassShaderArgs {
                mode: args.mode,
                device: args.device,
                interfaces: args.interfaces,
                texture_bgl: gpu_d2_texture_bgl,
                materials: args.materials,
                samples: args.samples,
                ty: common::depth_pass::DepthPassType::Prepass,
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
        let opaque_pipeline = Arc::new(common::forward_pass::build_forward_pass_pipeline(
            forward_pass_args.clone(),
        ));
        let cutout_pipeline = Arc::new(common::forward_pass::build_forward_pass_pipeline(
            common::forward_pass::BuildForwardPassShaderArgs {
                transparency: TransparencyType::Cutout,
                ..forward_pass_args.clone()
            },
        ));
        let transparent_pipeline = Arc::new(common::forward_pass::build_forward_pass_pipeline(
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

pub struct SkyboxRoutine {
    pub interfaces: common::interfaces::ShaderInterfaces,
    pub samplers: common::samplers::Samplers,
    pub skybox_pass: Mutex<skybox::SkyboxPass>,
    pub options: RenderTextureOptions,

    pub skybox_texture: Option<TextureHandle>,
}

impl SkyboxRoutine {
    pub fn new(renderer: &Renderer, options: RenderTextureOptions) -> Self {
        // TODO: clean up
        let interfaces = common::interfaces::ShaderInterfaces::new(&renderer.device);
        let samplers = common::samplers::Samplers::new(&renderer.device, renderer.mode, &interfaces.samplers_bgl);

        let skybox_pipeline = common::skybox_pass::build_skybox_pipeline(common::skybox_pass::BuildSkyboxShaderArgs {
            mode: renderer.mode,
            device: &renderer.device,
            interfaces: &interfaces,
            samples: SampleCount::One,
        });

        let skybox_pass = Mutex::new(skybox::SkyboxPass::new(skybox_pipeline));

        Self {
            skybox_pass,
            options,
            samplers,
            interfaces,
            skybox_texture: None,
        }
    }

    pub fn set_background_texture(&mut self, texture: Option<TextureHandle>) {
        self.skybox_texture = texture;
    }

    pub fn resize(&mut self, renderer: &Renderer, options: RenderTextureOptions) {
        if self.options.samples != options.samples {
            let skybox_pipeline =
                common::skybox_pass::build_skybox_pipeline(common::skybox_pass::BuildSkyboxShaderArgs {
                    mode: renderer.mode,
                    device: &renderer.device,
                    interfaces: &self.interfaces,
                    samples: options.samples,
                });
            self.skybox_pass = Mutex::new(skybox::SkyboxPass::new(skybox_pipeline));
        }

        self.options = options;
    }

    pub fn add_to_graph<'node>(&'node self, graph: &mut RenderGraph<'node>) {
        let mut builder = graph.add_node();

        let hdr_color_handle = builder.add_render_target_output(
            "hdr color",
            RenderTargetDescriptor {
                dim: self.options.resolution,
                format: TextureFormat::Rgba16Float,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            },
        );

        let hdr_depth_handle = builder.add_render_target_input("hdr depth");

        builder.build(move |renderer, _prefix_cmd_bufs, cmd_bufs, _ready, texture_store| {
            let hdr_color = texture_store.get_render_target(hdr_color_handle);
            let hdr_depth = texture_store.get_render_target(hdr_depth_handle);
            let mut encoder = renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("tonemapper encoder"),
            });

            let d2c_texture_manager = renderer.d2c_texture_manager.read();
            let mut profiler = renderer.profiler.lock();
            let mut skybox_pass = self.skybox_pass.lock();

            skybox_pass.update_skybox(skybox::UpdateSkyboxArgs {
                device: &renderer.device,
                interfaces: &self.interfaces,
                d2c_texture_manager: &d2c_texture_manager,
                new_skybox_handle: self.skybox_texture.clone(),
            });

            drop(d2c_texture_manager);

            let camera_manager = renderer.camera_manager.read();

            let primary_camera_uniform_bg = uniforms::create_shader_uniform(uniforms::CreateShaderUniformArgs {
                device: &renderer.device,
                camera: &camera_manager,
                interfaces: &self.interfaces,
                ambient: Vec4::ZERO,
            });

            let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: None,
                color_attachments: &[RenderPassColorAttachment {
                    view: hdr_color,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Load,
                        store: true,
                    },
                }],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: hdr_depth,
                    depth_ops: Some(Operations {
                        load: LoadOp::Load,
                        store: true,
                    }),
                    stencil_ops: None,
                }),
            });

            profiler.begin_scope("forward", &mut rpass, &renderer.device);

            skybox_pass.draw_skybox(skybox::SkyboxPassDrawArgs {
                rpass: &mut rpass,
                samplers: &self.samplers,
                shader_uniform_bg: &primary_camera_uniform_bg,
            });

            profiler.end_scope(&mut rpass);

            drop(rpass);

            cmd_bufs.send(encoder.finish()).unwrap();
        });
    }
}

pub struct TonemappingRoutine {
    pub interfaces: common::interfaces::ShaderInterfaces,
    pub samplers: common::samplers::Samplers,
    pub tonemapping_pass: tonemapping::TonemappingPass,
    pub size: UVec2,
}

impl TonemappingRoutine {
    pub fn new(renderer: &Renderer, size: UVec2, output_format: TextureFormat) -> Self {
        // TODO: clean up
        let interfaces = common::interfaces::ShaderInterfaces::new(&renderer.device);
        let samplers = common::samplers::Samplers::new(&renderer.device, renderer.mode, &interfaces.samplers_bgl);

        let tonemapping_pass = tonemapping::TonemappingPass::new(tonemapping::TonemappingPassNewArgs {
            device: &renderer.device,
            interfaces: &interfaces,
            output_format,
        });

        Self {
            tonemapping_pass,
            size,
            samplers,
            interfaces,
        }
    }

    pub fn resize(&mut self, size: UVec2) {
        self.size = size;
    }

    pub fn add_to_graph<'node>(&'node self, graph: &mut RenderGraph<'node>) {
        let mut builder = graph.add_node();

        let hdr_color_handle = builder.add_render_target_input("hdr color");

        let output_handle = builder.add_surface_output();

        builder.build(move |renderer, _prefix_cmd_bufs, cmd_bufs, _ready, texture_store| {
            let hdr_color = texture_store.get_render_target(hdr_color_handle);
            let output = texture_store.get_render_target(output_handle);
            let mut encoder = renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("tonemapper encoder"),
            });

            let mut profiler = renderer.profiler.lock();

            self.tonemapping_pass.blit(tonemapping::TonemappingPassBlitArgs {
                device: &renderer.device,
                profiler: &mut profiler,
                encoder: &mut encoder,
                interfaces: &self.interfaces,
                samplers: &self.samplers,
                source: hdr_color,
                target: output,
            });

            cmd_bufs.send(encoder.finish()).unwrap();
        });
    }
}
