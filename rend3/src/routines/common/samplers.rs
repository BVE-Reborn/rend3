use std::num::NonZeroU8;

use wgpu::{AddressMode, BindGroup, BindGroupLayout, CompareFunction, Device, FilterMode, Sampler, SamplerDescriptor};

use crate::util::bind_merge::BindGroupBuilder;

pub struct Samplers {
    pub linear: Sampler,
    pub nearest: Sampler,
    pub shadow: Sampler,

    pub bg: BindGroup,
}

impl Samplers {
    pub fn new(device: &Device, samplers_bgl: &BindGroupLayout) -> Self {
        let linear = create_sampler(device, FilterMode::Linear, None);
        let nearest = create_sampler(device, FilterMode::Nearest, None);
        let shadow = create_sampler(device, FilterMode::Linear, Some(CompareFunction::LessEqual));

        let bg = BindGroupBuilder::new(Some("samplers"))
            .with_sampler(&linear)
            .with_sampler(&nearest)
            .with_sampler(&shadow)
            .build(device, samplers_bgl);

        Self {
            linear,
            nearest,
            shadow,
            bg,
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
