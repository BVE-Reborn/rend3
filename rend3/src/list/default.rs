use crate::{
    list::{
        ImageOutput, ImageOutputReference, RenderList, RenderPassDescriptor, RenderPassSetDescriptor,
        RenderPassSetRunRate, ResourceBinding, ShaderSource, ShaderSourceType, SourceShaderDescriptor,
    },
    renderer::MAX_MATERIALS,
};

pub fn default_render_list() -> RenderList {
    let mut list = RenderList::new();

    list.start_render_pass_set(RenderPassSetDescriptor {
        run_rate: RenderPassSetRunRate::PerShadow,
    });

    list.add_render_pass(RenderPassDescriptor {
        // TODO: allow specification of shadow-particular pipeline attributes
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
        outputs: vec![],
        depth: Some(ImageOutputReference::OutputImage),
    });

    unimplemented!()
}
