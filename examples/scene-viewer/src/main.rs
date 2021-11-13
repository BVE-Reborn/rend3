use glam::{DVec2, Mat3A, Mat4, UVec2, Vec3, Vec3A};
use instant::Instant;
use pico_args::Arguments;
use rend3::{
    types::{Backend, Camera, CameraProjection, DirectionalLight, DirectionalLightHandle, Texture, TextureFormat},
    util::typedefs::FastHashMap,
    Renderer, RendererMode,
};
use rend3_framework::AsyncMutex;
use rend3_pbr::SkyboxRoutine;
use std::{collections::HashMap, hash::BuildHasher, path::Path, sync::Arc, time::Duration};
use wgpu_profiler::GpuTimerScopeResult;
use winit::{
    event::{DeviceEvent, ElementState, Event, KeyboardInput, WindowEvent},
    event_loop::ControlFlow,
    window::WindowBuilder,
};

mod platform;

async fn load_skybox_image(loader: &rend3_framework::AssetLoader, data: &mut Vec<u8>, path: &str) {
    let decoded = image::load_from_memory(
        &loader
            .get_asset(path)
            .await
            .unwrap_or_else(|e| panic!("Error {}: {}", path, e)),
    )
    .unwrap()
    .into_rgba8();

    data.extend_from_slice(decoded.as_raw());
}

async fn load_skybox(
    renderer: &Renderer,
    loader: &rend3_framework::AssetLoader,
    skybox_routine: &AsyncMutex<SkyboxRoutine>,
) -> Result<(), Box<dyn std::error::Error>> {
    profiling::scope!("load skybox");

    let mut data = Vec::new();
    load_skybox_image(loader, &mut data, "skybox/right.jpg").await;
    load_skybox_image(loader, &mut data, "skybox/left.jpg").await;
    load_skybox_image(loader, &mut data, "skybox/top.jpg").await;
    load_skybox_image(loader, &mut data, "skybox/bottom.jpg").await;
    load_skybox_image(loader, &mut data, "skybox/front.jpg").await;
    load_skybox_image(loader, &mut data, "skybox/back.jpg").await;

    let handle = renderer.add_texture_cube(Texture {
        format: TextureFormat::Rgba8UnormSrgb,
        size: UVec2::new(2048, 2048),
        data,
        label: Some("background".into()),
        mip_count: rend3::types::MipmapCount::ONE,
        mip_source: rend3::types::MipmapSource::Uploaded,
    });
    skybox_routine.lock().await.set_background_texture(Some(handle));
    Ok(())
}

async fn load_gltf(
    renderer: &Renderer,
    loader: &rend3_framework::AssetLoader,
    location: String,
) -> rend3_gltf::LoadedGltfScene {
    profiling::scope!("loading gltf");
    let path = Path::new(&location);
    let parent = path.parent().unwrap();

    let parent_str = parent.to_string_lossy();
    let path_str = path.as_os_str().to_string_lossy();
    log::info!("Reading gltf file: {}", path_str);
    let gltf_data = {
        profiling::scope!("reading gltf file", &path_str);
        loader.get_asset(&path_str).await.unwrap()
    };

    rend3_gltf::load_gltf(renderer, &gltf_data, |uri| async {
        let uri = uri;
        let full_uri = parent_str.clone() + "/" + uri.as_str();
        loader.get_asset(&full_uri).await
    })
    .await
    .unwrap()
}

fn button_pressed<Hash: BuildHasher>(map: &HashMap<u32, bool, Hash>, key: u32) -> bool {
    map.get(&key).map_or(false, |b| *b)
}

fn extract_backend(value: &str) -> Result<Backend, &'static str> {
    Ok(match value.to_lowercase().as_str() {
        "vulkan" | "vk" => Backend::Vulkan,
        "dx12" | "12" => Backend::Dx12,
        "dx11" | "11" => Backend::Dx11,
        "metal" | "mtl" => Backend::Metal,
        "opengl" | "gl" => Backend::Gl,
        _ => return Err("backend requested but not found"),
    })
}

