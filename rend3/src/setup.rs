#![cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]

use std::sync::Arc;

use wgpu::{
    Adapter, AdapterInfo, Backend, Backends, BufferAddress, Device, DeviceDescriptor, DeviceType, Features,
    Gles3MinorVersion, Instance, InstanceFlags, Limits, Queue,
};

#[allow(unused_imports)]
use crate::format_sso;
use crate::{
    managers::STARTING_2D_TEXTURES, util::typedefs::FastHashMap, LimitType, RendererInitializationError,
    RendererProfile,
};

/// Largest uniform buffer binding needed to run rend3.
pub const MAX_UNIFORM_BUFFER_BINDING_SIZE: BufferAddress = 1024;

/// Features required to run in the GpuDriven profile.
pub const GPU_DRIVEN_REQUIRED_FEATURES: Features = {
    // We need to do this whole bits thing to make this const as OpOr isn't const
    Features::from_bits_truncate(
        Features::PUSH_CONSTANTS.bits()
            | Features::TEXTURE_COMPRESSION_BC.bits()
            | Features::DEPTH_CLIP_CONTROL.bits()
            | Features::TEXTURE_BINDING_ARRAY.bits()
            | Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING.bits()
            | Features::PARTIALLY_BOUND_BINDING_ARRAY.bits()
            | Features::MULTI_DRAW_INDIRECT.bits()
            | Features::MULTI_DRAW_INDIRECT_COUNT.bits()
            | Features::SPIRV_SHADER_PASSTHROUGH.bits(),
    )
};

/// Features required to run in the GpuDriven profile.
pub const CPU_DRIVEN_REQUIRED_FEATURES: Features = Features::from_bits_truncate(0);

/// Features that rend3 can use if it they are available, but we don't require.
pub const OPTIONAL_FEATURES: Features = Features::from_bits_truncate(
    Features::DEPTH_CLIP_CONTROL.bits()
        | Features::TEXTURE_COMPRESSION_BC.bits()
        | Features::TEXTURE_COMPRESSION_ETC2.bits()
        | Features::TEXTURE_COMPRESSION_ASTC.bits()
        | Features::TIMESTAMP_QUERY.bits()
        | Features::TIMESTAMP_QUERY_INSIDE_PASSES.bits(),
);

/// Check that all required features for a given profile are present in the feature
/// set given.
pub fn check_features(profile: RendererProfile, device: Features) -> Result<Features, RendererInitializationError> {
    let required = match profile {
        RendererProfile::GpuDriven => GPU_DRIVEN_REQUIRED_FEATURES,
        RendererProfile::CpuDriven => CPU_DRIVEN_REQUIRED_FEATURES,
    };
    let optional = OPTIONAL_FEATURES & device;
    let missing = required - device;
    if !missing.is_empty() {
        Err(RendererInitializationError::MissingDeviceFeatures { features: missing })
    } else {
        Ok(required | optional)
    }
}

// TODO: this is all relatively antiquated and should be improved.

/// Limits required to run in the GpuDriven profile.
pub const GPU_REQUIRED_LIMITS: Limits = Limits {
    max_texture_dimension_1d: 2048,
    max_texture_dimension_2d: 2048,
    max_texture_dimension_3d: 512,
    max_texture_array_layers: 256,
    max_bind_groups: 6,
    max_dynamic_uniform_buffers_per_pipeline_layout: 0,
    max_dynamic_storage_buffers_per_pipeline_layout: 0,
    max_sampled_textures_per_shader_stage: STARTING_2D_TEXTURES as _,
    max_samplers_per_shader_stage: 2,
    max_storage_buffers_per_shader_stage: 5,
    max_storage_textures_per_shader_stage: 0,
    max_uniform_buffers_per_shader_stage: 2,
    max_uniform_buffer_binding_size: MAX_UNIFORM_BUFFER_BINDING_SIZE as u32,
    max_storage_buffer_binding_size: 128 << 20,
    max_vertex_buffer_array_stride: 128,
    max_vertex_buffers: 7,
    max_vertex_attributes: 7,
    max_bindings_per_bind_group: 640,
    max_push_constant_size: 128,
    min_uniform_buffer_offset_alignment: 256,
    min_storage_buffer_offset_alignment: 256,
    max_inter_stage_shader_components: 60,
    max_compute_workgroup_storage_size: 16352,
    max_compute_invocations_per_workgroup: 256,
    max_compute_workgroup_size_x: 256,
    max_compute_workgroup_size_y: 256,
    max_compute_workgroup_size_z: 64,
    max_compute_workgroups_per_dimension: 65535,
    max_buffer_size: 8192 * 8 * 18,
    max_non_sampler_bindings: 1_000_000,
    max_color_attachment_bytes_per_sample: 32, // From WGPU 0.20 (JN)
    max_color_attachments: 8,
    min_subgroup_size: 0,
    max_subgroup_size: 0,
};

