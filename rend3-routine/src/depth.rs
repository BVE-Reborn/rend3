use std::{mem, num::NonZeroU64};

use arrayvec::ArrayVec;
use rend3::{
    format_sso,
    types::{Handedness, Material, SampleCount},
    util::{
        bind_merge::{BindGroupBuilder, BindGroupLayoutBuilder},
        math::round_up_pot,
    },
    DataHandle, DepthHandle, ModeData, RenderGraph, RenderPassDepthTarget, RenderPassTarget, RenderPassTargets,
    RenderTargetHandle, Renderer, RendererDataCore, RendererMode,
};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    BindGroup, BindGroupLayout, BufferUsages, Color, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState,
    DepthStencilState, Face, FragmentState, FrontFace, MultisampleState, PipelineLayoutDescriptor, PolygonMode,
    PrimitiveState, PrimitiveTopology, RenderPipeline, RenderPipelineDescriptor, ShaderStages, StencilState,
    TextureFormat, VertexState,
};

use crate::{
    common::{interfaces::ShaderInterfaces, shaders::mode_safe_shader},
    culling,
    material::PbrMaterial,
    vertex::{cpu_vertex_buffers, gpu_vertex_buffers},
    CulledPerMaterial,
};

/// Trait for all materials that can use the built-in shadow/prepass rendering.
pub trait DepthRenderableMaterial: Material {
    /// If Some it is possible to do alpha cutouts
    const ALPHA_CUTOUT: Option<AlphaCutoutSpec>;
}

/// How the material should be read for alpha cutouting.
pub struct AlphaCutoutSpec {
    /// Index into the texture array to read the alpha from. Currently _must_ be 0. This will be lifted.
    pub index: u32,
    /// Byte index into the data array that represents a single f32 to use as the alpha cutout value.
    pub cutoff_offset: u32,
    /// Byte index into the data array that represents a single mat3 to use as uv transform.
    pub uv_transform_offset: Option<u32>,
}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
struct AlphaDataAbi {
    stride: u32,              // Stride in offset into a float array (i.e. byte index / 4). Unused in CPU mode.
    texture_offset: u32, // Must be zero in gpu mode. In cpu mode, it's the index into the material data with the texture enable bitflag.
    cutoff_offset: u32,  // Stride in offset into a float array  (i.e. byte index / 4)
    uv_transform_offset: u32, // Stride in offset into a float array pointing to a mat3 with the uv transform (i.e. byte index / 4). 0xFFFFFFFF represents "no transform"
}

unsafe impl bytemuck::Pod for AlphaDataAbi {}
unsafe impl bytemuck::Zeroable for AlphaDataAbi {}

pub struct DepthPipelines {
    pipelines: DepthOnlyPipelines,
    bg: Option<BindGroup>,
}

impl DepthPipelines {
    pub fn new<M: DepthRenderableMaterial>(
        renderer: &Renderer,
        data_core: &RendererDataCore,
        interfaces: &ShaderInterfaces,
        unclipped_depth_supported: bool,
    ) -> Self {
        let abi_bgl;
        let bg;
        if let Some(alpha) = M::ALPHA_CUTOUT {
            let abi = if renderer.mode == RendererMode::GPUPowered {
                let data_base_offset = round_up_pot(M::TEXTURE_COUNT * 4, 16);
                let stride = data_base_offset + round_up_pot(M::DATA_SIZE, 16);

                AlphaDataAbi {
                    stride: stride / 4,
                    texture_offset: 0,
                    cutoff_offset: (data_base_offset + alpha.cutoff_offset) / 4,
                    uv_transform_offset: alpha
                        .uv_transform_offset
                        .map(|o| (data_base_offset + o) / 4)
                        .unwrap_or(0xFF_FF_FF_FF),
                }
            } else {
                let texture_enable_offset = round_up_pot(M::DATA_SIZE, 16);
                AlphaDataAbi {
                    stride: 0,
                    texture_offset: texture_enable_offset / 4,
                    cutoff_offset: alpha.cutoff_offset / 4,
                    uv_transform_offset: alpha.uv_transform_offset.map(|o| o / 4).unwrap_or(0xFF_FF_FF_FF),
                }
            };

            let buffer = renderer.device.create_buffer_init(&BufferInitDescriptor {
                label: None,
                contents: bytemuck::bytes_of(&abi),
                usage: BufferUsages::UNIFORM,
            });
            abi_bgl = Some(
                BindGroupLayoutBuilder::new()
                    .append(
                        ShaderStages::FRAGMENT,
                        wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: NonZeroU64::new(mem::size_of::<AlphaDataAbi>() as _),
                        },
                        None,
                    )
                    .build(&renderer.device, Some("AlphaDataAbi BGL")),
            );
            bg = Some(BindGroupBuilder::new().append_buffer(&buffer).build(
                &renderer.device,
                Some("AlphaDataAbi BG"),
                abi_bgl.as_ref().unwrap(),
            ))
        } else {
            bg = None;
            abi_bgl = None;
        };

