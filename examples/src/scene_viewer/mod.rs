#![allow(clippy::field_reassign_with_default)] // much clearer this way

use std::{collections::HashMap, future::Future, hash::BuildHasher, path::Path, sync::Arc};

use flume::Receiver;
use glam::{DVec2, Mat3A, Mat4, UVec2, Vec3, Vec3A};
use pico_args::Arguments;
use rend3::{
    types::{
        Backend, Camera, CameraProjection, DirectionalLight, DirectionalLightHandle, SampleCount, Texture,
        TextureFormat,
    },
    util::typedefs::{FastHashMap, RendererStatistics},
    Renderer, RendererProfile,
};
use rend3_framework::{lock, AssetPath, Mutex};
use rend3_gltf::{GltfLoadSettings, GltfSceneInstance, LoadedGltfScene};
use rend3_routine::{pbr::NormalTextureYDirection, skybox::SkyboxRoutine};
use web_time::Instant;
use winit::{
    event::{DeviceEvent, ElementState, Event, KeyEvent, MouseButton, WindowEvent},
    keyboard::{KeyCode, PhysicalKey},
    window::{Fullscreen},
};

async fn load_skybox_image(loader: &rend3_framework::AssetLoader, data: &mut Vec<u8>, path: &str) {
    let request_response =
        loader.get_asset(AssetPath::Internal(path)).await.unwrap_or_else(|e| panic!("Error {}: {}", path, e));
    let decoded = image::load_from_memory(&request_response).unwrap().into_rgba8();

    data.extend_from_slice(decoded.as_raw());
}

async fn load_skybox(
    renderer: &Arc<Renderer>,
    loader: &rend3_framework::AssetLoader,
    skybox_routine: &Mutex<SkyboxRoutine>,
) -> anyhow::Result<()> {
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
    })?;
    lock(skybox_routine).set_background_texture(Some(handle));
    Ok(())
}

async fn load_gltf(
    renderer: &Arc<Renderer>,
    loader: &rend3_framework::AssetLoader,
    settings: &rend3_gltf::GltfLoadSettings,
    location: AssetPath<'_>,
) -> anyhow::Result<(rend3_gltf::LoadedGltfScene, GltfSceneInstance)> {
    // profiling::scope!("loading gltf");
    let gltf_start = Instant::now();
    let is_default_scene = matches!(location, AssetPath::Internal(_));
    let path = loader.get_asset_path(location);
    let path = Path::new(&*path);
    let parent = path.parent().unwrap();

    let parent_str = parent.to_string_lossy();
    let path_str = path.as_os_str().to_string_lossy();
    log::info!("Reading gltf file: {}", path_str);
    let gltf_data_result = loader.get_asset(AssetPath::External(&path_str)).await;

    let gltf_data = match gltf_data_result {
        Ok(d) => d,
        Err(_) if is_default_scene => {
            let suffix = if cfg!(target_os = "windows") { ".exe" } else { "" };

            indoc::eprintdoc!("
                *** WARNING ***

                It appears you are running scene-viewer with no file to display.
                
                The default scene is no longer bundled into the repository. If you are running on git, use the following commands
                to download and unzip it into the right place. If you're running it through not-git, pass a custom folder to the -C argument
                to tar, then run scene-viewer path/to/scene.gltf.
                
                curl{0} https://cdn.cwfitz.com/scenes/rend3-default-scene.tar -o ./examples/src/scene_viewer/resources/rend3-default-scene.tar
                tar{0} xf ./examples/src/scene_viewer/resources/rend3-default-scene.tar -C ./examples/src/scene_viewer/resources

                ***************
            ", suffix);

            anyhow::bail!("No file to display");
        }
        e => e.unwrap(),
    };

    let gltf_elapsed = gltf_start.elapsed();
    let resources_start = Instant::now();
    let (scene, instance) = rend3_gltf::load_gltf(renderer, &gltf_data, settings, |uri| async {
        if let Some(base64) = rend3_gltf::try_load_base64(&uri) {
            Ok(base64)
        } else {
            log::info!("Loading resource {}", uri);
            let uri = uri;
            let full_uri = parent_str.clone() + "/" + uri.as_str();
            loader.get_asset(AssetPath::External(&full_uri)).await
        }
    })
    .await?;

    log::info!("Loaded gltf in {:.3?}, resources loaded in {:.3?}", gltf_elapsed, resources_start.elapsed());
    Ok((scene, instance))
}

fn button_pressed<Hash: BuildHasher>(map: &HashMap<KeyCode, bool, Hash>, key: KeyCode) -> bool {
    map.get(&key).map_or(false, |b| *b)
}

fn extract_backend(value: &str) -> Result<Backend, &'static str> {
    Ok(match value.to_lowercase().as_str() {
        "vulkan" | "vk" => Backend::Vulkan,
        "dx12" | "12" => Backend::Dx12,
        "metal" | "mtl" => Backend::Metal,
        "opengl" | "gl" => Backend::Gl,
        _ => return Err("unknown backend"),
    })
}

