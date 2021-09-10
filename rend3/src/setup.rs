use std::sync::Arc;

use arrayvec::ArrayVec;
use fnv::FnvHashMap;
use wgpu::{
    Adapter, AdapterInfo, Backend, Backends, BufferAddress, Device, DeviceDescriptor, DeviceType, Features, Instance,
    Limits, Queue,
};

use crate::{
    renderer::info::ExtendedAdapterInfo, resources::STARTING_2D_TEXTURES, LimitType, RendererInitializationError,
    RendererMode,
};

pub const MAX_UNIFORM_BUFFER_BINDING_SIZE: BufferAddress = 1024;

pub fn gpu_required_features() -> Features {
    wgpu::Features::PUSH_CONSTANTS
        | wgpu::Features::TEXTURE_COMPRESSION_BC
        | wgpu::Features::DEPTH_CLAMPING
        | wgpu::Features::TEXTURE_BINDING_ARRAY
        | wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING
        | wgpu::Features::UNSIZED_BINDING_ARRAY
        | wgpu::Features::MULTI_DRAW_INDIRECT
        | wgpu::Features::MULTI_DRAW_INDIRECT_COUNT
        | wgpu::Features::SPIRV_SHADER_PASSTHROUGH
}

pub fn cpu_required_features() -> Features {
    Features::empty()
}

pub fn optional_features() -> Features {
    wgpu::Features::TEXTURE_COMPRESSION_BC | wgpu::Features::TIMESTAMP_QUERY
}

pub fn check_features(mode: RendererMode, device: Features) -> Result<Features, RendererInitializationError> {
    let required = match mode {
        RendererMode::GPUPowered => gpu_required_features(),
        RendererMode::CPUPowered => cpu_required_features(),
    };
    let optional = optional_features() & device;
    let missing = required - device;
    if !missing.is_empty() {
        Err(RendererInitializationError::MissingDeviceFeatures { features: missing })
    } else {
        Ok(required | optional)
    }
}

pub const GPU_REQUIRED_LIMITS: Limits = Limits {
    max_texture_dimension_1d: 2048,
    max_texture_dimension_2d: 2048,
    max_texture_dimension_3d: 512,
    max_texture_array_layers: 512,
    max_bind_groups: 8,
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
    max_push_constant_size: 128,
};

pub const CPU_REQUIRED_LIMITS: Limits = Limits {
    max_texture_dimension_1d: 2048,
    max_texture_dimension_2d: 2048,
    max_texture_dimension_3d: 512,
    max_texture_array_layers: 512,
    max_bind_groups: 8,
    max_dynamic_uniform_buffers_per_pipeline_layout: 0,
    max_dynamic_storage_buffers_per_pipeline_layout: 0,
    max_sampled_textures_per_shader_stage: 10,
    max_samplers_per_shader_stage: 2,
    max_storage_buffers_per_shader_stage: 2,
    max_storage_textures_per_shader_stage: 0,
    max_uniform_buffers_per_shader_stage: 2,
    max_uniform_buffer_binding_size: MAX_UNIFORM_BUFFER_BINDING_SIZE as u32,
    max_storage_buffer_binding_size: 128 << 20,
    max_vertex_buffer_array_stride: 128,
    max_vertex_buffers: 6,
    max_vertex_attributes: 6,
    max_push_constant_size: 0,
};

fn check_limit_unlimited(d: u32, r: u32, ty: LimitType) -> Result<u32, RendererInitializationError> {
    if d < r {
        Err(RendererInitializationError::LowDeviceLimit {
            ty,
            device_limit: d,
            required_limit: r,
        })
    } else {
        Ok(d)
    }
}

pub fn check_limits(mode: RendererMode, device_limits: &Limits) -> Result<Limits, RendererInitializationError> {
    let required_limits = match mode {
        RendererMode::GPUPowered => GPU_REQUIRED_LIMITS,
        RendererMode::CPUPowered => CPU_REQUIRED_LIMITS,
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
    })
}

pub struct PotentialAdapter<T> {
    pub inner: T,
    pub info: ExtendedAdapterInfo,
    pub features: Features,
    pub limits: Limits,
    pub mode: RendererMode,
}
impl<T> PotentialAdapter<T> {
    pub fn new(
        inner: T,
        inner_info: AdapterInfo,
        inner_limits: Limits,
        inner_features: Features,
        desired_mode: Option<RendererMode>,
    ) -> Result<Self, RendererInitializationError> {
        let info = ExtendedAdapterInfo::from(inner_info);

        let mut features = check_features(RendererMode::GPUPowered, inner_features);
        let mut limits = check_limits(RendererMode::GPUPowered, &inner_limits);
        let mut mode = RendererMode::GPUPowered;

        if (features.is_err() || limits.is_err() || desired_mode == Some(RendererMode::CPUPowered))
            && desired_mode != Some(RendererMode::GPUPowered)
        {
            features = check_features(RendererMode::CPUPowered, inner_features);
            limits = check_limits(RendererMode::CPUPowered, &inner_limits);
            mode = RendererMode::CPUPowered;
        }

        Ok(PotentialAdapter {
            inner,
            info,
            features: features?,
            limits: limits?,
            mode,
        })
    }
}

pub struct InstanceAdapterDevice {
    pub instance: Arc<Instance>,
    pub adapter: Arc<Adapter>,
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    pub mode: RendererMode,
    pub info: ExtendedAdapterInfo,
}

pub async fn create_iad(
    desired_backend: Option<Backend>,
    desired_device: Option<String>,
    desired_mode: Option<RendererMode>,
) -> Result<InstanceAdapterDevice, RendererInitializationError> {
    let backend_bits = Backends::VULKAN | Backends::DX12 | Backends::DX11 | Backends::METAL | Backends::GL;
    let default_backend_order = [
        Backend::Vulkan,
        Backend::Metal,
        Backend::Dx12,
        Backend::Dx11,
        Backend::Gl,
    ];

    let instance = Instance::new(backend_bits);

    let mut valid_adapters = FnvHashMap::default();

    for backend in &default_backend_order {
        let adapters = instance.enumerate_adapters(Backends::from(*backend));

        let mut potential_adapters = ArrayVec::<PotentialAdapter<Adapter>, 4>::new();
        for (idx, adapter) in adapters.enumerate() {
            let info = adapter.get_info();
            let limits = adapter.limits();
            let features = adapter.features();
            let potential = PotentialAdapter::new(adapter, info, limits, features, desired_mode);

            log::debug!(
                "{:?} Adapter {}: {:#?}",
                backend,
                idx,
                potential.as_ref().map(|p| &p.info)
            );

            let desired = if let Some(ref desired_device) = desired_device {
                potential
                    .as_ref()
                    .map(|i| i.info.name.to_lowercase().contains(desired_device))
                    .unwrap_or(false)
            } else {
                true
            };

            if let (Ok(potential), true) = (potential, desired) {
                log::debug!("Adapter usable in {:?} mode", potential.mode);
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
            log::debug!("Chosen mode: {:#?}", adapter.mode);

            let (device, queue) = adapter
                .inner
                .request_device(
                    &DeviceDescriptor {
                        label: None,
                        features: adapter.features,
                        limits: adapter.limits,
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
                mode: adapter.mode,
                info: adapter.info,
            });
        }
    }

    Err(RendererInitializationError::MissingAdapter)
}