/// Limits required to run in the CpuDriven profile.
pub const CPU_REQUIRED_LIMITS: Limits = Limits {
    max_texture_dimension_1d: 2048,
    max_texture_dimension_2d: 2048,
    max_texture_dimension_3d: 512,
    max_texture_array_layers: 256,
    max_bind_groups: 4,
    max_dynamic_uniform_buffers_per_pipeline_layout: 0,
    max_dynamic_storage_buffers_per_pipeline_layout: 0,
    max_sampled_textures_per_shader_stage: 10,
    max_samplers_per_shader_stage: 2,
    max_storage_buffers_per_shader_stage: 2,
    max_storage_textures_per_shader_stage: 0,
    max_uniform_buffers_per_shader_stage: 2,
    max_uniform_buffer_binding_size: MAX_UNIFORM_BUFFER_BINDING_SIZE as u32,
    max_storage_buffer_binding_size: 128 << 20,
    max_vertex_buffers: 6,
    max_vertex_attributes: 6,
    max_bindings_per_bind_group: 640,
    max_vertex_buffer_array_stride: 128,
    max_push_constant_size: 0,
    min_uniform_buffer_offset_alignment: 256,
    min_storage_buffer_offset_alignment: 256,
    max_inter_stage_shader_components: 60,
    max_compute_workgroup_storage_size: 16352,
    max_compute_invocations_per_workgroup: 256,
    max_compute_workgroup_size_x: 256,
    max_compute_workgroup_size_y: 256,
    max_compute_workgroup_size_z: 64,
    max_compute_workgroups_per_dimension: 65535,
    max_buffer_size: 8192 * 8 * 8,
    max_non_sampler_bindings: 1_000_000,
    max_color_attachment_bytes_per_sample: 32, // From WGPU 0.20 (JN)
    max_color_attachments: 8,
    min_subgroup_size: 0,
    max_subgroup_size: 0,
};

fn check_limit_unlimited<LimitValue: Into<u64> + Ord>(
    d: LimitValue,
    r: LimitValue,
    ty: LimitType,
) -> Result<LimitValue, RendererInitializationError> {
    if d < r {
        Err(RendererInitializationError::LowDeviceLimit { ty, device_limit: d.into(), required_limit: r.into() })
    } else {
        Ok(d)
    }
}

fn check_limit_low<LimitValue: Into<u64> + Ord>(
    d: LimitValue,
    r: LimitValue,
    ty: LimitType,
) -> Result<LimitValue, RendererInitializationError> {
    if r < d {
        Err(RendererInitializationError::LowDeviceLimit { ty, device_limit: d.into(), required_limit: r.into() })
    } else {
        Ok(d)
    }
}

