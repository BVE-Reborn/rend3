use crate::{
    instruction::InstructionStreamPair,
    renderer::{
        info::ExtendedAdapterInfo,
        limits::{check_features, check_limits},
        resources::RendererGlobalResources,
    },
    resources::{
        DirectionalLightManager, MaterialManager, MeshManager, ObjectManager, TextureManager, STARTING_2D_TEXTURES,
        STARTING_CUBE_TEXTURES,
    },
    Renderer, RendererBuilder, RendererInitializationError, RendererMode,
};
use arrayvec::ArrayVec;
use fnv::FnvHashMap;
use parking_lot::RwLock;
use raw_window_handle::HasRawWindowHandle;
use std::{path::Path, sync::Arc};
use wgpu::{
    Adapter, AdapterInfo, Backend, Backends, DeviceDescriptor, DeviceType, Features, Instance, Limits,
    TextureViewDimension,
};

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

        if (features.is_err() || limits.is_err() || dbg!(desired_mode) == Some(RendererMode::CPUPowered))
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
            return Ok((instance, adapter));
        }
    }

    Err(RendererInitializationError::MissingAdapter)
}

pub async fn create_renderer<W: HasRawWindowHandle>(
    builder: RendererBuilder<'_, W>,
) -> Result<Arc<Renderer>, RendererInitializationError> {
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
                Some(Path::new("trace")),
            )
            .await
            .map_err(|_| RendererInitializationError::RequestDeviceFailed)?;

        let instance = Arc::new(instance);
        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let surface = builder.window.map(|window| unsafe { instance.create_surface(window) });

        (instance, surface, device, queue, adapter_info, chosen_adapter.mode)
    };

    let global_resources = RwLock::new(RendererGlobalResources::new(
        &device,
        surface.as_ref(),
        &builder.options,
    ));

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
    let mesh_manager = RwLock::new(MeshManager::new(&device));
    let material_manager = RwLock::new(MaterialManager::new(&device, mode));
    let object_manager = RwLock::new(ObjectManager::new());
    let directional_light_manager = RwLock::new(DirectionalLightManager::new(&device));

    Ok(Arc::new(Renderer {
        instructions: InstructionStreamPair::new(),

        mode,
        adapter_info,
        instance,
        queue,
        device,
        surface,

        global_resources,
        mesh_manager,
        d2_texture_manager: texture_manager_2d,
        d2c_texture_manager: texture_manager_cube,
        material_manager,
        object_manager,
        directional_light_manager,

        options: RwLock::new(builder.options),
    }))
}