fn extract_mode(value: &str) -> Result<rend3::RendererMode, &'static str> {
    Ok(match value.to_lowercase().as_str() {
        "legacy" | "c" | "cpu" => rend3::RendererMode::CPUPowered,
        "modern" | "g" | "gpu" => rend3::RendererMode::GPUPowered,
        _ => return Err("mode requested but not found"),
    })
}

struct SceneViewer {
    absolute_mouse: bool,
    desired_backend: Option<Backend>,
    desired_device_name: Option<String>,
    desired_mode: Option<RendererMode>,
    file_to_load: Option<String>,

    directional_light_handle: Option<DirectionalLightHandle>,

    scancode_status: FastHashMap<u32, bool>,
    camera_pitch: f32,
    camera_yaw: f32,
    camera_location: Vec3A,
    previous_profiling_stats: Option<Vec<GpuTimerScopeResult>>,
    timestamp_last_second: Instant,
    timestamp_last_frame: Instant,
    frame_times: histogram::Histogram,
    last_mouse_delta: Option<DVec2>,
}
impl SceneViewer {
    pub fn new() -> Self {
        let mut args = Arguments::from_vec(std::env::args_os().skip(1).collect());
        let absolute_mouse: bool = args.contains("--absolute-mouse");
        let desired_backend = args.value_from_fn(["-b", "--backend"], extract_backend).ok();
        let desired_device_name: Option<String> = args
            .value_from_str(["-d", "--device"])
            .ok()
            .map(|s: String| s.to_lowercase());
        let desired_mode = args.value_from_fn(["-m", "--mode"], extract_mode).ok();
        let file_to_load: Option<String> = args.free_from_str().ok();

        Self {
            absolute_mouse,
            desired_backend,
            desired_device_name,
            desired_mode,
            file_to_load,

            directional_light_handle: None,

            scancode_status: FastHashMap::default(),
            camera_pitch: std::f32::consts::FRAC_PI_4,
            camera_yaw: -std::f32::consts::FRAC_PI_4,
            camera_location: Vec3A::new(20.0, 20.0, -20.0),
            previous_profiling_stats: None,
            timestamp_last_second: Instant::now(),
            timestamp_last_frame: Instant::now(),
            frame_times: histogram::Histogram::new(),
            last_mouse_delta: None,
        }
    }
}
impl rend3_framework::App for SceneViewer {
    fn create_iad<'a>(
        &'a mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<rend3::InstanceAdapterDevice>> + 'a>> {
        Box::pin(async move {
            Ok(rend3::create_iad(
                self.desired_backend,
                self.desired_device_name.clone(),
                self.desired_mode,
            )
            .await?)
        })
    }

