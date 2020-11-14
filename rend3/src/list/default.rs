use crate::{
    list::{
        ImageInputReference, ImageOutput, ImageOutputReference, ImageResourceDescriptor, ImageUsage, RenderList,
        RenderOpDescriptor, RenderOpInputType, RenderPassDescriptor, RenderPassSetDescriptor, RenderPassSetRunRate,
        ResourceBinding, ShaderSource, ShaderSourceStage, ShaderSourceType, SourceShaderDescriptor,
    },
    renderer::MAX_MATERIALS,
};
use wgpu::TextureFormat;
use winit::dpi::PhysicalSize;

pub fn default_render_list(resolution: PhysicalSize<u32>) -> RenderList {
    let resolution: [u32; 2] = resolution.into();

    let mut list = RenderList::new();

    list.create_shader(
        "depth vert",
        ShaderSource::Glsl(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/depth.vert".to_string()),
            stage: ShaderSourceStage::Vertex,
            includes: vec![],
            defines: vec![(String::from("MATERIAL_COUNT"), Some(MAX_MATERIALS.to_string()))],
        }),
    );

    list.create_shader(
        "depth frag",
        ShaderSource::Glsl(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/depth.frag".to_string()),
            stage: ShaderSourceStage::Fragment,
            includes: vec![],
            defines: vec![(String::from("MATERIAL_COUNT"), Some(MAX_MATERIALS.to_string()))],
        }),
    );

    list.start_render_pass_set(RenderPassSetDescriptor {
        run_rate: RenderPassSetRunRate::PerShadow,
    });

    list.add_render_pass(RenderPassDescriptor {
        outputs: vec![],
        depth: Some(ImageOutputReference::OutputImage),
    });

    list.add_render_op(RenderOpDescriptor {
        input: RenderOpInputType::Models3D,
        vertex: String::from("depth vert"),
        fragment: Some(String::from("depth frag")),
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

    list.start_render_pass_set(RenderPassSetDescriptor {
        run_rate: RenderPassSetRunRate::Once,
    });

    list.add_render_pass(RenderPassDescriptor {
        outputs: vec![
            ImageOutput {
                output: ImageOutputReference::Custom(internal_renderbuffer_name.to_owned()),
                resolve_target: None,
            },
            ImageOutput {
                output: ImageOutputReference::Custom(String::from("normal buffer")),
                resolve_target: None,
            },
        ],
        depth: Some(ImageOutputReference::Custom(String::from("depth buffer"))),
    });

    list.add_render_op(RenderOpDescriptor {
        input: RenderOpInputType::Models3D,
        vertex: String::from("depth vert"),
        fragment: Some(String::from("depth frag")),
        bindings: vec![
            ResourceBinding::GeneralData,
            ResourceBinding::ObjectData,
            ResourceBinding::GPU2DTextures,
            ResourceBinding::CameraData,
        ],
    });

    list.create_shader(
        "skybox vert",
        ShaderSource::Glsl(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/skybox.vert".to_string()),
            stage: ShaderSourceStage::Vertex,
            includes: vec![],
            defines: vec![],
        }),
    );

    list.create_shader(
        "skybox vert",
        ShaderSource::Glsl(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/skybox.frag".to_string()),
            stage: ShaderSourceStage::Fragment,
            includes: vec![],
            defines: vec![],
        }),
    );

    list.add_render_op(RenderOpDescriptor {
        input: RenderOpInputType::FullscreenTriangle,
        vertex: String::from("skybox vert"),
        fragment: Some(String::from("skybox frag")),
        bindings: vec![
            ResourceBinding::GeneralData,
            ResourceBinding::SkyboxTexture,
            ResourceBinding::CameraData,
        ],
    });

    list.create_shader(
        "opaque vert",
        ShaderSource::Glsl(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/opaque.vert".to_string()),
            stage: ShaderSourceStage::Vertex,
            includes: vec![],
            defines: vec![(String::from("MATERIAL_COUNT"), Some(MAX_MATERIALS.to_string()))],
        }),
    );

    list.create_shader(
        "opaque frag",
        ShaderSource::Glsl(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/opaque.frag".to_string()),
            stage: ShaderSourceStage::Fragment,
            includes: vec![],
            defines: vec![(String::from("MATERIAL_COUNT"), Some(MAX_MATERIALS.to_string()))],
        }),
    );

    list.add_render_op(RenderOpDescriptor {
        input: RenderOpInputType::Models3D,
        vertex: String::from("opaque vert"),
        fragment: Some(String::from("opaque frag")),
        bindings: vec![
            ResourceBinding::GeneralData,
            ResourceBinding::ObjectData,
            ResourceBinding::GPU2DTextures,
            ResourceBinding::CameraData,
        ],
    });

    list.add_render_pass(RenderPassDescriptor {
        outputs: vec![ImageOutput {
            output: ImageOutputReference::OutputImage,
            resolve_target: None,
        }],
        depth: None,
    });

    list.create_shader(
        "blit vert",
        ShaderSource::Glsl(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/blit.vert".to_string()),
            stage: ShaderSourceStage::Vertex,
            includes: vec![],
            defines: vec![],
        }),
    );

    list.create_shader(
        "blit vert",
        ShaderSource::Glsl(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/blit.frag".to_string()),
            stage: ShaderSourceStage::Fragment,
            includes: vec![],
            defines: vec![],
        }),
    );

    list.add_render_op(RenderOpDescriptor {
        input: RenderOpInputType::FullscreenTriangle,
        vertex: String::from("blit vert"),
        fragment: Some(String::from("blit frag")),
        bindings: vec![
            ResourceBinding::GeneralData,
            ResourceBinding::Custom2DTexture(vec![ImageInputReference::Custom(
                internal_renderbuffer_name.to_string(),
            )]),
        ],
    });

    list
}
