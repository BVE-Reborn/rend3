use arrayvec::ArrayVec;
use fnv::FnvBuildHasher;
use glam::{DVec2, UVec2, Vec3, Vec3A};
use pico_args::Arguments;
use rend3::{
    types::{Backend, Camera, CameraProjection, DirectionalLight, Texture, TextureFormat},
    Renderer,
};
use rend3_pbr::PbrRenderRoutine;
use std::{
    collections::HashMap,
    hash::BuildHasher,
    num::NonZeroU32,
    path::Path,
    time::{Duration, Instant},
};
use wgpu_profiler::GpuTimerScopeResult;
use winit::{
    event::{DeviceEvent, ElementState, Event, KeyboardInput, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

mod platform;

fn load_skybox(renderer: &Renderer, routine: &mut PbrRenderRoutine) -> Result<(), Box<dyn std::error::Error>> {
    let name = concat!(env!("CARGO_MANIFEST_DIR"), "/data/skybox.basis");
    let file = std::fs::read(name).unwrap_or_else(|_| panic!("Could not read skybox {}", name));

    let transcoder = basis::Transcoder::new();
    let image_info = transcoder.get_image_info(&file, 0).ok_or("skybox image missing")?;

    let mips = transcoder.get_total_image_levels(&file, 0);

    let mut prepared = transcoder
        .prepare_transcoding(&file)
        .ok_or("could not prepare skybox transcoding")?;
    let mut image = Vec::with_capacity(image_info.total_blocks as usize * 16 * 6);
    for i in 0..6 {
        for mip in 0..mips {
            let mip_info = transcoder.get_image_level_info(&file, 0, mip).unwrap();
            let data = prepared.transcode_image_level(i, mip, basis::TargetTextureFormat::Rgba32)?;
            image.extend_from_slice(&data[0..(mip_info.orig_width * mip_info.orig_height * 4) as usize]);
        }
    }
    drop(prepared);

    let handle = renderer.add_texture_cube(Texture {
        format: TextureFormat::Rgba8UnormSrgb,
        size: UVec2::new(image_info.width, image_info.height),
        data: image,
        label: Some("background".into()),
        mip_count: rend3::types::MipmapCount::Specific(NonZeroU32::new(mips).unwrap()),
        mip_source: rend3::types::MipmapSource::Uploaded,
    });
    routine.set_background_texture(Some(handle));
    Ok(())
}

fn load_gltf(renderer: &Renderer, location: String) -> rend3_gltf::LoadedGltfScene {
    let path = Path::new(&location);
    let parent = path.parent().unwrap();

    println!("Reading gltf file: {}", path.display());
    let gltf_data =
        std::fs::read(&path).unwrap_or_else(|e| panic!("tried to load gltf file {}: {}", path.display(), e));

    pollster::block_on(rend3_gltf::load_gltf(renderer, &gltf_data, |uri| {
        rend3_gltf::filesystem_io_func(&parent, uri)
    }))
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

fn main() {
    env_logger::init();

    let mut args = Arguments::from_env();
    let absolute_mouse: bool = args.contains("--absolute-mouse");
    let desired_backend = args.value_from_fn(["-b", "--backend"], extract_backend).ok();
    let desired_device_name: Option<String> = args
        .value_from_str(["-d", "--device"])
        .ok()
        .map(|s: String| s.to_lowercase());
    let desired_mode = args.value_from_fn(["-m", "--mode"], extract_mode).ok();
    let file_to_load: Option<String> = args.free_from_str().ok();

    let event_loop = EventLoop::new();

    let window = {
        let mut builder = WindowBuilder::new();
        builder = builder.with_title("scene-viewer");
        builder.build(&event_loop).expect("Could not build window")
    };

    let window_size = window.inner_size();

    // Create the Instance, Adapter, and Device needed
    let iad = pollster::block_on(rend3::create_iad(desired_backend, desired_device_name, desired_mode)).unwrap();

    // The one line of unsafe needed. We just need to guarentee that the window outlives the use of the surface.
    let surface = unsafe { iad.instance.create_surface(&window) };
    // Get the preferred format for the surface.
    let format = surface.get_preferred_format(&iad.adapter).unwrap();
    // Configure the surface to be ready for rendering.
    rend3::configure_surface(
        &surface,
        &iad.device,
        format,
        UVec2::new(window_size.width, window_size.height),
        rend3::types::PresentMode::Mailbox,
    );

    // Make us a renderer.
    let renderer = rend3::Renderer::new(iad, Some(window_size.width as f32 / window_size.height as f32)).unwrap();

    // Create the pbr pipeline with the same internal resolution and 4x multisampling
    let mut routine = rend3_pbr::PbrRenderRoutine::new(
        &renderer,
        rend3_pbr::RenderTextureOptions {
            resolution: UVec2::new(window_size.width, window_size.height),
            samples: rend3_pbr::SampleCount::One,
        },
        format,
    );

    let _loaded_gltf = load_gltf(
        &renderer,
        file_to_load.unwrap_or_else(|| concat!(env!("CARGO_MANIFEST_DIR"), "/data/scene.gltf").to_owned()),
    );
    load_skybox(&renderer, &mut routine).unwrap();

    // let _directional_light = renderer.add_directional_light(DirectionalLight {
    //     color: Vec3::ONE,
    //     intensity: 10.0,
    //     direction: Vec3::new(-1.0, -1.0, 1.0),
    //     distances: {
    //         let mut vec = ArrayVec::new();
    //         vec.push(0.0);
    //         vec.push(20.0);
    //         vec.push(50.0);
    //         vec
    //     }
    // });
    let mut scancode_status = HashMap::with_hasher(FnvBuildHasher::default());

    let mut camera_location = Camera {
        projection: CameraProjection::Projection {
            vfov: 60.0,
            near: 0.1,
            pitch: std::f32::consts::FRAC_PI_4,
            yaw: -std::f32::consts::FRAC_PI_4,
        },
        location: Vec3A::new(20.0, 20.0, -20.0),
    };

    let mut previous_profiling_stats: Option<Vec<GpuTimerScopeResult>> = None;

    let mut timestamp_last_second = Instant::now();
    let mut timestamp_last_frame = Instant::now();

    let mut frame_times = histogram::Histogram::new();

    let mut last_mouse_delta = None;

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
                if let CameraProjection::Projection { yaw, pitch, .. } = camera_location.projection {
                    Vec3A::new(yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos())
                } else {
                    unreachable!()
                }
            };
            let up = Vec3A::Y;
            let side: Vec3A = forward.cross(up).normalize();
            let velocity = if button_pressed(&scancode_status, platform::Scancodes::SHIFT) {
                100.0
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

            if button_pressed(&scancode_status, platform::Scancodes::P) {
                // write out gpu side performance info into a trace readable by chrome://tracing
                if let Some(ref stats) = previous_profiling_stats {
                    println!("Outputing gpu timing chrome trace to profile.json");
                    wgpu_profiler::chrometrace::write_chrometrace(Path::new("profile.json"), stats).unwrap();
                } else {
                    println!("No gpu timing trace available, either timestamp queries are unsupported or not enough frames have elapsed yet!");
                }
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

            let mouse_delta = if absolute_mouse {
                let prev = last_mouse_delta.replace(DVec2::new(delta_x, delta_y));
                if let Some(prev) = prev {
                    (DVec2::new(delta_x, delta_y) - prev) / 4.0
                } else {
                    return;
                }
            } else {
                DVec2::new(delta_x, delta_y)
            };


            if let CameraProjection::Projection { ref mut yaw, ref mut pitch, .. } = camera_location.projection {
                *yaw += (mouse_delta.x / 1000.0) as f32;
                *pitch += (mouse_delta.y / 1000.0) as f32;
                if *yaw < 0.0 {
                    *yaw += TAU;
                } else if *yaw >= TAU {
                    *yaw -= TAU;
                }
                *pitch = pitch
                    .max(-std::f32::consts::FRAC_PI_2 + 0.0001)
                    .min(std::f32::consts::FRAC_PI_2 - 0.0001);
            } else {
                unreachable!()
            }
        }
        Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } => {
            *control = ControlFlow::Exit;
        }
        // Window was resized, need to resize renderer.
        winit::event::Event::WindowEvent {
            event: winit::event::WindowEvent::Resized(size),
            ..
        } => {
            let size = UVec2::new(size.width, size.height);
            // Reconfigure the surface for the new size.
            rend3::configure_surface(
                &surface,
                &renderer.device,
                format,
                UVec2::new(size.x, size.y),
                rend3::types::PresentMode::Mailbox,
            );
            // Tell the renderer about the new aspect ratio.
            renderer.set_aspect_ratio(size.x as f32 / size.y as f32);
            // Resize the internal buffers to the same size as the screen.
            routine.resize(
                &renderer,
                rend3_pbr::RenderTextureOptions {
                    resolution: size,
                    samples: rend3_pbr::SampleCount::One,
                },
            );
        }
        // Render!
        winit::event::Event::RedrawRequested(..) => {
            // Update camera
            renderer.set_camera_data(camera_location);
            // Get a frame
            let frame = rend3::util::output::OutputFrame::from_surface(&surface).unwrap();
            // Dispatch a render!
            previous_profiling_stats = renderer.render(&mut routine, frame);
            // mark the end of the frame for tracy/other profilers
            profiling::finish_frame!();
        }
        _ => {}
    })
}
