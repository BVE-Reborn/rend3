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
    },
    JobPriorities, Renderer, RendererBuilder, RendererInitializationError, RendererMode,
};
use arrayvec::ArrayVec;
use fnv::FnvHashMap;
use parking_lot::{Mutex, RwLock};
use raw_window_handle::HasRawWindowHandle;
use std::sync::Arc;
use wgpu::{
    Adapter, AdapterInfo, Backend, BackendBit, DeviceDescriptor, DeviceType, Features, Instance, Limits,
    TextureViewDimension,
};
use wgpu_conveyor::{AutomatedBufferManager, UploadStyle};

pub struct PotentialAdapter<T> {
    inner: T,
    info: ExtendedAdapterInfo,
    features: Features,
    limits: Limits,
    mode: RendererMode,
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

pub fn create_adapter(
    desired_backend: Option<Backend>,
    desired_device: Option<String>,
    desired_mode: Option<RendererMode>,
) -> Result<(Instance, PotentialAdapter<Adapter>), RendererInitializationError> {
    let backend_bits = BackendBit::VULKAN | BackendBit::DX12 | BackendBit::DX11 | BackendBit::METAL;
    let default_backend_order = [Backend::Vulkan, Backend::Metal, Backend::Dx12, Backend::Dx11];

    let instance = Instance::new(backend_bits);

    let mut valid_adapters = FnvHashMap::default();

    for backend in &default_backend_order {
        let adapters = instance.enumerate_adapters(BackendBit::from(*backend));

        let mut potential_adapters = ArrayVec::<PotentialAdapter<Adapter>, 4>::new();
        for (idx, adapter) in adapters.enumerate() {
            let info = adapter.get_info();
            let limits = adapter.limits();
            let features = adapter.features();
            let potential = PotentialAdapter::new(adapter, info, limits, features, desired_mode);

            tracing::debug!(
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
                tracing::debug!("Adapter usable in {:?} mode", potential.mode);
                potential_adapters.push(potential)
            } else {
                tracing::debug!("Adapter not usable");
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
                tracing::debug!("Skipping unwanted backend {:?}", backend);
                continue;
            }
        }

        let adapter: Option<PotentialAdapter<Adapter>> =
            valid_adapters.remove(backend).and_then(|arr| arr.into_iter().next());

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
    builder: RendererBuilder<'_, W, TLD>,
) -> Result<Arc<Renderer<TLD>>, RendererInitializationError> {
    let (instance, surface, device, queue, adapter_info, mode) = if let Some(crate::CustomDevice {
        instance,
        queue,
        device,
        info,
    }) = builder.device
    {
        let limits = device.limits();
        let features = device.features();
        let potential = PotentialAdapter::new(device, info, limits, features, builder.desired_mode)?;

        let surface = builder.window.map(|window| unsafe { instance.create_surface(window) });

        (
            instance,
            surface,
            potential.inner,
            queue,
            potential.info,
            potential.mode,
        )
    } else {
        let (instance, chosen_adapter) = create_adapter(
            builder.desired_backend,
            builder.desired_device_name,
            builder.desired_mode,
        )?;

        let adapter_info = chosen_adapter.info;

        let (device, queue) = chosen_adapter
            .inner
            .request_device(
                &DeviceDescriptor {
                    label: None,
                    features: chosen_adapter.features,
                    limits: chosen_adapter.limits,
                },
                None,
            )
            .await
            .map_err(|_| RendererInitializationError::RequestDeviceFailed)?;

        let instance = Arc::new(instance);
        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let surface = builder.window.map(|window| unsafe { instance.create_surface(window) });

        (instance, surface, device, queue, adapter_info, chosen_adapter.mode)
    };

    let shader_manager = ShaderManager::new(Arc::clone(&device));

    let mut global_resources = RwLock::new(RendererGlobalResources::new(
        &device,
        surface.as_ref(),
        mode,
        &builder.options,
    ));
    let global_resource_guard = global_resources.get_mut();

    let gpu_copy = GpuCopy::new(&device, &shader_manager, adapter_info.subgroup_size());

    let culling_pass = culling::CullingPass::new(
        &device,
        culling::CullingPassCreationArgs {
            mode,
            shader_manager: &shader_manager,
            prefix_sum_bgl: &global_resource_guard.prefix_sum_bgl,
            pre_cull_bgl: &global_resource_guard.pre_cull_bgl,
            object_input_bgl: &global_resource_guard.object_input_bgl,
            output_bgl: &global_resource_guard.object_output_bgl,
            uniform_bgl: &global_resource_guard.camera_data_bgl,
            subgroup_size: adapter_info.subgroup_size(),
        },
    );

    let texture_manager_2d = RwLock::new(TextureManager::new(
        &device,
        mode,
        STARTING_2D_TEXTURES,
        TextureViewDimension::D2,
    ));
    let texture_manager_cube = RwLock::new(TextureManager::new(
        &device,
        mode,
        STARTING_CUBE_TEXTURES,
        TextureViewDimension::Cube,
    ));

    let pipeline_manager = PipelineManager::new();

    let mut buffer_manager = Mutex::new(AutomatedBufferManager::new(UploadStyle::from_device_type(
        &adapter_info.device_type,
    )));
    let mesh_manager = RwLock::new(MeshManager::new(&device));
    let material_manager = RwLock::new(MaterialManager::new(&device, mode, buffer_manager.get_mut()));
    let object_manager = RwLock::new(ObjectManager::new(&device, mode, buffer_manager.get_mut()));
    let directional_light_manager = RwLock::new(DirectionalLightManager::new(&device, buffer_manager.get_mut()));

    span_transfer!(_ -> imgui_guard, INFO, "Creating Imgui Renderer");

    // let imgui_renderer = imgui_wgpu::Renderer::new(imgui, &device, &queue, SWAPCHAIN_FORMAT);

    span_transfer!(imgui_guard -> _);

    let render_list_cache = RwLock::new(RenderListCache::new());

    let (culling_pass, gpu_copy) = futures::join!(culling_pass, gpu_copy);

    Ok(Arc::new(Renderer {
        yard: builder.yard.expect("The yard should be populated by the builder"),
        yard_priorites: builder.priorities.unwrap_or_else(JobPriorities::default),
        instructions: InstructionStreamPair::new(),

        mode,
        adapter_info,
        instance,
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
        options: RwLock::new(builder.options),
    }))
}
