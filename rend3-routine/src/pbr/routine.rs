use std::borrow::Cow;

use rend3::{Renderer, RendererDataCore, RendererProfile, ShaderPreProcessor, ShaderVertexBufferConfig};
use serde::Serialize;
use wgpu::{BlendState, ShaderModuleDescriptor, ShaderSource};

use crate::{
    common::{PerMaterialArchetypeInterface, WholeFrameInterfaces},
    forward::{ForwardRoutine, RoutineArgs, RoutineType, ShaderModulePair},
    pbr::{PbrMaterial, TransparencyType},
};

#[derive(Serialize)]
struct BlendModeWrapper {
    profile: RendererProfile,
    discard: bool,
}

/// Render routine that renders the using PBR materials
pub struct PbrRoutine {
    pub opaque_depth: ForwardRoutine<PbrMaterial>,
    pub cutout_depth: ForwardRoutine<PbrMaterial>,
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

        let per_material = PerMaterialArchetypeInterface::<PbrMaterial>::new(&renderer.device);

        let pbr_depth_cutout = renderer.device.create_shader_module(ShaderModuleDescriptor {
            label: Some("pbr depth cutout sm"),
            source: ShaderSource::Wgsl(Cow::Owned(
                spp.render_shader(
                    "rend3-routine/depth.wgsl",
                    &BlendModeWrapper {
                        profile: renderer.profile,
                        discard: true,
                    },
                    Some(&ShaderVertexBufferConfig::from_material::<PbrMaterial>()),
                )
                .unwrap(),
            )),
        });

        let pbr_depth = renderer.device.create_shader_module(ShaderModuleDescriptor {
            label: Some("pbr depth sm"),
            source: ShaderSource::Wgsl(Cow::Owned(
                spp.render_shader(
                    "rend3-routine/depth.wgsl",
                    &BlendModeWrapper {
                        profile: renderer.profile,
                        discard: false,
                    },
                    Some(&ShaderVertexBufferConfig::from_material::<PbrMaterial>()),
                )
                .unwrap(),
            )),
        });

        let pbr_cutout = renderer.device.create_shader_module(ShaderModuleDescriptor {
            label: Some("pbr opaque cutout sm"),
            source: ShaderSource::Wgsl(Cow::Owned(
                spp.render_shader(
                    "rend3-routine/opaque.wgsl",
                    &BlendModeWrapper {
                        profile: renderer.profile,
                        discard: true,
                    },
                    Some(&ShaderVertexBufferConfig::from_material::<PbrMaterial>()),
                )
                .unwrap(),
            )),
        });

        let pbr_forward = renderer.device.create_shader_module(ShaderModuleDescriptor {
            label: Some("pbr opaque sm"),
            source: ShaderSource::Wgsl(Cow::Owned(
                spp.render_shader(
                    "rend3-routine/opaque.wgsl",
                    &BlendModeWrapper {
                        profile: renderer.profile,
                        discard: false,
                    },
                    Some(&ShaderVertexBufferConfig::from_material::<PbrMaterial>()),
                )
                .unwrap(),
            )),
        });

        let mut inner = |routine_type, module, transparency| {
            ForwardRoutine::new(RoutineArgs {
                name: &format!("pbr {routine_type:?} {transparency:?}"),
                renderer,
                data_core,
                spp,
                interfaces,
                per_material: &per_material,
                material_key: transparency as u64,
                routine_type,
                shaders: ShaderModulePair {
                    vs_entry: "vs_main",
                    vs_module: module,
                    fs_entry: "fs_main",
                    fs_module: module,
                },
                extra_bgls: &[],
                descriptor_callback: Some(&|desc, targets| {
                    if transparency == TransparencyType::Blend {
                        desc.depth_stencil.as_mut().unwrap().depth_write_enabled = false;
                        targets[0].as_mut().unwrap().blend = Some(BlendState::ALPHA_BLENDING)
                    }
                }),
            })
        };

        Self {
            opaque_depth: inner(RoutineType::Depth, &pbr_depth, TransparencyType::Opaque),
            cutout_depth: inner(RoutineType::Depth, &pbr_depth_cutout, TransparencyType::Cutout),
            opaque_routine: inner(RoutineType::Forward, &pbr_forward, TransparencyType::Opaque),
            cutout_routine: inner(RoutineType::Forward, &pbr_cutout, TransparencyType::Cutout),
            blend_routine: inner(RoutineType::Forward, &pbr_forward, TransparencyType::Blend),
            per_material,
        }
    }
}
