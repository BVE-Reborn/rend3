use std::sync::Arc;

use crate::routines::*;
use crate::ModeData;
use crate::RenderRoutine;
use crate::Renderer;

pub struct DefaultRenderRoutine {
    pub interfaces: common::interfaces::ShaderInterfaces,
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

        let cpu_culler = culling::cpu::CpuCuller::new();
        let gpu_culler = mode.into_data(|| (), || culling::gpu::GpuCuller::new(device));

        let gpu_texture_manager_guard = mode.into_data(|| (), || renderer.texture_manager_2d.read());
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
            cpu_culler,
            gpu_culler,
            shadow_passes,
            depth_prepass,
        }
    }
}

impl<TLD: 'static> RenderRoutine<TLD> for DefaultRenderRoutine {
    fn render(&self, context: Arc<Renderer<TLD>>, frame: crate::util::output::OutputFrame) {}
}
