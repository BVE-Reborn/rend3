use crate::{
    cache::Cached,
    util::typedefs::{FastHashMap, SsoString},
};
use std::{ops::Deref, sync::Arc};
use wgpu::{
    BindGroupLayout, BufferAddress, ColorTargetState, CompareFunction, ComputePipeline, ComputePipelineDescriptor,
    CullMode, DepthBiasState, DepthStencilState, Device, FragmentState, FrontFace, IndexFormat, InputStepMode,
    MultisampleState, PipelineLayout, PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology,
    PushConstantRange, RenderPipeline, RenderPipelineDescriptor, ShaderModule, StencilState, TextureFormat,
    VertexAttribute, VertexBufferLayout, VertexState,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AddressedRenderPipelineDescriptor {
    label: Option<SsoString>,
    layout: AddressedPipelineLayoutDescriptor,
    vertex: AddressedVertexState,
    primitive: AddressedPrimitiveState,
    depth_stencil: Option<AddressedDepthStencilState>,
    multisample: AddressedMultisampleState,
    fragment: Option<AddressedFragmentState>,
}

impl AddressedRenderPipelineDescriptor {
    fn from_wgpu(pipeline_layout: &PipelineLayoutDescriptor<'_>, pipeline: &RenderPipelineDescriptor<'_>) -> Self {
        assert!(
            pipeline.layout.is_none(),
            "Do not attach a pipeline layout in the render pipeline descriptor"
        );
        Self {
            label: pipeline.label.map(SsoString::from),
            layout: AddressedPipelineLayoutDescriptor::from_wgpu(pipeline_layout),
            vertex: AddressedVertexState::from_wgpu(&pipeline.vertex),
            primitive: AddressedPrimitiveState::from_wgpu(&pipeline.primitive),
            depth_stencil: pipeline
                .depth_stencil
                .as_ref()
                .map(AddressedDepthStencilState::from_wgpu),
            multisample: AddressedMultisampleState::from_wgpu(&pipeline.multisample),
            fragment: pipeline.fragment.as_ref().map(AddressedFragmentState::from_wgpu),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AddressedComputePipelineDescriptor {
    label: Option<SsoString>,
    layout: AddressedPipelineLayoutDescriptor,
    module: usize,
    entry_point: SsoString,
}

impl AddressedComputePipelineDescriptor {
    fn from_wgpu(pipeline_layout: &PipelineLayoutDescriptor<'_>, pipeline: &ComputePipelineDescriptor<'_>) -> Self {
        assert!(
            pipeline.layout.is_none(),
            "Do not attach a pipeline layout in the compute pipeline descriptor"
        );
        Self {
            label: pipeline.label.map(SsoString::from),
            layout: AddressedPipelineLayoutDescriptor::from_wgpu(pipeline_layout),
            module: pipeline.module as *const ShaderModule as usize,
            entry_point: SsoString::from(pipeline.entry_point),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AddressedPipelineLayoutDescriptor {
    label: Option<SsoString>,
    bind_group_layouts: Vec<usize>,
    push_constant_ranges: Vec<PushConstantRange>,
}

impl AddressedPipelineLayoutDescriptor {
    fn from_wgpu(layout: &PipelineLayoutDescriptor<'_>) -> Self {
        Self {
            label: layout.label.map(SsoString::from),
            bind_group_layouts: layout
                .bind_group_layouts
                .iter()
                .map(|&bgl| bgl as *const BindGroupLayout as usize)
                .collect(),
            push_constant_ranges: layout.push_constant_ranges.to_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AddressedVertexState {
    module: usize,
    entry_point: SsoString,
    buffers: Vec<AddressedVertexBufferLayout>,
}

impl AddressedVertexState {
    fn from_wgpu(state: &VertexState<'_>) -> Self {
        Self {
            module: state.module as *const ShaderModule as usize,
            entry_point: SsoString::from(state.entry_point),
            buffers: state
                .buffers
                .iter()
                .map(AddressedVertexBufferLayout::from_wgpu)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AddressedVertexBufferLayout {
    array_stride: BufferAddress,
    step_mode: InputStepMode,
    attributes: Vec<VertexAttribute>,
}

impl AddressedVertexBufferLayout {
    fn from_wgpu(layout: &VertexBufferLayout<'_>) -> Self {
        Self {
            array_stride: layout.array_stride,
            step_mode: layout.step_mode,
            attributes: layout.attributes.to_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AddressedPrimitiveState {
    topology: PrimitiveTopology,
    strip_index_format: Option<IndexFormat>,
    front_face: FrontFace,
    cull_mode: CullMode,
    polygon_mode: PolygonMode,
}

impl AddressedPrimitiveState {
    fn from_wgpu(state: &PrimitiveState) -> Self {
        Self {
            topology: state.topology,
            strip_index_format: state.strip_index_format,
            front_face: state.front_face,
            cull_mode: state.cull_mode,
            polygon_mode: state.polygon_mode,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AddressedDepthStencilState {
    format: TextureFormat,
    depth_write_enabled: bool,
    depth_compare: CompareFunction,
    stencil: StencilState,
    bias: AddressedDepthBiasState,
    clamp_depth: bool,
}

impl AddressedDepthStencilState {
    fn from_wgpu(state: &DepthStencilState) -> Self {
        Self {
            format: state.format,
            depth_write_enabled: state.depth_write_enabled,
            depth_compare: state.depth_compare,
            stencil: state.stencil.clone(),
            bias: AddressedDepthBiasState::from_wgpu(&state.bias),
            clamp_depth: state.clamp_depth,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AddressedDepthBiasState {
    constant: i32,
    // f32 in disguize
    slope_scale: u32,
    // f32 in disguize
    clamp: u32,
}

impl AddressedDepthBiasState {
    fn from_wgpu(state: &DepthBiasState) -> Self {
        Self {
            constant: state.constant,
            slope_scale: state.slope_scale.to_bits(),
            clamp: state.clamp.to_bits(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AddressedMultisampleState {
    count: u32,
    mask: u64,
    alpha_to_coverage_enabled: bool,
}

impl AddressedMultisampleState {
    fn from_wgpu(state: &MultisampleState) -> Self {
        Self {
            count: state.count,
            mask: state.mask,
            alpha_to_coverage_enabled: state.alpha_to_coverage_enabled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AddressedFragmentState {
    module: usize,
    entry_point: SsoString,
    targets: Vec<ColorTargetState>,
}

impl AddressedFragmentState {
    fn from_wgpu(state: &FragmentState) -> Self {
        Self {
            module: state.module as *const ShaderModule as usize,
            entry_point: SsoString::from(state.entry_point),
            targets: state.targets.to_vec(),
        }
    }
}

pub struct PipelineCache {
    layout_cache: FastHashMap<AddressedPipelineLayoutDescriptor, Cached<PipelineLayout>>,
    render_cache: FastHashMap<AddressedRenderPipelineDescriptor, Cached<RenderPipeline>>,
    compute_cache: FastHashMap<AddressedComputePipelineDescriptor, Cached<ComputePipeline>>,
    current_epoch: usize,
}

impl PipelineCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            layout_cache: FastHashMap::default(),
            render_cache: FastHashMap::default(),
            compute_cache: FastHashMap::default(),
            current_epoch: 0,
        }
    }

    pub fn mark_new_epoch(&mut self) {
        self.current_epoch += 1;
    }

    pub fn clear_old_epochs(&mut self) {
        let current_epoch = self.current_epoch;
        self.layout_cache.retain(|_, v| v.epoch == current_epoch);
        self.render_cache.retain(|_, v| v.epoch == current_epoch);
        self.compute_cache.retain(|_, v| v.epoch == current_epoch);
    }

    pub fn compute_pipeline(
        &mut self,
        device: &Device,
        pipeline_layout_descriptor: &PipelineLayoutDescriptor<'_>,
        pipeline_descriptor: &ComputePipelineDescriptor<'_>,
    ) -> Arc<ComputePipeline>
    {
        let pll_key = AddressedPipelineLayoutDescriptor::from_wgpu(&pipeline_layout_descriptor);
        let pl_key = AddressedComputePipelineDescriptor::from_wgpu(&pipeline_layout_descriptor, &pipeline_descriptor);

        let current_epoch = self.current_epoch;
        let compute_cache = &mut self.compute_cache;
        let layout_cache = &mut self.layout_cache;

        let render_pipeline = compute_cache.entry(pl_key).or_insert_with(|| {
            let pipeline_layout = layout_cache.entry(pll_key).or_insert_with(|| Cached {
                inner: Arc::new(device.create_pipeline_layout(pipeline_layout_descriptor)),
                epoch: current_epoch,
            });
            pipeline_layout.epoch = current_epoch;

            let mut pl_descriptor = pipeline_descriptor.clone();
            pl_descriptor.layout = Some(&pipeline_layout.inner);
            Cached {
                inner: Arc::new(device.create_compute_pipeline(&pl_descriptor)),
                epoch: current_epoch,
            }
        });

        render_pipeline.epoch = current_epoch;
        Arc::clone(&render_pipeline.inner)
    }

    pub fn render_pipeline(
        &mut self,
        device: &Device,
        pipeline_layout_descriptor: &PipelineLayoutDescriptor<'_>,
        pipeline_descriptor: &RenderPipelineDescriptor,
    ) -> Arc<RenderPipeline>
    {
        let pll_key = AddressedPipelineLayoutDescriptor::from_wgpu(&pipeline_layout_descriptor);
        let pl_key = AddressedRenderPipelineDescriptor::from_wgpu(&pipeline_layout_descriptor, &pipeline_descriptor);

        let current_epoch = self.current_epoch;
        let render_cache = &mut self.render_cache;
        let layout_cache = &mut self.layout_cache;

        let render_pipeline = render_cache.entry(pl_key).or_insert_with(|| {
            let pipeline_layout = layout_cache.entry(pll_key).or_insert_with(|| Cached {
                inner: Arc::new(device.create_pipeline_layout(pipeline_layout_descriptor)),
                epoch: current_epoch,
            });
            pipeline_layout.epoch = current_epoch;

            let mut pl_descriptor = pipeline_descriptor.clone();
            pl_descriptor.layout = Some(&pipeline_layout.inner);
            Cached {
                inner: Arc::new(device.create_render_pipeline(&pl_descriptor)),
                epoch: current_epoch,
            }
        });

        render_pipeline.epoch = current_epoch;
        Arc::clone(&render_pipeline.inner)
    }
}

impl Default for PipelineCache {
    fn default() -> Self {
        Self::new()
    }
}
