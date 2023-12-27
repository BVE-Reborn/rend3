use rend3::graph::{NodeResourceUsage, RenderGraph, RenderPassDepthTarget, RenderPassTargets, RenderTargetHandle};

/// Due to limitations of how we auto-clear buffers, we need to explicitly clear the shadow depth buffer.
pub fn add_depth_clear_to_graph(graph: &mut RenderGraph<'_>, depth: RenderTargetHandle, depth_clear: f32) {
    let mut builder = graph.add_node("Clear Depth");

    let _rpass_handle = builder.add_renderpass(
        RenderPassTargets {
            targets: vec![],
            depth_stencil: Some(RenderPassDepthTarget {
                target: depth,
                depth_clear: Some(depth_clear),
                stencil_clear: None,
            }),
        },
        NodeResourceUsage::Output,
    );

    builder.build(|_| ())
}
