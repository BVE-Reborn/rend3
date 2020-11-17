use crate::{
    datatypes::{
        DepthCompare, Pipeline, PipelineBindingType, PipelineDepthState, PipelineHandle, PipelineInputType,
        PipelineOutputAttachment, ShaderHandle,
    },
    list::{
        DepthOutput, ImageFormat, ImageInputReference, ImageOutput, ImageOutputReference, ImageResourceDescriptor,
        ImageUsage, RenderList, RenderOpDescriptor, RenderOpInputType, RenderPassDescriptor, RenderPassRunRate,
        ResourceBinding, ShaderSourceStage, ShaderSourceType, SourceShaderDescriptor,
    },
    renderer::MAX_MATERIALS,
    Renderer,
};
use std::future::Future;
use wgpu::{Color, LoadOp, TextureFormat};
use winit::dpi::PhysicalSize;

pub struct DefaultShaders {
    pub depth_vert: ShaderHandle,
    pub depth_frag: ShaderHandle,
    pub skybox_vert: ShaderHandle,
    pub skybox_frag: ShaderHandle,
    pub opaque_vert: ShaderHandle,
    pub opaque_frag: ShaderHandle,
    pub blit_vert: ShaderHandle,
    pub blit_frag: ShaderHandle,
}
impl DefaultShaders {
    pub fn new<TLD>(renderer: &Renderer<TLD>) -> impl Future<Output = Self>
    where
        TLD: 'static,
    {
        let depth_vert = renderer.add_source_shader(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/depth.vert".to_string()),
            stage: ShaderSourceStage::Vertex,
            includes: vec![],
            defines: vec![(String::from("MATERIAL_COUNT"), Some(MAX_MATERIALS.to_string()))],
        });
        let depth_frag = renderer.add_source_shader(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/depth.frag".to_string()),
            stage: ShaderSourceStage::Fragment,
            includes: vec![],
            defines: vec![(String::from("MATERIAL_COUNT"), Some(MAX_MATERIALS.to_string()))],
        });

        let skybox_vert = renderer.add_source_shader(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/skybox.vert".to_string()),
            stage: ShaderSourceStage::Vertex,
            includes: vec![],
            defines: vec![],
        });
        let skybox_frag = renderer.add_source_shader(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/skybox.frag".to_string()),
            stage: ShaderSourceStage::Fragment,
            includes: vec![],
            defines: vec![],
        });

        let opaque_vert = renderer.add_source_shader(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/opaque.vert".to_string()),
            stage: ShaderSourceStage::Vertex,
            includes: vec![],
            defines: vec![(String::from("MATERIAL_COUNT"), Some(MAX_MATERIALS.to_string()))],
        });
        let opaque_frag = renderer.add_source_shader(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/opaque.frag".to_string()),
            stage: ShaderSourceStage::Fragment,
            includes: vec![],
            defines: vec![(String::from("MATERIAL_COUNT"), Some(MAX_MATERIALS.to_string()))],
        });

        let blit_vert = renderer.add_source_shader(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/blit.vert".to_string()),
            stage: ShaderSourceStage::Vertex,
            includes: vec![],
            defines: vec![],
        });
        let blit_frag = renderer.add_source_shader(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/blit.frag".to_string()),
            stage: ShaderSourceStage::Fragment,
            includes: vec![],
            defines: vec![],
        });

        async move {
            Self {
                depth_vert: depth_vert.await,
                depth_frag: depth_frag.await,
                skybox_vert: skybox_vert.await,
                skybox_frag: skybox_frag.await,
                opaque_vert: opaque_vert.await,
                opaque_frag: opaque_frag.await,
                blit_vert: blit_vert.await,
                blit_frag: blit_frag.await,
            }
        }
    }
}

pub struct DefaultPipelines {
    pub shadow_depth_pipeline: PipelineHandle,
    pub depth_pipeline: PipelineHandle,
    pub skybox_pipeline: PipelineHandle,
    pub opaque_pipeline: PipelineHandle,
    pub blit_pipeline: PipelineHandle,
}

