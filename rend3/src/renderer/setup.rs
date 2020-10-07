use crate::{
    instruction::InstructionStreamPair,
    renderer::{
        info::ExtendedAdapterInfo,
        limits::{check_features, check_limits},
        material::MaterialManager,
        mesh::MeshManager,
        object::ObjectManager,
        passes,
        passes::ForwardPassSet,
        resources::RendererGlobalResources,
        shaders::ShaderManager,
        texture::{TextureManager, STARTING_2D_TEXTURES, STARTING_CUBE_TEXTURES},
        Renderer, SWAPCHAIN_FORMAT,
    },
    RendererInitializationError, RendererOptions,
};
use parking_lot::{Mutex, RwLock};
use raw_window_handle::HasRawWindowHandle;
use std::sync::Arc;
use switchyard::Switchyard;
use wgpu::{BackendBit, DeviceDescriptor, Instance, PowerPreference, RequestAdapterOptions, TextureViewDimension};
use wgpu_conveyor::{AutomatedBufferManager, UploadStyle};

pub async fn create_renderer<W: HasRawWindowHandle, TLD: 'static>(
    window: &W,
    yard: Arc<Switchyard<TLD>>,
    imgui: &mut imgui::Context,
    options: RendererOptions,
) -> Result<Arc<Renderer<TLD>>, RendererInitializationError> {
    let instance = Instance::new(BackendBit::VULKAN);

    let surface = unsafe { instance.create_surface(window) };

    let adapter = instance
        .request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
        })
        .await
        .ok_or(RendererInitializationError::MissingAdapter)?;

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
        &global_resource_guard.general_bgl,
        &global_resource_guard.object_output_bgl,
        &global_resource_guard.uniform_bgl,
        adapter_info.subgroup_size(),
    );

    let swapchain_blit_pass = passes::BlitPass::new(
        &device,
        &shader_manager,
        &global_resource_guard.blit_bgl,
        SWAPCHAIN_FORMAT,
    );

    let mut texture_manager_2d = RwLock::new(TextureManager::new(
        &device,
        &global_resource_guard.sampler,
        STARTING_2D_TEXTURES,
        TextureViewDimension::D2,
    ));
    let texture_manager_2d_guard = texture_manager_2d.get_mut();

    let depth_pass = passes::DepthPass::new(
        &device,
        &shader_manager,
        &global_resource_guard.general_bgl,
        &global_resource_guard.object_output_noindirect_bgl,
        &texture_manager_2d_guard.bind_group_layout(),
        &global_resource_guard.uniform_bgl,
    );

    let opaque_pass = passes::OpaquePass::new(
        &device,
        &shader_manager,
        &global_resource_guard.general_bgl,
        &global_resource_guard.object_output_noindirect_bgl,
        &texture_manager_2d_guard.bind_group_layout(),
        &global_resource_guard.uniform_bgl,
    );

    let mut texture_manager_cube = RwLock::new(TextureManager::new(
        &device,
        &global_resource_guard.sampler,
        STARTING_CUBE_TEXTURES,
        TextureViewDimension::Cube,
    ));
    let texture_manager_cube_guard = texture_manager_cube.get_mut();

    let skybox_pass = passes::SkyboxPass::new(
        &device,
        &shader_manager,
        &texture_manager_cube_guard.bind_group_layout(),
        &global_resource_guard.uniform_bgl,
    );

    let forward_pass_set = ForwardPassSet::new(
        &device,
        &global_resource_guard.uniform_bgl,
        String::from("Forward Pass"),
    );

    let mut buffer_manager = Mutex::new(AutomatedBufferManager::new(UploadStyle::from_device_type(
        &adapter_info.device_type,
    )));
    let mesh_manager = RwLock::new(MeshManager::new(&device));
    let material_manager = RwLock::new(MaterialManager::new(&device, buffer_manager.get_mut()));
    let object_manager = RwLock::new(ObjectManager::new(&device, buffer_manager.get_mut()));

    span_transfer!(_ -> imgui_guard, INFO, "Creating Imgui Renderer");

    let imgui_renderer = imgui_wgpu::Renderer::new(imgui, &device, &queue, SWAPCHAIN_FORMAT);

    span_transfer!(imgui_guard -> _);

    let (culling_pass, depth_pass, opaque_pass, swapchain_blit_pass, skybox_pass) =
        futures::join!(culling_pass, depth_pass, opaque_pass, swapchain_blit_pass, skybox_pass);
    let depth_pass = RwLock::new(depth_pass);
    let skybox_pass = RwLock::new(skybox_pass);
    let opaque_pass = RwLock::new(opaque_pass);

    Ok(Arc::new(Renderer {
        yard,
        instructions: InstructionStreamPair::new(),

        _adapter_info: adapter_info,
        queue,
        device,
        surface,

        buffer_manager,
        global_resources,
        _shader_manager: shader_manager,
        mesh_manager,
        texture_manager_2d,
        texture_manager_cube,
        material_manager,
        object_manager,

        forward_pass_set,

        swapchain_blit_pass,
        culling_pass,
        skybox_pass,
        depth_pass,
        opaque_pass,

        _imgui_renderer: imgui_renderer,

        options: RwLock::new(options),
    }))
}
