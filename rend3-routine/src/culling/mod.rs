//! Material agnostic culling on either the CPU or GPU.

use rend3::{
    format_sso,
    graph::{DataHandle, RenderGraph},
    types::Material,
    util::bind_merge::BindGroupBuilder,
    ProfileData, RendererProfile,
};
use wgpu::{BindGroup, Buffer};

use crate::{
    common::{PerMaterialArchetypeInterface, Sorting},
    skinning::SkinningOutput,
};

mod cpu;
mod gpu;

pub use cpu::*;
pub use gpu::*;

/// Handles to the data that corresponds with a single material archetype.
pub struct PerMaterialArchetypeData {
    pub inner: CulledObjectSet,
    pub per_material: BindGroup,
}

/// A set of objects that have been called. Contains the information needed to
/// dispatch a render.
pub struct CulledObjectSet {
    pub calls: ProfileData<Vec<cpu::CpuDrawCall>, gpu::GpuIndirectData>,
    pub output_buffer: Buffer,
}

/// Add the profile-approprate culling for the given material archetype to the
/// graph.
#[allow(clippy::too_many_arguments)]
pub fn add_culling_to_graph<'node, M: Material>(
    graph: &mut RenderGraph<'node>,
    pre_cull_data: DataHandle<Buffer>,
    culled: DataHandle<PerMaterialArchetypeData>,
    skinned: DataHandle<SkinningOutput>,
    per_material: &'node PerMaterialArchetypeInterface<M>,
    gpu_culler: &'node ProfileData<(), gpu::GpuCuller>,
    shadow_index: Option<usize>,
    key: u64,
    sorting: Option<Sorting>,
    name: &str,
) {
    let mut builder = graph.add_node(format_sso!("Culling {}", name));

    let pre_cull_handle = gpu_culler
        .profile()
        .into_data(|| (), || builder.add_data_input(pre_cull_data));
    let cull_handle = builder.add_data_output(culled);

    // Just connect the input, we don't need its value.
    builder.add_data_input(skinned);

    builder.build(move |_pt, renderer, encoder_or_rpass, temps, ready, graph_data| {
        let encoder = encoder_or_rpass.get_encoder();

        let culling_input = pre_cull_handle.map_gpu(|handle| graph_data.get_data::<Buffer>(temps, handle).unwrap());

        let count = graph_data.object_manager.get_objects::<M>(key).len();

        let camera = match shadow_index {
            Some(idx) => &ready.directional_light_cameras[idx],
            None => graph_data.camera_manager,
        };

        let culled_objects = match gpu_culler {
            ProfileData::Cpu(_) => {
                cpu::cull_cpu::<M>(&renderer.device, camera, graph_data.object_manager, sorting, key)
            }
            ProfileData::Gpu(ref gpu_culler) => gpu_culler.cull(
                &renderer.device,
                encoder,
                camera,
                culling_input.into_gpu(),
                count,
                sorting,
            ),
        };

        let mut per_material_bgb = BindGroupBuilder::new();
        per_material_bgb.append_buffer(&culled_objects.output_buffer);

        if renderer.profile == RendererProfile::GpuDriven {
            graph_data.material_manager.add_to_bg_gpu::<M>(&mut per_material_bgb);
        }

        let per_material_bg = per_material_bgb.build(&renderer.device, None, &per_material.bgl);

        graph_data.set_data(
            cull_handle,
            Some(PerMaterialArchetypeData {
                inner: culled_objects,
                per_material: per_material_bg,
            }),
        );
    });
}
