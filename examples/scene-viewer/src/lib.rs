use glam::{DVec2, Mat3A, Mat4, UVec2, Vec3, Vec3A};
use instant::Instant;
use pico_args::Arguments;
use rend3::{
    types::{Backend, Camera, CameraProjection, DirectionalLight, DirectionalLightHandle, Texture, TextureFormat},
    util::typedefs::FastHashMap,
    Renderer, RendererMode,
};
use rend3_framework::{lock, AssetPath, Mutex};
use rend3_routine::SkyboxRoutine;
use std::{collections::HashMap, future::Future, hash::BuildHasher, path::Path, sync::Arc, time::Duration};
use wgpu_profiler::GpuTimerScopeResult;
use winit::{
    event::{DeviceEvent, ElementState, Event, KeyboardInput, MouseButton, WindowEvent},
    window::{Fullscreen, WindowBuilder},
};

mod platform;

async fn load_skybox_image(loader: &rend3_framework::AssetLoader, data: &mut Vec<u8>, path: &str) {
    let decoded = image::load_from_memory(
        &loader
            .get_asset(AssetPath::Internal(path))
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
    skybox_routine: &Mutex<SkyboxRoutine>,
) -> Result<(), Box<dyn std::error::Error>> {
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
    lock(skybox_routine).set_background_texture(Some(handle));
    Ok(())
}

async fn load_gltf(
    renderer: &Renderer,
    loader: &rend3_framework::AssetLoader,
    location: AssetPath<'_>,
) -> rend3_gltf::LoadedGltfScene {
    // profiling::scope!("loading gltf");
    let gltf_start = Instant::now();
    let path = loader.get_asset_path(location);
    let path = Path::new(&*path);
    let parent = path.parent().unwrap();

    let parent_str = parent.to_string_lossy();
    let path_str = path.as_os_str().to_string_lossy();
    log::info!("Reading gltf file: {}", path_str);
    let gltf_data = {
        // profiling::scope!("reading gltf file", &path_str);
        loader.get_asset(AssetPath::External(&path_str)).await.unwrap()
    };

    let gltf_elapsed = gltf_start.elapsed();
    let resources_start = Instant::now();
    let scene = rend3_gltf::load_gltf(renderer, &gltf_data, |uri| async {
        log::info!("Loading resource {}", uri);
        let uri = uri;
        let full_uri = parent_str.clone() + "/" + uri.as_str();
        loader.get_asset(AssetPath::External(&full_uri)).await
    })
    .await
    .unwrap();
    log::info!(
        "Loaded gltf in {:.3?}, resources loaded in {:.3?}",
        gltf_elapsed,
        resources_start.elapsed()
    );
    scene
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

#[cfg(not(target_arch = "wasm32"))]
pub fn spawn<Fut>(fut: Fut)
where
    Fut: Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    tokio::spawn(fut);
}

#[cfg(target_arch = "wasm32")]
pub fn spawn<Fut>(fut: Fut)
where
    Fut: Future + 'static,
    Fut::Output: 'static,
{
    wasm_bindgen_futures::spawn_local(async move {
        fut.await;
    });
}

struct SceneViewer {
    absolute_mouse: bool,
    desired_backend: Option<Backend>,
    desired_device_name: Option<String>,
    desired_mode: Option<RendererMode>,
    file_to_load: Option<String>,
    walk_speed: f32,

    fullscreen: bool,

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

    grabber: Option<rend3_framework::Grabber>,
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
        let fullscreen = args.contains("--fullscreen");
        let walk_speed = args.value_from_str("--walk").unwrap_or(10.0_f32);

        Self {
            absolute_mouse,
            desired_backend,
            desired_device_name,
            desired_mode,
            file_to_load,
            walk_speed,

            fullscreen,

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

            grabber: None,
        }
    }
}
impl rend3_framework::App for SceneViewer {
    const HANDEDNESS: rend3::types::Handedness = rend3::types::Handedness::Right;
    const DEFAULT_SAMPLE_COUNT: rend3::types::SampleCount = rend3::types::SampleCount::One;

    fn create_iad<'a>(
        &'a mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<rend3::InstanceAdapterDevice>> + 'a>> {
        Box::pin(async move {
            Ok(rend3::create_iad(
                self.desired_backend,
                self.desired_device_name.clone(),
                self.desired_mode,
                None,
            )
            .await?)
        })
    }