/// Check that all required limits for a given profile are present in the given
/// limit set.
pub fn check_limits(profile: RendererProfile, device_limits: &Limits) -> Result<Limits, RendererInitializationError> {
    let required_limits = match profile {
        RendererProfile::GpuDriven => GPU_REQUIRED_LIMITS,
        RendererProfile::CpuDriven => CPU_REQUIRED_LIMITS,
    };

    Ok(Limits {
        max_texture_dimension_1d: check_limit_unlimited(
            device_limits.max_texture_dimension_1d,
            required_limits.max_texture_dimension_1d,
            LimitType::MaxTextureDimension1d,
        )?,
        max_texture_dimension_2d: check_limit_unlimited(
            device_limits.max_texture_dimension_2d,
            required_limits.max_texture_dimension_2d,
            LimitType::MaxTextureDimension2d,
        )?,
        max_texture_dimension_3d: check_limit_unlimited(
            device_limits.max_texture_dimension_3d,
            required_limits.max_texture_dimension_3d,
            LimitType::MaxTextureDimension3d,
        )?,
        max_texture_array_layers: check_limit_unlimited(
            device_limits.max_texture_array_layers,
            required_limits.max_texture_array_layers,
            LimitType::MaxTextureArrayLayers,
        )?,
        max_bind_groups: check_limit_unlimited(
            device_limits.max_bind_groups,
            required_limits.max_bind_groups,
            LimitType::BindGroups,
        )?,
        max_bindings_per_bind_group: check_limit_unlimited(
            device_limits.max_bindings_per_bind_group,
            required_limits.max_bindings_per_bind_group,
            LimitType::MaxBindingsPerBindGroup,
        )?,
        max_dynamic_uniform_buffers_per_pipeline_layout: check_limit_unlimited(
            device_limits.max_dynamic_uniform_buffers_per_pipeline_layout,
            required_limits.max_dynamic_uniform_buffers_per_pipeline_layout,
            LimitType::DynamicUniformBuffersPerPipelineLayout,
        )?,
        max_dynamic_storage_buffers_per_pipeline_layout: check_limit_unlimited(
            device_limits.max_dynamic_storage_buffers_per_pipeline_layout,
            required_limits.max_dynamic_storage_buffers_per_pipeline_layout,
            LimitType::DynamicStorageBuffersPerPipelineLayout,
        )?,
        max_sampled_textures_per_shader_stage: check_limit_unlimited(
            device_limits.max_sampled_textures_per_shader_stage,
            required_limits.max_sampled_textures_per_shader_stage,
            LimitType::SampledTexturesPerShaderStages,
        )?,
        max_samplers_per_shader_stage: check_limit_unlimited(
            device_limits.max_samplers_per_shader_stage,
            required_limits.max_samplers_per_shader_stage,
            LimitType::SamplersPerShaderStages,
        )?,
        max_storage_buffers_per_shader_stage: check_limit_unlimited(
            device_limits.max_storage_buffers_per_shader_stage,
            required_limits.max_storage_buffers_per_shader_stage,
            LimitType::StorageBuffersPerShaderStages,
        )?,
        max_storage_textures_per_shader_stage: check_limit_unlimited(
            device_limits.max_storage_textures_per_shader_stage,
            required_limits.max_storage_textures_per_shader_stage,
            LimitType::StorageTexturesPerShaderStages,
        )?,
        max_uniform_buffers_per_shader_stage: check_limit_unlimited(
            device_limits.max_uniform_buffers_per_shader_stage,
            required_limits.max_uniform_buffers_per_shader_stage,
            LimitType::StorageTexturesPerShaderStages,
        )?,
        max_uniform_buffer_binding_size: check_limit_unlimited(
            device_limits.max_uniform_buffer_binding_size,
            required_limits.max_uniform_buffer_binding_size,
            LimitType::UniformBufferBindingSize,
        )?,
        max_storage_buffer_binding_size: check_limit_unlimited(
            device_limits.max_storage_buffer_binding_size,
            required_limits.max_storage_buffer_binding_size,
            LimitType::MaxStorageBufferBindingSize,
        )?,
        max_vertex_buffers: check_limit_unlimited(
            device_limits.max_vertex_buffers,
            required_limits.max_vertex_buffers,
            LimitType::MaxVertexBuffers,
        )?,
        max_vertex_attributes: check_limit_unlimited(
            device_limits.max_vertex_attributes,
            required_limits.max_vertex_attributes,
            LimitType::MaxVertexAttributes,
        )?,
        max_vertex_buffer_array_stride: check_limit_unlimited(
            device_limits.max_vertex_buffer_array_stride,
            required_limits.max_vertex_buffer_array_stride,
            LimitType::MaxVertexBufferArrayStride,
        )?,
        max_push_constant_size: check_limit_unlimited(
            device_limits.max_push_constant_size,
            required_limits.max_push_constant_size,
            LimitType::PushConstantSize,
        )?,
        min_storage_buffer_offset_alignment: check_limit_low(
            device_limits.min_storage_buffer_offset_alignment,
            required_limits.min_storage_buffer_offset_alignment,
            LimitType::StorageBufferBindingAlignment,
        )?,
        min_uniform_buffer_offset_alignment: check_limit_low(
            device_limits.min_uniform_buffer_offset_alignment,
            required_limits.min_uniform_buffer_offset_alignment,
            LimitType::UniformBufferBindingAlignment,
        )?,
        max_inter_stage_shader_components: check_limit_unlimited(
            device_limits.max_inter_stage_shader_components,
            required_limits.max_inter_stage_shader_components,
            LimitType::MaxInterStageShaderComponents,
        )?,
        max_compute_workgroup_storage_size: check_limit_unlimited(
            device_limits.max_compute_workgroup_storage_size,
            required_limits.max_compute_workgroup_storage_size,
            LimitType::MaxComputeWorkgroupStorageSize,
        )?,
        max_compute_invocations_per_workgroup: check_limit_unlimited(
            device_limits.max_compute_invocations_per_workgroup,
            required_limits.max_compute_invocations_per_workgroup,
            LimitType::MaxComputeInvocationsPerWorkgroup,
        )?,
        max_compute_workgroup_size_x: check_limit_unlimited(
            device_limits.max_compute_workgroup_size_x,
            required_limits.max_compute_workgroup_size_x,
            LimitType::MaxComputeWorkgroupSizeX,
        )?,
        max_compute_workgroup_size_y: check_limit_unlimited(
            device_limits.max_compute_workgroup_size_y,
            required_limits.max_compute_workgroup_size_y,
            LimitType::MaxComputeWorkgroupSizeY,
        )?,
        max_compute_workgroup_size_z: check_limit_unlimited(
            device_limits.max_compute_workgroup_size_z,
            required_limits.max_compute_workgroup_size_z,
            LimitType::MaxComputeWorkgroupSizeZ,
        )?,
        max_compute_workgroups_per_dimension: check_limit_unlimited(
            device_limits.max_compute_workgroups_per_dimension,
            required_limits.max_compute_workgroups_per_dimension,
            LimitType::MaxComputeWorkgroupsPerDimension,
        )?,
        max_buffer_size: check_limit_unlimited(
            device_limits.max_buffer_size,
            required_limits.max_buffer_size,
            LimitType::MaxBufferSize,
        )?,
        max_non_sampler_bindings: 1_000_000,
        //  New in WGPU 0.20 (JN)
        max_color_attachment_bytes_per_sample: check_limit_unlimited(
            device_limits.max_color_attachment_bytes_per_sample,
            required_limits.max_color_attachment_bytes_per_sample,
            LimitType::MaxColorAttachmentBytesPerSample,
        )?,
        max_color_attachments: check_limit_unlimited(
            device_limits.max_color_attachments,
            required_limits.max_color_attachments,
            LimitType::MaxColorAttachments,
        )?,
        max_subgroup_size: check_limit_unlimited(
            device_limits.max_subgroup_size,
            required_limits.max_subgroup_size,
            LimitType::MaxSubgroupSize,
        )?,
        min_subgroup_size: check_limit_unlimited(
            device_limits.min_subgroup_size,
            required_limits.min_subgroup_size,
            LimitType::MinSubgroupSize,
        )?,
    })
}

