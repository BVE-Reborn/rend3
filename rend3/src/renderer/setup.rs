use crate::{
    instruction::InstructionStreamPair,
    renderer::{
        copy::GpuCopy,
        culling,
        info::ExtendedAdapterInfo,
        light::DirectionalLightManager,
        limits::{check_features, check_limits},
        list::RenderListCache,
        material::MaterialManager,
        mesh::MeshManager,
        object::ObjectManager,
        pipeline::PipelineManager,
        resources::RendererGlobalResources,
        shaders::ShaderManager,
        texture::{TextureManager, STARTING_2D_TEXTURES, STARTING_CUBE_TEXTURES},
        Renderer, RendererMode,
    },
    RendererInitializationError, RendererOptions,
};
use arrayvec::ArrayVec;
use fnv::FnvHashMap;
use parking_lot::{Mutex, RwLock};
use raw_window_handle::HasRawWindowHandle;
use std::sync::Arc;
use switchyard::Switchyard;
use wgpu::{
    Adapter, Backend, BackendBit, DeviceDescriptor, DeviceType, Features, Instance, Limits, TextureViewDimension,
};
use wgpu_conveyor::{AutomatedBufferManager, UploadStyle};

pub struct PotentialAdapter {
    inner: Adapter,
    info: ExtendedAdapterInfo,
    features: Features,
    limits: Limits,
    mode: RendererMode,
}

pub fn create_adapter(
    desired_backend: Option<Backend>,
    desired_device: Option<String>,
    desired_mode: Option<RendererMode>,
) -> Result<(Instance, PotentialAdapter), RendererInitializationError> {
    let backend_bits = BackendBit::VULKAN | BackendBit::DX12 | BackendBit::DX11 | BackendBit::METAL;
    let default_backend_order = [Backend::Vulkan, Backend::Metal, Backend::Dx12, Backend::Dx11];

    let instance = Instance::new(backend_bits);

    let mut valid_adapters = FnvHashMap::default();

    for backend in &default_backend_order {
        let adapters = instance.enumerate_adapters(BackendBit::from(*backend));

        let mut potential_adapters = ArrayVec::<[PotentialAdapter; 4]>::new();
        for (idx, adapter) in adapters.enumerate() {
            let info = ExtendedAdapterInfo::from(adapter.get_info());

            tracing::debug!("{:?} Adapter {}: {:#?}", backend, idx, info);

            let adapter_features = adapter.features();
            let adapter_limits = adapter.limits();

            tracing::trace!("Features: {:?}", adapter_features);
            tracing::trace!("Limits: {:#?}", adapter_limits);

            let mut features = check_features(RendererMode::GPUPowered, adapter_features).ok();
            let mut limits = check_limits(RendererMode::GPUPowered, &adapter_limits).ok();
            let mut mode = RendererMode::GPUPowered;

            if (features.is_none() || limits.is_none() || desired_mode == Some(RendererMode::CPUPowered))
                && desired_mode != Some(RendererMode::GPUPowered)
            {
                features = check_features(RendererMode::CPUPowered, adapter_features).ok();
                limits = check_limits(RendererMode::CPUPowered, &adapter_limits).ok();
                mode = RendererMode::CPUPowered;
            }

            let desired = if let Some(ref desired_device) = desired_device {
                info.name.to_lowercase().contains(desired_device)
            } else {
                true
            };

            if let (Some(features), Some(limits), true) = (features, limits, desired) {
                tracing::debug!("Adapter usable in {:?} mode", mode);
                potential_adapters.push(PotentialAdapter {
                    inner: adapter,
                    info,
                    features,
                    limits,
                    mode,
                })
            } else {
                tracing::debug!("Adapter not usable");
            }
        }
        valid_adapters.insert(*backend, potential_adapters);
    }

    for backend_adapters in valid_adapters.values_mut() {
        backend_adapters.sort_by_key(|a: &PotentialAdapter| match a.info.device_type {
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
                tracing::debug!("Skipping unwanted backend {:?}", backend);
                continue;
            }
        }

        let adapter: Option<PotentialAdapter> = valid_adapters.remove(backend).and_then(|arr| arr.into_iter().next());

        if let Some(adapter) = adapter {
            tracing::debug!("Chosen adapter: {:#?}", adapter.info);
            tracing::debug!("Chosen backend: {:?}", backend);
            tracing::debug!("Chosen features: {:#?}", adapter.features);
            tracing::debug!("Chosen limits: {:#?}", adapter.limits);
            return Ok((instance, adapter));
        }
    }

    Err(RendererInitializationError::MissingAdapter)
}

