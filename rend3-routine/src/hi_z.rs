use std::borrow::Cow;

use glam::UVec2;
use rend3::{
    graph::{
        DeclaredDependency, NodeExecutionContext, NodeResourceUsage, RenderGraph, RenderPassDepthTarget,
        RenderPassHandle, RenderPassTargets, RenderTargetHandle, ViewportRect,
    },
    Renderer, ShaderPreProcessor,
};
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingResource, BindingType, CompareFunction, DepthBiasState, DepthStencilState, Extent3d, FragmentState,
    MultisampleState, PipelineLayoutDescriptor, PrimitiveState, RenderPipeline, RenderPipelineDescriptor,
    ShaderModuleDescriptor, ShaderStages, StencilState, TextureDimension, TextureFormat, TextureSampleType,
    TextureViewDimension, VertexState,
};

pub struct HiZRoutine {
    texture_bgl: BindGroupLayout,
    pipeline: RenderPipeline,
}

impl HiZRoutine {
    pub fn new(renderer: &Renderer, spp: &ShaderPreProcessor) -> Self {
        let shader_source = spp.render_shader("rend3-routine/hi_z.wgsl", &(), None).unwrap();

        let shader_module = renderer.device.create_shader_module(ShaderModuleDescriptor {
            label: Some("HiZ Downscaler"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(shader_source)),
        });

        let texture_bgl = renderer.device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("HiZ Bind Group Layout"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX_FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Depth,
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });

        let pipeline_layout = renderer.device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("HiZ Pipeline Layout"),
            bind_group_layouts: &[&texture_bgl],
            push_constant_ranges: &[],
        });

        let pipeline = renderer.device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("HiZ Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader_module,
                entry_point: "vs_main",
                buffers: &[],
            },
            primitive: PrimitiveState::default(),
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: CompareFunction::Always,
                stencil: StencilState::default(),
                bias: DepthBiasState::default(),
            }),
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &shader_module,
                entry_point: "fs_main",
                targets: &[],
            }),
            multiview: None,
        });

        Self { texture_bgl, pipeline }
    }

    pub fn downscale<'pass>(
        &'pass self,
        mut ctx: NodeExecutionContext<'_, 'pass, '_>,
        renderpass_handle: DeclaredDependency<RenderPassHandle>,
        source_handle: DeclaredDependency<RenderTargetHandle>,
    ) {
        let rpass = ctx.encoder_or_pass.take_rpass(renderpass_handle);
        let source = ctx.graph_data.get_render_target(source_handle);

        let bind_group = ctx
            .temps
            .add(ctx.renderer.device.create_bind_group(&BindGroupDescriptor {
                label: Some("HiZ Bind Group Layout"),
                layout: &self.texture_bgl,
                entries: &[BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&source),
                }],
            }));

        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, bind_group, &[]);
        rpass.draw(0..3, 0..1);
    }

    pub fn add_hi_z_to_graph<'node>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        depth_target: RenderTargetHandle,
        resolution: UVec2,
    ) {
        let extent = Extent3d {
            width: resolution.x,
            height: resolution.y,
            depth_or_array_layers: 1,
        };
        let mips = extent.max_mips(TextureDimension::D2) as u8;

        for dst_mip in 1..mips {
            let src_mip = dst_mip - 1;

            let mut node = graph.add_node(&format!("HiZ Mip {src_mip} -> {dst_mip}"));

            let dst_extent = extent.mip_level_size(dst_mip as u32, false);
            let src_extent = extent.mip_level_size(src_mip as u32, false);

            let dst_target = node.add_render_target(
                depth_target
                    .set_mips(dst_mip..dst_mip + 1)
                    .set_viewport(ViewportRect::from_size(UVec2::new(dst_extent.width, dst_extent.height))),
                NodeResourceUsage::Output,
            );
            let src_target = node.add_render_target(
                depth_target
                    .set_mips(src_mip..src_mip + 1)
                    .set_viewport(ViewportRect::from_size(UVec2::new(src_extent.width, src_extent.height))),
                NodeResourceUsage::Input,
            );

            let rpass_handle = node.add_renderpass(RenderPassTargets {
                targets: vec![],
                depth_stencil: Some(RenderPassDepthTarget {
                    target: dst_target,
                    depth_clear: Some(0.0),
                    stencil_clear: None,
                }),
            });

            node.add_side_effect();

            node.build(move |ctx| {
                self.downscale(ctx, rpass_handle, src_target);
            });
        }
    }
}