/// Validated set of features and limits for a given T.
pub struct PotentialAdapter<T> {
    pub inner: T,
    pub info: ExtendedAdapterInfo,
    pub features: Features,
    pub limits: Limits,
    pub profile: RendererProfile,
}
impl<T> PotentialAdapter<T> {
    pub fn new(
        inner: T,
        inner_info: AdapterInfo,
        inner_limits: Limits,
        inner_features: Features,
        desired_profile: Option<RendererProfile>,
    ) -> Result<Self, RendererInitializationError> {
        let info = ExtendedAdapterInfo::from(inner_info);

        let mut features = check_features(RendererProfile::GpuDriven, inner_features);
        let mut limits = check_limits(RendererProfile::GpuDriven, &inner_limits);
        let mut profile = RendererProfile::GpuDriven;

        if (features.is_err() || limits.is_err() || desired_profile == Some(RendererProfile::CpuDriven))
            && desired_profile != Some(RendererProfile::GpuDriven)
        {
            features = check_features(RendererProfile::CpuDriven, inner_features);
            limits = check_limits(RendererProfile::CpuDriven, &inner_limits);
            profile = RendererProfile::CpuDriven;
        }

        Ok(PotentialAdapter { inner, info, features: features?, limits: limits?, profile })
    }
}

/// Set of common GPU vendors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Vendor {
    Nv,
    Amd,
    Intel,
    Microsoft,
    Arm,
    Broadcom,
    Qualcomm,
    /// Don't recognize this vendor. This is the given PCI id.
    Unknown(usize),
}

