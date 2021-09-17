use std::sync::Arc;

use glam::Vec4;
use rend3::Renderer;
use wgpu::{Color, LoadOp, Operations, RenderPassColorAttachment, RenderPassDescriptor};

pub struct PbrBakerOutput {}

pub struct BakeData {
    pub object: rend3::types::ObjectHandle,
    pub transform: glam::Mat3A,
}

pub struct PbrBakerRenderRoutine {
    pub interfaces: rend3_pbr::common::interfaces::ShaderInterfaces,
    pub cpu_culler: rend3_pbr::culling::cpu::CpuCuller,
    pub gpu_culler: rend3::ModeData<(), rend3_pbr::culling::gpu::GpuCuller>,
    pub shadow_passes: rend3_pbr::directional::DirectionalShadowPass,
    pub forward_opaque_pass: rend3_pbr::forward::ForwardPass,
    pub forward_cutout_pass: rend3_pbr::forward::ForwardPass,
    pub samplers: rend3_pbr::common::samplers::Samplers,

    pub bake_data: Vec<BakeData>,
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
            samples: rend3_pbr::SampleCount::Four,
            transparency: rend3::types::TransparencyType::Opaque,
            baking: rend3_pbr::common::forward_pass::Baking::Enabled,
        };
        let opaque_pipeline = Arc::new(rend3_pbr::common::forward_pass::build_forward_pass_shader(
            pipeline_desc.clone(),
        ));
        let cutout_pipeline = Arc::new(rend3_pbr::common::forward_pass::build_forward_pass_shader(
            rend3_pbr::common::forward_pass::BuildForwardPassShaderArgs {
                transparency: rend3::types::TransparencyType::Opaque,
                ..pipeline_desc
            },
        ));

        let shadow_pipelines = rend3_pbr::common::depth_pass::build_depth_pass_shader(
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
            rend3_pbr::forward::ForwardPass::new(None, opaque_pipeline, rend3::types::TransparencyType::Opaque);

        let forward_cutout_pass =
            rend3_pbr::forward::ForwardPass::new(None, cutout_pipeline, rend3::types::TransparencyType::Cutout);

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

impl rend3::RenderRoutine<PbrBakerOutput> for PbrBakerRenderRoutine {
    fn render(
        &mut self,
        renderer: Arc<Renderer>,
        encoders: &mut Vec<wgpu::CommandBuffer>,
        frame: &rend3::util::output::OutputFrame<PbrBakerOutput>,
    ) {
        profiling::scope!("PBR Render Routine");

        let mut encoder = renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("primary encoder"),
        });

        let mut profiler = renderer.profiler.lock();

        let mut mesh_manager = renderer.mesh_manager.write();
        let mut object_manager = renderer.object_manager.write();
        let mut directional_light = renderer.directional_light_manager.write();
        let mut materials = renderer.material_manager.write();
        let mut d2_textures = renderer.d2_texture_manager.write();
        let mut d2c_textures = renderer.d2c_texture_manager.write();
        let camera_manager = renderer.camera_manager.read();

        // Do these in dependency order
        // Level 2
        let objects = object_manager.ready();

        // Level 1
        materials.ready(&renderer.device, &renderer.queue, &d2_textures);

        // Level 0
        let d2_texture_output = d2_textures.ready(&renderer.device);
        let _d2c_texture_output = d2c_textures.ready(&renderer.device);
        let directional_light_cameras = directional_light.ready(&renderer.device, &renderer.queue, &camera_manager);
        mesh_manager.ready();

        let culler = self.gpu_culler.as_ref().map_cpu(|_| &self.cpu_culler);

        let culled_lights =
            self.shadow_passes
                .cull_shadows(rend3_pbr::directional::DirectionalShadowPassCullShadowsArgs {
                    device: &renderer.device,
                    profiler: &mut profiler,
                    encoder: &mut encoder,
                    culler,
                    materials: &materials,
                    interfaces: &self.interfaces,
                    lights: &directional_light,
                    directional_light_cameras: &directional_light_cameras,
                    objects: &objects,
                });

        let d2_texture_output_bg_ref = d2_texture_output.bg.as_ref().map(|_| (), |a| &**a);

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

        let cutout_culled_objects = self.forward_opaque_pass.cull(rend3_pbr::forward::ForwardPassCullArgs {
            device: &renderer.device,
            profiler: &mut profiler,
            encoder: &mut encoder,
            culler,
            materials: &materials,
            interfaces: &self.interfaces,
            camera: &camera_manager,
            objects: &objects,
        });

        let opaque_culled_objects = self.forward_cutout_pass.cull(rend3_pbr::forward::ForwardPassCullArgs {
            device: &renderer.device,
            profiler: &mut profiler,
            encoder: &mut encoder,
            culler,
            materials: &materials,
            interfaces: &self.interfaces,
            camera: &camera_manager,
            objects: &objects,
        });

        let primary_camera_uniform_bg =
            rend3_pbr::uniforms::create_shader_uniform(rend3_pbr::uniforms::CreateShaderUniformArgs {
                device: &renderer.device,
                camera: &camera_manager,
                interfaces: &self.interfaces,
                ambient: Vec4::new(0.0, 0.0, 0.0, 1.0),
            });

        {
            profiling::scope!("primary renderpass");
            profiler.begin_scope("primary renderpass", &mut encoder, &renderer.device);

            let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: None,
                color_attachments: &[RenderPassColorAttachment {
                    view: &frame.as_view().unwrap(),
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });

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
                culled_objects: &opaque_culled_objects,
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
                culled_objects: &cutout_culled_objects,
            });

            profiler.end_scope(&mut rpass);

            drop(rpass);
            profiler.end_scope(&mut encoder);
        }

        encoders.push(encoder.finish());
    }
}
