//! Render routine integrating egui into a rend3 rendergraph.
//!
//! Call [`EguiRenderRoutine::add_to_graph`] to add it to the graph.

use std::num::{NonZeroU32, NonZeroU64};

use bytemuck::cast_slice;
use egui::{ClippedPrimitive, PaintCallback, TextureId, TexturesDelta};
use intmap::IntMap;

use graph::*;
use rend3::{
    graph::{RenderGraph, RenderTargetHandle},
    types::SampleCount,
    *,
};
use wgpu::*;

const EGUI_VERTEX_BUFFER_NAME: &str = "Egui Vertex Buffer";
const EGUI_INDEX_BUFFER_NAME: &str = "Egui Index Buffer";
const EGUI_SCREEN_SIZE_UNIFORM_BUFFER_NAME: &str = "Egui ScreenSize Uniform Buffer";
const EGUI_SCREEN_SIZE_UNIFORM_BUFFER_SIZE: u64 = 8;
const EGUI_SCREEN_SIZE_BINDGROUP_NAME: &str = "Egui ScreenSize Bindgroup";
const EGUI_FONT_TEXTURE_NAME: &str = "Egui Font Texture";
const EGUI_LINEAR_SAMPLER_NAME: &str = "Egui Linear Sampler";
const EGUI_NEAREST_SAMPLER_NAME: &str = "Egui Nearest Sampler";
const EGUI_TEXTURE_BINDGROUP_LAYOUT_NAME: &str = "Egui Texture Bindgroup Layout";
const EGUI_PIPELINE_NAME: &str = "Egui Pipeline";

const EGUI_VERTEX_SIZE: u64 = 20;
const EGUI_INDEX_SIZE: u64 = 4;
/// Stores wgpu data structures required for egui rendering
/// we store all vertex and index data in a single buffer each.
/// allows us to avoid using a variable number of buffers (and bindings ) per frame
pub struct EguiRenderRoutine {
    /// The content scale of the window. needed to calculate scissor region
    scale: f32,
    /// physical framebuffer size. needed to calculate scissor region
    /// technically, this needs to match the surface config size when it acquired the current texture (swapchain image).
    /// if the size is inaccurate and the scissor region goes beyond the bounds of surface texture size, wgpu will panic.
    framebuffer_size: [u32; 2],
    /// screen size in logical pixels. should match whatever is used by the latest egui's frame (`RawInput::screen_rect`)
    screen_size: [f32; 2],
    /// The uniform buffer which contains screen size in logical coordinates
    screen_size_ub: Buffer,
    /// The bindgroup to bind for screen size uniform buffer.
    screen_size_bindgroup: BindGroup,
    /// Egui Render Pipeline
    pipeline: RenderPipeline,
    /// Vertex Buffer containing ALL vertices
    vb: Buffer,
    /// present capacity of vertex buffer
    vb_len: usize,
    /// Index Buffer containing ALL indices
    ib: Buffer,
    /// present capacity of index buffer
    ib_len: usize,
    /// Linear sampler for use with normal textures.
    /// when egui sends us new textures to create, we will also create the relevant bindgroup for the texture.
    /// and that's when we will use this sampler in that bindgroup if that texture wants linear filtering
    linear_sampler: Sampler,
    /// Nearest sampler primarily used for font texture of egui. see above on where its used.
    nearest_sampler: Sampler,
    /// The layout to use for newly create egui textures (contains a sampler + texture).
    texture_layout: BindGroupLayout,
    /// These draw calls store info on the "slices" of index and vertex buffers to use for a certain call as well as other per draw call data.
    draw_calls: Vec<DrawCallInfo>,
    /// These are the textures uploaded by egui. IntMap is super fast and suitable for this use case.
    managed_textures: IntMap<EguiManagedTexture>,
    /// These are textures created manually by users bypassing egui. we only care about the bindgroup as the textures / views must be created by the user.
    user_textures: IntMap<BindGroup>,
    /// A counter to give a unique ID to user textures
    user_texture_id: u64,
    /// these are textures to be cleared. due to renderpass lifetime complexity, we will avoid free-ing textures until next frame.
    textures_to_clear: Vec<TextureId>,
}

