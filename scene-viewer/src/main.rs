use imgui::FontSource;
use rend3::{RendererOptions, VSyncMode};
use std::sync::Arc;
use switchyard::{threads, Switchyard};
use winit::{event::Event, event_loop::EventLoop, window::WindowBuilder};

fn main() {
    let event_loop = EventLoop::new();

    let window = {
        let mut builder = WindowBuilder::new();
        builder = builder.with_title("BVE-Reborn");
        builder.build(&event_loop).expect("Could not build window")
    };

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

    let yard = Arc::new(
        Switchyard::new(
            2,
            threads::double_pool_two_to_one(threads::thread_info(), Some("scene-viewer")),
            || rend3::TLS::new().unwrap(),
        )
        .unwrap(),
    );

    let mut options = RendererOptions {
        vsync: VSyncMode::On,
        size: window.inner_size(),
    };

    futures::executor::block_on(rend3::Renderer::new(
        &window,
        Arc::clone(&yard),
        &mut imgui,
        options.clone(),
    ));

    event_loop.run(move |event, window, control| match event {
        Event::MainEventsCleared => {}
        Event::RedrawRequested(_) => {}
        _ => {}
    })
}
