use crate::{
    instruction::InstructionStreamPair,
    renderer::{
        info::{ExtendedAdapterInfo, Vendor},
        light::DirectionalLightManager,
        limits::{check_features, check_limits},
        list::RenderListCache,
        material::MaterialManager,
        mesh::MeshManager,
        object::ObjectManager,
        passes,
        pipeline::PipelineManager,
        resources::RendererGlobalResources,
        shaders::ShaderManager,
        texture::{TextureManager, STARTING_2D_TEXTURES, STARTING_CUBE_TEXTURES},
        Renderer,
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

struct PotentialAdapter {
    adapter: Adapter,
    info: ExtendedAdapterInfo,
    features: Features,
    limits: Limits,
}

pub fn create_adapter() -> Result<(Instance, Adapter), RendererInitializationError> {
    let backend_bits = BackendBit::VULKAN | BackendBit::DX12;
    let default_backend_order = [Backend::Vulkan, Backend::Dx12];
    let intel_backend_order = [Backend::Dx12, Backend::Vulkan];

    let instance = Instance::new(backend_bits);

    let mut valid_adapters = FnvHashMap::default();

    for backend in &default_backend_order {
        let adapters = instance.enumerate_adapters(BackendBit::from(*backend));

        let mut potential_adapters = ArrayVec::<[PotentialAdapter; 4]>::new();
        for (idx, adapter) in adapters.enumerate() {
            let info = ExtendedAdapterInfo::from(adapter.get_info());

            tracing::debug!("{:?} Adapter {}: {:#?}", backend, idx, info);

            let features = check_features(adapter.features()).ok();
            let limits = check_limits(adapter.limits()).ok();

            if let (Some(features), Some(limits)) = (features, limits) {
                tracing::debug!("Adapter usable");
                potential_adapters.push(PotentialAdapter {
                    adapter,
                    info,
                    features,
                    limits,
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

    let intel_vendor = valid_adapters
        .get(&Backend::Vulkan)
        .and_then(|arr| arr.get(0))
        .map(|a: &PotentialAdapter| a.info.vendor.clone());
    let is_intel = Some(Vendor::Intel) == intel_vendor;

    let backend_order = if is_intel {
        &intel_backend_order
    } else {
        &default_backend_order
    };

    for backend in backend_order {
        let adapter: Option<PotentialAdapter> = valid_adapters.remove(backend).and_then(|arr| arr.into_iter().next());

        if let Some(adapter) = adapter {
            tracing::debug!("Chosen adapter: {:#?}", adapter.info);
            tracing::debug!("Chosen backend: {:?}", backend);
            tracing::debug!("Chosen features: {:#?}", adapter.features);
            tracing::debug!("Chosen limits: {:#?}", adapter.limits);
            return Ok((instance, adapter.adapter));
        }
    }

    Err(RendererInitializationError::MissingAdapter)
}

pub async fn create_renderer<W: HasRawWindowHandle, TLD: 'static>(
    window: &W,
    yard: Arc<Switchyard<TLD>>,
    _imgui: &mut imgui::Context,
    options: RendererOptions,
) -> Result<Arc<Renderer<TLD>>, RendererInitializationError> {
    let (instance, adapter) = create_adapter()?;

    let surface = unsafe { instance.create_surface(window) };

    let adapter_info = ExtendedAdapterInfo::from(adapter.get_info());
    let features = check_features(adapter.features())?;
    let limits = check_limits(adapter.limits())?;

    let (device, queue) = adapter
        .request_device(
            &DeviceDescriptor {
                features,
                limits,
                shader_validation: true,
            },
            None,
        )
        .await
        .map_err(|_| RendererInitializationError::RequestDeviceFailed)?;

    let device = Arc::new(device);

    let shader_manager = ShaderManager::new(Arc::clone(&device));

    let mut global_resources = RwLock::new(RendererGlobalResources::new(&device, &surface, &options));
    let global_resource_guard = global_resources.get_mut();

    let culling_pass = passes::CullingPass::new(
        &device,
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
        STARTING_2D_TEXTURES,
        TextureViewDimension::D2,
    ));
    let texture_manager_cube = RwLock::new(TextureManager::new(
        &device,
        STARTING_CUBE_TEXTURES,
        TextureViewDimension::Cube,
    ));

    let pipeline_manager = PipelineManager::new();

    let mut buffer_manager = Mutex::new(AutomatedBufferManager::new(UploadStyle::from_device_type(
        &adapter_info.device_type,
    )));
    let mesh_manager = RwLock::new(MeshManager::new(&device));
    let material_manager = RwLock::new(MaterialManager::new(&device, buffer_manager.get_mut()));
    let object_manager = RwLock::new(ObjectManager::new(&device, buffer_manager.get_mut()));
    let directional_light_manager = RwLock::new(DirectionalLightManager::new(&device, buffer_manager.get_mut()));

    span_transfer!(_ -> imgui_guard, INFO, "Creating Imgui Renderer");

    // let imgui_renderer = imgui_wgpu::Renderer::new(imgui, &device, &queue, SWAPCHAIN_FORMAT);

    span_transfer!(imgui_guard -> _);

    let render_list_cache = RwLock::new(RenderListCache::new());

    let (culling_pass,) = futures::join!(culling_pass);

    Ok(Arc::new(Renderer {
        yard,
        instructions: InstructionStreamPair::new(),

        _adapter_info: adapter_info,
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
        culling_pass,

        // _imgui_renderer: imgui_renderer,
        options: RwLock::new(options),
    }))
}