/// Information about an adapter. Includes named PCI IDs for vendors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtendedAdapterInfo {
    /// Adapter name
    pub name: String,
    /// Vendor/brand of adapter.
    pub vendor: Vendor,
    /// PCI id of the adapter.
    pub device: usize,
    /// Type of device.
    pub device_type: DeviceType,
    /// Backend used for device
    pub backend: Backend,
}

impl From<AdapterInfo> for ExtendedAdapterInfo {
    fn from(info: AdapterInfo) -> Self {
        Self {
            name: info.name,
            vendor: match info.vendor {
                0x1002 => Vendor::Amd,
                0x10DE => Vendor::Nv,
                0x13B5 => Vendor::Arm,
                0x1414 => Vendor::Microsoft,
                0x14E4 => Vendor::Broadcom,
                0x5143 => Vendor::Qualcomm,
                0x8086 => Vendor::Intel,
                v => Vendor::Unknown(v as usize),
            },
            device: info.device as usize,
            device_type: info.device_type,
            backend: info.backend,
        }
    }
}

/// Container for Instance/Adapter/Device/Queue etc.
///
/// Create these yourself, or call [`create_iad`].
#[derive(Clone)]
pub struct InstanceAdapterDevice {
    pub instance: Arc<Instance>,
    pub adapter: Arc<Adapter>,
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    pub profile: RendererProfile,
    pub info: ExtendedAdapterInfo,
}

