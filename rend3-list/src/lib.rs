use rend3::list::{Color, LoadOp};
use rend3::{
    datatypes::{
        DepthCompare, Pipeline, PipelineBindingType, PipelineDepthState, PipelineHandle, PipelineInputType,
        PipelineOutputAttachment, ShaderHandle,
    },
    list::{
        DepthOutput, ImageFormat, ImageInputReference, ImageOutput, ImageOutputReference, ImageResourceDescriptor,
        ImageUsage, PerObjectResourceBinding, RenderList, RenderOpDescriptor, RenderOpInputType, RenderPassDescriptor,
        RenderPassRunRate, ResourceBinding, ShaderSourceStage, ShaderSourceType, SourceShaderDescriptor,
    },
    Renderer, RendererMode, SWAPCHAIN_FORMAT,
};
use std::{future::Future, sync::Arc};

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
        let mode = renderer.mode();

        let mode_define = match mode {
            RendererMode::CPUPowered => (String::from("CPU_MODE"), None),
            RendererMode::GPUPowered => (String::from("GPU_MODE"), None),
        };

        let depth_vert = renderer.add_source_shader(SourceShaderDescriptor {
            source: ShaderSourceType::Builtin("depth.vert".to_string()),
            stage: ShaderSourceStage::Vertex,
            includes: vec![],
            defines: vec![mode_define.clone()],
        });
        let depth_frag = renderer.add_source_shader(SourceShaderDescriptor {
            source: ShaderSourceType::Builtin("depth.frag".to_string()),
            stage: ShaderSourceStage::Fragment,
            includes: vec![],
            defines: vec![mode_define.clone()],
        });

        let skybox_vert = renderer.add_source_shader(SourceShaderDescriptor {
            source: ShaderSourceType::Builtin("skybox.vert".to_string()),
            stage: ShaderSourceStage::Vertex,
            includes: vec![],
            defines: vec![mode_define.clone()],
        });
        let skybox_frag = renderer.add_source_shader(SourceShaderDescriptor {
            source: ShaderSourceType::Builtin("skybox.frag".to_string()),
            stage: ShaderSourceStage::Fragment,
            includes: vec![],
            defines: vec![mode_define.clone()],
        });

        let opaque_vert = renderer.add_source_shader(SourceShaderDescriptor {
            source: ShaderSourceType::Builtin("opaque.vert".to_string()),
            stage: ShaderSourceStage::Vertex,
            includes: vec![],
            defines: vec![mode_define.clone()],
        });
        let opaque_frag = renderer.add_source_shader(SourceShaderDescriptor {
            source: ShaderSourceType::Builtin("opaque.frag".to_string()),
            stage: ShaderSourceStage::Fragment,
            includes: vec![],
            defines: vec![mode_define.clone()],
        });

        let blit_vert = renderer.add_source_shader(SourceShaderDescriptor {
            source: ShaderSourceType::Builtin("blit.vert".to_string()),
            stage: ShaderSourceStage::Vertex,
            includes: vec![],
            defines: vec![mode_define.clone()],
        });
        let blit_frag = renderer.add_source_shader(SourceShaderDescriptor {
            source: ShaderSourceType::Builtin("blit.frag".to_string()),
            stage: ShaderSourceStage::Fragment,
            includes: vec![],
            defines: vec![mode_define],
        });

        async move {
            let depth_vert = depth_vert.await;
            let depth_frag = depth_frag.await;
            let skybox_vert = skybox_vert.await;
            let skybox_frag = skybox_frag.await;
            let opaque_vert = opaque_vert.await;
            let opaque_frag = opaque_frag.await;
            let blit_vert = blit_vert.await;
            let blit_frag = blit_frag.await;
            Self {
                depth_vert,
                depth_frag,
                skybox_vert,
                skybox_frag,
                opaque_vert,
                opaque_frag,
                blit_vert,
                blit_frag,
            }
        }
    }
}

#[derive(Debug)]
pub struct DefaultPipelines {
    pub shadow_depth_pipeline: PipelineHandle,
    pub depth_pipeline: PipelineHandle,
    pub skybox_pipeline: PipelineHandle,
    pub opaque_pipeline: PipelineHandle,
    pub blit_pipeline: PipelineHandle,
}

