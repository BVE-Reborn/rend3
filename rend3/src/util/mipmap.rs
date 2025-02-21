//! Mipmap generation tools.

use arrayvec::ArrayVec;
use parking_lot::RwLock;
use rend3_types::TextureFormat;
use thiserror::Error;
use wgpu::{
    AddressMode, BindGroup, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, Color,
    ColorTargetState, ColorWrites, CommandEncoder, Device, FilterMode, FragmentState, FrontFace, LoadOp,
    MultisampleState, Operations, PipelineCompilationOptions, PipelineLayout, PipelineLayoutDescriptor, PolygonMode,
    PrimitiveState, PrimitiveTopology, RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline,
    RenderPipelineDescriptor, SamplerBindingType, SamplerDescriptor, ShaderModule, ShaderStages, StoreOp, Texture,
    TextureDescriptor, TextureSampleType, TextureViewDescriptor, TextureViewDimension, VertexState,
};

use crate::{
    format_sso,
    util::{bind_merge::BindGroupBuilder, error_scope::AllocationErrorScope, typedefs::FastHashMap},
};

#[derive(Debug, Error)]
pub enum MipmapGenerationError {
    #[error("Failed to create texture view for mipmap level {mip_level}")]
    TextureViewCreationFailed {
        mip_level: u32,
        #[source]
        source: wgpu::Error,
    },
    #[error("Failed to create bind group")]
    BindGroupCreationFailed(#[source] wgpu::Error),
}

/// Generator for mipmaps.
pub struct MipmapGenerator {
    texture_bgl: BindGroupLayout,
    sampler_bg: BindGroup,
    sm: ShaderModule,
    pll: PipelineLayout,
    pipelines: RwLock<FastHashMap<TextureFormat, RenderPipeline>>,
}

impl MipmapGenerator {
    pub fn new(device: &Device, default_formats: &[TextureFormat]) -> Self {
        profiling::scope!("MipmapGenerator::new");

        let texture_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Mipmap generator texture bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });

        let sampler_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("mipmap generator sampler bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            }],
        });

        let sampler = device.create_sampler(&SamplerDescriptor {
            label: Some("mipmap generator sampler"),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Linear,
            lod_min_clamp: 0.0,
            lod_max_clamp: 100.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });

        let sampler_bg = BindGroupBuilder::new().append_sampler(&sampler).build(
            device,
            Some("mipmap generator sampler bg"),
            &sampler_bgl,
        );

        let sm = device.create_shader_module(wgpu::include_wgsl!("../../shaders/mipmap.wgsl"));

        let pll = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("mipmap generator pipeline layout"),
            bind_group_layouts: &[&texture_bgl, &sampler_bgl],
            push_constant_ranges: &[],
        });

        let pipelines = default_formats
            .iter()
            .map(|&format| (format, Self::build_blit_pipeline(device, format, &pll, &sm)))
            .collect();

        Self { texture_bgl, sampler_bg, sm, pll, pipelines: RwLock::new(pipelines) }
    }

    fn build_blit_pipeline(
        device: &Device,
        format: TextureFormat,
        pll: &PipelineLayout,
        sm: &ShaderModule,
    ) -> RenderPipeline {
        let label = format_sso!("mipmap pipeline {:?}", format);
        profiling::scope!("mipmap pipeline");
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some(&label),
            layout: Some(pll),
            vertex: VertexState {
                module: sm,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: PipelineCompilationOptions::default(), // use default WGPU options. New in WGPU 0.20 (JN)
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Cw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: sm,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState { format, blend: None, write_mask: ColorWrites::all() })],
                compilation_options: PipelineCompilationOptions::default(), // use default WGPU options. New in WGPU 0.20 (JN)
            }),
            multiview: None,
            cache: None,    // no pipeline cache. New in WGPU 21.
        })
    }

    pub fn generate_mipmaps(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        texture: &Texture,
        desc: &TextureDescriptor,
    ) -> Result<(), MipmapGenerationError> {
        profiling::scope!("generating mipmaps");
        let mips: Result<ArrayVec<_, 14>, _> = (0..desc.size.max_mips(desc.dimension))
            .map(|mip_level| {
                let scope = AllocationErrorScope::new(device);
                let view = texture.create_view(&TextureViewDescriptor {
                    label: None,
                    base_mip_level: mip_level,
                    mip_level_count: Some(1),
                    ..Default::default()
                });
                scope.end().map_err(|source| MipmapGenerationError::TextureViewCreationFailed { mip_level, source })?;

                Ok(view)
            })
            .collect();
        let mips = mips?;

        let mut read_pipelines = self.pipelines.read();
        let pipeline = match read_pipelines.get(&desc.format) {
            Some(p) => p,
            None => {
                drop(read_pipelines);

                self.pipelines
                    .write()
                    .insert(desc.format, Self::build_blit_pipeline(device, desc.format, &self.pll, &self.sm));

                read_pipelines = self.pipelines.read();

                read_pipelines.get(&desc.format).unwrap()
            }
        };

        for (idx, view_window) in mips.windows(2).enumerate() {
            let src_view = &view_window[0];
            let dst_view = &view_window[1];

            let src_label = format_sso!("Mipmap level {}", idx);

            profiling::scope!("mip level generation");
            // profiler.lock().begin_scope(&dst_label, encoder, device);

            let scope = AllocationErrorScope::new(device);
            let bg = BindGroupBuilder::new().append_texture_view(src_view).build(
                device,
                Some(&src_label),
                &self.texture_bgl,
            );
            scope.end().map_err(MipmapGenerationError::BindGroupCreationFailed)?;

            let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: dst_view,
                    resolve_target: None,
                    ops: Operations { load: LoadOp::Clear(Color::BLACK), store: StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            rpass.set_pipeline(pipeline);
            rpass.set_bind_group(0, &bg, &[]);
            rpass.set_bind_group(1, &self.sampler_bg, &[]);
            rpass.draw(0..3, 0..1);

            drop(rpass);

            // profiler.lock().end_scope(encoder);
        }

        Ok(())
    }
}
