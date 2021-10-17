use std::sync::Arc;

use rend3::{ReadyData, Renderer};
use wgpu::TextureView;

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
        _renderer: Arc<Renderer>,
        _encoders: flume::Sender<wgpu::CommandBuffer>,
        _ready: ReadyData,
        _input: Vec<BakeData>,
        _output: PbrBakerOutput,
    ) {
        todo!()
    }
}
