use crate::{
    list::{ImageInputReference, ImageOutputReference, RenderOpInputType, RenderPass, ResourceBinding},
    renderer::{passes::CullingPassData, pipeline::create_custom_texture_bgl, uniforms::WrappedUniform},
    Renderer,
};
use std::sync::Arc;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindingResource, CommandBuffer, CommandEncoderDescriptor,
    Operations, RenderPassColorAttachmentDescriptor, RenderPassDepthStencilAttachmentDescriptor, RenderPassDescriptor,
    SwapChainFrame, TextureView, TextureViewDimension,
};

pub(crate) enum OutputFrame {
    Swapchain(Arc<SwapChainFrame>),
    Shadow(Arc<TextureView>),
}

impl OutputFrame {
    fn as_view(&self) -> &TextureView {
        match self {
            Self::Swapchain(inner) => &inner.output.view,
            Self::Shadow(inner) => &**inner,
        }
    }
}

#[derive(Clone)]
pub(crate) struct BindingData {
    pub general_bg: Arc<BindGroup>,
    pub object_bg: Arc<BindGroup>,
    pub material_bg: Arc<BindGroup>,
    pub gpu_2d_textures_bg: Arc<BindGroup>,
    pub gpu_cube_textures_bg: Arc<BindGroup>,
    pub shadow_texture_bg: Arc<BindGroup>,
    pub skybox_texture_bg: Arc<BindGroup>,
    pub wrapped_uniform: Arc<WrappedUniform>,
}

pub(crate) async fn render_single_render_pass<TD>(
    renderer: Arc<Renderer<TD>>,
    pass: RenderPass,
    output: OutputFrame,
    culling_data: Arc<CullingPassData>,
    binding_data: BindingData,
) -> CommandBuffer
where
    TD: 'static,
{
    let cache_guard = renderer.render_list_cache.read();

    let colors: Vec<_> = pass
        .desc
        .outputs
        .iter()
        .map(|out| RenderPassColorAttachmentDescriptor {
            attachment: match out.output {
                ImageOutputReference::OutputImage => output.as_view(),
                ImageOutputReference::Custom(ref name) => cache_guard.get_image(name),
            },
            resolve_target: out.resolve_target.as_ref().map(|depth| match depth {
                ImageOutputReference::OutputImage => output.as_view(),
                ImageOutputReference::Custom(ref name) => cache_guard.get_image(name),
            }),
            ops: Operations {
                load: out.clear,
                store: true,
            },
        })
        .collect();

    let depth = pass
        .desc
        .depth
        .as_ref()
        .map(|depth| RenderPassDepthStencilAttachmentDescriptor {
            attachment: match depth.output {
                ImageOutputReference::OutputImage => output.as_view(),
                ImageOutputReference::Custom(ref name) => cache_guard.get_image(name),
            },
            depth_ops: Some(Operations {
                load: depth.clear,
                store: true,
            }),
            stencil_ops: None,
        });

    let texture_2d_guard = renderer.texture_manager_2d.read();
    let texture_cube_guard = renderer.texture_manager_cube.read();

    let owned_data: Vec<_> = pass
        .ops
        .iter()
        .flat_map(|op| {
            op.bindings.iter().filter_map(|binding| match binding {
                ResourceBinding::Custom2DTexture(refs) => {
                    let bgl = create_custom_texture_bgl(&renderer.device, TextureViewDimension::D2, refs.len() as u32);

                    let bindings: Vec<_> = refs
                        .iter()
                        .enumerate()
                        .map(|(idx, im_ref)| {
                            let image_ref = match im_ref {
                                ImageInputReference::Handle(handle) => texture_2d_guard.get_view(*handle),
                                ImageInputReference::Custom(name) => cache_guard.get_image(name),
                            };
                            BindGroupEntry {
                                binding: idx as u32,
                                resource: BindingResource::TextureView(image_ref),
                            }
                        })
                        .collect();

                    Some(renderer.device.create_bind_group(&BindGroupDescriptor {
                        label: Some("custom texture"),
                        layout: &bgl,
                        entries: &bindings,
                    }))
                }
                ResourceBinding::CustomCubeTexture(refs) => {
                    let bgl =
                        create_custom_texture_bgl(&renderer.device, TextureViewDimension::Cube, refs.len() as u32);

                    let bindings: Vec<_> = refs
                        .iter()
                        .enumerate()
                        .map(|(idx, im_ref)| {
                            let image_ref = match im_ref {
                                ImageInputReference::Handle(handle) => texture_cube_guard.get_view(*handle),
                                ImageInputReference::Custom(name) => cache_guard.get_image(name),
                            };
                            BindGroupEntry {
                                binding: idx as u32,
                                resource: BindingResource::TextureView(image_ref),
                            }
                        })
                        .collect();

                    Some(renderer.device.create_bind_group(&BindGroupDescriptor {
                        label: Some("custom texture"),
                        layout: &bgl,
                        entries: &bindings,
                    }))
                }
                _ => None,
            })
        })
        .collect();

    drop((texture_2d_guard, texture_cube_guard));

    let mut owned_bg_iter = owned_data.iter();

    let ops: Vec<_> = pass
        .ops
        .iter()
        .map(|op| {
            let bindings: Vec<_> = op
                .bindings
                .iter()
                .map(|binding| match binding {
                    ResourceBinding::GeneralData => &*binding_data.general_bg,
                    ResourceBinding::ObjectData => &*binding_data.object_bg,
                    ResourceBinding::Material => &*binding_data.material_bg,
                    ResourceBinding::CameraData => &binding_data.wrapped_uniform.uniform_bg,
                    ResourceBinding::GPU2DTextures => &*binding_data.gpu_2d_textures_bg,
                    ResourceBinding::GPUCubeTextures => &*binding_data.gpu_cube_textures_bg,
                    ResourceBinding::ShadowTexture => &*binding_data.shadow_texture_bg,
                    ResourceBinding::SkyboxTexture => &*binding_data.skybox_texture_bg,
                    ResourceBinding::Custom2DTexture(..) | ResourceBinding::CustomCubeTexture(..) => {
                        owned_bg_iter.next().unwrap()
                    }
                })
                .collect();

            let pipeline = renderer.pipeline_manager.get_arc(op.pipeline);

            (op, bindings, pipeline)
        })
        .collect();

    let mesh_manager_guard = renderer.mesh_manager.read();
    let (vertex, index) = mesh_manager_guard.buffers();

    let mut encoder = renderer.device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("single renderpass render encoder"),
    });

    let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
        color_attachments: &colors,
        depth_stencil_attachment: depth,
    });

    for (op, bindings, pipeline) in &ops {
        rpass.set_pipeline(&pipeline);
        for (idx, binding) in bindings.iter().enumerate() {
            rpass.set_bind_group(idx as u32, binding, &[]);
        }
        match op.input {
            RenderOpInputType::FullscreenTriangle => {
                rpass.draw(0..3, 0..1);
            }
            RenderOpInputType::Models3D => {
                rpass.set_vertex_buffer(0, vertex.slice(..));
                rpass.set_vertex_buffer(1, culling_data.indirect_buffer.slice(..));
                rpass.set_index_buffer(index.slice(..));
                rpass.multi_draw_indexed_indirect_count(
                    &culling_data.indirect_buffer,
                    0,
                    &culling_data.count_buffer,
                    0,
                    culling_data.object_count,
                );
            }
        }
    }

    drop(rpass);

    encoder.finish()
}
