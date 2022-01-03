use rend3::{
    format_sso, types::Material, util::bind_merge::BindGroupBuilder, DataHandle, ModeData, RenderGraph, RendererMode,
};
use wgpu::Buffer;

use crate::{common::interfaces::ShaderInterfaces, culling::gpu::GpuCuller, CulledPerMaterial};

pub mod cpu;
pub mod gpu;

pub struct CulledObjectSet {
    pub calls: ModeData<Vec<CPUDrawCall>, GPUIndirectData>,
    pub output_buffer: Buffer,
}

pub struct GPUIndirectData {
    pub indirect_buffer: Buffer,
    pub count: usize,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Sorting {
    FrontToBack,
    BackToFront,
}

#[derive(Debug, Clone)]
pub struct CPUDrawCall {
    pub start_idx: u32,
    pub end_idx: u32,
    pub vertex_offset: i32,
    pub material_index: u32,
}

#[allow(clippy::too_many_arguments)]
pub fn add_culling_to_graph<'node, M: Material>(
    graph: &mut RenderGraph<'node>,
    pre_cull_data: DataHandle<Buffer>,
    culled: DataHandle<CulledPerMaterial>,
    interfaces: &'node ShaderInterfaces,
    gpu_culler: &'node ModeData<(), GpuCuller>,
    shadow_index: Option<usize>,
    key: u64,
    sorting: Option<Sorting>,
    name: &str,
) {
    let mut builder = graph.add_node(format_sso!("Culling {}", name));

    let pre_cull_handle = gpu_culler
        .mode()
        .into_data(|| (), || builder.add_data_input(pre_cull_data));
    let cull_handle = builder.add_data_output(culled);

    builder.build(move |_pt, renderer, encoder_or_rpass, temps, ready, graph_data| {
        let encoder = encoder_or_rpass.get_encoder();

        let culling_input = pre_cull_handle.map_gpu(|handle| graph_data.get_data::<Buffer>(temps, handle).unwrap());

        let count = graph_data.object_manager.get_objects::<M>(key).len();

        let camera = match shadow_index {
            Some(idx) => &ready.directional_light_cameras[idx],
            None => graph_data.camera_manager,
        };

        let culled_objects = match gpu_culler {
            ModeData::CPU(_) => cpu::cull::<M>(&renderer.device, camera, graph_data.object_manager, sorting, key),
            ModeData::GPU(ref gpu_culler) => gpu_culler.cull(gpu::GpuCullerCullArgs {
                device: &renderer.device,
                encoder,
                interfaces,
                camera,
                input_buffer: culling_input.into_gpu(),
                input_count: count,
                sorting,
                key,
            }),
        };

        let mut per_material_bgb = BindGroupBuilder::new();
        per_material_bgb.append_buffer(&culled_objects.output_buffer);

        if renderer.mode == RendererMode::GPUPowered {
            graph_data.material_manager.add_to_bg_gpu::<M>(&mut per_material_bgb);
        }

        let per_material_bg = per_material_bgb.build(&renderer.device, None, &interfaces.per_material_bgl);

        graph_data.set_data(
            cull_handle,
            Some(CulledPerMaterial {
                inner: culled_objects,
                per_material: per_material_bg,
            }),
        );
    });
}