        let pipelines = build_depth_pass_pipeline(
            renderer,
            data_core,
            interfaces,
            abi_bgl.as_ref(),
            unclipped_depth_supported,
        );

        Self { pipelines, bg }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_prepass_to_graph<'node>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        forward_uniform_bg: DataHandle<BindGroup>,
        culled: DataHandle<CulledPerMaterial>,
        samples: SampleCount,
        cutout: bool,
        color: RenderTargetHandle,
        resolve: Option<RenderTargetHandle>,
        depth: RenderTargetHandle,
    ) {
        let mut builder = graph.add_node(if cutout { "Prepass Cutout" } else { "Prepass Opaque" });

        let hdr_color_handle = builder.add_render_target_output(color);
        let hdr_resolve = builder.add_optional_render_target_output(resolve);
        let hdr_depth_handle = builder.add_render_target_output(depth);

        let forward_uniform_handle = builder.add_data_input(forward_uniform_bg);
        let cull_handle = builder.add_data_input(culled);

        let rpass_handle = builder.add_renderpass(RenderPassTargets {
            targets: vec![RenderPassTarget {
                color: hdr_color_handle,
                clear: Color::BLACK,
                resolve: hdr_resolve,
            }],
            depth_stencil: Some(RenderPassDepthTarget {
                target: DepthHandle::RenderTarget(hdr_depth_handle),
                depth_clear: Some(0.0),
                stencil_clear: None,
            }),
        });

        let pt_handle = builder.passthrough_ref(self);

        builder.build(move |pt, _renderer, encoder_or_pass, temps, ready, graph_data| {
            let this = pt.get(pt_handle);
            let rpass = encoder_or_pass.get_rpass(rpass_handle);
            let forward_uniform_bg = graph_data.get_data(temps, forward_uniform_handle).unwrap();
            let culled = graph_data.get_data(temps, cull_handle).unwrap();

            let pipeline = match (cutout, samples) {
                (false, SampleCount::One) => &this.pipelines.prepass_opaque_s1,
                (false, SampleCount::Four) => &this.pipelines.prepass_opaque_s4,
                (true, SampleCount::One) => this.pipelines.prepass_cutout_s1.as_ref().unwrap(),
                (true, SampleCount::Four) => this.pipelines.prepass_cutout_s4.as_ref().unwrap(),
            };

            graph_data.mesh_manager.buffers().bind(rpass);

            rpass.set_pipeline(pipeline);
            rpass.set_bind_group(0, forward_uniform_bg, &[]);
            rpass.set_bind_group(1, &culled.per_material, &[]);
            if let Some(ref bg) = this.bg {
                rpass.set_bind_group(2, bg, &[]);
            }

            match culled.inner.calls {
                ModeData::CPU(ref draws) => culling::cpu::run(rpass, draws, graph_data.material_manager, 3),
                ModeData::GPU(ref data) => {
                    rpass.set_bind_group(3, ready.d2_texture.bg.as_gpu(), &[]);
                    culling::gpu::run(rpass, data);
                }
            }
        });
    }

    pub fn add_shadow_rendering_to_graph<'node>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        cutout: bool,
        shadow_index: usize,
        shadow_uniform_bg: DataHandle<BindGroup>,
        culled: DataHandle<CulledPerMaterial>,
    ) {
        let mut builder = graph.add_node(&*if cutout {
            format_sso!("Shadow Cutout S{}", shadow_index)
        } else {
            format_sso!("Shadow Opaque S{}", shadow_index)
        });

        let shadow_uniform_handle = builder.add_data_input(shadow_uniform_bg);
        let culled_handle = builder.add_data_input(culled);
        let shadow_output_handle = builder.add_shadow_output(shadow_index);

        let rpass_handle = builder.add_renderpass(RenderPassTargets {
            targets: vec![],
            depth_stencil: Some(RenderPassDepthTarget {
                target: DepthHandle::Shadow(shadow_output_handle),
                depth_clear: Some(0.0),
                stencil_clear: None,
            }),
        });

        let pt_handle = builder.passthrough_ref(self);

        builder.build(move |pt, _renderer, encoder_or_pass, temps, ready, graph_data| {
            let this = pt.get(pt_handle);
            let rpass = encoder_or_pass.get_rpass(rpass_handle);
            let shadow_uniform = graph_data.get_data(temps, shadow_uniform_handle).unwrap();
            let culled = graph_data.get_data(temps, culled_handle).unwrap();

            let pipeline = match cutout {
                false => &this.pipelines.shadow_opaque_s1,
                true => this.pipelines.shadow_cutout_s1.as_ref().unwrap(),
            };

            graph_data.mesh_manager.buffers().bind(rpass);
            rpass.set_pipeline(pipeline);
            rpass.set_bind_group(0, shadow_uniform, &[]);
            rpass.set_bind_group(1, &culled.per_material, &[]);
            if let Some(ref bg) = this.bg {
                rpass.set_bind_group(2, bg, &[]);
            }

            match culled.inner.calls {
                ModeData::CPU(ref draws) => culling::cpu::run(rpass, draws, graph_data.material_manager, 3),
                ModeData::GPU(ref data) => {
                    rpass.set_bind_group(3, ready.d2_texture.bg.as_gpu(), &[]);
                    culling::gpu::run(rpass, data);
                }
            }
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepthPassType {
    Shadow,
    Prepass,
}

pub struct DepthOnlyPipelines {
    pub shadow_opaque_s1: RenderPipeline,
    pub shadow_cutout_s1: Option<RenderPipeline>,
    pub prepass_opaque_s1: RenderPipeline,
    pub prepass_cutout_s1: Option<RenderPipeline>,
    pub prepass_opaque_s4: RenderPipeline,
    pub prepass_cutout_s4: Option<RenderPipeline>,
}

pub fn build_depth_pass_pipeline(
    renderer: &Renderer,
    data_core: &RendererDataCore,
    interfaces: &ShaderInterfaces,
    abi_bgl: Option<&BindGroupLayout>,
    unclipped_depth_supported: bool,
) -> DepthOnlyPipelines {
    profiling::scope!("build depth pass pipelines");
    let depth_vert = unsafe {
        mode_safe_shader(
            &renderer.device,
            renderer.mode,
            "depth pass vert",
            "depth.vert.cpu.wgsl",
            "depth.vert.gpu.spv",
        )
    };

    let depth_opaque_frag = unsafe {
        mode_safe_shader(
            &renderer.device,
            renderer.mode,
            "depth pass opaque frag",
            "depth-opaque.frag.cpu.wgsl",
            "depth-opaque.frag.gpu.spv",
        )
    };

    let depth_cutout_frag = unsafe {
        mode_safe_shader(
            &renderer.device,
            renderer.mode,
            "depth pass cutout frag",
            "depth-cutout.frag.cpu.wgsl",
            "depth-cutout.frag.gpu.spv",
        )
    };

    let mut bgls: ArrayVec<&BindGroupLayout, 4> = ArrayVec::new();
    bgls.push(&interfaces.shadow_uniform_bgl);
    bgls.push(&interfaces.per_material_bgl);
    if let Some(abi_bgl) = abi_bgl {
        bgls.push(abi_bgl);
    }
    if renderer.mode == RendererMode::GPUPowered {
        bgls.push(data_core.d2_texture_manager.gpu_bgl())
    } else {
        bgls.push(data_core.material_manager.get_bind_group_layout_cpu::<PbrMaterial>());
    }

    let shadow_pll = renderer.device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("shadow pll"),
        bind_group_layouts: &bgls,
        push_constant_ranges: &[],
    });

    bgls[0] = &interfaces.forward_uniform_bgl;
    let prepass_pll = renderer.device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("prepass pll"),
        bind_group_layouts: &bgls,
        push_constant_ranges: &[],
    });

    let inner = |name, ty, pll, frag, samples| {
        create_depth_inner(
            renderer,
            samples,
            ty,
            unclipped_depth_supported,
            pll,
            &depth_vert,
            frag,
            name,
        )
    };

    DepthOnlyPipelines {
        shadow_opaque_s1: inner(
            "Shadow Opaque 1x",
            DepthPassType::Shadow,
            &shadow_pll,
            &depth_opaque_frag,
            SampleCount::One,
        ),
        shadow_cutout_s1: Some(inner(
            "Shadow Cutout 1x",
            DepthPassType::Shadow,
            &shadow_pll,
            &depth_cutout_frag,
            SampleCount::One,
        )),
        prepass_opaque_s1: inner(
            "Prepass Opaque 1x",
            DepthPassType::Prepass,
            &prepass_pll,
            &depth_opaque_frag,
            SampleCount::One,
        ),
        prepass_cutout_s1: Some(inner(
            "Prepass Cutout 1x",
            DepthPassType::Prepass,
            &prepass_pll,
            &depth_cutout_frag,
            SampleCount::One,
        )),
        prepass_opaque_s4: inner(
            "Prepass Opaque 4x",
            DepthPassType::Prepass,
            &prepass_pll,
            &depth_opaque_frag,
            SampleCount::Four,
        ),
        prepass_cutout_s4: Some(inner(
            "Prepass Cutout 4x",
            DepthPassType::Prepass,
            &prepass_pll,
            &depth_cutout_frag,
            SampleCount::Four,
        )),
    }
}