impl DefaultPipelines {
    pub fn new<TLD>(renderer: &Arc<Renderer<TLD>>, shaders: &DefaultShaders) -> impl Future<Output = Self>
    where
        TLD: 'static,
    {
        let mode = renderer.mode();

        let depth_bindings = match mode {
            RendererMode::CPUPowered => vec![
                PipelineBindingType::GeneralData,
                PipelineBindingType::ObjectData,
                PipelineBindingType::CameraData,
                PipelineBindingType::CPUMaterial,
            ],
            RendererMode::GPUPowered => vec![
                PipelineBindingType::GeneralData,
                PipelineBindingType::ObjectData,
                PipelineBindingType::CameraData,
                PipelineBindingType::GPUMaterial,
                PipelineBindingType::GPU2DTextures,
            ],
        };

        let opaque_bindings = match mode {
            RendererMode::CPUPowered => vec![
                PipelineBindingType::GeneralData,
                PipelineBindingType::ObjectData,
                PipelineBindingType::ShadowTexture,
                PipelineBindingType::CameraData,
                PipelineBindingType::CPUMaterial,
            ],
            RendererMode::GPUPowered => vec![
                PipelineBindingType::GeneralData,
                PipelineBindingType::ObjectData,
                PipelineBindingType::ShadowTexture,
                PipelineBindingType::CameraData,
                PipelineBindingType::GPUMaterial,
                PipelineBindingType::GPU2DTextures,
            ],
        };

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
            bindings: depth_bindings.clone(),
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
            bindings: depth_bindings,
            samples: 1,
        });

        let skybox_pipeline = renderer.add_pipeline(Pipeline {
            run_rate: RenderPassRunRate::Once,
            input: PipelineInputType::FullscreenTriangle,
            outputs: vec![
                PipelineOutputAttachment {
                    format: ImageFormat::Rgba16Float,
                    write: true,
                },
                PipelineOutputAttachment {
                    format: ImageFormat::Rgba16Float,
                    write: false,
                },
            ],
            depth: Some(PipelineDepthState {
                format: ImageFormat::Depth32Float,
                compare: DepthCompare::Equal,
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
            bindings: opaque_bindings,
            samples: 1,
        });

        let blit_pipeline = renderer.add_pipeline(Pipeline {
            run_rate: RenderPassRunRate::Once,
            input: PipelineInputType::FullscreenTriangle,
            outputs: vec![PipelineOutputAttachment {
                format: SWAPCHAIN_FORMAT,
                write: true,
            }],
            depth: None,
            vertex: shaders.blit_vert,
            fragment: Some(shaders.blit_frag),
            bindings: vec![
                PipelineBindingType::GeneralData,
                PipelineBindingType::Custom2DTexture { count: 1 },
            ],
            samples: 1,
        });

        async move {
            let shadow_depth_pipeline = shadow_depth_pipeline.await;
            let depth_pipeline = depth_pipeline.await;
            let skybox_pipeline = skybox_pipeline.await;
            let opaque_pipeline = opaque_pipeline.await;
            let blit_pipeline = blit_pipeline.await;
            Self {
                shadow_depth_pipeline,
                depth_pipeline,
                skybox_pipeline,
                opaque_pipeline,
                blit_pipeline,
            }
        }
    }
}

pub fn default_render_list(mode: RendererMode, resolution: [u32; 2], pipelines: &DefaultPipelines) -> RenderList {
    let (depth_bindings, depth_per_obj_bindings) = match mode {
        RendererMode::CPUPowered => (
            vec![
                ResourceBinding::GeneralData,
                ResourceBinding::ObjectData,
                ResourceBinding::CameraData,
            ],
            vec![PerObjectResourceBinding::CPUMaterial],
        ),
        RendererMode::GPUPowered => (
            vec![
                ResourceBinding::GeneralData,
                ResourceBinding::ObjectData,
                ResourceBinding::CameraData,
                ResourceBinding::GPUMaterial,
                ResourceBinding::GPU2DTextures,
            ],
            vec![],
        ),
    };

    let (opaque_bindings, opaque_per_obj_binding) = match mode {
        RendererMode::CPUPowered => (
            vec![
                ResourceBinding::GeneralData,
                ResourceBinding::ObjectData,
                ResourceBinding::ShadowTexture,
                ResourceBinding::CameraData,
            ],
            vec![PerObjectResourceBinding::CPUMaterial],
        ),
        RendererMode::GPUPowered => (
            vec![
                ResourceBinding::GeneralData,
                ResourceBinding::ObjectData,
                ResourceBinding::ShadowTexture,
                ResourceBinding::CameraData,
                ResourceBinding::GPUMaterial,
                ResourceBinding::GPU2DTextures,
            ],
            vec![],
        ),
    };

    let mut list = RenderList::new();

    list.add_render_pass(RenderPassDescriptor {
        run_rate: RenderPassRunRate::PerShadow,
        outputs: vec![],
        depth: Some(DepthOutput {
            clear: LoadOp::Clear(1.0),
            output: ImageOutputReference::OutputImage,
        }),
    });

    list.add_render_op(RenderOpDescriptor {
        pipeline: pipelines.shadow_depth_pipeline,
        input: RenderOpInputType::Models3D,
        per_op_bindings: depth_bindings.clone(),
        per_object_bindings: depth_per_obj_bindings.clone(),
    });

    let internal_renderbuffer_name = "color renderbuffer";

    list.create_image(
        internal_renderbuffer_name,
        ImageResourceDescriptor {
            resolution,
            format: ImageFormat::Rgba16Float,
            samples: 1,
            usage: ImageUsage::SAMPLED | ImageUsage::OUTPUT_ATTACHMENT,
        },
    );

    list.create_image(
        "normal buffer",
        ImageResourceDescriptor {
            resolution,
            format: ImageFormat::Rgba16Float,
            samples: 1,
            usage: ImageUsage::SAMPLED | ImageUsage::OUTPUT_ATTACHMENT,
        },
    );

    list.create_image(
        "depth buffer",
        ImageResourceDescriptor {
            resolution,
            format: ImageFormat::Depth32Float,
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
        per_op_bindings: depth_bindings,
        per_object_bindings: depth_per_obj_bindings,
    });

    list.add_render_op(RenderOpDescriptor {
        pipeline: pipelines.skybox_pipeline,
        input: RenderOpInputType::FullscreenTriangle,
        per_op_bindings: vec![
            ResourceBinding::GeneralData,
            ResourceBinding::SkyboxTexture,
            ResourceBinding::CameraData,
        ],
        per_object_bindings: vec![],
    });

    list.add_render_op(RenderOpDescriptor {
        pipeline: pipelines.opaque_pipeline,
        input: RenderOpInputType::Models3D,
        per_op_bindings: opaque_bindings,
        per_object_bindings: opaque_per_obj_binding,
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
        per_op_bindings: vec![
            ResourceBinding::GeneralData,
            ResourceBinding::Custom2DTexture(vec![ImageInputReference::Custom(internal_renderbuffer_name.to_owned())]),
        ],
        per_object_bindings: vec![],
    });

    list
}