fn extract_profile(value: &str) -> Result<rend3::RendererProfile, &'static str> {
    Ok(match value.to_lowercase().as_str() {
        "legacy" | "c" | "cpu" => rend3::RendererProfile::CpuDriven,
        "modern" | "g" | "gpu" => rend3::RendererProfile::GpuDriven,
        _ => return Err("unknown rendermode"),
    })
}

fn extract_msaa(value: &str) -> Result<SampleCount, &'static str> {
    Ok(match value {
        "1" => SampleCount::One,
        "4" => SampleCount::Four,
        _ => return Err("invalid msaa count"),
    })
}

fn extract_vsync(value: &str) -> Result<rend3::types::PresentMode, &'static str> {
    Ok(match value.to_lowercase().as_str() {
        "immediate" => rend3::types::PresentMode::Immediate,
        "fifo" => rend3::types::PresentMode::Fifo,
        "mailbox" => rend3::types::PresentMode::Mailbox,
        _ => return Err("invalid msaa count"),
    })
}

fn extract_array<const N: usize>(value: &str, default: [f32; N]) -> Result<[f32; N], &'static str> {
    let mut res = default;
    let split: Vec<_> = value.split(',').enumerate().collect();

    if split.len() != N {
        return Err("Mismatched argument count");
    }

    for (idx, inner) in split {
        let inner = inner.trim();

        res[idx] = inner.parse().map_err(|_| "Cannot parse argument number")?;
    }
    Ok(res)
}

fn extract_vec3(value: &str) -> Result<Vec3, &'static str> {
    let mut res = [0.0_f32, 0.0, 0.0];
    let split: Vec<_> = value.split(',').enumerate().collect();

    if split.len() != 3 {
        return Err("Directional lights are defined with 3 values");
    }

    for (idx, inner) in split {
        let inner = inner.trim();

        res[idx] = inner.parse().map_err(|_| "Cannot parse direction number")?;
    }
    Ok(Vec3::from(res))
}

