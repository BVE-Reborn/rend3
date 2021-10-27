use std::num::NonZeroU32;

use arrayvec::ArrayVec;
use parking_lot::{Mutex, RwLock};
use rend3_types::TextureFormat;
use wgpu::{
    AddressMode, BindGroup, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, Color,
    ColorTargetState, ColorWrites, CommandEncoder, Device, FilterMode, FragmentState, FrontFace, LoadOp,
    MultisampleState, Operations, PipelineLayout, PipelineLayoutDescriptor, PolygonMode, PrimitiveState,
    PrimitiveTopology, RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor,
    SamplerDescriptor, ShaderModule, ShaderStages, Texture, TextureDescriptor, TextureSampleType,
    TextureViewDescriptor, TextureViewDimension, VertexState,
};
use wgpu_profiler::GpuProfiler;

use crate::{
    format_sso,
    util::{bind_merge::BindGroupBuilder, typedefs::FastHashMap},
};

pub struct MipmapGenerator {
    pub texture_bgl: BindGroupLayout,
    pub sampler_bg: BindGroup,
    pub sm: ShaderModule,
    pub pll: PipelineLayout,
    pub pipelines: RwLock<FastHashMap<TextureFormat, RenderPipeline>>,
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
                ty: BindingType::Sampler {
                    filtering: true,
                    comparison: false,
                },
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
            anisotropy_clamp: None,
            border_color: None,
        });

        let sampler_bg = BindGroupBuilder::new(Some("mipmap generator sampler bg"))
            .with_sampler(&sampler)
            .build(device, &sampler_bgl);

        let sm = device.create_shader_module(&wgpu::include_wgsl!("mipmap.wgsl"));

        let pll = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("mipmap generator pipeline layout"),
            bind_group_layouts: &[&texture_bgl, &sampler_bgl],
            push_constant_ranges: &[],
        });

        let pipelines = default_formats
            .iter()
            .map(|&format| (format, Self::build_blit_pipeline(device, format, &pll, &sm)))
            .collect();

        Self {
            texture_bgl,
            sampler_bg,
            sm,
            pll,
            pipelines: RwLock::new(pipelines),
        }
    }

    fn build_blit_pipeline(
        device: &Device,
        format: TextureFormat,
        pll: &PipelineLayout,
        sm: &ShaderModule,
    ) -> RenderPipeline {
        let label = format_sso!("mipmap pipeline {:?}", format);
        profiling::scope!(&label);
        device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some(&label),
            layout: Some(pll),
            vertex: VertexState {
                module: sm,
                entry_point: "vs_main",
                buffers: &[],
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Cw,
                cull_mode: None,
                clamp_depth: false,
                polygon_mode: PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: sm,
                entry_point: "fs_main",
                targets: &[ColorTargetState {
                    format,
                    blend: None,
                    write_mask: ColorWrites::all(),
                }],
            }),
        })
    }

    pub fn generate_mipmaps(
        &self,
        device: &Device,
        // TODO: allow profiler to run asynchronously
        _profiler: &Mutex<GpuProfiler>,
        encoder: &mut CommandEncoder,
        texture: &Texture,
        desc: &TextureDescriptor,
    ) {
        profiling::scope!("generating mipmaps");
        let mips: ArrayVec<_, 14> = (0..desc.size.max_mips())
            .map(|mip_level| {
                texture.create_view(&TextureViewDescriptor {
                    label: None,
                    base_mip_level: mip_level,
                    mip_level_count: NonZeroU32::new(1),
                    ..Default::default()
                })
            })
            .collect();

        let mut read_pipelines = self.pipelines.read();
        let pipeline = match read_pipelines.get(&desc.format) {
            Some(p) => p,
            None => {
                drop(read_pipelines);

                self.pipelines.write().insert(
                    desc.format,
                    Self::build_blit_pipeline(device, desc.format, &self.pll, &self.sm),
                );

                read_pipelines = self.pipelines.read();

                read_pipelines.get(&desc.format).unwrap()
            }
        };

        for (idx, view_window) in mips.windows(2).enumerate() {
            let src_view = &view_window[0];
            let dst_view = &view_window[1];

            let src_label = format_sso!("Mipmap level {}", idx);
            let _dst_label = format_sso!("Mipmap level {}", idx + 1);

            profiling::scope!(&_dst_label);
            // profiler.lock().begin_scope(&dst_label, encoder, device);

            let bg = BindGroupBuilder::new(Some(&src_label))
                .with_texture_view(src_view)
                .build(device, &self.texture_bgl);

            let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: None,
                color_attachments: &[RenderPassColorAttachment {
                    view: dst_view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });

            rpass.set_pipeline(pipeline);
            rpass.set_bind_group(0, &bg, &[]);
            rpass.set_bind_group(1, &self.sampler_bg, &[]);
            rpass.draw(0..3, 0..1);

            drop(rpass);

            // profiler.lock().end_scope(encoder);
        }
    }
}
