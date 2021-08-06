use std::sync::Arc;

use crate::routines::*;
use crate::ModeData;
use crate::RenderRoutine;
use crate::Renderer;

pub struct DefaultRenderRoutine {
    pub interfaces: common::interfaces::ShaderInterfaces,
    pub samplers: common::samplers::Samplers,
    pub cpu_culler: culling::cpu::CpuCuller,
    pub gpu_culler: ModeData<(), culling::gpu::GpuCuller>,
    pub shadow_passes: directional::DirectionalShadowPass,
    pub depth_prepass: prepass::DepthPrepass,
}

impl DefaultRenderRoutine {
    pub fn new(renderer: &Renderer) -> Self {
        let device = renderer.device();
        let mode = renderer.mode();
        let interfaces = common::interfaces::ShaderInterfaces::new(device);

        let samplers = common::samplers::Samplers::new(device, &interfaces.samplers_bgl);

        let cpu_culler = culling::cpu::CpuCuller::new();
        let gpu_culler = mode.into_data(|| (), || culling::gpu::GpuCuller::new(device));

        let gpu_texture_manager_guard = mode.into_data(|| (), || renderer.d2_texture_manager.read());
        let depth_pipeline = Arc::new(common::depth_pass::build_depth_pass_shader(
            common::depth_pass::BuildDepthPassShaderArgs {
                mode,
                device,
                interfaces: &interfaces,
                texture_bgl: gpu_texture_manager_guard.as_ref().map(|_| (), |guard| guard.gpu_bgl()),
                materials: &renderer.material_manager.read(),
            },
        ));
        let shadow_passes = directional::DirectionalShadowPass::new(Arc::clone(&depth_pipeline));
        let depth_prepass = prepass::DepthPrepass::new(Arc::clone(&depth_pipeline));

        Self {
            interfaces,
            samplers,
            cpu_culler,
            gpu_culler,
            shadow_passes,
            depth_prepass,
        }
    }
}

impl<TLD: 'static> RenderRoutine<TLD> for DefaultRenderRoutine {
    fn render(&self, renderer: Arc<Renderer<TLD>>, frame: crate::util::output::OutputFrame) {
        let mut encoder = renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("primary encoder"),
        });

        let directional_light = renderer.directional_light_manager.read();
        let materials = renderer.material_manager.read();
        let mut d2_textures = renderer.d2_texture_manager.write();
        let mut d2c_textures = renderer.d2c_texture_manager.write();

        let d2_texture_output = d2_textures.ready(&renderer.device);
        let d2c_texture_output = d2c_textures.ready(&renderer.device);
        let objects = renderer.object_manager.read().ready();

        let culled_lights = self
            .shadow_passes
            .cull_shadows(directional::DirectionalShadowPassCullShadowsArgs {
                device: &renderer.device,
                encoder: &mut encoder,
                culler: self.gpu_culler.as_ref().map_cpu(|_| &self.cpu_culler),
                materials: &materials,
                interfaces: &self.interfaces,
                lights: &directional_light,
                objects: &objects,
            });

        self.shadow_passes.draw_culled_shadows(directional::DirectionalShadowPassDrawCulledShadowsArgs {
            encoder: &mut encoder,
            materials: &materials,
            sampler_bg: &self.samplers.bg,
            texture_bg: d2_texture_output.bg.as_ref().map(|_|(), |a| &**a),
            culled_lights: &culled_lights,
        })
    }
}