    fn scale_factor(&self) -> f32 {
        // Android has very low memory bandwidth, so lets run internal buffers at half res by default
        cfg_if::cfg_if! {
            if #[cfg(target_os = "android")] {
                0.5
            } else {
                1.0
            }
        }
    }

    fn setup<'a>(
        &'a mut self,
        window: &'a winit::window::Window,
        renderer: &'a Arc<Renderer>,
        routines: &'a Arc<rend3_framework::DefaultRoutines>,
        _surface_format: rend3::types::TextureFormat,
    ) {
        lock(&routines.pbr).set_ambient_color(glam::Vec4::new(0.15, 0.15, 0.15, 1.0));

        self.directional_light_handle = Some(renderer.add_directional_light(DirectionalLight {
            color: Vec3::ONE,
            intensity: 10.0,
            direction: Vec3::new(-1.0, -1.0, 0.0),
            distance: 400.0,
        }));

        self.grabber = Some(rend3_framework::Grabber::new(window));

        let file_to_load = self.file_to_load.take();
        let renderer = Arc::clone(renderer);
        let routines = Arc::clone(routines);
        spawn(async move {
            let loader = rend3_framework::AssetLoader::new_local(
                concat!(env!("CARGO_MANIFEST_DIR"), "/resources/"),
                "",
                "http://localhost:8000/resources/",
            );
            if let Err(e) = load_skybox(&renderer, &loader, &routines.skybox).await {
                println!("Failed to load skybox {}", e)
            };
            Box::leak(Box::new(
                load_gltf(
                    &renderer,
                    &loader,
                    file_to_load
                        .as_deref()
                        .map_or_else(|| AssetPath::Internal("default-scene/scene.gltf"), AssetPath::External),
                )
                .await,
            ));
        });
    }

    fn handle_event(
        &mut self,
        window: &winit::window::Window,
        renderer: &Arc<rend3::Renderer>,
        routines: &Arc<rend3_framework::DefaultRoutines>,
        surface: Option<&Arc<rend3::types::Surface>>,
        event: rend3_framework::Event<'_, ()>,
        control_flow: impl FnOnce(winit::event_loop::ControlFlow),
    ) {
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
                    self.walk_speed
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

                if button_pressed(&self.scancode_status, platform::Scancodes::ESCAPE) {
                    self.grabber.as_mut().unwrap().request_ungrab(window);
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

                window.request_redraw()
            }
            Event::RedrawRequested(_) => {
                let view = Mat4::from_euler(glam::EulerRot::XYZ, -self.camera_pitch, -self.camera_yaw, 0.0);
                let view = view * Mat4::from_translation((-self.camera_location).into());

                renderer.set_camera_data(Camera {
                    projection: CameraProjection::Perspective { vfov: 60.0, near: 0.1 },
                    view,
                });

                // Get a frame
                let frame = rend3::util::output::OutputFrame::Surface {
                    surface: Arc::clone(surface.unwrap()),
                };
                // Lock all the routines
                let pbr_routine = lock(&routines.pbr);
                let mut skybox_routine = lock(&routines.skybox);
                let tonemapping_routine = lock(&routines.tonemapping);

                // Ready up the renderer
                let (cmd_bufs, ready) = renderer.ready();
                // Ready up the routines
                skybox_routine.ready(renderer);

                // Build a rendergraph
                let mut graph = rend3::RenderGraph::new();

                // Add the default rendergraph
                rend3_routine::add_default_rendergraph(
                    &mut graph,
                    &ready,
                    &pbr_routine,
                    Some(&skybox_routine),
                    &tonemapping_routine,
                    Self::DEFAULT_SAMPLE_COUNT,
                );

                // Dispatch a render using the built up rendergraph!
                self.previous_profiling_stats = graph.execute(renderer, frame, cmd_bufs, &ready);
                // mark the end of the frame for tracy/other profilers
                profiling::finish_frame!();
            }
            Event::WindowEvent {
                event: WindowEvent::Focused(focus),
                ..
            } => {
                if !focus {
                    self.grabber.as_mut().unwrap().request_ungrab(window);
                }
            }
            Event::WindowEvent {
                event:
                    WindowEvent::KeyboardInput {
                        input: KeyboardInput { scancode, state, .. },
                        ..
                    },
                ..
            } => {
                log::info!("WE scancode {:x}", scancode);
                self.scancode_status.insert(
                    scancode,
                    match state {
                        ElementState::Pressed => true,
                        ElementState::Released => false,
                    },
                );
            }
            Event::WindowEvent {
                event:
                    WindowEvent::MouseInput {
                        button: MouseButton::Left,
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                let grabber = self.grabber.as_mut().unwrap();

                if !grabber.grabbed() {
                    grabber.request_grab(window);
                }
            }
            Event::DeviceEvent {
                event:
                    DeviceEvent::MouseMotion {
                        delta: (delta_x, delta_y),
                        ..
                    },
                ..
            } => {
                if !self.grabber.as_ref().unwrap().grabbed() {
                    return;
                }

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
                control_flow(winit::event_loop::ControlFlow::Exit);
            }
            _ => {}
        }
    }
}

#[cfg_attr(target_os = "android", ndk_glue::main(backtrace = "on", logger(level = "debug")))]
pub fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    let _rt = tokio::runtime::Runtime::new().unwrap();
    #[cfg(not(target_arch = "wasm32"))]
    let _guard = _rt.enter();

    let app = SceneViewer::new();

    let mut builder = WindowBuilder::new().with_title("scene-viewer").with_maximized(true);
    if app.fullscreen {
        builder = builder.with_fullscreen(Some(Fullscreen::Borderless(None)));
    }

    rend3_framework::start(app, builder);
}
