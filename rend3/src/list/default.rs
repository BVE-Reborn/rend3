use crate::{
    list::{
        ImageOutput, ImageOutputReference, ImageReference, ImageResolution, ImageResourceDescriptor, RenderList,
        RenderOpDescriptor, RenderOpInputType, RenderPassDescriptor, RenderPassSetDescriptor, RenderPassSetRunRate,
        ResourceBinding, ShaderSource, ShaderSourceType, SourceShaderDescriptor,
    },
    renderer::MAX_MATERIALS,
};
use glam::Vec2;
use wgpu::TextureFormat;

pub fn default_render_list() -> RenderList {
    let mut list = RenderList::new();

    list.start_render_pass_set(RenderPassSetDescriptor {
        run_rate: RenderPassSetRunRate::PerShadow,
    });

    list.add_render_pass(RenderPassDescriptor {
        outputs: vec![],
        depth: Some(ImageOutputReference::OutputImage),
    });

    list.add_render_op(RenderOpDescriptor {
        input: RenderOpInputType::Models3D,
        vertex: ShaderSource::Glsl(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/depth.vert".to_string()),
            includes: vec![],
            defines: vec![(String::from("MATERIAL_COUNT"), Some(MAX_MATERIALS.to_string()))],
        }),
        fragment: Some(ShaderSource::Glsl(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/depth.frag".to_string()),
            includes: vec![],
            defines: vec![(String::from("MATERIAL_COUNT"), Some(MAX_MATERIALS.to_string()))],
        })),
        bindings: vec![
            ResourceBinding::GeneralData,
            ResourceBinding::ObjectData,
            ResourceBinding::GPU2DTextures,
            ResourceBinding::CameraData,
        ],
    });

    let internal_renderbuffer_name = String::from("color renderbuffer");

    list.create_image(ImageResourceDescriptor {
        identifier: internal_renderbuffer_name.clone(),
        resolution: ImageResolution::Relative(ImageReference::OutputImage, Vec2::splat(1.0)),
        format: TextureFormat::Rgba16Float,
        samples: 1,
    });

    list.create_image(ImageResourceDescriptor {
        identifier: String::from("normal renderbuffer"),
        resolution: ImageResolution::Relative(
            ImageReference::Custom(internal_renderbuffer_name.clone()),
            Vec2::splat(1.0),
        ),
        format: TextureFormat::Rgba16Float,
        samples: 1,
    });

    list.create_image(ImageResourceDescriptor {
        identifier: String::from("depth buffer"),
        resolution: ImageResolution::Relative(
            ImageReference::Custom(internal_renderbuffer_name.clone()),
            Vec2::splat(1.0),
        ),
        format: TextureFormat::Depth32Float,
        samples: 1,
    });

    list.start_render_pass_set(RenderPassSetDescriptor {
        run_rate: RenderPassSetRunRate::Once,
    });

    list.add_render_pass(RenderPassDescriptor {
        outputs: vec![
            ImageOutput {
                output: ImageOutputReference::Custom(internal_renderbuffer_name.clone()),
                resolve_target: None,
            },
            ImageOutput {
                output: ImageOutputReference::Custom(String::from("normal renderbuffer")),
                resolve_target: None,
            },
        ],
        depth: Some(ImageOutputReference::Custom(String::from("depth buffer"))),
    });

    list.add_render_op(RenderOpDescriptor {
        input: RenderOpInputType::Models3D,
        vertex: ShaderSource::Glsl(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/depth.vert".to_string()),
            includes: vec![],
            defines: vec![(String::from("MATERIAL_COUNT"), Some(MAX_MATERIALS.to_string()))],
        }),
        fragment: Some(ShaderSource::Glsl(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/depth.frag".to_string()),
            includes: vec![],
            defines: vec![(String::from("MATERIAL_COUNT"), Some(MAX_MATERIALS.to_string()))],
        })),
        bindings: vec![
            ResourceBinding::GeneralData,
            ResourceBinding::ObjectData,
            ResourceBinding::GPU2DTextures,
            ResourceBinding::CameraData,
        ],
    });

    list.add_render_op(RenderOpDescriptor {
        input: RenderOpInputType::FullscreenTriangle,
        vertex: ShaderSource::Glsl(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/skybox.vert".to_string()),
            includes: vec![],
            defines: vec![],
        }),
        fragment: Some(ShaderSource::Glsl(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/skybox.frag".to_string()),
            includes: vec![],
            defines: vec![],
        })),
        bindings: vec![
            ResourceBinding::GeneralData,
            ResourceBinding::SkyboxTexture,
            ResourceBinding::CameraData,
        ],
    });

    list.add_render_op(RenderOpDescriptor {
        input: RenderOpInputType::Models3D,
        vertex: ShaderSource::Glsl(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/opaque.vert".to_string()),
            includes: vec![],
            defines: vec![(String::from("MATERIAL_COUNT"), Some(MAX_MATERIALS.to_string()))],
        }),
        fragment: Some(ShaderSource::Glsl(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/opaque.frag".to_string()),
            includes: vec![],
            defines: vec![(String::from("MATERIAL_COUNT"), Some(MAX_MATERIALS.to_string()))],
        })),
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

    list.add_render_op(RenderOpDescriptor {
        input: RenderOpInputType::FullscreenTriangle,
        vertex: ShaderSource::Glsl(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/blit.vert".to_string()),
            includes: vec![],
            defines: vec![],
        }),
        fragment: Some(ShaderSource::Glsl(SourceShaderDescriptor {
            source: ShaderSourceType::File("rend3/shaders/blit.frag".to_string()),
            includes: vec![],
            defines: vec![],
        })),
        bindings: vec![],
    });

    list
}