pub async fn create_renderer<W: HasRawWindowHandle, TLD: 'static>(
    window: &W,
    yard: Arc<Switchyard<TLD>>,
    _imgui: &mut imgui::Context,
    desired_backend: Option<Backend>,
    desired_device: Option<String>,
    desired_mode: Option<RendererMode>,
    options: RendererOptions,
) -> Result<Arc<Renderer<TLD>>, RendererInitializationError> {
    let (instance, chosen_adapter) = create_adapter(desired_backend, desired_device, desired_mode)?;

    let surface = unsafe { instance.create_surface(window) };

    let adapter_info = chosen_adapter.info;

    let (device, queue) = chosen_adapter
        .inner
        .request_device(
            &DeviceDescriptor {
                features: chosen_adapter.features,
                limits: chosen_adapter.limits,
                shader_validation: true,
            },
            None,
        )
        .await
        .map_err(|_| RendererInitializationError::RequestDeviceFailed)?;

    let device = Arc::new(device);

    let shader_manager = ShaderManager::new(Arc::clone(&device));

    let mut global_resources = RwLock::new(RendererGlobalResources::new(
        &device,
        &surface,
        chosen_adapter.mode,
        &options,
    ));
    let global_resource_guard = global_resources.get_mut();

    let gpu_copy = GpuCopy::new(&device, &shader_manager, adapter_info.subgroup_size());

    let culling_pass = culling::CullingPass::new(
        &device,
        chosen_adapter.mode,
        &shader_manager,
        &global_resource_guard.prefix_sum_bgl,
        &global_resource_guard.pre_cull_bgl,
        &global_resource_guard.object_input_bgl,
        &global_resource_guard.object_output_bgl,
        &global_resource_guard.camera_data_bgl,
        adapter_info.subgroup_size(),
    );

    let texture_manager_2d = RwLock::new(TextureManager::new(
        &device,
        chosen_adapter.mode,
        STARTING_2D_TEXTURES,
        TextureViewDimension::D2,
    ));
    let texture_manager_cube = RwLock::new(TextureManager::new(
        &device,
        chosen_adapter.mode,
        STARTING_CUBE_TEXTURES,
        TextureViewDimension::Cube,
    ));

    let pipeline_manager = PipelineManager::new();

    let mut buffer_manager = Mutex::new(AutomatedBufferManager::new(UploadStyle::from_device_type(
        &adapter_info.device_type,
    )));
    let mesh_manager = RwLock::new(MeshManager::new(&device));
    let material_manager = RwLock::new(MaterialManager::new(
        &device,
        chosen_adapter.mode,
        buffer_manager.get_mut(),
    ));
    let object_manager = RwLock::new(ObjectManager::new(
        &device,
        chosen_adapter.mode,
        buffer_manager.get_mut(),
    ));
    let directional_light_manager = RwLock::new(DirectionalLightManager::new(&device, buffer_manager.get_mut()));

    span_transfer!(_ -> imgui_guard, INFO, "Creating Imgui Renderer");

    // let imgui_renderer = imgui_wgpu::Renderer::new(imgui, &device, &queue, SWAPCHAIN_FORMAT);

    span_transfer!(imgui_guard -> _);

    let render_list_cache = RwLock::new(RenderListCache::new());

    let (culling_pass, gpu_copy) = futures::join!(culling_pass, gpu_copy);

    Ok(Arc::new(Renderer {
        yard,
        instructions: InstructionStreamPair::new(),

        mode: chosen_adapter.mode,
        adapter_info,
        queue,
        device,
        surface,

        buffer_manager,
        global_resources,
        shader_manager,
        pipeline_manager,
        mesh_manager,
        texture_manager_2d,
        texture_manager_cube,
        material_manager,
        object_manager,
        directional_light_manager,

        render_list_cache,

        gpu_copy,
        culling_pass,

        // _imgui_renderer: imgui_renderer,
        options: RwLock::new(options),
    }))
}
