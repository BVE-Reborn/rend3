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

use crate::base::DepthTargets;

pub struct HiZRoutine {
    multisampled_bgl: BindGroupLayout,
    single_sampled_bgl: BindGroupLayout,
    downscale_pipeline: RenderPipeline,
    resolve_pipeline: RenderPipeline,
}

impl HiZRoutine {
    pub fn new(renderer: &Renderer, spp: &ShaderPreProcessor) -> Self {
        let resolve_source = spp
            .render_shader(
                "rend3-routine/resolve_depth_min.wgsl",
                &serde_json::json!({"SAMPLES": 4}),
                None,
            )
            .unwrap();
        let downscale_source = spp.render_shader("rend3-routine/hi_z.wgsl", &(), None).unwrap();

        let resolve_sm = renderer.device.create_shader_module(ShaderModuleDescriptor {
            label: Some("HiZ Resolver"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(resolve_source)),
        });
        let downscale_sm = renderer.device.create_shader_module(ShaderModuleDescriptor {
            label: Some("HiZ Downscaler"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(downscale_source)),
        });

        let multisampled_bgl = renderer.device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Multi Sample HiZ Texture BGL"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX_FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Depth,
                    view_dimension: TextureViewDimension::D2,
                    multisampled: true,
                },
                count: None,
            }],
        });

        let single_sampled_bgl = renderer.device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Single Sample HiZ Texture BGL"),
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

        let resolve_pipline_layout = renderer.device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("HiZ Resolve PLL"),
            bind_group_layouts: &[&multisampled_bgl],
            push_constant_ranges: &[],
        });

        let downscale_pipline_layout = renderer.device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("HiZ Downscale PLL"),
            bind_group_layouts: &[&single_sampled_bgl],
            push_constant_ranges: &[],
        });

        let resolve_pipeline = renderer.device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("HiZ Resolve Pipeline"),
            layout: Some(&resolve_pipline_layout),
            vertex: VertexState {
                module: &resolve_sm,
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
                module: &resolve_sm,
                entry_point: "fs_main",
                targets: &[],
            }),
            multiview: None,
        });

        let downscale_pipeline = renderer.device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("HiZ Downscale Pipeline"),
            layout: Some(&downscale_pipline_layout),
            vertex: VertexState {
                module: &downscale_sm,
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
                module: &downscale_sm,
                entry_point: "fs_main",
                targets: &[],
            }),
            multiview: None,
        });

        Self {
            single_sampled_bgl,
            downscale_pipeline,
            multisampled_bgl,
            resolve_pipeline,
        }
    }

    pub fn resolve<'pass>(
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
                label: Some("HiZ Resolve BG"),
                layout: &self.multisampled_bgl,
                entries: &[BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(source),
                }],
            }));

        rpass.set_pipeline(&self.resolve_pipeline);
        rpass.set_bind_group(0, bind_group, &[]);
        rpass.draw(0..3, 0..1);
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
                layout: &self.single_sampled_bgl,
                entries: &[BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(source),
                }],
            }));

        rpass.set_pipeline(&self.downscale_pipeline);
        rpass.set_bind_group(0, bind_group, &[]);
        rpass.draw(0..3, 0..1);
    }

    pub fn add_hi_z_to_graph<'node>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        depth_targets: DepthTargets,
        resolution: UVec2,
    ) {
        let extent = Extent3d {
            width: resolution.x,
            height: resolution.y,
            depth_or_array_layers: 1,
        };
        let mips = extent.max_mips(TextureDimension::D2) as u8;

        // First we need to downscale the depth buffer to a single sample texture
        // if we are doing multisampling.
        if let Some(multi_sample) = depth_targets.multi_sample {
            let mut node = graph.add_node("HiZ Resolve");

            let target = node.add_render_target(
                depth_targets.single_sample_mipped.set_mips(0..1),
                NodeResourceUsage::Output,
            );

            let source = node.add_render_target(multi_sample, NodeResourceUsage::Output);

            let rpass_handle = node.add_renderpass(RenderPassTargets {
                targets: vec![],
                depth_stencil: Some(RenderPassDepthTarget {
                    target,
                    depth_clear: Some(0.0),
                    stencil_clear: None,
                }),
            });

            node.add_side_effect();

            node.build(move |ctx| {
                self.resolve(ctx, rpass_handle, source);
            });
        }

        for dst_mip in 1..mips {
            let src_mip = dst_mip - 1;

            let mut node = graph.add_node(&format!("HiZ Mip {src_mip} -> {dst_mip}"));

            let dst_extent = extent.mip_level_size(dst_mip as u32, TextureDimension::D2);
            let src_extent = extent.mip_level_size(src_mip as u32, TextureDimension::D2);

            let dst_target = node.add_render_target(
                depth_targets
                    .single_sample_mipped
                    .set_mips(dst_mip..dst_mip + 1)
                    .set_viewport(ViewportRect::from_size(UVec2::new(dst_extent.width, dst_extent.height))),
                NodeResourceUsage::Output,
            );
            let src_target = node.add_render_target(
                depth_targets
                    .single_sample_mipped
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
