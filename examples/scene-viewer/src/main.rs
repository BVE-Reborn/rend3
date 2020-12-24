use fnv::FnvBuildHasher;
use glam::{Vec3, Vec3A};
use pico_args::Arguments;
use rend3::{
    datatypes::{CameraLocation, DirectionalLight, RendererTextureFormat, Texture},
    list::{DefaultPipelines, DefaultShaders},
    Renderer,
};
use std::{
    collections::HashMap,
    hash::BuildHasher,
    path::Path,
    time::{Duration, Instant},
};
use winit::{
    event::{DeviceEvent, ElementState, Event, KeyboardInput, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

mod platform;

fn load_skybox(renderer: &Renderer) -> Result<(), Box<dyn std::error::Error>> {
    let name = concat!(env!("CARGO_MANIFEST_DIR"), "/data/skybox.basis");
    let file = std::fs::read(name).unwrap_or_else(|_| panic!("Could not read skybox {}", name));

    let transcoder = basis::Transcoder::new();
    let image_info = transcoder.get_image_info(&file, 0).ok_or("skybox image missing")?;

    let mut prepared = transcoder
        .prepare_transcoding(&file)
        .ok_or("could not prepare skybox transcoding")?;
    let mut image = Vec::with_capacity(image_info.total_blocks as usize * 16 * 6);
    for i in 0..6 {
        image.extend_from_slice(&prepared.transcode_image_level(i, 0, basis::TargetTextureFormat::Bc7Rgba)?);
    }
    drop(prepared);

    let handle = renderer.add_texture_cube(Texture {
        format: RendererTextureFormat::Bc7Srgb,
        width: image_info.width,
        height: image_info.height,
        data: image,
        label: Some("background".into()),
    });
    renderer.set_background_texture(handle);
    Ok(())
}

fn load_gltf(renderer: &Renderer, location: String) -> rend3_gltf::LoadedGltfScene {
    let path = Path::new(&location);
    let binary_path = path.with_extension("bin");
    let parent = path.parent().unwrap();

    println!("Reading gltf file: {}", path.display());
    let gltf_data =
        std::fs::read(&path).unwrap_or_else(|e| panic!("tried to load gltf file {}: {}", path.display(), e));
    println!("Reading gltf sidecar file file: {}", binary_path.display());
    let bin_data = std::fs::read(&binary_path).unwrap_or_else(|e| {
        panic!(
            "tried to load gltf binary sidecar file {}: {}",
            binary_path.display(),
            e
        )
    });

    pollster::block_on(rend3_gltf::load_gltf(
        renderer,
        &gltf_data,
        &bin_data,
        move |tex_path| {
            println!("Reading image file: {}", tex_path);
            let tex_path = tex_path.to_owned();
            async move {
                let tex_resolved = parent.join(&tex_path);
                async_std::fs::read(tex_resolved).await
            }
        },
    ))
    .unwrap()
}

fn button_pressed<Hash: BuildHasher>(map: &HashMap<u32, bool, Hash>, key: u32) -> bool {
    map.get(&key).map_or(false, |b| *b)
}

fn extract_backend(value: &str) -> Result<wgpu::Backend, &'static str> {
    Ok(match value.to_lowercase().as_str() {
        "vulkan" | "vk" => wgpu::Backend::Vulkan,
        "dx12" | "12" => wgpu::Backend::Dx12,
        "dx11" | "11" => wgpu::Backend::Dx11,
        "metal" | "mtl" => wgpu::Backend::Metal,
        "opengl" | "gl" => wgpu::Backend::Gl,
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

fn main() {
    wgpu_subscriber::initialize_default_subscriber(None);

    let mut args = Arguments::from_env();
    let desired_backend = args.value_from_fn(["-b", "--backend"], extract_backend).ok();
    let desired_device_name: Option<String> = args
        .value_from_str(["-d", "--device"])
        .ok()
        .map(|s: String| s.to_lowercase());
    let desired_mode = args.value_from_fn(["-m", "--mode"], extract_mode).ok();
    let file_to_load: Option<String> = args.free_from_str().unwrap();

    rend3::span_transfer!(_ -> main_thread_span, INFO, "Main Thread Setup");
    rend3::span_transfer!(_ -> event_loop_span, INFO, "Building Event Loop");

    let event_loop = EventLoop::new();

    rend3::span_transfer!(event_loop_span -> window_span, INFO, "Building Window");

    let window = {
        let mut builder = WindowBuilder::new();
        builder = builder.with_title("scene-viewer");
        builder.build(&event_loop).expect("Could not build window")
    };

    rend3::span_transfer!(window_span -> renderer_span, INFO, "Building Renderer");

    let window_size = window.inner_size();

    let mut options = rend3::RendererOptions {
        vsync: rend3::VSyncMode::Off,
        size: [window_size.width, window_size.height],
    };

    let renderer = pollster::block_on(
        rend3::RendererBuilder::new(options.clone())
            .window(&window)
            .desired_device(desired_backend, desired_device_name, desired_mode)
            .build(),
    )
    .unwrap();

    let pipelines = pollster::block_on(async {
        let shaders = DefaultShaders::new(&renderer).await;
        DefaultPipelines::new(&renderer, &shaders).await
    });

    rend3::span_transfer!(renderer_span -> loading_span, INFO, "Loading resources");

    load_gltf(
        &renderer,
        file_to_load.unwrap_or_else(|| concat!(env!("CARGO_MANIFEST_DIR"), "/data/scene.gltf").to_owned()),
    );
    load_skybox(&renderer).unwrap();

    renderer.add_directional_light(DirectionalLight {
        color: Vec3::one(),
        intensity: 10.0,
        direction: Vec3::new(-1.0, -1.0, 0.0),
    });
    // renderer.add_directional_light(DirectionalLight {
    //     color: Vec3::one(),
    //     intensity: 2.0,
    //     direction: Vec3::new(1.0, 0.0, 0.0),
    // });

    rend3::span_transfer!(loading_span -> _);
    rend3::span_transfer!(main_thread_span -> _);

    let mut scancode_status = HashMap::with_hasher(FnvBuildHasher::default());

    let mut camera_location = CameraLocation::default();

    let mut timestamp_last_second = Instant::now();
    let mut timestamp_last_frame = Instant::now();

    let mut frame_times = histogram::Histogram::new();

    event_loop.run(move |event, _window_target, control| match event {
        Event::MainEventsCleared => {
            let now = Instant::now();

            let delta_time = now - timestamp_last_frame;
            frame_times.increment(delta_time.as_micros() as u64).unwrap();

            let elapsed_since_second = now - timestamp_last_second;
            if elapsed_since_second > Duration::from_secs(1) {
                let count = frame_times.entries();
                println!(
                    "{:0>5} frames over {:0>5.2}s. Min: {:0>5.2}ms; Average: {:0>5.2}ms; 95%: {:0>5.2}ms; 99%: {:0>5.2}ms; Max: {:0>5.2}ms; StdDev: {:0>5.2}ms",
                    count,
                    elapsed_since_second.as_secs_f32(),
                    frame_times.minimum().unwrap() as f32 / 1_000.0,
                    frame_times.mean().unwrap() as f32 / 1_000.0,
                    frame_times.percentile(95.0).unwrap() as f32 / 1_000.0,
                    frame_times.percentile(99.0).unwrap() as f32 / 1_000.0,
                    frame_times.maximum().unwrap() as f32 / 1_000.0,
                    frame_times.stddev().unwrap() as f32 / 1_000.0,
                );
                timestamp_last_second = now;
                frame_times.clear();
            }

            timestamp_last_frame = now;

            let forward = {
                let CameraLocation { yaw, pitch, .. } = camera_location;
                Vec3A::new(yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos())
            };
            let up = Vec3A::unit_y();
            let side: Vec3A = forward.cross(up).normalize();
            let velocity = if button_pressed(&scancode_status, platform::Scancodes::SHIFT) {
                10.0
            } else {
                1.0
            };
            if button_pressed(&scancode_status, platform::Scancodes::W) {
                camera_location.location += forward * velocity * delta_time.as_secs_f32();
            }
            if button_pressed(&scancode_status, platform::Scancodes::S) {
                camera_location.location -= forward * velocity * delta_time.as_secs_f32();
            }
            if button_pressed(&scancode_status, platform::Scancodes::A) {
                camera_location.location += side * velocity * delta_time.as_secs_f32();
            }
            if button_pressed(&scancode_status, platform::Scancodes::D) {
                camera_location.location -= side * velocity * delta_time.as_secs_f32();
            }
            if button_pressed(&scancode_status, platform::Scancodes::Q) {
                camera_location.location += up * velocity * delta_time.as_secs_f32();
            }
            if button_pressed(&scancode_status, platform::Scancodes::Z) {
                camera_location.location -= up * velocity * delta_time.as_secs_f32();
            }

            window.request_redraw();
        }
        Event::WindowEvent {
            event:
                WindowEvent::KeyboardInput {
                    input: KeyboardInput { scancode, state, .. },
                    ..
                },
            ..
        } => {
            scancode_status.insert(
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

            camera_location.yaw += (delta_x / 1000.0) as f32;
            camera_location.pitch += (delta_y / 1000.0) as f32;
            if camera_location.yaw < 0.0 {
                camera_location.yaw += TAU;
            } else if camera_location.yaw >= TAU {
                camera_location.yaw -= TAU;
            }
            camera_location.pitch = camera_location
                .pitch
                .max(-std::f32::consts::FRAC_PI_2 + 0.0001)
                .min(std::f32::consts::FRAC_PI_2 - 0.0001);
        }
        Event::WindowEvent {
            event: WindowEvent::Resized(size),
            ..
        } => {
            options.size = [size.width, size.height];
        }
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            *control = ControlFlow::Exit;
        }
        Event::RedrawRequested(_) => {
            rend3::span_transfer!(_ -> redraw_span, INFO, "Redraw");

            renderer.set_camera_location(camera_location);
            renderer.set_options(options.clone());

            let list = rend3::list::default_render_list(
                renderer.mode(),
                [
                    (options.size[0] as f32 * 1.0) as u32,
                    (options.size[1] as f32 * 1.0) as u32,
                ],
                &pipelines,
            );
            let handle = renderer.render(list, rend3::RendererOutput::InternalSwapchain);

            rend3::span_transfer!(redraw_span -> render_wait_span, INFO, "Waiting for render");
            pollster::block_on(handle);
        }
        _ => {}
    })
}
