use std::num::NonZeroU8;

use rend3::{util::bind_merge::BindGroupBuilder, ModeData, RendererMode};
use wgpu::{AddressMode, BindGroup, BindGroupLayout, CompareFunction, Device, FilterMode, Sampler, SamplerDescriptor};

pub struct Samplers {
    pub linear: Sampler,
    pub nearest: Sampler,
    pub shadow: Sampler,

    pub linear_nearest_bg: BindGroup,
    // OpenGL doesn't allow runtime switching of samplers, so we swap them out cpu side.
    pub nearest_linear_bg: ModeData<BindGroup, ()>,
}

impl Samplers {
    pub fn new(device: &Device, mode: RendererMode, samplers_bgl: &BindGroupLayout) -> Self {
        let linear = create_sampler(device, FilterMode::Linear, None);
        let nearest = create_sampler(device, FilterMode::Nearest, None);
        let shadow = create_sampler(device, FilterMode::Linear, Some(CompareFunction::LessEqual));

        let linear_nearest_bg = BindGroupBuilder::new(Some("linear-nearest samplers"))
            .with_sampler(&linear)
            .with_sampler(&nearest)
            .with_sampler(&shadow)
            .build(device, samplers_bgl);

        let nearest_linear_bg = mode.into_data(
            || {
                BindGroupBuilder::new(Some("samplers"))
                    .with_sampler(&nearest)
                    .with_sampler(&linear)
                    .with_sampler(&shadow)
                    .build(device, samplers_bgl)
            },
            || (),
        );

        Self {
            linear,
            nearest,
            shadow,
            linear_nearest_bg,
            nearest_linear_bg,
        }
    }
}

fn create_sampler(device: &Device, filter: FilterMode, compare: Option<CompareFunction>) -> Sampler {
    device.create_sampler(&SamplerDescriptor {
        label: Some("linear"),
        address_mode_u: AddressMode::Repeat,
        address_mode_v: AddressMode::Repeat,
        address_mode_w: AddressMode::Repeat,
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter: filter,
        lod_min_clamp: -100.0,
        lod_max_clamp: 100.0,
        compare,
        anisotropy_clamp: NonZeroU8::new(16),
        border_color: None,
    })
}
