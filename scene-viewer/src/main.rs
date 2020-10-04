use fnv::FnvBuildHasher;
use glam::{Mat4, Vec2, Vec3};
use imgui::FontSource;
use obj::{IndexTuple, Obj, ObjMaterial};
use rend3::{
    datatypes::{
        AffineTransform, AlbedoComponent, CameraLocation, Material, MaterialComponent, Mesh, ModelVertex, Object,
        RendererTextureFormat, Texture, TextureHandle,
    },
    Renderer, RendererOptions, VSyncMode,
};
use smallvec::SmallVec;
use std::{
    collections::HashMap,
    hash::BuildHasher,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use switchyard::{threads, Switchyard};
use winit::{
    event::{DeviceEvent, ElementState, Event, KeyboardInput, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

mod platform;

fn load_texture(
    renderer: &Renderer,
    cache: &mut HashMap<String, TextureHandle>,
    texture: &Option<String>,
) -> Option<TextureHandle> {
    rend3::span!(_guard, INFO, "Loading Texture", name = ?texture);
    if let Some(name) = texture {
        if let Some(handle) = cache.get(name) {
            Some(*handle)
        } else {
            let img = image::open(name).unwrap();
            let rgba = img.into_rgba();
            let handle = renderer.add_texture_2d(Texture {
                format: RendererTextureFormat::Rgba8Srgb,
                width: rgba.width(),
                height: rgba.height(),
                data: rgba.into_vec(),
                label: Some(name.clone()),
            });

            cache.insert(name.clone(), handle);

            Some(handle)
        }
    } else {
        None
    }
}

fn load_skybox(renderer: &Renderer) {
    let original = image::open("tmp/skybox/right.jpg").unwrap().into_rgba();
    let width = original.width();
    let height = original.height();

    let mut data = original.into_raw();
    data.extend_from_slice(&image::open("tmp/skybox/left.jpg").unwrap().into_rgba());
    data.extend_from_slice(&image::open("tmp/skybox/top.jpg").unwrap().into_rgba());
    data.extend_from_slice(&image::open("tmp/skybox/bottom.jpg").unwrap().into_rgba());
    data.extend_from_slice(&image::open("tmp/skybox/front.jpg").unwrap().into_rgba());
    data.extend_from_slice(&image::open("tmp/skybox/back.jpg").unwrap().into_rgba());

    let handle = renderer.add_texture_cube(Texture {
        data,
        format: RendererTextureFormat::Rgba8Srgb,
        width,
        height,
        label: Some("background".into()),
    });
    renderer.set_background_texture(handle);
}

fn load_resources(renderer: &Renderer) {
    rend3::span!(obj_guard, INFO, "Loading Obj");

    let mut object = Obj::load("tmp/suzanne.obj").unwrap();
    object.load_mtls().unwrap();

    drop(obj_guard);

    let mut textures = HashMap::new();
    let mut material_index_map = HashMap::new();
    let mut materials = SmallVec::new();

    for lib in object.data.material_libs {
        for material in lib.materials {
            let albedo = &material.map_kd;
            let normal = &material.map_bump;
            let roughness = &material.map_ns;

            let albedo_handle = load_texture(renderer, &mut textures, albedo);
            let normal_handle = load_texture(renderer, &mut textures, normal);
            let roughness_handle = load_texture(renderer, &mut textures, roughness);

            material_index_map.insert(material.name.clone(), materials.len() as u32);

            let handle = renderer.add_material(Material {
                albedo: match albedo_handle {
                    None => AlbedoComponent::Vertex { srgb: false },
                    Some(handle) => AlbedoComponent::Texture(handle),
                },
                normal: normal_handle,
                roughness: match roughness_handle {
                    None => MaterialComponent::None,
                    Some(handle) => MaterialComponent::Texture(handle),
                },
                ..Material::default()
            });

            materials.push(handle);
        }
    }

    rend3::span!(_guard, INFO, "Converting Mesh");

    let mut mesh = Mesh {
        vertices: vec![],
        indices: vec![],
        material_count: materials.len() as u32,
    };

    let mut translation: HashMap<(usize, Option<usize>, Option<usize>), u32> = HashMap::new();
    let mut vert_count = 0_u32;
    for group in &object.data.objects[0].groups {
        let material_name = if let ObjMaterial::Mtl(mtl) = group.material.as_ref().unwrap() {
            &mtl.name
        } else {
            unreachable!()
        };
        for polygon in &group.polys {
            for &IndexTuple(position_idx, texture_idx, normal_idx) in &polygon.0 {
                let vert_idx = if let Some(&vert_idx) = translation.get(&(position_idx, normal_idx, texture_idx)) {
                    vert_idx
                } else {
                    let this_vert = vert_count;
                    vert_count += 1;

                    let position = object.data.position[position_idx];
                    let normal = object.data.normal[normal_idx.unwrap()];
                    let texture_coords = object.data.texture[texture_idx.unwrap()];

                    mesh.vertices.push(ModelVertex {
                        position: Vec3::from(position),
                        normal: Vec3::from(normal),
                        uv: Vec2::from(texture_coords),
                        color: [255; 4],
                        material: *material_index_map.get(material_name).unwrap(),
                    });

                    translation.insert((position_idx, texture_idx, normal_idx), this_vert);

                    this_vert
                };

                mesh.indices.push(vert_idx);
            }
        }
    }

    let mesh_handle = renderer.add_mesh(mesh.clone());

    for x in (-11..=11).step_by(4) {
        for y in (-11..=11).step_by(4) {
            for z in (0..=100).step_by(10) {
                renderer.add_object(Object {
                    mesh: mesh_handle,
                    materials: materials.clone(),
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

fn main() {
    wgpu_subscriber::initialize_default_subscriber(Some(Path::new("target/profile.json")));

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
            threads::single_pool_single_thread(Some("scene-viewer".into()), None),
            // threads::double_pool_two_to_one(threads::thread_info(), Some("scene-viewer")),
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
        options.clone(),
    ))
    .unwrap();

    rend3::span_transfer!(renderer_span -> loading_span, INFO, "Loading resources");

    load_resources(&renderer);
    load_skybox(&renderer);

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

            let velocity = if button_pressed(&scancode_status, platform::Scancodes::SHIFT) {
                10.0
            } else {
                1.0
            };
            if button_pressed(&scancode_status, platform::Scancodes::W) {
                *camera_location.location.z_mut() += velocity * delta_time.as_secs_f32();
            }
            if button_pressed(&scancode_status, platform::Scancodes::S) {
                *camera_location.location.z_mut() -= velocity * delta_time.as_secs_f32();
            }
            if button_pressed(&scancode_status, platform::Scancodes::A) {
                *camera_location.location.x_mut() -= velocity * delta_time.as_secs_f32();
            }
            if button_pressed(&scancode_status, platform::Scancodes::D) {
                *camera_location.location.x_mut() += velocity * delta_time.as_secs_f32();
            }
            if button_pressed(&scancode_status, platform::Scancodes::Q) {
                *camera_location.location.y_mut() += velocity * delta_time.as_secs_f32();
            }
            if button_pressed(&scancode_status, platform::Scancodes::Z) {
                *camera_location.location.y_mut() -= velocity * delta_time.as_secs_f32();
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
            let handle = renderer.render();

            rend3::span_transfer!(redraw_span -> render_wait_span, INFO, "Waiting for render");
            futures::executor::block_on(handle);
        }
        _ => {}
    })
}