fn option_arg<T>(result: Result<Option<T>, pico_args::Error>) -> Option<T> {
    match result {
        Ok(o) => o,
        Err(pico_args::Error::Utf8ArgumentParsingFailed { value, cause }) => {
            eprintln!("{}: '{}'\n\n{}", cause, value, HELP);
            std::process::exit(1);
        }
        Err(pico_args::Error::OptionWithoutAValue(value)) => {
            eprintln!("{} flag needs an argument", value);
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("{:?}", e);
            std::process::exit(1);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn spawn<Fut>(fut: Fut)
where
    Fut: Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    std::thread::spawn(|| pollster::block_on(fut));
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

const HELP: &str = "\
scene-viewer

gltf and glb scene viewer powered by the rend3 rendering library.

usage: scene-viewer --options ./path/to/gltf/file.gltf

Meta:
  --help            This menu.

Rendering:
  -b --backend                 Choose backend to run on ('vk', 'dx12', 'dx11', 'metal', 'gl').
  -d --device                  Choose device to run on (case insensitive device substring).
  -p --profile                 Choose rendering profile to use ('cpu', 'gpu').
  -v --vsync                   Choose vsync mode ('immediate' [no-vsync], 'fifo' [vsync], 'fifo_relaxed' [adaptive vsync], 'mailbox' [fast vsync])
  --msaa <level>               Level of antialiasing (either 1 or 4). Default 1.

Windowing:
  --absolute-mouse             Interpret the relative mouse coordinates as absolute. Useful when using things like VNC.
  --fullscreen                 Open the window in borderless fullscreen.

Assets:
  --normal-y-down                        Interpret all normals as having the DirectX convention of Y down. Defaults to Y up.
  --directional-light <x,y,z>            Create a directional light pointing towards the given coordinates.
  --directional-light-intensity <value>  All lights created by the above flag have this intensity. Defaults to 4.
  --gltf-disable-directional-lights      Disable all directional lights in the gltf
  --ambient <value>                      Set the value of the minimum ambient light. This will be treated as white light of this intensity. Defaults to 0.1.
  --scale <scale>                        Scale all objects loaded by this factor. Defaults to 1.0.
  --shadow-distance <value>              Distance from the camera there will be directional shadows. Lower values means higher quality shadows. Defaults to 100.
  --shadow-resolution <value>            Resolution of the shadow map. Higher values mean higher quality shadows with high performance cost. Defaults to 2048.

Controls:
  --walk <speed>               Walk speed (speed without holding shift) in units/second (typically meters). Default 10.
  --run  <speed>               Run speed (speed while holding shift) in units/second (typically meters). Default 50.
  --camera x,y,z,pitch,yaw     Spawns the camera at the given position. Press Period to get the current camera position.

Debug:
  --wait-for-load              Wait for the gltf before rendering the first frame. Useful for debugging and testing rend3.
";

pub struct SceneViewer {
    absolute_mouse: bool,
    desired_backend: Option<Backend>,
    desired_device_name: Option<String>,
    desired_profile: Option<RendererProfile>,
    file_to_load: Option<String>,
    walk_speed: f32,
    run_speed: f32,
    gltf_settings: rend3_gltf::GltfLoadSettings,
    directional_light_direction: Option<Vec3>,
    directional_light_intensity: f32,
    directional_light: Option<DirectionalLightHandle>,
    ambient_light_level: f32,
    present_mode: rend3::types::PresentMode,
    samples: SampleCount,

    fullscreen: bool,
    wait_for_load: bool,
    loading_reciever: Option<Receiver<anyhow::Result<(LoadedGltfScene, GltfSceneInstance)>>>,

    scancode_status: FastHashMap<KeyCode, bool>,
    camera_pitch: f32,
    camera_yaw: f32,
    camera_location: Vec3A,
    previous_profiling_stats: Option<RendererStatistics>,
    last_mouse_delta: Option<DVec2>,

    scene: Option<LoadedGltfScene>,
    instance: Option<GltfSceneInstance>,
    grabber: Option<rend3_framework::Grabber>,
}

impl Default for SceneViewer {
    fn default() -> Self {
        Self {
            absolute_mouse: false,
            desired_backend: None,
            desired_device_name: None,
            desired_profile: None,
            file_to_load: None,
            walk_speed: 10.0,
            run_speed: 50.0,
            gltf_settings: GltfLoadSettings::default(),
            directional_light_direction: None,
            directional_light_intensity: 1.0,
            directional_light: None,
            ambient_light_level: 0.1,
            present_mode: wgpu::PresentMode::Fifo,
            samples: SampleCount::One,
            fullscreen: false,
            wait_for_load: false,
            loading_reciever: None,
            scancode_status: HashMap::default(),

            // Camera settings for the default scene
            camera_pitch: -0.08869916,
            camera_yaw: 5.899576,
            camera_location: Vec3A::new(-2.9936655, 2.189423, 5.308956),

            previous_profiling_stats: None,
            last_mouse_delta: None,
            scene: None,
            instance: None,
            grabber: None,
        }
    }
}
impl SceneViewer {
    pub fn from_args() -> Self {
        #[cfg(feature = "tracy")]
        tracy_client::Client::start();

        // Skip the first two arguments, which are the binary name and the example name.
        let mut args = Arguments::from_vec(std::env::args_os().skip(2).collect());

        // Meta
        let help = args.contains(["-h", "--help"]);

        let mut app = SceneViewer::default();

        // Rendering
        app.desired_backend = option_arg(args.opt_value_from_fn(["-b", "--backend"], extract_backend));
        app.desired_device_name =
            option_arg(args.opt_value_from_str(["-d", "--device"])).map(|s: String| s.to_lowercase());
        app.desired_profile = option_arg(args.opt_value_from_fn(["-p", "--profile"], extract_profile));
        if let Some(samples) = option_arg(args.opt_value_from_fn("--msaa", extract_msaa)) {
            app.samples = samples;
        }
        if let Some(present_mode) = option_arg(args.opt_value_from_fn(["-v", "--vsync"], extract_vsync)) {
            app.present_mode = present_mode;
        }

        // Windowing
        app.absolute_mouse = args.contains("--absolute-mouse");
        app.fullscreen = args.contains("--fullscreen");

        // Assets
        app.gltf_settings.normal_direction = match args.contains("--normal-y-down") {
            true => NormalTextureYDirection::Down,
            false => NormalTextureYDirection::Up,
        };
        app.directional_light_direction = option_arg(args.opt_value_from_fn("--directional-light", extract_vec3));
        if let Some(directional_light_intensity) = option_arg(args.opt_value_from_str("--directional-light-intensity"))
        {
            app.directional_light_intensity = directional_light_intensity;
        }
        if let Some(ambient_light_level) = option_arg(args.opt_value_from_str("--ambient")) {
            app.ambient_light_level = ambient_light_level;
        }
        if let Some(scale) = option_arg(args.opt_value_from_str("--scale")) {
            app.gltf_settings.scale = scale;
        }
        if let Some(shadow_distance) = option_arg(args.opt_value_from_str("--shadow-distance")) {
            app.gltf_settings.directional_light_shadow_distance = shadow_distance;
        }
        if let Some(shadow_resolution) = option_arg(args.opt_value_from_str("--shadow-resolution")) {
            app.gltf_settings.directional_light_resolution = shadow_resolution;
        }
        app.gltf_settings.enable_directional = !args.contains("--gltf-disable-directional-lights");

        // Controls
        if let Some(walk_speed) = option_arg(args.opt_value_from_str("--walk")) {
            app.walk_speed = walk_speed;
        }
        if let Some(run_speed) = option_arg(args.opt_value_from_str("--run")) {
            app.run_speed = run_speed;
        }

        let camera_default =
            [app.camera_location.x, app.camera_location.y, app.camera_location.z, app.camera_pitch, app.camera_yaw];
        if let Ok(camera_info) =
            args.value_from_str("--camera").map(|s: String| extract_array(&s, camera_default).unwrap())
        {
            app.camera_location = Vec3A::new(camera_info[0], camera_info[1], camera_info[2]);
            app.camera_pitch = camera_info[3];
            app.camera_yaw = camera_info[4];
        }

        // Debug
        app.wait_for_load = args.contains("--wait-for-load");

        // Free args
        app.file_to_load = args.free_from_str().ok();

        let remaining = args.finish();

        if !remaining.is_empty() {
            eprint!("Unknown arguments:");
            for flag in remaining {
                eprint!(" '{}'", flag.to_string_lossy());
            }
            eprintln!("\n");

            eprintln!("{}", HELP);
            std::process::exit(1);
        }

        if help {
            eprintln!("{}", HELP);
            std::process::exit(1);
        }

        app
    }
}
impl rend3_framework::App for SceneViewer {
    /// Set handedness of coordinate system
    fn get_handedness(&self) -> rend3::types::Handedness {
        rend3::types::Handedness::Right    // Unlike the other examples, scene_viewer is right-handed.
    }


    fn create_iad<'a>(
        &'a mut self,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<rend3::InstanceAdapterDevice, rend3::RendererInitializationError>>
                + 'a,
        >,
    > {
        Box::pin(async move {
            rend3::create_iad(self.desired_backend, self.desired_device_name.clone(), self.desired_profile, None).await
        })
    }

    fn sample_count(&self) -> SampleCount {
        self.samples
    }

    fn present_mode(&self) -> rend3::types::PresentMode {
        self.present_mode
    }

    fn scale_factor(&self) -> f32 {
        1.0
    }

    fn setup(&mut self, context: rend3_framework::SetupContext<'_>) {
        self.grabber = context.windowing.map(|windowing| rend3_framework::Grabber::new(windowing.window));

        if let Some(direction) = self.directional_light_direction {
            self.directional_light = Some(context.renderer.add_directional_light(DirectionalLight {
                color: Vec3::splat(1.0),
                intensity: self.directional_light_intensity,
                direction,
                distance: self.gltf_settings.directional_light_shadow_distance,
                resolution: 2048,
            }));
        }

        let gltf_settings = self.gltf_settings;
        let file_to_load = self.file_to_load.take();
        let renderer = Arc::clone(context.renderer);
        let routines = Arc::clone(context.routines);

        let (sender, receiver) = flume::bounded(1);

        let wait_for_load = self.wait_for_load;
        spawn(async move {
            let loader = rend3_framework::AssetLoader::new_local(
                concat!(env!("CARGO_MANIFEST_DIR"), "/src/scene_viewer/resources/"),
                "",
                "http://localhost:8000/resources/",
            );
            if let Err(e) = load_skybox(&renderer, &loader, &routines.skybox).await {
                println!("Failed to load skybox {}", e)
            };
            let loaded = load_gltf(
                &renderer,
                &loader,
                &gltf_settings,
                file_to_load
                    .as_deref()
                    .map_or_else(|| AssetPath::Internal("default-scene/scene.gltf"), AssetPath::External),
            )
            .await;

            sender.send(loaded).unwrap();
        });

        if wait_for_load {
            let (scene, instance) = receiver.recv().unwrap().unwrap();
            self.scene = Some(scene);
            self.instance = Some(instance);
        } else {
            self.loading_reciever = Some(receiver);
        }
    }

    fn handle_event(&mut self, context: rend3_framework::EventContext<'_>, event: winit::event::Event<()>) {
        match event {
            Event::WindowEvent { event: WindowEvent::Focused(focus), .. } => {
                if !focus {
                    self.grabber.as_mut().unwrap().request_ungrab(context.window.as_ref().unwrap());
                }
            }

            Event::WindowEvent {
                event: WindowEvent::KeyboardInput { event: KeyEvent { physical_key, state, .. }, .. },
                ..
            } => {
                let PhysicalKey::Code(scancode) = physical_key else {
                    return;
                };

                log::info!("Key Pressed {:?}", scancode);
                self.scancode_status.insert(
                    scancode,
                    match state {
                        ElementState::Pressed => true,
                        ElementState::Released => false,
                    },
                );
            }

            Event::WindowEvent {
                event: WindowEvent::MouseInput { button: MouseButton::Left, state: ElementState::Pressed, .. },
                ..
            } => {
                let grabber = self.grabber.as_mut().unwrap();

                if !grabber.grabbed() {
                    grabber.request_grab(context.window.as_ref().unwrap());
                }
            }
            Event::DeviceEvent { event: DeviceEvent::MouseMotion { delta: (delta_x, delta_y), .. }, .. } => {
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

                self.camera_yaw -= (mouse_delta.x / 1000.0) as f32;
                self.camera_pitch -= (mouse_delta.y / 1000.0) as f32;
                if self.camera_yaw < 0.0 {
                    self.camera_yaw += TAU;
                } else if self.camera_yaw >= TAU {
                    self.camera_yaw -= TAU;
                }
                self.camera_pitch =
                    self.camera_pitch.clamp(-std::f32::consts::FRAC_PI_2 + 0.0001, std::f32::consts::FRAC_PI_2 - 0.0001)
            }
            _ => {}
        }
    }

    fn handle_redraw(&mut self, context: rend3_framework::RedrawContext<'_>) {
        profiling::scope!("RedrawRequested");

        if let Some(ref receiver) = self.loading_reciever {
            if let Ok(loaded) = receiver.try_recv() {
                let (scene, instance) = loaded.unwrap();
                self.scene = Some(scene);
                self.instance = Some(instance);
                self.loading_reciever = None;
            }
        }

        let rotation = Mat3A::from_euler(glam::EulerRot::XYZ, -self.camera_pitch, -self.camera_yaw, 0.0).transpose();
        let forward = -rotation.z_axis;
        let up = rotation.y_axis;
        let side = -rotation.x_axis;
        let velocity =
            if button_pressed(&self.scancode_status, KeyCode::ShiftLeft) { self.run_speed } else { self.walk_speed };
        if button_pressed(&self.scancode_status, KeyCode::KeyW) {
            self.camera_location += forward * velocity * context.delta_t_seconds;
        }
        if button_pressed(&self.scancode_status, KeyCode::KeyS) {
            self.camera_location -= forward * velocity * context.delta_t_seconds;
        }
        if button_pressed(&self.scancode_status, KeyCode::KeyA) {
            self.camera_location += side * velocity * context.delta_t_seconds;
        }
        if button_pressed(&self.scancode_status, KeyCode::KeyD) {
            self.camera_location -= side * velocity * context.delta_t_seconds;
        }
        if button_pressed(&self.scancode_status, KeyCode::KeyQ) {
            self.camera_location += up * velocity * context.delta_t_seconds;
        }
        if button_pressed(&self.scancode_status, KeyCode::Period) {
            println!(
                "{x},{y},{z},{pitch},{yaw}",
                x = self.camera_location.x,
                y = self.camera_location.y,
                z = self.camera_location.z,
                pitch = self.camera_pitch,
                yaw = self.camera_yaw
            );
        }

        if button_pressed(&self.scancode_status, winit::keyboard::KeyCode::Escape) {
            self.grabber.as_mut().unwrap().request_ungrab(context.window.as_ref().unwrap());
        }

        if button_pressed(&self.scancode_status, KeyCode::KeyP) {
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

        context
            .renderer
            .set_camera_data(Camera { projection: CameraProjection::Perspective { vfov: 60.0, near: 0.1 }, view });

        // Lock all the routines
        let pbr_routine = lock(&context.routines.pbr);
        let mut skybox_routine = lock(&context.routines.skybox);
        let tonemapping_routine = lock(&context.routines.tonemapping);

        // Swap the instruction buffers so that our frame's changes can be processed.
        context.renderer.swap_instruction_buffers();
        // Evaluate our frame's world-change instructions
        let mut eval_output = context.renderer.evaluate_instructions();
        // Evaluate changes to routines.
        skybox_routine.evaluate(context.renderer);

        // Build a rendergraph
        let mut graph = rend3::graph::RenderGraph::new();

        let frame_handle = graph.add_imported_render_target(
            context.surface_texture,
            0..1,
            0..1,
            rend3::graph::ViewportRect::from_size(context.resolution),
        );
        // Add the default rendergraph
        context.base_rendergraph.add_to_graph(
            &mut graph,
            rend3_routine::base::BaseRenderGraphInputs {
                eval_output: &eval_output,
                routines: rend3_routine::base::BaseRenderGraphRoutines {
                    pbr: &pbr_routine,
                    skybox: Some(&skybox_routine),
                    tonemapping: &tonemapping_routine,
                },
                target: rend3_routine::base::OutputRenderTarget {
                    handle: frame_handle,
                    resolution: context.resolution,
                    samples: self.samples,
                },
            },
            rend3_routine::base::BaseRenderGraphSettings {
                ambient_color: Vec3::splat(self.ambient_light_level).extend(1.0),
                clear_color: glam::Vec4::new(0.0, 0.0, 0.0, 1.0),
            },
        );

        // Dispatch a render using the built up rendergraph!
        self.previous_profiling_stats = graph.execute(context.renderer, &mut eval_output);

        // mark the end of the frame for tracy/other profilers
        profiling::finish_frame!();
    }
}

pub fn main() {
    let app = SceneViewer::from_args();

    let mut builder = winit::window::WindowAttributes::default().with_title("scene-viewer").with_maximized(true);
    if app.fullscreen {
        builder = builder.with_fullscreen(Some(Fullscreen::Borderless(None)));
    }

    rend3_framework::start(app, builder);
}

#[cfg(test)]
#[rend3_test::test_attr]
async fn default_scene() {
    let mut app = SceneViewer::default();
    app.file_to_load = Some("src/scene_viewer/resources/default-scene/scene.gltf".into());
    app.wait_for_load = true;
    app.samples = SampleCount::Four;

    crate::tests::test_app(crate::tests::TestConfiguration {
        app,
        reference_path: "src/scene_viewer/screenshot.png",
        size: glam::UVec2::new(1280, 720),
        threshold_set: rend3_test::Threshold::Mean(0.01).into(),
    })
    .await
    .unwrap();
}

#[cfg(test)]
#[rend3_test::test_attr]
async fn bistro() {
    let mut app = SceneViewer::default();
    app.file_to_load = Some("src/scene_viewer/resources/bistro-full/bistro.gltf".into());
    app.wait_for_load = true;
    app.samples = SampleCount::Four;
    app.gltf_settings.normal_direction = NormalTextureYDirection::Down;
    app.gltf_settings.enable_directional = false;
    app.directional_light_direction = Some(Vec3::new(1.0, -5.0, -1.0));
    app.directional_light_intensity = 15.0;

    app.camera_location = Vec3A::new(-17.174278, 3.715882, -4.631997);
    app.camera_pitch = 0.04430086;
    app.camera_yaw = 4.6065736;

    crate::tests::test_app(crate::tests::TestConfiguration {
        app,
        reference_path: "src/scene_viewer/bistro.png",
        size: glam::UVec2::new(1280, 720),
        threshold_set: rend3_test::Threshold::Mean(0.02).into(),
    })
    .await
    .unwrap();
}
