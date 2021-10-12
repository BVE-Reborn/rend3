use std::sync::Arc;

use glam::Vec4;
use rend3::{ReadyData, Renderer};
use wgpu::{Color, LoadOp, Operations, RenderPassColorAttachment, RenderPassDescriptor, TextureView};

pub struct PbrBakerOutput<'a> {
    pub view: &'a TextureView,
}

pub struct BakeData {
    pub object: rend3::types::ObjectHandle,
}

pub struct PbrBakerRenderRoutine {
    pub interfaces: rend3_pbr::common::interfaces::ShaderInterfaces,
    pub cpu_culler: rend3_pbr::culling::cpu::CpuCuller,
    pub gpu_culler: rend3::ModeData<(), rend3_pbr::culling::gpu::GpuCuller>,
    pub shadow_passes: rend3_pbr::directional::DirectionalShadowPass,
    pub forward_opaque_pass: rend3_pbr::forward::ForwardPass,
    pub forward_cutout_pass: rend3_pbr::forward::ForwardPass,
    pub samplers: rend3_pbr::common::samplers::Samplers,
}

impl PbrBakerRenderRoutine {
    pub fn new(renderer: &Renderer) -> Self {
        let interfaces = rend3_pbr::common::interfaces::ShaderInterfaces::new(&renderer.device);

        let directional_light = renderer.directional_light_manager.read();
        let d2_texture_manager = renderer.d2_texture_manager.read();
        let material_manager = renderer.material_manager.read();

        let directional_light_bgl = directional_light.get_bgl();
        let texture_bgl = renderer.mode.into_data(|| (), || d2_texture_manager.gpu_bgl());
        let pipeline_desc = rend3_pbr::common::forward_pass::BuildForwardPassShaderArgs {
            mode: renderer.mode,
            device: &renderer.device,
            interfaces: &interfaces,
            directional_light_bgl,
            texture_bgl,
            materials: &material_manager,
            samples: rend3_pbr::SampleCount::One,
            transparency: rend3_pbr::material::TransparencyType::Opaque,
            baking: rend3_pbr::common::forward_pass::Baking::Enabled,
        };
        let opaque_pipeline = Arc::new(rend3_pbr::common::forward_pass::build_forward_pass_pipeline(
            pipeline_desc.clone(),
        ));
        let cutout_pipeline = Arc::new(rend3_pbr::common::forward_pass::build_forward_pass_pipeline(
            rend3_pbr::common::forward_pass::BuildForwardPassShaderArgs {
                transparency: rend3_pbr::material::TransparencyType::Opaque,
                ..pipeline_desc
            },
        ));

        let shadow_pipelines = rend3_pbr::common::depth_pass::build_depth_pass_pipeline(
            rend3_pbr::common::depth_pass::BuildDepthPassShaderArgs {
                mode: renderer.mode,
                device: &renderer.device,
                interfaces: &interfaces,
                texture_bgl,
                materials: &material_manager,
                samples: rend3_pbr::SampleCount::One,
                ty: rend3_pbr::common::depth_pass::DepthPassType::Shadow,
            },
        );
        let shadow_passes =
            rend3_pbr::directional::DirectionalShadowPass::new(shadow_pipelines.cutout, shadow_pipelines.opaque);

        let forward_opaque_pass =
            rend3_pbr::forward::ForwardPass::new(None, opaque_pipeline, rend3_pbr::material::TransparencyType::Opaque);

        let forward_cutout_pass =
            rend3_pbr::forward::ForwardPass::new(None, cutout_pipeline, rend3_pbr::material::TransparencyType::Cutout);

        let samplers =
            rend3_pbr::common::samplers::Samplers::new(&renderer.device, renderer.mode, &interfaces.samplers_bgl);

        let cpu_culler = rend3_pbr::culling::cpu::CpuCuller::new();
        let gpu_culler = renderer
            .mode
            .into_data(|| (), || rend3_pbr::culling::gpu::GpuCuller::new(&renderer.device));

        Self {
            interfaces,
            forward_opaque_pass,
            forward_cutout_pass,
            cpu_culler,
            gpu_culler,
            shadow_passes,
            samplers,
        }
    }
}