impl DefaultPipelines {
    pub fn new<TLD>(renderer: &Renderer<TLD>, shaders: &DefaultShaders) -> impl Future<Output = Self>
    where
        TLD: 'static,
    {
        let shadow_depth_pipeline = renderer.add_pipeline(Pipeline {
            run_rate: RenderPassRunRate::PerShadow,
            input: PipelineInputType::Models3d,
            outputs: vec![],
            depth: Some(PipelineDepthState {
                format: ImageFormat::Depth32Float,
                compare: DepthCompare::Closer,
            }),
            vertex: shaders.depth_vert,
            fragment: Some(shaders.depth_frag),
            bindings: vec![
                PipelineBindingType::GeneralData,
                PipelineBindingType::ObjectData,
                PipelineBindingType::GPU2DTextures,
                PipelineBindingType::CameraData,
            ],
            samples: 1,
        });

        let depth_pipeline = renderer.add_pipeline(Pipeline {
            run_rate: RenderPassRunRate::Once,
            input: PipelineInputType::Models3d,
            outputs: vec![
                PipelineOutputAttachment {
                    format: ImageFormat::Rgba16Float,
                    write: false,
                },
                PipelineOutputAttachment {
                    format: ImageFormat::Rgba16Float,
                    write: false,
                },
            ],
            depth: Some(PipelineDepthState {
                format: ImageFormat::Depth32Float,
                compare: DepthCompare::Closer,
            }),
            vertex: shaders.depth_vert,
            fragment: Some(shaders.depth_frag),
            bindings: vec![
                PipelineBindingType::GeneralData,
                PipelineBindingType::ObjectData,
                PipelineBindingType::GPU2DTextures,
                PipelineBindingType::CameraData,
            ],
            samples: 1,
        });

        let skybox_pipeline = renderer.add_pipeline(Pipeline {
            run_rate: RenderPassRunRate::Once,
            input: PipelineInputType::FullscreenTriangle,
            outputs: vec![
                PipelineOutputAttachment {
                    format: ImageFormat::Rgba16Float,
                    write: false,
                },
                PipelineOutputAttachment {
                    format: ImageFormat::Rgba16Float,
                    write: false,
                },
            ],
            depth: Some(PipelineDepthState {
                format: ImageFormat::Depth32Float,
                compare: DepthCompare::Further,
            }),
            vertex: shaders.skybox_vert,
            fragment: Some(shaders.skybox_frag),
            bindings: vec![
                PipelineBindingType::GeneralData,
                PipelineBindingType::SkyboxTexture,
                PipelineBindingType::CameraData,
            ],
            samples: 1,
        });

        let opaque_pipeline = renderer.add_pipeline(Pipeline {
            run_rate: RenderPassRunRate::Once,
            input: PipelineInputType::Models3d,
            outputs: vec![
                PipelineOutputAttachment {
                    format: ImageFormat::Rgba16Float,
                    write: true,
                },
                PipelineOutputAttachment {
                    format: ImageFormat::Rgba16Float,
                    write: true,
                },
            ],
            depth: Some(PipelineDepthState {
                format: ImageFormat::Depth32Float,
                compare: DepthCompare::Equal,
            }),
            vertex: shaders.opaque_vert,
            fragment: Some(shaders.opaque_frag),
            bindings: vec![
                PipelineBindingType::GeneralData,
                PipelineBindingType::ObjectData,
                PipelineBindingType::GPU2DTextures,
                PipelineBindingType::CameraData,
            ],
            samples: 1,
        });

        let blit_pipeline = renderer.add_pipeline(Pipeline {
            run_rate: RenderPassRunRate::Once,
            input: PipelineInputType::Models3d,
            outputs: vec![PipelineOutputAttachment {
                format: ImageFormat::Bgra8UnormSrgb,
                write: true,
            }],
            depth: None,
            vertex: shaders.blit_vert,
            fragment: Some(shaders.blit_frag),
            bindings: vec![
                PipelineBindingType::GeneralData,
                PipelineBindingType::ObjectData,
                PipelineBindingType::GPU2DTextures,
                PipelineBindingType::CameraData,
            ],
            samples: 1,
        });

        async move {
            Self {
                shadow_depth_pipeline: shadow_depth_pipeline.await,
                depth_pipeline: depth_pipeline.await,
                skybox_pipeline: skybox_pipeline.await,
                opaque_pipeline: opaque_pipeline.await,
                blit_pipeline: blit_pipeline.await,
            }
        }
    }
}

