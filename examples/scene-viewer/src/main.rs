use glam::{DVec2, Mat3A, Mat4, UVec2, Vec3, Vec3A};
use parking_lot::Mutex;
use pico_args::Arguments;
use rend3::{
    format_sso,
    types::{Backend, Camera, CameraProjection, DirectionalLight, Texture, TextureFormat},
    util::typedefs::FastHashMap,
    Renderer,
};
use rend3_pbr::SkyboxRoutine;
use std::{
    collections::HashMap,
    hash::BuildHasher,
    num::NonZeroU32,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use wgpu_profiler::GpuTimerScopeResult;
use winit::{
    event::{DeviceEvent, ElementState, Event, KeyboardInput, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

mod platform;

fn load_skybox(renderer: &Renderer, skybox_routine: &Mutex<SkyboxRoutine>) -> Result<(), Box<dyn std::error::Error>> {
    profiling::scope!("load skybox");

    let name = concat!(env!("CARGO_MANIFEST_DIR"), "/data/skybox.basis");
    let file = std::fs::read(name)?;

    let mut transcoder = basis_universal::Transcoder::new();
    let image_info = transcoder.image_info(&file, 0).ok_or("skybox image missing")?;

    let mips = transcoder.image_level_count(&file, 0);

    {
        profiling::scope!("prepare basis transcoding");
        transcoder
            .prepare_transcoding(&file)
            .map_err(|_| "could not prepare skybox transcoding")?
    }
    let mut image = Vec::with_capacity(image_info.m_total_blocks as usize * 16 * 6);
    for i in 0..6 {
        for mip in 0..mips {
            profiling::scope!("basis transcoding", &format_sso!("layer {} mip {}", i, mip));
            let mip_info = transcoder.image_level_info(&file, 0, mip).unwrap();
            let data = transcoder
                .transcode_image_level(
                    &file,
                    basis_universal::TranscoderTextureFormat::RGBA32,
                    basis_universal::TranscodeParameters {
                        image_index: i,
                        level_index: mip,
                        decode_flags: None,
                        output_row_pitch_in_blocks_or_pixels: None,
                        output_rows_in_pixels: None,
                    },
                )
                .map_err(|_| "failed to transcode")?;
            image.extend_from_slice(&data[0..(mip_info.m_orig_width * mip_info.m_orig_height * 4) as usize]);
        }
    }

    let handle = renderer.add_texture_cube(Texture {
        format: TextureFormat::Rgba8UnormSrgb,
        size: UVec2::new(image_info.m_width, image_info.m_height),
        data: image,
        label: Some("background".into()),
        mip_count: rend3::types::MipmapCount::Specific(NonZeroU32::new(mips).unwrap()),
        mip_source: rend3::types::MipmapSource::Uploaded,
    });
    skybox_routine.lock().set_background_texture(Some(handle));
    Ok(())
}

fn load_gltf(renderer: &Renderer, location: String) -> rend3_gltf::LoadedGltfScene {
    profiling::scope!("loading gltf");
    let path = Path::new(&location);
    let parent = path.parent().unwrap();

    let path_str = path.as_os_str().to_string_lossy();
    log::info!("Reading gltf file: {}", path_str);
    let gltf_data = {
        profiling::scope!("reading gltf file", &path_str);
        std::fs::read(&path).unwrap_or_else(|e| panic!("tried to load gltf file {}: {}", path_str, e))
    };

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

    let (event_loop, window) = {
        profiling::scope!("creating window");
        let event_loop = EventLoop::new();
        let mut builder = WindowBuilder::new();
        builder = builder.with_title("scene-viewer");
        let window = builder.build(&event_loop).expect("Could not build window");

        (event_loop, window)
    };

    let window_size = window.inner_size();

    // Create the Instance, Adapter, and Device needed
    let iad = pollster::block_on(rend3::create_iad(desired_backend, desired_device_name, desired_mode)).unwrap();

    // The one line of unsafe needed. We just need to guarentee that the window outlives the use of the surface.
    let surface = unsafe {
        profiling::scope!("creating surface");
        Arc::new(iad.instance.create_surface(&window))
    };
    // Get the preferred format for the surface.
    let format = {
        profiling::scope!("getting preferred format");
        surface.get_preferred_format(&iad.adapter).unwrap()
    };
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
    let render_texture_options = rend3_pbr::RenderTextureOptions {
        resolution: UVec2::new(window_size.width, window_size.height),
        samples: rend3_pbr::SampleCount::One,
    };
    let pbr_routine = Arc::new(Mutex::new(rend3_pbr::PbrRenderRoutine::new(
        &renderer,
        render_texture_options,
    )));
    let skybox_routine = Arc::new(Mutex::new(rend3_pbr::SkyboxRoutine::new(
        &renderer,
        render_texture_options,
    )));
    let tonemapping_routine = Arc::new(Mutex::new(rend3_pbr::TonemappingRoutine::new(
        &renderer,
        render_texture_options.resolution,
        format,
    )));

    pbr_routine
        .lock()
        .set_ambient_color(glam::Vec4::new(0.15, 0.15, 0.15, 1.0));

    let skybox_routine_clone = Arc::clone(&skybox_routine);
    let renderer_clone = Arc::clone(&renderer);
    let _loaded_gltf = std::thread::spawn(move || {
        profiling::register_thread!("asset loading");
        if let Err(e) = load_skybox(&renderer_clone, &skybox_routine_clone) {
            println!("Failed to load skybox {}", e)
        };
        load_gltf(
            &renderer_clone,
            file_to_load.unwrap_or_else(|| concat!(env!("CARGO_MANIFEST_DIR"), "/data/scene.gltf").to_owned()),
        )
    });

    let _directional_light = renderer.add_directional_light(DirectionalLight {
        color: Vec3::ONE,
        intensity: 10.0,
        direction: Vec3::new(-1.0, -1.0, 0.0),
        distance: 400.0,
    });
    let mut scancode_status = FastHashMap::default();

    let mut camera_pitch = std::f32::consts::FRAC_PI_4;
    let mut camera_yaw = -std::f32::consts::FRAC_PI_4;
    let mut camera_location = Vec3A::new(20.0, 20.0, -20.0);

    let mut previous_profiling_stats: Option<Vec<GpuTimerScopeResult>> = None;

    let mut timestamp_last_second = Instant::now();
    let mut timestamp_last_frame = Instant::now();

    let mut frame_times = histogram::Histogram::new();

    let mut last_mouse_delta = None;

    event_loop.run(move |event, _window_target, control| match event {
        Event::MainEventsCleared => {
            profiling::scope!("MainEventsCleared");
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

            let rotation = Mat3A::from_euler(glam::EulerRot::XYZ, -camera_pitch, -camera_yaw, 0.0).transpose();
            let forward = rotation.z_axis;
            let up = rotation.y_axis;
            let side = -rotation.x_axis;
            let velocity = if button_pressed(&scancode_status, platform::Scancodes::SHIFT) {
                50.0
            } else {
                10.0
            };
            if button_pressed(&scancode_status, platform::Scancodes::W) {
                camera_location += forward * velocity * delta_time.as_secs_f32();
            }
            if button_pressed(&scancode_status, platform::Scancodes::S) {
                camera_location -= forward * velocity * delta_time.as_secs_f32();
            }
            if button_pressed(&scancode_status, platform::Scancodes::A) {
                camera_location += side * velocity * delta_time.as_secs_f32();
            }
            if button_pressed(&scancode_status, platform::Scancodes::D) {
                camera_location -= side * velocity * delta_time.as_secs_f32();
            }
            if button_pressed(&scancode_status, platform::Scancodes::Q) {
                camera_location += up * velocity * delta_time.as_secs_f32();
            }
            if button_pressed(&scancode_status, platform::Scancodes::Z) {
                camera_location -= up * velocity * delta_time.as_secs_f32();
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


            camera_yaw += (mouse_delta.x / 1000.0) as f32;
            camera_pitch += (mouse_delta.y / 1000.0) as f32;
            if camera_yaw < 0.0 {
                camera_yaw += TAU;
            } else if camera_yaw >= TAU {
                camera_yaw -= TAU;
            }
            camera_pitch = camera_pitch
                .max(-std::f32::consts::FRAC_PI_2 + 0.0001)
                .min(std::f32::consts::FRAC_PI_2 - 0.0001);
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
            profiling::scope!("Resized", &format_sso!("{}x{}", size.width, size.height));
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
            pbr_routine.lock().resize(
                &renderer,
                rend3_pbr::RenderTextureOptions {
                    resolution: size,
                    samples: rend3_pbr::SampleCount::One,
                },
            );
            tonemapping_routine.lock().resize(
                size,
            );
        }
        // Render!
        winit::event::Event::RedrawRequested(..) => {
        profiling::scope!("RedrawRequested");
            // Update camera

            let view  = Mat4::from_euler(glam::EulerRot::XYZ, -camera_pitch, -camera_yaw, 0.0);
            let view = view * Mat4::from_translation((-camera_location).into());

            renderer.set_camera_data(Camera {
                projection: CameraProjection::Perspective {
                    vfov: 60.0,
                    near: 0.1,
                },
                view
            });

            // Get a frame
            let frame = rend3::util::output::OutputFrame::Surface { surface: Arc::clone(&surface) };
            // Lock all the routines
            let pbr_routine = pbr_routine.lock();
            let skybox_routine = skybox_routine.lock();
            let tonemapping_routine = tonemapping_routine.lock();

            // Ready up the renderer
            let (cmd_bufs, ready) = renderer.ready();
            
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
            previous_profiling_stats = graph.execute(&renderer, frame, cmd_bufs, &ready);
            // mark the end of the frame for tracy/other profilers
            profiling::finish_frame!();
        }
        _ => {}
    })
}
