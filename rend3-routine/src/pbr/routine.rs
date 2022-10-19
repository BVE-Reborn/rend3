use rend3::{Renderer, RendererDataCore, ShaderPreProcessor};
use wgpu::{BlendState};

use crate::{
    common::{PerMaterialArchetypeInterface, WholeFrameInterfaces},
    forward::ForwardRoutine,
    pbr::{PbrMaterial, TransparencyType},
};

/// Render routine that renders the using PBR materials
pub struct PbrRoutine {
    pub opaque_routine: ForwardRoutine<PbrMaterial>,
    pub cutout_routine: ForwardRoutine<PbrMaterial>,
    pub blend_routine: ForwardRoutine<PbrMaterial>,
    pub per_material: PerMaterialArchetypeInterface<PbrMaterial>,
}

impl PbrRoutine {
    pub fn new(
        renderer: &Renderer,
        data_core: &mut RendererDataCore,
        spp: &ShaderPreProcessor,
        interfaces: &WholeFrameInterfaces,
    ) -> Self {
        profiling::scope!("PbrRenderRoutine::new");

        // This ensures the BGLs for the material are created
        data_core
            .material_manager
            .ensure_archetype::<PbrMaterial>(&renderer.device, renderer.profile);

        let per_material = PerMaterialArchetypeInterface::<PbrMaterial>::new(&renderer.device, renderer.profile);

        let mut inner = |transparency| {
            ForwardRoutine::new(
                renderer,
                data_core,
                spp,
                interfaces,
                &per_material,
                None,
                None,
                &[],
                match transparency {
                    TransparencyType::Opaque | TransparencyType::Cutout => None,
                    TransparencyType::Blend => Some(BlendState::ALPHA_BLENDING),
                },
                wgpu::PrimitiveTopology::TriangleList,
                match transparency {
                    TransparencyType::Opaque => "opaque pass",
                    TransparencyType::Cutout => "cutout pass",
                    TransparencyType::Blend => "blend forward pass",
                },
            )
        };

        Self {
            opaque_routine: inner(TransparencyType::Opaque),
            cutout_routine: inner(TransparencyType::Cutout),
            blend_routine: inner(TransparencyType::Blend),
            per_material,
        }
    }
}
