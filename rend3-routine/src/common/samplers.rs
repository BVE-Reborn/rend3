use std::num::NonZeroU8;

use rend3::util::bind_merge::{BindGroupBuilder, BindGroupLayoutBuilder};
use wgpu::{
    AddressMode, BindingType, CompareFunction, Device, FilterMode, Sampler, SamplerBindingType, SamplerDescriptor,
    ShaderStages,
};

/// Container holding a variety of samplers.
pub struct Samplers {
    /// Aniso 16 sampler
    pub linear: Sampler,
    /// Nearest neighbor sampler
    pub nearest: Sampler,
    /// Bilinear greater-or-equal comparison sampler
    pub shadow: Sampler,
}

impl Samplers {
    /// Create a new set of samplers with this device.
    pub fn new(device: &Device) -> Self {
        profiling::scope!("Samplers::new");

        let linear = create_sampler(device, FilterMode::Linear, None);
        let nearest = create_sampler(device, FilterMode::Nearest, None);
        let shadow = create_sampler(device, FilterMode::Linear, Some(CompareFunction::GreaterEqual));

        Self {
            linear,
            nearest,
            shadow,
        }
    }

    /// Add the samplers to the given bind group layout builder.
    pub fn add_to_bgl(bglb: &mut BindGroupLayoutBuilder) {
        bglb.append(
            ShaderStages::FRAGMENT,
            BindingType::Sampler(SamplerBindingType::Filtering),
            None,
        )
        .append(
            ShaderStages::FRAGMENT,
            BindingType::Sampler(SamplerBindingType::NonFiltering),
            None,
        )
        .append(
            ShaderStages::FRAGMENT,
            BindingType::Sampler(SamplerBindingType::Comparison),
            None,
        );
    }

    /// Add the samplers to the given bind group builder.
    pub fn add_to_bg<'a>(&'a self, bgb: &mut BindGroupBuilder<'a>) {
        bgb.append_sampler(&self.linear)
            .append_sampler(&self.nearest)
            .append_sampler(&self.shadow);
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
        lod_min_clamp: 0.0,
        lod_max_clamp: 100.0,
        compare,
        anisotropy_clamp: NonZeroU8::new(16),
        border_color: None,
    })
}
