use glam::{Mat4, Vec2, Vec3};
use imgui::FontSource;
use obj::{IndexTuple, Obj, ObjMaterial};
use rend3::{
    datatypes::{AffineTransform, Material, Mesh, ModelVertex, Object, RendererTextureFormat, Texture, TextureHandle},
    Renderer, RendererOptions, VSyncMode,
};
use smallvec::SmallVec;
use std::time::{Duration, Instant};
use std::{collections::HashMap, path::Path, sync::Arc};
use switchyard::{threads, Switchyard};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

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
            let handle = renderer.add_texture(Texture {
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
                color: albedo_handle,
                normal: normal_handle,
                roughness: roughness_handle,
                specular: None,
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

    let mesh_handle = renderer.add_mesh(mesh);

    renderer.add_object(Object {
        mesh: mesh_handle,
        materials,
        transform: AffineTransform {
            transform: Mat4::identity(),
        },
    });
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
            threads::double_pool_two_to_one(threads::thread_info(), Some("scene-viewer")),
            || rend3::TLS::new().unwrap(),
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

    rend3::span_transfer!(loading_span -> _);
    rend3::span_transfer!(main_thread_span -> _);

    let mut handle = None;
    let mut timestamp = Instant::now();
    let mut frames = 0_usize;

    event_loop.run(move |event, _window_target, control| match event {
        Event::MainEventsCleared => {
            if let Some(handle) = handle.take() {
                rend3::span_transfer!(_ -> render_wait_span, INFO, "Waiting for render");
                futures::executor::block_on(handle);
            }

            frames += 1;
            let now = Instant::now();
            let elapsed = now - timestamp;
            if elapsed > Duration::from_secs(1) {
                println!(
                    "{} frames over {:.3}s: {:.3}ms/frame",
                    frames,
                    elapsed.as_secs_f32(),
                    elapsed.as_secs_f64() * 1000.0 / frames as f64
                );
                timestamp = now;
                frames = 0;
            }

            window.request_redraw();
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

            renderer.set_options(options.clone());
            handle = Some(renderer.render())
        }
        _ => {}
    })
}