pub fn default_render_list(resolution: PhysicalSize<u32>, pipelines: &DefaultPipelines) -> RenderList {
    let resolution: [u32; 2] = resolution.into();

    let mut list = RenderList::new();

    list.add_render_pass(RenderPassDescriptor {
        run_rate: RenderPassRunRate::PerShadow,
        outputs: vec![],
        depth: Some(DepthOutput {
            clear: LoadOp::Clear(0.0),
            output: ImageOutputReference::OutputImage,
        }),
    });

    list.add_render_op(RenderOpDescriptor {
        pipeline: pipelines.shadow_depth_pipeline,
        input: RenderOpInputType::Models3D,
        bindings: vec![
            ResourceBinding::GeneralData,
            ResourceBinding::ObjectData,
            ResourceBinding::GPU2DTextures,
            ResourceBinding::CameraData,
        ],
    });

    let internal_renderbuffer_name = "color renderbuffer";

    list.create_image(
        internal_renderbuffer_name,
        ImageResourceDescriptor {
            resolution,
            format: TextureFormat::Rgba16Float,
            samples: 1,
            usage: ImageUsage::SAMPLED | ImageUsage::OUTPUT_ATTACHMENT,
        },
    );

    list.create_image(
        "normal buffer",
        ImageResourceDescriptor {
            resolution,
            format: TextureFormat::Rgba16Float,
            samples: 1,
            usage: ImageUsage::SAMPLED | ImageUsage::OUTPUT_ATTACHMENT,
        },
    );

    list.create_image(
        "depth_buffer",
        ImageResourceDescriptor {
            resolution,
            format: TextureFormat::Depth32Float,
            samples: 1,
            usage: ImageUsage::SAMPLED | ImageUsage::OUTPUT_ATTACHMENT,
        },
    );

    list.add_render_pass(RenderPassDescriptor {
        run_rate: RenderPassRunRate::Once,
        outputs: vec![
            ImageOutput {
                output: ImageOutputReference::Custom(internal_renderbuffer_name.to_owned()),
                resolve_target: None,
                clear: LoadOp::Clear(Color::BLACK),
            },
            ImageOutput {
                output: ImageOutputReference::Custom(String::from("normal buffer")),
                resolve_target: None,
                clear: LoadOp::Clear(Color::BLACK),
            },
        ],
        depth: Some(DepthOutput {
            clear: LoadOp::Clear(0.0),
            output: ImageOutputReference::Custom(String::from("depth buffer")),
        }),
    });

    list.add_render_op(RenderOpDescriptor {
        pipeline: pipelines.depth_pipeline,
        input: RenderOpInputType::Models3D,
        bindings: vec![
            ResourceBinding::GeneralData,
            ResourceBinding::ObjectData,
            ResourceBinding::GPU2DTextures,
            ResourceBinding::CameraData,
        ],
    });

    list.add_render_op(RenderOpDescriptor {
        pipeline: pipelines.skybox_pipeline,
        input: RenderOpInputType::FullscreenTriangle,
        bindings: vec![
            ResourceBinding::GeneralData,
            ResourceBinding::SkyboxTexture,
            ResourceBinding::CameraData,
        ],
    });

    list.add_render_op(RenderOpDescriptor {
        pipeline: pipelines.opaque_pipeline,
        input: RenderOpInputType::Models3D,
        bindings: vec![
            ResourceBinding::GeneralData,
            ResourceBinding::ObjectData,
            ResourceBinding::GPU2DTextures,
            ResourceBinding::CameraData,
        ],
    });

    list.add_render_pass(RenderPassDescriptor {
        run_rate: RenderPassRunRate::Once,
        outputs: vec![ImageOutput {
            output: ImageOutputReference::OutputImage,
            resolve_target: None,
            clear: LoadOp::Clear(Color::BLACK),
        }],
        depth: None,
    });

    list.add_render_op(RenderOpDescriptor {
        pipeline: pipelines.blit_pipeline,
        input: RenderOpInputType::FullscreenTriangle,
        bindings: vec![
            ResourceBinding::GeneralData,
            ResourceBinding::Custom2DTexture(vec![ImageInputReference::Custom(
                internal_renderbuffer_name.to_string(),
            )]),
        ],
    });

    list
}
