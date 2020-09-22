use imgui::FontSource;
use rend3::{RendererOptions, VSyncMode};
use std::{path::Path, sync::Arc};
use switchyard::{threads, Switchyard};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

fn main() {
    wgpu_subscriber::initialize_default_subscriber(Some(Path::new("target/profile.json")));

    rend3::span!(main_thread_guard, INFO, "Main Thread Setup");

    let event_loop = EventLoop::new();

    let window = {
        rend3::span!(_guard, INFO, "Building Window");

        let mut builder = WindowBuilder::new();
        builder = builder.with_title("scene-viewer");
        builder.build(&event_loop).expect("Could not build window")
    };

    rend3::span!(imgui_guard, INFO, "Building Imgui");

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

    drop(imgui_guard);
    rend3::span!(switchyard_guard, INFO, "Building Switchyard");

    let yard = Arc::new(
        Switchyard::new(
            2,
            threads::double_pool_two_to_one(threads::thread_info(), Some("scene-viewer")),
            || rend3::TLS::new().unwrap(),
        )
        .unwrap(),
    );

    drop(switchyard_guard);
    rend3::span!(renderer_guard, INFO, "Building Renderer");

    let mut options = RendererOptions {
        vsync: VSyncMode::On,
        size: window.inner_size(),
    };

    let renderer = futures::executor::block_on(rend3::Renderer::new(
        &window,
        Arc::clone(&yard),
        &mut imgui,
        options.clone(),
    ))
    .unwrap();
    drop(renderer_guard);
    drop(main_thread_guard);

    let mut handle = None;

    event_loop.run(move |event, window_target, control| match event {
        Event::MainEventsCleared => {
            if let Some(handle) = handle.take() {
                rend3::span!(_guard, INFO, "Waiting for render");
                futures::executor::block_on(handle);
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
            rend3::span!(_guard, INFO, "Redraw");
            renderer.set_options(options.clone());
            handle = Some(yard.spawn(0, 1, renderer.render()))
        }
        _ => {}
    })
}