/// Creates an Instance/Adapter/Device/Queue using the given choices. Tries to
/// get the best combination.
///
/// **NOTE:** Some adapters will not advertise all of its supported features.
/// The `additional_features` parameter can be used to explicitly request
/// additional features during device creation.
pub async fn create_iad(
    desired_backend: Option<Backend>,
    desired_device: Option<String>,
    desired_profile: Option<RendererProfile>,
    additional_features: Option<Features>,
) -> Result<InstanceAdapterDevice, RendererInitializationError> {
    profiling::scope!("create_iad");
    #[cfg(not(target_arch = "wasm32"))]
    let backend_bits = Backends::VULKAN |
         // Backends::DX12 | https://github.com/gfx-rs/wgpu/issues/4423
         Backends::METAL;
    #[cfg(target_arch = "wasm32")]
    let backend_bits = Backends::BROWSER_WEBGPU;
    #[cfg(not(target_arch = "wasm32"))]
    let default_backend_order = [Backend::Vulkan, Backend::Metal, Backend::Dx12, Backend::Gl];
    #[cfg(target_arch = "wasm32")]
    let default_backend_order = [Backend::BrowserWebGpu];

    let instance = Instance::new(&wgpu::InstanceDescriptor {
        backends: backend_bits,
        //////dx12_shader_compiler: wgpu::Dx12Compiler::Fxc,
        //////gles_minor_version: Gles3MinorVersion::default(),
        backend_options: wgpu::BackendOptions { // per refactoring at WGPU
            gl: wgpu::GlBackendOptions { gles_minor_version: Gles3MinorVersion::default()},
            dx12: wgpu::Dx12BackendOptions { shader_compiler: wgpu::Dx12Compiler::Fxc, },
        },
        flags: InstanceFlags::default(),
    });

    let mut valid_adapters = FastHashMap::<Backend, Vec<PotentialAdapter<Adapter>>>::default();

    for backend in &default_backend_order {
        profiling::scope!("enumerating backend");
        #[cfg(not(target_arch = "wasm32"))]
        let adapters = instance.enumerate_adapters(Backends::from(*backend));
        #[cfg(target_arch = "wasm32")]
        let adapters = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .into_iter();

        let mut potential_adapters = Vec::new();
        for (idx, adapter) in adapters.into_iter().enumerate() {
            let info = adapter.get_info();
            let limits = adapter.limits();
            let features = adapter.features();
            let potential = PotentialAdapter::new(adapter, info, limits, features, desired_profile);

            log::info!("{:?} Adapter {}: {:#?}", backend, idx, potential.as_ref().map(|p| &p.info));

            let desired = if let Some(ref desired_device) = desired_device {
                potential.as_ref().map(|i| i.info.name.to_lowercase().contains(desired_device)).unwrap_or(false)
            } else {
                true
            };

            if let (Ok(potential), true) = (potential, desired) {
                log::debug!("Adapter usable in the {:?} profile", potential.profile);
                potential_adapters.push(potential)
            } else {
                log::debug!("Adapter not usable");
            }
        }
        valid_adapters.insert(*backend, potential_adapters);
    }

    for backend_adapters in valid_adapters.values_mut() {
        backend_adapters.sort_by_key(|a: &PotentialAdapter<Adapter>| match a.info.device_type {
            DeviceType::DiscreteGpu => 0,
            DeviceType::IntegratedGpu => 1,
            DeviceType::VirtualGpu => 2,
            DeviceType::Cpu => 3,
            DeviceType::Other => 4,
        });
    }

    for backend in &default_backend_order {
        if let Some(desired_backend) = desired_backend {
            if desired_backend != *backend {
                log::debug!("Skipping unwanted backend {:?}", backend);
                continue;
            }
        }

        let adapter: Option<PotentialAdapter<Adapter>> =
            valid_adapters.remove(backend).and_then(|arr| arr.into_iter().next());

        if let Some(adapter) = adapter {
            log::debug!("Chosen adapter: {:#?}", adapter.info);
            log::debug!("Chosen backend: {:?}", backend);
            log::debug!("Chosen features: {:#?}", adapter.features);
            log::debug!("Chosen limits: {:#?}", adapter.limits);
            log::debug!("Chosen profile: {:#?}", adapter.profile);

            let (device, queue) = adapter
                .inner
                .request_device(
                    &DeviceDescriptor {
                        label: None,
                        required_features: adapter.features.union(additional_features.unwrap_or_else(Features::empty)),
                        required_limits: adapter.limits,
                        memory_hints: Default::default(), // Use default for memory hints for now. Possible future optimization.
                    },
                    None,
                )
                .await
                .map_err(|_| RendererInitializationError::RequestDeviceFailed)?;

            return Ok(InstanceAdapterDevice {
                instance: Arc::new(instance),
                adapter: Arc::new(adapter.inner),
                device: Arc::new(device),
                queue: Arc::new(queue),
                profile: adapter.profile,
                info: adapter.info,
            });
        }
    }

    Err(RendererInitializationError::MissingAdapter)
}
