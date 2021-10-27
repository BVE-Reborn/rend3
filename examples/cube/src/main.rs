use std::sync::Arc;

fn vertex(pos: [f32; 3]) -> glam::Vec3 {
    glam::Vec3::from(pos)
}

fn create_mesh() -> rend3::types::Mesh {
    let vertex_positions = [
        // far side (0.0, 0.0, 1.0)
        vertex([-1.0, -1.0, 1.0]),
        vertex([1.0, -1.0, 1.0]),
        vertex([1.0, 1.0, 1.0]),
        vertex([-1.0, 1.0, 1.0]),
        // near side (0.0, 0.0, -1.0)
        vertex([-1.0, 1.0, -1.0]),
        vertex([1.0, 1.0, -1.0]),
        vertex([1.0, -1.0, -1.0]),
        vertex([-1.0, -1.0, -1.0]),
        // right side (1.0, 0.0, 0.0)
        vertex([1.0, -1.0, -1.0]),
        vertex([1.0, 1.0, -1.0]),
        vertex([1.0, 1.0, 1.0]),
        vertex([1.0, -1.0, 1.0]),
        // left side (-1.0, 0.0, 0.0)
        vertex([-1.0, -1.0, 1.0]),
        vertex([-1.0, 1.0, 1.0]),
        vertex([-1.0, 1.0, -1.0]),
        vertex([-1.0, -1.0, -1.0]),
        // top (0.0, 1.0, 0.0)
        vertex([1.0, 1.0, -1.0]),
        vertex([-1.0, 1.0, -1.0]),
        vertex([-1.0, 1.0, 1.0]),
        vertex([1.0, 1.0, 1.0]),
        // bottom (0.0, -1.0, 0.0)
        vertex([1.0, -1.0, 1.0]),
        vertex([-1.0, -1.0, 1.0]),
        vertex([-1.0, -1.0, -1.0]),
        vertex([1.0, -1.0, -1.0]),
    ];

    let index_data: &[u32] = &[
        0, 1, 2, 2, 3, 0, // far
        4, 5, 6, 6, 7, 4, // near
        8, 9, 10, 10, 11, 8, // right
        12, 13, 14, 14, 15, 12, // left
        16, 17, 18, 18, 19, 16, // top
        20, 21, 22, 22, 23, 20, // bottom
    ];

    rend3::types::MeshBuilder::new(vertex_positions.to_vec())
        .with_indices(index_data.to_vec())
        .build()
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    pollster::block_on(async_main());
}
#[cfg(target_arch = "wasm32")]
fn main() {
    wasm_bindgen_futures::spawn_local(async_main());
}