/// Data sent by egui to the renderer to render the UI
pub struct EguiRenderOutput {
    pub meshes: Vec<ClippedPrimitive>,
    pub textures_delta: TexturesDelta,
}

/// This is how we store an egui uploaded texture.
/// we keep the bindgroup as it is easier to just bind it individually (as the draw call of clipped primitive WILL use only a single texture)
/// In future, someone with advanced rend3 knowledge can figure out a more optimal way to store and bind textures.
pub struct EguiManagedTexture {
    /// just wgpu texture
    pub texture: Texture,
    /// the view of texture
    pub view: TextureView,
    /// bindgroup of a texture using either linear or nearest sampler as egui told us to.
    pub bindgroup: BindGroup,
}

/// Basically encapsulates the necessary info to issue draw calls and set scissor regions
/// **NOTE**: we don't allow Callback variants to be used at the moment. First, got to decide on
///             how to expose rend3 renderer to the callbacks.
pub enum DrawCallInfo {
    /// This is just a ClippedPrimitive from egui
    Mesh {
        /// This is the scissor region. x, y, width, height in that order
        /// just destrcture and set the region.
        /// if the width or height is 0, skip setting scissor rect and skip issuing the draw call
        clip_rect: [u32; 4],
        /// This will be considererd the "start" of the vertex buffer. An `index_start` of `0` will use this vertex
        base_vertex: usize,
        /// This is where the indices start in the bound index buffer for the current primitive. these are relative to the
        /// `base_vertex` (see above), and will be calculated automatically by the draw call
        index_start: usize,
        /// This is where the indices end. we can pretty much guarantee that this range will always be a multiple of three as `egui`
        /// only uses triangles at the moment.
        index_end: usize,
        /// This is the texture id of the managed texture to be bound. most probably the font texture with value `0`.
        texture: u64,
    },
    /// needs more info on how to expose rend3 functionality to callbacks.
    Callback(PaintCallback),
}

impl EguiRenderRoutine {
    pub fn resize(&mut self, scale: f32, framebuffer_size: [u32; 2], screen_size: [f32; 2]) {
        self.scale = scale;
        self.framebuffer_size = framebuffer_size;
        self.screen_size = screen_size;
    }
    pub fn new(
        renderer: &Renderer,
        surface_format: TextureFormat,
        sample_count: SampleCount,
        screen_size: [f32; 2],
        scale: f32,
        framebuffer_size: [u32; 2],
    ) -> Self {
        // make sure that our assumptions are true
        assert_eq!(
            std::mem::size_of::<egui::epaint::Vertex>(),
            EGUI_VERTEX_SIZE as usize,
            "Egui's vertex size is not 20 anymore."
        );

        let dev = renderer.device.clone();

        let shader_module = dev.create_shader_module(include_wgsl!("egui.wgsl"));

        // create buffers
        let screen_size_ub = dev.create_buffer(&BufferDescriptor {
            label: Some(EGUI_SCREEN_SIZE_UNIFORM_BUFFER_NAME),
            size: EGUI_SCREEN_SIZE_UNIFORM_BUFFER_SIZE,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let vb = dev.create_buffer(&BufferDescriptor {
            label: Some(EGUI_VERTEX_BUFFER_NAME),
            size: 0,
            usage: BufferUsages::VERTEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ib = dev.create_buffer(&BufferDescriptor {
            label: Some(EGUI_INDEX_BUFFER_NAME),
            size: 0,
            usage: BufferUsages::INDEX | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        //
        let screen_size_bind_group_layout = dev.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("egui screen size bindgroup layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(
                        NonZeroU64::new(8).expect("screen size uniform buffer MUST BE 8 bytes in size"),
                    ),
                },
                count: None,
            }],
        });
        let screen_size_bindgroup = dev.create_bind_group(&BindGroupDescriptor {
            label: Some(EGUI_SCREEN_SIZE_BINDGROUP_NAME),
            layout: &screen_size_bind_group_layout,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: BindingResource::Buffer(BufferBinding {
                    buffer: &screen_size_ub,
                    offset: 0,
                    size: None,
                }),
            }],
        });

