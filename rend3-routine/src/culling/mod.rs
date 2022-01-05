use rend3::{
    format_sso, types::Material, util::bind_merge::BindGroupBuilder, DataHandle, ModeData, RenderGraph, RendererMode,
};
use wgpu::{BindGroup, Buffer};

use crate::common::{PerMaterialInterfaces, Sorting};

mod cpu;
mod gpu;

pub use cpu::*;
pub use gpu::*;

pub struct PerMaterialData {
    pub inner: CulledObjectSet,
    pub per_material: BindGroup,
}

pub struct CulledObjectSet {
    pub calls: ModeData<Vec<cpu::CpuDrawCall>, gpu::GpuIndirectData>,
    pub output_buffer: Buffer,
}

#[allow(clippy::too_many_arguments)]
pub fn add_culling_to_graph<'node, M: Material>(
    graph: &mut RenderGraph<'node>,
    pre_cull_data: DataHandle<Buffer>,
    culled: DataHandle<PerMaterialData>,
    per_material: &'node PerMaterialInterfaces<M>,
    gpu_culler: &'node ModeData<(), gpu::GpuCuller>,
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
            ModeData::CPU(_) => cpu::cull_cpu::<M>(&renderer.device, camera, graph_data.object_manager, sorting, key),
            ModeData::GPU(ref gpu_culler) => gpu_culler.cull(gpu::GpuCullerCullArgs {
                device: &renderer.device,
                encoder,
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

        let per_material_bg = per_material_bgb.build(&renderer.device, None, &per_material.bgl);

        graph_data.set_data(
            cull_handle,
            Some(PerMaterialData {
                inner: culled_objects,
                per_material: per_material_bg,
            }),
        );
    });
}
