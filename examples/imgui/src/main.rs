use std::{sync::Arc, time::Instant};

fn vertex(pos: [f32; 3]) -> glam::Vec3 {
    glam::Vec3::from(pos)
}

fn create_mesh() -> rend3::datatypes::Mesh {
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

    rend3::datatypes::MeshBuilder::new(vertex_positions.to_vec())
        .with_indices(index_data.to_vec())
        .build()
}

fn main() {
    // Setup logging
    wgpu_subscriber::initialize_default_subscriber(None);

    // Create event loop and window
    let event_loop = winit::event_loop::EventLoop::new();
    let window = {
        let mut builder = winit::window::WindowBuilder::new();
        builder = builder.with_title("rend3 cube");
        builder.build(&event_loop).expect("Could not build window")
    };

    let window_size = window.inner_size();

    let mut options = rend3::RendererOptions {
        vsync: rend3::VSyncMode::Off,
        size: [window_size.width, window_size.height],
    };

    // We want to control the swapchain, so we don't hand rend3 a window, we hand it an image later.
    let renderer = pollster::block_on(rend3::RendererBuilder::new(options.clone()).build()).unwrap();

    // Create our surface and swapchain
    let surface = unsafe { renderer.instance().create_surface(&window) };
    let mut swapchain = renderer.device().create_swap_chain(
        &surface,
        &wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
            format: rend3::SWAPCHAIN_FORMAT,
            width: window_size.width,
            height: window_size.height,
            present_mode: wgpu::PresentMode::Mailbox,
        },
    );

    // Create imgui context and renderer
    let mut imgui = imgui::Context::create();
    let mut imgui_platform = imgui_winit_support::WinitPlatform::init(&mut imgui);
    imgui_platform.attach_window(imgui.io_mut(), &window, imgui_winit_support::HiDpiMode::Default);
    imgui.set_ini_filename(None);
    imgui.fonts().add_font(&[imgui::FontSource::DefaultFontData {
        config: Some(imgui::FontConfig {
            oversample_h: 3,
            oversample_v: 1,
            pixel_snap_h: true,
            size_pixels: 13.0,
            ..imgui::FontConfig::default()
        }),
    }]);

    let imgui_renderer_config = imgui_wgpu::RendererConfig {
        texture_format: rend3::SWAPCHAIN_FORMAT,
        // We need to use new_srgb because we write to a non-srgb buffer, so conversion must take place
        // in the shaders. This is to work around an bug on AMD gpus.
        ..imgui_wgpu::RendererConfig::new_srgb()
    };
    let mut imgui_renderer =
        imgui_wgpu::Renderer::new(&mut imgui, &renderer.device(), &renderer.queue(), imgui_renderer_config);

    // Create the default set of shaders and pipelines
    let pipelines = pollster::block_on(async {
        let shaders = rend3_list::DefaultShaders::new(&renderer).await;
        rend3_list::DefaultPipelines::new(&renderer, &shaders).await
    });

    // Create mesh and calculate smooth normals based on vertices
    let mesh = create_mesh();

    // Add mesh to renderer's world
    let mesh_handle = renderer.add_mesh(mesh);

    // Add basic material with all defaults except a single color.
    let material = rend3::datatypes::Material {
        albedo: rend3::datatypes::AlbedoComponent::Value(glam::Vec4::new(0.0, 0.5, 0.5, 1.0)),
        ..rend3::datatypes::Material::default()
    };
    let material_handle = renderer.add_material(material);

    // Combine the mesh and the material with a location to give an object.
    let object = rend3::datatypes::Object {
        mesh: mesh_handle,
        material: material_handle,
        transform: rend3::datatypes::AffineTransform {
            transform: glam::Mat4::identity(),
        },
    };
    let _object_handle = renderer.add_object(object);

    // Set camera's location
    renderer.set_camera_data(rend3::datatypes::Camera {
        projection: rend3::datatypes::CameraProjection::Projection {
            vfov: 60.0,
            near: 0.1,
            pitch: 0.5,
            yaw: -0.55,
        },
        location: glam::Vec3A::new(3.0, 3.0, -5.0),
    });

    // Create a single directional light
    renderer.add_directional_light(rend3::datatypes::DirectionalLight {
        color: glam::Vec3::one(),
        intensity: 10.0,
        // Direction will be normalized
        direction: glam::Vec3::new(-1.0, -4.0, 2.0),
    });

    let mut last_frame = Instant::now();
    let mut demo_open = true;
    let mut last_cursor = None;

    event_loop.run(move |event, _, control| {
        match event {
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
                options.size = [size.width, size.height];
                renderer.set_options(options.clone());

                // recreate swapcahin as we're responsible for it
                swapchain = renderer.device().create_swap_chain(
                    &surface,
                    &wgpu::SwapChainDescriptor {
                        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
                        format: rend3::SWAPCHAIN_FORMAT,
                        width: size.width,
                        height: size.height,
                        present_mode: wgpu::PresentMode::Mailbox,
                    },
                );
            }
            // Render!
            winit::event::Event::MainEventsCleared => {
                // Update delta time
                let delta = {
                    let now = Instant::now();
                    let delta = now - last_frame;
                    last_frame = now;

                    delta
                };

                imgui.io_mut().update_delta_time(delta);

                // Size of the internal buffers used for rendering.
                //
                // This can be different from the size of the swapchain,
                // it will be scaled to the swapchain size when being
                // rendered onto the swapchain.
                let internal_renderbuffer_size = options.size;

                // Default set of rendering commands using the default shaders.
                let render_list =
                    rend3_list::default_render_list(renderer.mode(), internal_renderbuffer_size, &pipelines);

                // Get our swapchain image
                let image = Arc::new(swapchain.get_current_frame().unwrap());

                // Dispatch a render!
                let handle = renderer.render(
                    render_list,
                    rend3::RendererOutput::ExternalSwapchain(Arc::clone(&image)),
                );

                // While we wait, prepare imgui
                imgui_platform
                    .prepare_frame(imgui.io_mut(), &window)
                    .expect("Failed to prepare frame");
                let imgui_ui = imgui.frame();
                imgui_ui.show_demo_window(&mut demo_open);

                if last_cursor != Some(imgui_ui.mouse_cursor()) {
                    last_cursor = Some(imgui_ui.mouse_cursor());
                    imgui_platform.prepare_render(&imgui_ui, &window);
                }

                // Wait until it's done
                pollster::block_on(handle);

                // Render imgui onto the screen after rend3 is done
                let mut encoder = renderer
                    .device()
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("imgui renderer"),
                    });

                // Create a renderpass to render imgui onto
                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                        attachment: &image.output.view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            // We load the existing value as to not overwrite existing content on screen
                            load: wgpu::LoadOp::Load,
                            store: true,
                        },
                    }],
                    depth_stencil_attachment: None,
                });

                imgui_renderer
                    .render(imgui_ui.render(), &renderer.queue(), &renderer.device(), &mut rpass)
                    .expect("Rendering failed");

                drop(rpass);

                renderer.queue().submit(Some(encoder.finish()));
            }
            // Other events we don't care about
            _ => {}
        }
        imgui_platform.handle_event(imgui.io_mut(), &window, &event);
    });
}
