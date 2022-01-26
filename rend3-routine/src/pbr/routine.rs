use rend3::{Renderer, RendererDataCore};
use wgpu::{BlendState, Features};

use crate::{
    common::{PerMaterialArchetypeInterface, WholeFrameInterfaces},
    depth::DepthRoutine,
    forward::ForwardRoutine,
    pbr::{PbrMaterial, TransparencyType},
};

/// Render routine that renders the using PBR materials
pub struct PbrRoutine {
    pub opaque_routine: ForwardRoutine<PbrMaterial>,
    pub cutout_routine: ForwardRoutine<PbrMaterial>,
    pub blend_routine: ForwardRoutine<PbrMaterial>,
    pub depth_pipelines: DepthRoutine<PbrMaterial>,
    pub per_material: PerMaterialArchetypeInterface<PbrMaterial>,
}

impl PbrRoutine {
    pub fn new(renderer: &Renderer, data_core: &mut RendererDataCore, interfaces: &WholeFrameInterfaces) -> Self {
        profiling::scope!("PbrRenderRoutine::new");

        // This ensures the BGLs for the material are created
        data_core
            .material_manager
            .ensure_archetype::<PbrMaterial>(&renderer.device, renderer.profile);

        let unclipped_depth_supported = renderer.features.contains(Features::DEPTH_CLIP_CONTROL);

        let per_material = PerMaterialArchetypeInterface::<PbrMaterial>::new(&renderer.device, renderer.profile);

        let depth_pipelines = DepthRoutine::<PbrMaterial>::new(
            renderer,
            data_core,
            interfaces,
            &per_material,
            unclipped_depth_supported,
        );

        let mut inner = |transparency| {
            ForwardRoutine::new(
                renderer,
                data_core,
                interfaces,
                &per_material,
                None,
                None,
                &[],
                match transparency {
                    TransparencyType::Opaque | TransparencyType::Cutout => None,
                    TransparencyType::Blend => Some(BlendState::ALPHA_BLENDING),
                },
                !matches!(transparency, TransparencyType::Blend),
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
            depth_pipelines,
            per_material,
        }
    }
}