    fn setup<'a>(
        &'a mut self,
        renderer: &'a Renderer,
        routines: &'a rend3_framework::DefaultRoutines,
        _surface: &'a rend3::types::Surface,
    ) -> std::pin::Pin<Box<dyn rend3_framework::NativeSendFuture<()> + 'a>> {
        Box::pin(async move {
            routines
                .pbr
                .lock()
                .await
                .set_ambient_color(glam::Vec4::new(0.15, 0.15, 0.15, 1.0));

            self.directional_light_handle = Some(renderer.add_directional_light(DirectionalLight {
                color: Vec3::ONE,
                intensity: 10.0,
                direction: Vec3::new(-1.0, -1.0, 0.0),
                distance: 400.0,
            }));
        })
    }

    fn async_setup(
        &mut self,
        renderer: Arc<Renderer>,
        routines: Arc<rend3_framework::DefaultRoutines>,
        _surface: Arc<rend3::types::Surface>,
    ) -> std::pin::Pin<Box<dyn rend3_framework::NativeSendFuture<()>>> {
        let file_to_load = self.file_to_load.take();
        Box::pin(async move {
            let loader = rend3_framework::AssetLoader::new_local(
                concat!(env!("CARGO_MANIFEST_DIR"), "/../resources/"),
                "http://localhost:8000/resources/",
            );
            if let Err(e) = load_skybox(&renderer, &loader, &routines.skybox).await {
                println!("Failed to load skybox {}", e)
            };
            Box::leak(Box::new(
                load_gltf(
                    &renderer,
                    &loader,
                    file_to_load.unwrap_or_else(|| "/default-scene/scene.gltf".to_owned()),
                )
                .await,
            ));
        })
    }

    fn handle_event<'a, T: rend3_framework::NativeSend>(
        &'a mut self,
        renderer: &'a Arc<rend3::Renderer>,
        routines: &'a Arc<rend3_framework::DefaultRoutines>,
        surface: &'a Arc<rend3::types::Surface>,
        event: Event<'a, T>,
        control_flow: &'a mut winit::event_loop::ControlFlow,
    ) -> std::pin::Pin<Box<dyn rend3_framework::NativeSendFuture<()> + 'a>> {
        Box::pin(async move {
            match event {
                Event::MainEventsCleared => {
                    profiling::scope!("MainEventsCleared");
                    let now = Instant::now();

                    let delta_time = now - self.timestamp_last_frame;
                    self.frame_times.increment(delta_time.as_micros() as u64).unwrap();

                    let elapsed_since_second = now - self.timestamp_last_second;
                    if elapsed_since_second > Duration::from_secs(1) {
                        let count = self.frame_times.entries();
                        println!(
                            "{:0>5} frames over {:0>5.2}s. Min: {:0>5.2}ms; Average: {:0>5.2}ms; 95%: {:0>5.2}ms; 99%: {:0>5.2}ms; Max: {:0>5.2}ms; StdDev: {:0>5.2}ms",
                            count,
                            elapsed_since_second.as_secs_f32(),
                            self.frame_times.minimum().unwrap() as f32 / 1_000.0,
                            self.frame_times.mean().unwrap() as f32 / 1_000.0,
                            self.frame_times.percentile(95.0).unwrap() as f32 / 1_000.0,
                            self.frame_times.percentile(99.0).unwrap() as f32 / 1_000.0,
                            self.frame_times.maximum().unwrap() as f32 / 1_000.0,
                            self.frame_times.stddev().unwrap() as f32 / 1_000.0,
                        );
                        self.timestamp_last_second = now;
                        self.frame_times.clear();
                    }

                    self.timestamp_last_frame = now;

                    let rotation =
                        Mat3A::from_euler(glam::EulerRot::XYZ, -self.camera_pitch, -self.camera_yaw, 0.0).transpose();
                    let forward = rotation.z_axis;
                    let up = rotation.y_axis;
                    let side = -rotation.x_axis;
                    let velocity = if button_pressed(&self.scancode_status, platform::Scancodes::SHIFT) {
                        50.0
                    } else {
                        10.0
                    };
                    if button_pressed(&self.scancode_status, platform::Scancodes::W) {
                        self.camera_location += forward * velocity * delta_time.as_secs_f32();
                    }
                    if button_pressed(&self.scancode_status, platform::Scancodes::S) {
                        self.camera_location -= forward * velocity * delta_time.as_secs_f32();
                    }
                    if button_pressed(&self.scancode_status, platform::Scancodes::A) {
                        self.camera_location += side * velocity * delta_time.as_secs_f32();
                    }
                    if button_pressed(&self.scancode_status, platform::Scancodes::D) {
                        self.camera_location -= side * velocity * delta_time.as_secs_f32();
                    }
                    if button_pressed(&self.scancode_status, platform::Scancodes::Q) {
                        self.camera_location += up * velocity * delta_time.as_secs_f32();
                    }
                    if button_pressed(&self.scancode_status, platform::Scancodes::Z) {
                        self.camera_location -= up * velocity * delta_time.as_secs_f32();
                    }

                    if button_pressed(&self.scancode_status, platform::Scancodes::P) {
                        // write out gpu side performance info into a trace readable by chrome://tracing
                        if let Some(ref stats) = self.previous_profiling_stats {
                            println!("Outputing gpu timing chrome trace to profile.json");
                            wgpu_profiler::chrometrace::write_chrometrace(Path::new("profile.json"), stats).unwrap();
                        } else {
                            println!("No gpu timing trace available, either timestamp queries are unsupported or not enough frames have elapsed yet!");
                        }
                    }

                    let view = Mat4::from_euler(glam::EulerRot::XYZ, -self.camera_pitch, -self.camera_yaw, 0.0);
                    let view = view * Mat4::from_translation((-self.camera_location).into());

                    renderer.set_camera_data(Camera {
                        projection: CameraProjection::Perspective { vfov: 60.0, near: 0.1 },
                        view,
                    });

                    // Get a frame
                    let frame = rend3::util::output::OutputFrame::Surface {
                        surface: Arc::clone(&surface),
                    };
                    // Lock all the routines
                    let pbr_routine = routines.pbr.lock().await;
                    let mut skybox_routine = routines.skybox.lock().await;
                    let tonemapping_routine = routines.tonemapping.lock().await;

                    // Ready up the renderer
                    let (cmd_bufs, ready) = renderer.ready();
                    // Ready up the routines
                    skybox_routine.ready(&renderer);

                    // Build a rendergraph
                    let mut graph = rend3::RenderGraph::new();
                    // Upload culling information to the GPU and into the graph.
                    pbr_routine.add_pre_cull_to_graph(&mut graph);

                    // Run all culling for shadows and the camera.
                    pbr_routine.add_shadow_culling_to_graph(&mut graph, &ready);
                    pbr_routine.add_culling_to_graph(&mut graph);

                    // Render shadows.
                    pbr_routine.add_shadow_rendering_to_graph(&mut graph, &ready);

                    // Depth prepass and forward pass.
                    pbr_routine.add_prepass_to_graph(&mut graph);
                    skybox_routine.add_to_graph(&mut graph);
                    pbr_routine.add_forward_to_graph(&mut graph);

                    // Tonemap onto the output.
                    tonemapping_routine.add_to_graph(&mut graph);

                    // Dispatch a render using the built up rendergraph!
                    self.previous_profiling_stats = graph.execute(&renderer, frame, cmd_bufs, &ready);
                    // mark the end of the frame for tracy/other profilers
                    profiling::finish_frame!();
                }
                Event::WindowEvent {
                    event:
                        WindowEvent::KeyboardInput {
                            input: KeyboardInput { scancode, state, .. },
                            ..
                        },
                    ..
                } => {
                    log::info!("WE scancode {}", scancode);
                    self.scancode_status.insert(
                        scancode,
                        match state {
                            ElementState::Pressed => true,
                            ElementState::Released => false,
                        },
                    );
                }
                Event::DeviceEvent {
                    event: DeviceEvent::Key(KeyboardInput { scancode, state, .. }),
                    ..
                } => {
                    log::info!("DE scancode {}", scancode);
                    self.scancode_status.insert(
                        scancode,
                        match state {
                            ElementState::Pressed => true,
                            ElementState::Released => false,
                        },
                    );
                }
                Event::DeviceEvent {
                    event:
                        DeviceEvent::MouseMotion {
                            delta: (delta_x, delta_y),
                            ..
                        },
                    ..
                } => {
                    const TAU: f32 = std::f32::consts::PI * 2.0;

                    let mouse_delta = if self.absolute_mouse {
                        let prev = self.last_mouse_delta.replace(DVec2::new(delta_x, delta_y));
                        if let Some(prev) = prev {
                            (DVec2::new(delta_x, delta_y) - prev) / 4.0
                        } else {
                            return;
                        }
                    } else {
                        DVec2::new(delta_x, delta_y)
                    };

                    self.camera_yaw += (mouse_delta.x / 1000.0) as f32;
                    self.camera_pitch += (mouse_delta.y / 1000.0) as f32;
                    if self.camera_yaw < 0.0 {
                        self.camera_yaw += TAU;
                    } else if self.camera_yaw >= TAU {
                        self.camera_yaw -= TAU;
                    }
                    self.camera_pitch = self
                        .camera_pitch
                        .max(-std::f32::consts::FRAC_PI_2 + 0.0001)
                        .min(std::f32::consts::FRAC_PI_2 - 0.0001);
                }
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    *control_flow = ControlFlow::Exit;
                }
                _ => {}
            }
        })
    }
}

fn main() {
    let app = SceneViewer::new();
    rend3_framework::start(app, WindowBuilder::new().with_title("scene-viewer"));
}
