//! Builds and uploads the input to gpu culling.

use rend3::{
    format_sso,
    graph::{DataHandle, RenderGraph},
    types::Material,
};
use wgpu::Buffer;

use crate::{
    common::{self, Sorting},
    culling,
};

/// Uploads the input to gpu culling for the given material archetype.
pub fn add_to_graph<'node, M: Material>(
    graph: &mut RenderGraph<'node>,
    key: u64,
    sorting: Option<Sorting>,
    name: &str,
    pre_cull_data: DataHandle<Buffer>,
) {
    let mut builder = graph.add_node(format_sso!("pre-cull {:?}", name));
    let data_handle = builder.add_data_output(pre_cull_data);

    builder.build(move |_pt, renderer, _encoder_or_pass, _temps, _ready, graph_data| {
        let objects = graph_data.object_manager.get_objects::<M>(key);
        let objects = common::sort_objects(objects, graph_data.camera_manager, sorting);
        let buffer = culling::build_gpu_cull_input(&renderer.device, &objects);
        graph_data.set_data::<Buffer>(data_handle, Some(buffer));
    });
}
