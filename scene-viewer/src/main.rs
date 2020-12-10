use fnv::FnvBuildHasher;
use glam::{Mat4, Quat, Vec2, Vec3, Vec3A};
use imgui::FontSource;
use obj::{IndexTuple, Obj};
use pico_args::Arguments;
use rend3::{
    datatypes::{
        AffineTransform, AlbedoComponent, CameraLocation, DirectionalLight, Material, MaterialComponent,
        MaterialHandle, Mesh, MeshHandle, ModelVertex, Object, RendererTextureFormat, Texture, TextureHandle,
    },
    list::{DefaultPipelines, DefaultShaders},
    Renderer, RendererOptions, VSyncMode,
};
use std::{
    collections::hash_map::{self, HashMap},
    hash::BuildHasher,
    sync::Arc,
    time::{Duration, Instant},
};
use switchyard::{threads, Switchyard};
use winit::{
    dpi::PhysicalSize,
    event::{DeviceEvent, ElementState, Event, KeyboardInput, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

mod platform;

fn load_texture(
    renderer: &Renderer,
    cache: &mut HashMap<String, TextureHandle>,
    name: &String,
    format: RendererTextureFormat,
) -> Result<TextureHandle, Box<dyn std::error::Error>> {
    rend3::span!(_guard, INFO, "Loading Texture", name = ?name);
    Ok(match cache.entry(name.clone()) {
        hash_map::Entry::Occupied(o) => *o.get(),
        hash_map::Entry::Vacant(v) => *v.insert({
            let real_name = concat!(env!("CARGO_MANIFEST_DIR"), "/data/").to_owned() + name;
            let file = std::fs::read(&real_name).unwrap_or_else(|_| panic!("Could not read object {}", real_name));

            let transcoder = basis::Transcoder::new();
            let image_info = transcoder.get_image_level_info(&file, 0, 0).ok_or("can't transcode missing image")?;

            let basis_format = match format {
                RendererTextureFormat::Bc4Linear => basis::TargetTextureFormat::Bc4R,
                RendererTextureFormat::Bc5Normal => basis::TargetTextureFormat::Bc5Rg,
                RendererTextureFormat::Bc7Srgb => basis::TargetTextureFormat::Bc7Rgba,
                _ => unreachable!(),
            };

            let mut prepared = transcoder.prepare_transcoding(&file).ok_or("couldn't prepare transcoding")?;
            let image = prepared.transcode_image_level(0, 0, basis_format)?;
            drop(prepared);

            renderer.add_texture_2d(Texture {
                format,
                width: image_info.width,
                height: image_info.height,
                data: image,
                label: Some(name.clone()),
            })
        }),
    })
}

fn load_skybox(renderer: &Renderer) -> Result<(), Box<dyn std::error::Error>> {
    let name = concat!(env!("CARGO_MANIFEST_DIR"), "/data/skybox.basis");
    let file = std::fs::read(name).unwrap_or_else(|_| panic!("Could not read skybox {}", name));

    let transcoder = basis::Transcoder::new();
    let image_info = transcoder.get_image_level_info(&file, 0, 0).ok_or("skybox image missing")?;

    let mut prepared = transcoder.prepare_transcoding(&file).ok_or("could not prepare skybox transcoding")?;
    let mut image = Vec::with_capacity(image_info.total_blocks as usize * 16 * 6);
    for i in 0..6 {
        image.extend_from_slice(
            &prepared
                .transcode_image_level(i, 0, basis::TargetTextureFormat::Bc7Rgba)?,
        );
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

fn load_obj(renderer: &Renderer, file: &str) -> Result<(MeshHandle, MaterialHandle), Box<dyn std::error::Error>> {
    rend3::span!(obj_guard, INFO, "Loading Obj");

    let mut object = Obj::load(file)?;
    object.load_mtls()?;

    drop(obj_guard);

    let mut textures = HashMap::new();
    let mut material_index_map = HashMap::new();
    let mut materials = Vec::new();

    for lib in &object.data.material_libs {
        // These names are an artifact of the Obj spec, which came before PBR.
        for obj::Material {
            name,
            map_kd,
            map_bump,
            map_ns,
            map_d,
            ..
        } in lib.materials.iter().map(|m| m.as_ref())
        {
            material_index_map.insert(name.clone(), materials.len() as u32);

            let handle = renderer.add_material(Material {
                albedo: match map_kd {
                    None => AlbedoComponent::Vertex { srgb: false },
                    Some(name) => AlbedoComponent::Texture(load_texture(
                        renderer,
                        &mut textures,
                        name,
                        RendererTextureFormat::Bc7Srgb,
                    )?),
                },
                // `MaterialComponent` is a superset of `Option<TextureHandle>`. It adds a `Value`
                // variant which represents a kind of "flat plane" texture where each texel in the
                // is the same. This behavio does not make sense for normals, however, so it uses
                // `Option<TextureHandle>` instead.
                normal: map_bump
                    .as_ref()
                    .map(|name| load_texture(renderer, &mut textures, name, RendererTextureFormat::Bc5Normal))
                    .transpose()?,
                roughness: match map_ns {
                    None => MaterialComponent::None,
                    Some(name) => MaterialComponent::Texture(load_texture(
                        renderer,
                        &mut textures,
                        name,
                        RendererTextureFormat::Bc4Linear,
                    )?),
                },
                ambient_occlusion: match map_d {
                    None => MaterialComponent::None,
                    Some(name) => MaterialComponent::Texture(load_texture(
                        renderer,
                        &mut textures,
                        name,
                        RendererTextureFormat::Bc4Linear,
                    )?),
                },
                ..Material::default()
            });

            materials.push(handle);
        }
    }

    assert!(
        materials.len() <= 1,
        "More than one material per obj is currently unsupported"
    );

    rend3::span!(_guard, INFO, "Converting Mesh");

    let mut mesh = Mesh {
        vertices: vec![],
        indices: vec![],
        material_count: materials.len() as u32,
    };

    let mut translation: HashMap<IndexTuple, u32> = HashMap::new();
    for group in &object.data.objects[0].groups {
        for polygon in &group.polys {
            for &index in &polygon.0 {
                let this_vert = translation.len() as u32;
                let vert_idx = *translation.entry(index).or_insert_with(|| {
                    let IndexTuple(position_idx, texture_idx, normal_idx) = index;

                    let position = object.data.position[position_idx];
                    let normal = object.data.normal[normal_idx.unwrap()];
                    let texture_coords = object.data.texture[texture_idx.unwrap()];

                    mesh.vertices.push(ModelVertex {
                        position: Vec3::from(position),
                        normal: Vec3::from(normal),
                        uv: Vec2::from(texture_coords),
                        color: [255; 4],
                    });

                    this_vert
                });

                mesh.indices.push(vert_idx);
            }
        }
    }

    let mesh_handle = renderer.add_mesh(mesh);

    Ok((mesh_handle, materials.into_iter().next().ok_or("no materials")?))
}

fn single(renderer: &Renderer, mesh: MeshHandle, material: MaterialHandle) {
    renderer.add_object(Object {
        mesh,
        material,
        transform: AffineTransform {
            transform: Mat4::from_scale_rotation_translation(
                Vec3::new(1.0, 1.0, 1.0),
                Quat::identity(),
                Vec3::new(0.0, -10.0, 10.0),
            ),
        },
    });
}

fn distribute(renderer: &Renderer, mesh: MeshHandle, material: MaterialHandle) {
    for x in (-11..=11).step_by(4) {
        for y in (-11..=11).step_by(4) {
            for z in (0..=50).step_by(25) {
                renderer.add_object(Object {
                    mesh,
                    material,
                    transform: AffineTransform {
                        transform: Mat4::from_translation(Vec3::new(x as f32, y as f32, z as f32)),
                    },
                });
            }
        }
    }
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
    let desired_device: Option<String> = args
        .value_from_str(["-d", "--device"])
        .ok()
        .map(|s: String| s.to_lowercase());
    let desired_mode = args.value_from_fn(["-m", "--mode"], extract_mode).ok();

    rend3::span_transfer!(_ -> main_thread_span, INFO, "Main Thread Setup");
    rend3::span_transfer!(_ -> event_loop_span, INFO, "Building Event Loop");

    let event_loop = EventLoop::new();

    rend3::span_transfer!(event_loop_span -> window_span, INFO, "Building Window");

    let window = {
        let mut builder = WindowBuilder::new();
        builder = builder.with_title("scene-viewer");
        builder.build(&event_loop).expect("Could not build window")
    };

    rend3::span_transfer!(window_span -> imgui_span, INFO, "Building imgui");

    let mut imgui = imgui::Context::create();
    let mut platform = imgui_winit_support::WinitPlatform::init(&mut imgui);
    platform.attach_window(imgui.io_mut(), &window, imgui_winit_support::HiDpiMode::Default);
    imgui.set_ini_filename(None);
    imgui.fonts().add_font(&[FontSource::DefaultFontData {
        config: Some(imgui::FontConfig {
            oversample_h: 3,
            oversample_v: 1,
            pixel_snap_h: true,
            size_pixels: 13.0,
            ..imgui::FontConfig::default()
        }),
    }]);

    rend3::span_transfer!(imgui_span -> switchyard_span, INFO, "Building Switchyard");

    let yard = Arc::new(
        Switchyard::new(
            2,
            // threads::single_pool_single_thread(Some("scene-viewer".into()), None),
            threads::double_pool_one_to_one(threads::thread_info(), Some("scene-viewer")),
            || (),
        )
        .unwrap(),
    );

    rend3::span_transfer!(switchyard_span -> renderer_span, INFO, "Building Renderer");

    let mut options = RendererOptions {
        vsync: VSyncMode::Off,
        size: window.inner_size(),
    };

    let renderer = futures::executor::block_on(rend3::Renderer::new(
        &window,
        Arc::clone(&yard),
        &mut imgui,
        desired_backend,
        desired_device,
        desired_mode,
        options.clone(),
    ))
    .unwrap();

    let pipelines = futures::executor::block_on(async {
        let shaders = DefaultShaders::new(&renderer).await;
        DefaultPipelines::new(&renderer, &shaders).await
    });

    rend3::span_transfer!(renderer_span -> loading_span, INFO, "Loading resources");

    let cube = load_obj(&renderer, concat!(env!("CARGO_MANIFEST_DIR"), "/data/cube.obj")).unwrap();
    single(&renderer, cube.0, cube.1);
    let suzanne = load_obj(&renderer, concat!(env!("CARGO_MANIFEST_DIR"), "/data/suzanne.obj")).unwrap();
    distribute(&renderer, suzanne.0, suzanne.1);
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
    let mut frames = 0_usize;

    event_loop.run(move |event, _window_target, control| match event {
        Event::MainEventsCleared => {
            frames += 1;
            let now = Instant::now();
            let elapsed_since_second = now - timestamp_last_second;
            if elapsed_since_second > Duration::from_secs(1) {
                println!(
                    "{} frames over {:.3}s: {:.3}ms/frame",
                    frames,
                    elapsed_since_second.as_secs_f32(),
                    elapsed_since_second.as_secs_f64() * 1000.0 / frames as f64
                );
                timestamp_last_second = now;
                frames = 0;
            }
            let delta_time = now - timestamp_last_frame;
            timestamp_last_frame = now;

            let forward = {
                let CameraLocation { yaw, pitch, .. } = camera_location;
                Vec3A::new(yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos())
            };
            let up = Vec3A::unit_y();
            let side: Vec3A = forward.cross(up).normalize().into();
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
            options.size = size;
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
            let handle = renderer.render(rend3::list::default_render_list(
                renderer.mode(),
                PhysicalSize {
                    width: (options.size.width as f32 * 1.0) as u32,
                    height: (options.size.height as f32 * 1.0) as u32,
                },
                &pipelines,
            ));

            rend3::span_transfer!(redraw_span -> render_wait_span, INFO, "Waiting for render");
            futures::executor::block_on(handle);
        }
        _ => {}
    })
}