#[allow(clippy::too_many_arguments)]
fn create_depth_inner(
    renderer: &Renderer,
    samples: SampleCount,
    ty: DepthPassType,
    unclipped_depth_supported: bool,
    pll: &wgpu::PipelineLayout,
    vert: &wgpu::ShaderModule,
    frag: &wgpu::ShaderModule,
    name: &str,
) -> RenderPipeline {
    profiling::scope!("build depth pipeline", name);
    let color_state = [ColorTargetState {
        format: TextureFormat::Rgba16Float,
        blend: None,
        write_mask: ColorWrites::empty(),
    }];
    let cpu_vertex_buffers = cpu_vertex_buffers();
    let gpu_vertex_buffers = gpu_vertex_buffers();
    renderer.device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some(name),
        layout: Some(pll),
        vertex: VertexState {
            module: vert,
            entry_point: "main",
            buffers: match renderer.mode {
                RendererMode::CPUPowered => &cpu_vertex_buffers,
                RendererMode::GPUPowered => &gpu_vertex_buffers,
            },
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: match renderer.handedness {
                Handedness::Left => FrontFace::Cw,
                Handedness::Right => FrontFace::Ccw,
            },
            cull_mode: Some(match ty {
                DepthPassType::Shadow => Face::Front,
                DepthPassType::Prepass => Face::Back,
            }),
            unclipped_depth: matches!(ty, DepthPassType::Shadow) && unclipped_depth_supported,
            polygon_mode: PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(DepthStencilState {
            format: TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: CompareFunction::GreaterEqual,
            stencil: StencilState::default(),
            bias: match ty {
                DepthPassType::Prepass => DepthBiasState::default(),
                DepthPassType::Shadow => DepthBiasState {
                    constant: -2,
                    slope_scale: -2.0,
                    clamp: 0.0,
                },
            },
        }),
        multisample: MultisampleState {
            count: samples as u32,
            ..Default::default()
        },
        fragment: Some(FragmentState {
            module: frag,
            entry_point: "main",
            targets: match ty {
                DepthPassType::Prepass => &color_state,
                DepthPassType::Shadow => &[],
            },
        }),
        multiview: None,
    })
}