impl PbrBakerRenderRoutine {
    pub fn render(
        &mut self,
        renderer: Arc<Renderer>,
        encoders: flume::Sender<wgpu::CommandBuffer>,
        ready: ReadyData,
        input: Vec<BakeData>,
        output: PbrBakerOutput,
    ) {
        profiling::scope!("PBR Render Routine");

        let mut encoder = renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("cull encoder"),
        });

        let mut profiler = renderer.profiler.lock();

        let mesh_manager = renderer.mesh_manager.read();
        let directional_light = renderer.directional_light_manager.read();
        let materials = renderer.material_manager.read();
        let camera_manager = renderer.camera_manager.read();
        let mut object_manager = renderer.object_manager.write();

        let culler = self.gpu_culler.as_ref().map_cpu(|_| &self.cpu_culler);

        let mut culling_input_opaque = culler.map(
            |_| (),
            |culler| {
                culler.pre_cull(rend3_pbr::culling::gpu::GpuCullerPreCullArgs {
                    device: &renderer.device,
                    camera: &camera_manager,
                    objects: &mut object_manager,
                    transparency: rend3_pbr::material::TransparencyType::Opaque,
                    sort: None,
                })
            },
        );
        let mut culling_input_cutout = culler.map(
            |_| (),
            |culler| {
                culler.pre_cull(rend3_pbr::culling::gpu::GpuCullerPreCullArgs {
                    device: &renderer.device,
                    camera: &camera_manager,
                    objects: &mut object_manager,
                    transparency: rend3_pbr::material::TransparencyType::Cutout,
                    sort: None,
                })
            },
        );

        let culled_lights =
            self.shadow_passes
                .cull_shadows(rend3_pbr::directional::DirectionalShadowPassCullShadowsArgs {
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

        let d2_texture_output_bg_ref = ready.d2_texture.bg.as_ref().map(|_| (), |a| &**a);

        self.shadow_passes
            .draw_culled_shadows(rend3_pbr::directional::DirectionalShadowPassDrawCulledShadowsArgs {
                device: &renderer.device,
                profiler: &mut profiler,
                encoder: &mut encoder,
                materials: &materials,
                meshes: mesh_manager.buffers(),
                samplers: &self.samplers,
                texture_bg: d2_texture_output_bg_ref,
                culled_lights: &culled_lights,
            });

        let primary_camera_uniform_bg =
            rend3_pbr::uniforms::create_shader_uniform(rend3_pbr::uniforms::CreateShaderUniformArgs {
                device: &renderer.device,
                camera: &camera_manager,
                interfaces: &self.interfaces,
                ambient: Vec4::new(0.0, 0.0, 0.0, 1.0),
            });
        let _object_manager = renderer.object_manager.read();

        let culled_objects: Vec<_> = input
            .into_iter()
            .map(|_object| {
                // let the_object = object_manager.get(object.object.get_raw());

                // let cutout_culled_objects = self.forward_opaque_pass.cull(rend3_pbr::forward::ForwardPassCullArgs {
                //     device: &renderer.device,
                //     profiler: &mut profiler,
                //     encoder: &mut encoder,
                //     culler,
                //     materials: &materials,
                //     interfaces: &self.interfaces,
                //     camera: &camera_manager,
                //     objects: std::slice::from_ref(the_object),
                // });

                // let opaque_culled_objects = self.forward_cutout_pass.cull(rend3_pbr::forward::ForwardPassCullArgs {
                //     device: &renderer.device,
                //     profiler: &mut profiler,
                //     encoder: &mut encoder,
                //     culler,
                //     materials: &materials,
                //     interfaces: &self.interfaces,
                //     camera: &camera_manager,
                //     objects: std::slice::from_ref(the_object),
                // });

                // (cutout_culled_objects, opaque_culled_objects)

                // TODO(material): figure out how to query a sinlg ething
                todo!()
            })
            .collect();

        profiler.begin_scope("primary renderpass", &mut encoder, &renderer.device);

        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: None,
            color_attachments: &[RenderPassColorAttachment {
                view: output.view,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: true,
                },
            }],
            depth_stencil_attachment: None,
        });

        for (cutout_culled_objects, opaque_culled_objects) in &culled_objects {
            profiling::scope!("primary renderpass");

            profiler.begin_scope("forward", &mut rpass, &renderer.device);

            self.forward_opaque_pass.draw(rend3_pbr::forward::ForwardPassDrawArgs {
                device: &renderer.device,
                profiler: &mut profiler,
                rpass: &mut rpass,
                materials: &materials,
                meshes: mesh_manager.buffers(),
                samplers: &self.samplers,
                directional_light_bg: directional_light.get_bg(),
                texture_bg: d2_texture_output_bg_ref,
                shader_uniform_bg: &primary_camera_uniform_bg,
                culled_objects: opaque_culled_objects,
            });

            self.forward_cutout_pass.draw(rend3_pbr::forward::ForwardPassDrawArgs {
                device: &renderer.device,
                profiler: &mut profiler,
                rpass: &mut rpass,
                materials: &materials,
                meshes: mesh_manager.buffers(),
                samplers: &self.samplers,
                directional_light_bg: directional_light.get_bg(),
                texture_bg: d2_texture_output_bg_ref,
                shader_uniform_bg: &primary_camera_uniform_bg,
                culled_objects: cutout_culled_objects,
            });
        }

        profiler.end_scope(&mut rpass);

        drop(rpass);
        profiler.end_scope(&mut encoder);

        encoders.send(encoder.finish()).unwrap();
    }
}