        let texture_bindgroup_layout = dev.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some(EGUI_TEXTURE_BINDGROUP_LAYOUT_NAME),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let egui_pipeline_layout = dev.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("egui pipeline layout"),
            bind_group_layouts: &[&screen_size_bind_group_layout, &texture_bindgroup_layout],
            push_constant_ranges: &[],
        });
        let egui_pipeline = dev.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some(EGUI_PIPELINE_NAME),
            layout: Some(&egui_pipeline_layout),
            vertex: VertexState {
                module: &shader_module,
                entry_point: "vs_main",
                buffers: &[VertexBufferLayout {
                    array_stride: 20,
                    step_mode: VertexStepMode::Vertex,
                    attributes: &[
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 0,
                            shader_location: 0,
                        },
                        VertexAttribute {
                            format: VertexFormat::Float32x2,
                            offset: 8,
                            shader_location: 1,
                        },
                        VertexAttribute {
                            format: VertexFormat::Unorm8x4,
                            offset: 16,
                            shader_location: 2,
                        },
                    ],
                }],
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState {
                count: sample_count as _,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            fragment: Some(FragmentState {
                module: &shader_module,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        });
        // samplers
        let linear_sampler = dev.create_sampler(&SamplerDescriptor {
            label: Some(EGUI_LINEAR_SAMPLER_NAME),
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Linear,
            ..Default::default()
        });
        let nearest_sampler = dev.create_sampler(&SamplerDescriptor {
            label: Some(EGUI_NEAREST_SAMPLER_NAME),
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Nearest,
            mipmap_filter: FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            vb,
            vb_len: 0,
            ib,
            ib_len: 0,
            draw_calls: Vec::new(),
            managed_textures: IntMap::new(),
            linear_sampler,
            nearest_sampler,
            screen_size_ub,
            texture_layout: texture_bindgroup_layout,
            screen_size_bindgroup,
            pipeline: egui_pipeline,
            textures_to_clear: Vec::new(),
            scale,
            framebuffer_size,
            screen_size,
            user_textures: IntMap::new(),
            user_texture_id: 0,
        }
    }
    pub fn add_to_graph<'node>(
        &'node mut self,
        graph: &mut RenderGraph<'node>,
        input: EguiRenderOutput,
        output: RenderTargetHandle,
    ) {
        let mut builder = graph.add_node("egui render routine node");

        let output_handle = builder.add_render_target_output(output);

        let rpass_handle = builder.add_renderpass(RenderPassTargets {
            targets: vec![RenderPassTarget {
                color: output_handle,
                clear: Color::BLACK,
                resolve: None,
            }],
            depth_stencil: None,
        });

        let pt_handle = builder.passthrough_ref_mut(self);

        builder.build(move |pt, renderer, encoder_or_pass, _temps, _ready, _graph_data| {
            let this = pt.get_mut(pt_handle);
            let rpass = encoder_or_pass.get_rpass(rpass_handle);

            this.update_data(renderer, input.meshes, input.textures_delta);

            this.execute_with_renderpass(rpass);
        });
    }
    pub fn execute_with_renderpass<'pass>(&'pass mut self, render_pass: &mut RenderPass<'pass>) {
        // set egui pipeline once
        render_pass.set_pipeline(&self.pipeline);
        // only set screen size bindgroup once.
        render_pass.set_bind_group(0, &self.screen_size_bindgroup, &[]);
        // bind vertex and index buffers just once
        render_pass.set_vertex_buffer(0, self.vb.slice(..));
        render_pass.set_index_buffer(self.ib.slice(..), IndexFormat::Uint32);
        // for each draw call (mesh)
        for draw_call in self.draw_calls.iter() {
            match draw_call {
                DrawCallInfo::Mesh {
                    clip_rect,
                    base_vertex,
                    index_start,
                    index_end,
                    texture,
                } => {
                    // set scissor region. if it is zero, just skip draw call
                    let [x, y, width, height] = *clip_rect;
                    if width != 0 && height != 0 {
                        render_pass.set_scissor_rect(x, y, width, height);
                    } else {
                        continue;
                    }
                    // set the texture bindgroup
                    render_pass.set_bind_group(
                        1,
                        &self
                            .managed_textures
                            .get(*texture)
                            .expect("failed to find texture")
                            .bindgroup,
                        &[],
                    );
                    // make sure the draw call offsets fit into relevant types
                    let ib_start: u32 = (*index_start).try_into().expect("failed to fit index start into u32");
                    let ib_end: u32 = (*index_end).try_into().expect("failed to fit index end into u32");
                    let base_vertex: i32 = (*base_vertex).try_into().expect("failed to fit base vertex into i32");
                    // issue the draw call
                    render_pass.draw_indexed(ib_start..ib_end, base_vertex, 0..1);
                }
                DrawCallInfo::Callback(_) => todo!(),
            }
        }
    }
    pub fn update_data(
        &mut self,
        renderer: &Renderer,
        clipped_meshes: Vec<ClippedPrimitive>,
        textures_delta: TexturesDelta,
    ) {
        // clear textures from previous frame
        for tex_id in self.textures_to_clear.drain(..) {
            match tex_id {
                TextureId::Managed(m) => {
                    self.managed_textures.remove(m);
                }
                TextureId::User(_) => todo!(),
            }
        }
        let dev = renderer.device.clone();
        let queue = renderer.queue.clone();
        // write the screen size uniform buffer the latest size once
        queue.write_buffer(&self.screen_size_ub, 0, bytemuck::cast_slice(&self.screen_size));
        // upload all the new textures and add the textures to free to `self.textures_to_clear` which will be cleared next frame
        self.deal_with_textures_delta(&dev, &queue, textures_delta);
        self.deal_with_buffers(&dev, &queue, clipped_meshes);
    }
    fn deal_with_buffers(&mut self, dev: &Device, queue: &Queue, clipped_meshes: Vec<ClippedPrimitive>) {
        // count total vb and ib length required
        let mut total_vb_len = 0;
        let mut total_ib_len = 0;
        for cp in &clipped_meshes {
            match cp.primitive {
                egui::epaint::Primitive::Mesh(ref m) => {
                    total_ib_len += m.indices.len();
                    total_vb_len += m.vertices.len();
                }
                egui::epaint::Primitive::Callback(_) => {
                    unimplemented!("eguicallbacks are NOT implemented in rend3 yet")
                }
            }
        }
        // if our allocated buffers are not big enough to store the vertex or index data, create bigger buffers.
        if self.vb_len < total_vb_len {
            self.vb = dev.create_buffer(&BufferDescriptor {
                label: Some(EGUI_VERTEX_BUFFER_NAME),
                size: total_vb_len as u64 * EGUI_VERTEX_SIZE, // size of Vertex is 20 bytes.
                usage: BufferUsages::COPY_DST | BufferUsages::VERTEX,
                mapped_at_creation: false,
            });
            self.vb_len = total_vb_len;
        }
        if self.ib_len < total_ib_len {
            self.ib = dev.create_buffer(&BufferDescriptor {
                label: Some(EGUI_INDEX_BUFFER_NAME),
                size: total_ib_len as u64 * EGUI_INDEX_SIZE, // index is u32, so 4 bytes
                usage: BufferUsages::COPY_DST | BufferUsages::INDEX,
                mapped_at_creation: false,
            });
            self.ib_len = total_ib_len;
        }

        // these are starting bounds of buffer slices which will be used for each draw call
        let mut base_vertex_offset = 0;
        let mut ib_offset = 0;

        // update the buffers with data and create draw calls
        self.draw_calls = clipped_meshes
            .into_iter()
            .map(|mesh| match mesh.primitive {
                egui::epaint::Primitive::Mesh(ref m) => {
                    // write to buffers the relevant data
                    // for vertex buffer, we keep track of the last write's end with base_vertex_offset (in number of vertices. so multiply by 20 to get offset in bytes)
                    queue.write_buffer(
                        &self.vb,
                        (base_vertex_offset * EGUI_VERTEX_SIZE as usize).try_into().unwrap(),
                        cast_slice(&m.vertices),
                    );
                    // for index buffer, we keep track of last write's end in ib_offset. multiply by 4 (u32's size) for offset in bytes
                    queue.write_buffer(
                        &self.ib,
                        (ib_offset * EGUI_INDEX_SIZE as usize).try_into().unwrap(),
                        cast_slice(&m.indices),
                    );

                    // draw call arguments
                    let base_vertex = base_vertex_offset;
                    let index_start = ib_offset;
                    // add the current transferred buffer data's length to get the end of indices range
                    let index_end = ib_offset + m.indices.len();

                    // bump the offsets so that next mesh can use these as starting bounds
                    ib_offset += m.indices.len();
                    base_vertex_offset += m.vertices.len();

                    // idk what these rects are doing, but whatever..
                    let scale = self.scale;
                    let fbw = self.framebuffer_size[0];
                    let fbh = self.framebuffer_size[1];
                    let clip_rect = mesh.clip_rect;
                    let clip_min_x = scale * clip_rect.min.x;
                    let clip_min_y = scale * clip_rect.min.y;
                    let clip_max_x = scale * clip_rect.max.x;
                    let clip_max_y = scale * clip_rect.max.y;

                    // Make sure clip rect can fit within an `u32`.
                    let clip_min_x = clip_min_x.clamp(0.0, fbw as f32);
                    let clip_min_y = clip_min_y.clamp(0.0, fbh as f32);
                    let clip_max_x = clip_max_x.clamp(clip_min_x, fbw as f32);
                    let clip_max_y = clip_max_y.clamp(clip_min_y, fbh as f32);

                    let clip_min_x = clip_min_x.round() as u32;
                    let clip_min_y = clip_min_y.round() as u32;
                    let clip_max_x = clip_max_x.round() as u32;
                    let clip_max_y = clip_max_y.round() as u32;

                    let width = (clip_max_x - clip_min_x).max(1);
                    let height = (clip_max_y - clip_min_y).max(1);

                    // Clip scissor rectangle to target size.
                    let x = clip_min_x.min(fbw);
                    let y = clip_min_y.min(fbh);
                    let width = width.min(fbw - x);
                    let height = height.min(fbh - y);

                    let texture = match m.texture_id {
                        TextureId::Managed(m) => m,
                        TextureId::User(_) => todo!(),
                    };

                    DrawCallInfo::Mesh {
                        clip_rect: [x, y, width, height],
                        texture,
                        base_vertex,
                        index_start,
                        index_end,
                    }
                }
                egui::epaint::Primitive::Callback(c) => DrawCallInfo::Callback(c),
            })
            .collect();
    }
    /// just a helper function to avoid making update data too big
    fn deal_with_textures_delta(&mut self, dev: &Device, queue: &Queue, mut textures_delta: TexturesDelta) {
        self.textures_to_clear.append(&mut textures_delta.free);
        // create and set textures
        for (tex_id, new_texture_data) in textures_delta.set {
            let key = match tex_id {
                TextureId::Managed(m) => m,
                TextureId::User(_) => unreachable!("egui only sends Managed textures."),
            };
            let pos = new_texture_data.pos;
            let data = new_texture_data.image;
            let (pixels, size, mipmap_levels) = match &data {
                egui::ImageData::Color(_) => todo!(),
                egui::ImageData::Font(font_image) => {
                    let pixels: Vec<u8> = font_image.srgba_pixels(1.0).flat_map(|c| c.to_array()).collect();
                    (pixels, font_image.size, 1)
                }
            };
            let texture_label = if key == 0 {
                EGUI_FONT_TEXTURE_NAME.to_string()
            } else {
                format!("Egui Texture {}", key)
            };
            // create texture
            if pos.is_none() {
                let texture = dev.create_texture(&TextureDescriptor {
                    label: Some(&texture_label),
                    size: Extent3d {
                        width: size[0].try_into().expect("failed to fit texture width into u32"),
                        height: size[1].try_into().expect("failed ot fit texture height into u32"),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: mipmap_levels,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: TextureFormat::Rgba8UnormSrgb,
                    usage: TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING,
                });
                let view = texture.create_view(&TextureViewDescriptor {
                    label: Some(&format!("{} view", texture_label)),
                    format: Some(TextureFormat::Rgba8UnormSrgb),
                    dimension: Some(TextureViewDimension::D2),
                    aspect: TextureAspect::default(),
                    base_mip_level: 0,
                    mip_level_count: Some(
                        NonZeroU32::try_from(mipmap_levels).expect("mip mpa levle count won't fit in u32"),
                    ),
                    base_array_layer: 0,
                    array_layer_count: Some(NonZeroU32::try_from(1).expect("array layer count not non-zero-u32")),
                });
                let sampler = match new_texture_data.filter {
                    egui::TextureFilter::Nearest => &self.nearest_sampler,
                    egui::TextureFilter::Linear => &self.linear_sampler,
                };
                let bindgroup = dev.create_bind_group(&BindGroupDescriptor {
                    label: Some(&format!("{texture_label} bindgroup")),
                    layout: &self.texture_layout,
                    entries: &[
                        BindGroupEntry {
                            binding: 0,
                            resource: BindingResource::Sampler(sampler),
                        },
                        BindGroupEntry {
                            binding: 1,
                            resource: BindingResource::TextureView(&view),
                        },
                    ],
                });
                self.managed_textures.insert(
                    key,
                    EguiManagedTexture {
                        texture,
                        view,
                        bindgroup,
                    },
                );
            }
            let t = &self
                .managed_textures
                .get(key)
                .as_ref()
                .expect("failed to get managed texture")
                .texture;
            queue.write_texture(
                ImageCopyTexture {
                    texture: t,
                    mip_level: 0,
                    origin: Origin3d::default(),
                    aspect: TextureAspect::All,
                },
                &pixels,
                ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(NonZeroU32::new(size[0] as u32 * 4).expect("texture bytes per row is zero")),
                    rows_per_image: Some(NonZeroU32::new(size[1] as u32).expect("texture rows count is zero")),
                },
                Extent3d {
                    width: size[0] as u32,
                    height: size[1] as u32,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
    /// Creates an egui texture from the given image data, format, and dimensions.
    pub fn create_egui_texture(
        &mut self,
        renderer: &Renderer,
        format: wgpu::TextureFormat,
        image_rgba: &[u8],
        dimensions: (u32, u32),
        label: Option<&str>,
    ) -> egui::TextureId {
        let texture_size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };

        let image_texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label,
        });
        let device = &renderer.device;
        let queue = &renderer.queue;

        let texture_size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };
        let textureformatinfo = format.describe();
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &image_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            image_rgba,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: std::num::NonZeroU32::new(
                    (dimensions.0 / textureformatinfo.block_dimensions.0 as u32) * textureformatinfo.block_size as u32,
                ),
                rows_per_image: None,
            },
            texture_size,
        );
        let view = image_texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2),
            ..Default::default()
        });
        let bindgroup = device.create_bind_group(&BindGroupDescriptor {
            label,
            layout: &self.texture_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::Sampler(&self.linear_sampler),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&view),
                },
            ],
        });
        self.user_textures.insert(self.user_texture_id, bindgroup);
        let texture_id = TextureId::User(self.user_texture_id);
        self.user_texture_id += 1;
        texture_id
    }
}