async fn async_main() {
    #[cfg(target_arch = "wasm32")]
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    #[cfg(target_arch = "wasm32")]
    console_log::init().unwrap();

    // Setup logging
    #[cfg(not(target_arch = "wasm32"))]
    env_logger::init();

    // Create event loop and window
    let event_loop = winit::event_loop::EventLoop::new();
    let window = {
        let mut builder = winit::window::WindowBuilder::new();
        builder = builder.with_title("rend3 cube");
        builder.build(&event_loop).expect("Could not build window")
    };

    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::WindowExtWebSys;
        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| doc.body())
            .and_then(|body| body.append_child(&web_sys::Element::from(window.canvas())).ok())
            .expect("couldn't append canvas to document body");
    }

    let window_size = window.inner_size();

    // Create the Instance, Adapter, and Device. We can specify preferred backend, device name, or rendering mode. In this case we let rend3 choose for us.
    let iad = rend3::create_iad(None, None, Some(rend3::RendererMode::CPUPowered))
        .await
        .unwrap();

    // The one line of unsafe needed. We just need to guarentee that the window outlives the use of the surface.
    let surface = Arc::new(unsafe { iad.instance.create_surface(&window) });
    // Get the preferred format for the surface.
    let format = surface.get_preferred_format(&iad.adapter).unwrap();
    // Configure the surface to be ready for rendering.
    rend3::configure_surface(
        &surface,
        &iad.device,
        format,
        glam::UVec2::new(window_size.width, window_size.height),
        rend3::types::PresentMode::Mailbox,
    );

    // Make us a renderer.
    let renderer = rend3::Renderer::new(iad, Some(window_size.width as f32 / window_size.height as f32)).unwrap();

    // Create the pbr pipeline with the same internal resolution and 4x multisampling
    let render_texture_options = rend3_pbr::RenderTextureOptions {
        resolution: glam::UVec2::new(window_size.width, window_size.height),
        samples: rend3_pbr::SampleCount::One,
    };
    let mut pbr_routine = rend3_pbr::PbrRenderRoutine::new(&renderer, render_texture_options);
    let mut tonemapping_routine =
        rend3_pbr::TonemappingRoutine::new(&renderer, render_texture_options.resolution, format);

    // Create mesh and calculate smooth normals based on vertices
    let mesh = create_mesh();

    // Add mesh to renderer's world.
    //
    // All handles are refcounted, so we only need to hang onto the handle until we make an object.
    let mesh_handle = renderer.add_mesh(mesh);

    // Add PBR material with all defaults except a single color.
    let material = rend3_pbr::material::PbrMaterial {
        albedo: rend3_pbr::material::AlbedoComponent::Value(glam::Vec4::new(0.0, 0.5, 0.5, 1.0)),
        ..rend3_pbr::material::PbrMaterial::default()
    };
    let material_handle = renderer.add_material(material);

    // Combine the mesh and the material with a location to give an object.
    let object = rend3::types::Object {
        mesh: mesh_handle,
        material: material_handle,
        transform: glam::Mat4::IDENTITY,
    };
    // Creating an object will hold onto both the mesh and the material
    // even if they are deleted.
    //
    // We need to keep the object handle alive.
    let _object_handle = renderer.add_object(object);

    let view_location = glam::Vec3::new(3.0, 3.0, -5.0);
    let view = glam::Mat4::from_euler(glam::EulerRot::XYZ, -0.55, 0.5, 0.0);
    let view = view * glam::Mat4::from_translation(-view_location);

    // Set camera's location
    renderer.set_camera_data(rend3::types::Camera {
        projection: rend3::types::CameraProjection::Perspective { vfov: 60.0, near: 0.1 },
        view,
    });

    // Create a single directional light
    //
    // We need to keep the directional light handle alive.
    let _directional_handle = renderer.add_directional_light(rend3::types::DirectionalLight {
        color: glam::Vec3::ONE,
        intensity: 10.0,
        // Direction will be normalized
        direction: glam::Vec3::new(-1.0, -4.0, 2.0),
        distance: 400.0,
    });

    winit_run(event_loop, move |event, _, control| match event {
        // Close button was clicked, we should close.
        winit::event::Event::WindowEvent {
            event: winit::event::WindowEvent::CloseRequested,
            ..
        } => {
            *control = winit::event_loop::ControlFlow::Exit;
        }
        // Window was resized, need to resize renderer.
        winit::event::Event::WindowEvent {
            event: winit::event::WindowEvent::Resized(size),
            ..
        } => {
            let size = glam::UVec2::new(size.width, size.height);
            // Reconfigure the surface for the new size.
            rend3::configure_surface(
                &surface,
                &renderer.device,
                format,
                glam::UVec2::new(size.x, size.y),
                rend3::types::PresentMode::Mailbox,
            );
            // Tell the renderer about the new aspect ratio.
            renderer.set_aspect_ratio(size.x as f32 / size.y as f32);
            // Resize the internal buffers to the same size as the screen.
            pbr_routine.resize(
                &renderer,
                rend3_pbr::RenderTextureOptions {
                    resolution: size,
                    samples: rend3_pbr::SampleCount::One,
                },
            );
            tonemapping_routine.resize(size);
        }
        // Render!
        winit::event::Event::MainEventsCleared => {
            // Get a frame
            let frame = rend3::util::output::OutputFrame::Surface {
                surface: Arc::clone(&surface),
            };
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
            pbr_routine.add_forward_to_graph(&mut graph);

            // Tonemap onto the output.
            tonemapping_routine.add_to_graph(&mut graph);

            // Dispatch a render using the built up rendergraph!
            graph.execute(&renderer, frame, cmd_bufs, &ready);
        }
        // Other events we don't care about
        _ => {}
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn winit_run<F, T>(event_loop: winit::event_loop::EventLoop<T>, event_handler: F) -> !
where
    F: 'static
        + FnMut(
            winit::event::Event<'_, T>,
            &winit::event_loop::EventLoopWindowTarget<T>,
            &mut winit::event_loop::ControlFlow,
        ),
{
    event_loop.run(event_handler)
}

#[cfg(target_arch = "wasm32")]
pub fn winit_run<F, T>(event_loop: winit::event_loop::EventLoop<T>, event_handler: F)
where
    F: 'static
        + FnMut(
            winit::event::Event<'_, T>,
            &winit::event_loop::EventLoopWindowTarget<T>,
            &mut winit::event_loop::ControlFlow,
        ),
{
    use wasm_bindgen::{prelude::*, JsCast};

    let winit_closure = Closure::once_into_js(move || event_loop.run(event_handler));

    // make sure to handle JS exceptions thrown inside start.
    // Otherwise wasm_bindgen_futures Queue would break and never handle any tasks again.
    // This is required, because winit uses JS exception for control flow to escape from `run`.
    if let Err(error) = call_catch(&winit_closure) {
        let is_control_flow_exception = error
            .dyn_ref::<js_sys::Error>()
            .map_or(false, |e| e.message().includes("Using exceptions for control flow", 0));

        if !is_control_flow_exception {
            web_sys::console::error_1(&error);
        }
    }

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(catch, js_namespace = Function, js_name = "prototype.call.call")]
        fn call_catch(this: &JsValue) -> Result<(), JsValue>;
    }
}
