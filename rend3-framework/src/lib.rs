#![cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]

use std::{future::Future, pin::Pin, sync::Arc};

use glam::UVec2;
use rend3::{
    types::{Handedness, SampleCount, Surface, TextureFormat},
    InstanceAdapterDevice, Renderer, ShaderPreProcessor,
};
use rend3_routine::base::BaseRenderGraph;
use wgpu::{Instance, PresentMode, SurfaceError};
use winit::{
    error::EventLoopError,
    event::Event,
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopWindowTarget},
    window::{Window, WindowBuilder},
};

mod assets;
mod grab;

pub use assets::*;
pub use grab::*;
pub use parking_lot::{Mutex, MutexGuard};

pub struct WindowingSetup<'a, T: 'static = ()> {
    pub event_loop: &'a EventLoop<T>,
    pub window: &'a Window,
}

/// Context passed to the setup function. Contains
/// everything needed to setup examples
pub struct SetupContext<'a, T: 'static = ()> {
    pub windowing: Option<WindowingSetup<'a, T>>,
    pub renderer: &'a Arc<Renderer>,
    pub routines: &'a Arc<DefaultRoutines>,
    pub surface_format: rend3::types::TextureFormat,
    pub resolution: UVec2,
    pub scale_factor: f32,
}

/// Context passed to the event handler.
pub struct EventContext<'a, T: 'static = ()> {
    pub window: Option<&'a Window>,
    pub renderer: &'a Arc<Renderer>,
    pub routines: &'a Arc<DefaultRoutines>,
    pub base_rendergraph: &'a BaseRenderGraph,
    pub resolution: UVec2,
    pub control_flow: &'a mut dyn FnMut(winit::event_loop::ControlFlow),
    pub event_loop_window_target: &'a EventLoopWindowTarget<T>,
}

pub struct RedrawContext<'a, T: 'static = ()> {
    pub window: Option<&'a Window>,
    pub renderer: &'a Arc<Renderer>,
    pub routines: &'a Arc<DefaultRoutines>,
    pub base_rendergraph: &'a BaseRenderGraph,
    pub surface_texture: &'a wgpu::Texture,
    pub resolution: UVec2,
    pub control_flow: &'a mut dyn FnMut(winit::event_loop::ControlFlow),
    pub event_loop_window_target: Option<&'a EventLoopWindowTarget<T>>,
    pub delta_t_seconds: f32,
}

pub trait App<T: 'static = ()> {
    /// The handedness of the coordinate system of the renderer.
    const HANDEDNESS: Handedness;

    fn register_logger(&mut self) {
        #[cfg(target_arch = "wasm32")]
        console_log::init().unwrap();

        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        env_logger::builder()
            .filter_module("rend3", log::LevelFilter::Info)
            .parse_default_env()
            .init();
    }

    fn register_panic_hook(&mut self) {
        #[cfg(target_arch = "wasm32")]
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    }

    fn create_window(&mut self, builder: WindowBuilder) -> Result<(EventLoop<T>, Window), EventLoopError> {
        profiling::scope!("creating window");

        let event_loop = EventLoopBuilder::with_user_event().build()?;
        let window = builder.build(&event_loop).expect("Could not build window");

        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowExtWebSys;

            let canvas = window.canvas().unwrap();
            let style = canvas.style();
            style.set_property("width", "100%").unwrap();
            style.set_property("height", "100%").unwrap();

            web_sys::window()
                .and_then(|win| win.document())
                .and_then(|doc| doc.body())
                .and_then(|body| body.append_child(&canvas).ok())
                .expect("couldn't append canvas to document body");
        }

        Ok((event_loop, window))
    }

    fn create_iad<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<InstanceAdapterDevice, rend3::RendererInitializationError>> + 'a>> {
        Box::pin(async move { rend3::create_iad(None, None, None, None).await })
    }

    fn create_base_rendergraph(&mut self, renderer: &Arc<Renderer>, spp: &ShaderPreProcessor) -> BaseRenderGraph {
        BaseRenderGraph::new(renderer, spp)
    }

    /// Determines the sample count used, this may change dynamically. This
    /// function is what the framework actually calls, so overriding this
    /// will always use the right values.
    ///
    /// It is called on main events cleared and things are remade if this
    /// changes.
    fn sample_count(&self) -> SampleCount;

    fn present_mode(&self) -> rend3::types::PresentMode {
        rend3::types::PresentMode::Fifo
    }

    /// Determines the scale factor used
    fn scale_factor(&self) -> f32 {
        1.0
    }

    fn setup(&mut self, context: SetupContext<'_, T>) {
        let _ = context;
    }

    fn handle_event(&mut self, context: EventContext<'_, T>, event: Event<T>) {
        let _ = (context, event);
    }

    fn handle_redraw(&mut self, context: RedrawContext<'_, T>);
}

pub fn lock<T>(lock: &parking_lot::Mutex<T>) -> parking_lot::MutexGuard<'_, T> {
    #[cfg(target_arch = "wasm32")]
    let guard = lock.try_lock().expect("Could not lock mutex on single-threaded wasm. Do not hold locks open while an .await causes you to yield execution.");
    #[cfg(not(target_arch = "wasm32"))]
    let guard = lock.lock();

    guard
}

pub struct DefaultRoutines {
    pub pbr: Mutex<rend3_routine::pbr::PbrRoutine>,
    pub skybox: Mutex<rend3_routine::skybox::SkyboxRoutine>,
    pub tonemapping: Mutex<rend3_routine::tonemapping::TonemappingRoutine>,
}

pub async fn async_start<A: App<T> + 'static, T: 'static>(mut app: A, window_builder: WindowBuilder) {
    app.register_logger();
    app.register_panic_hook();

    // Create the window invisible until we are rendering
    let (event_loop, window) = app.create_window(window_builder.with_visible(false)).unwrap();
    let window_size = window.inner_size();

    let iad = app.create_iad().await.unwrap();

    // The one line of unsafe needed. We just need to guarentee that the window
    // outlives the use of the surface.
    //
    // Android has to defer the surface until `Resumed` is fired. This doesn't fire
    // on other platforms though :|
    let mut surface = if cfg!(target_os = "android") {
        None
    } else {
        Some(Arc::new(unsafe { iad.instance.create_surface(&window) }.unwrap()))
    };

    // Make us a renderer.
    let renderer = rend3::Renderer::new(
        iad.clone(),
        A::HANDEDNESS,
        Some(window_size.width as f32 / window_size.height as f32),
    )
    .unwrap();

    // Get the preferred format for the surface.
    //
    // Assume android supports Rgba8Srgb, as it has 100% device coverage
    let format = surface.as_ref().map_or(TextureFormat::Rgba8UnormSrgb, |s| {
        let caps = s.get_capabilities(&iad.adapter);
        let format = caps.formats[0];

        // Configure the surface to be ready for rendering.
        rend3::configure_surface(
            s,
            &iad.device,
            format,
            glam::UVec2::new(window_size.width, window_size.height),
            rend3::types::PresentMode::Fifo,
        );

        format
    });

    let mut spp = rend3::ShaderPreProcessor::new();
    rend3_routine::builtin_shaders(&mut spp);

    let base_rendergraph = app.create_base_rendergraph(&renderer, &spp);
    let mut data_core = renderer.data_core.lock();
    let routines = Arc::new(DefaultRoutines {
        pbr: Mutex::new(rend3_routine::pbr::PbrRoutine::new(
            &renderer,
            &mut data_core,
            &spp,
            &base_rendergraph.interfaces,
            &base_rendergraph.gpu_culler.culling_buffer_map_handle,
        )),
        skybox: Mutex::new(rend3_routine::skybox::SkyboxRoutine::new(
            &renderer,
            &spp,
            &base_rendergraph.interfaces,
        )),
        tonemapping: Mutex::new(rend3_routine::tonemapping::TonemappingRoutine::new(
            &renderer,
            &spp,
            &base_rendergraph.interfaces,
            format,
        )),
    });
    drop(data_core);

    app.setup(SetupContext {
        windowing: Some(WindowingSetup {
            event_loop: &event_loop,
            window: &window,
        }),
        renderer: &renderer,
        routines: &routines,
        surface_format: format,
        resolution: UVec2::new(window_size.width, window_size.height),
        scale_factor: window.scale_factor() as f32,
    });

    // We're ready, so lets make things visible
    window.set_visible(true);

    let mut suspended = cfg!(target_os = "android");
    let mut last_user_control_mode = ControlFlow::Wait;
    let mut stored_surface_info = StoredSurfaceInfo {
        size: glam::UVec2::new(window_size.width, window_size.height),
        scale_factor: app.scale_factor(),
        sample_count: app.sample_count(),
        present_mode: app.present_mode(),
        requires_reconfigure: true,
    };

    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            use winit::platform::web::EventLoopExtWebSys;
            let event_loop_function = EventLoop::spawn;
        } else {
            let event_loop_function = EventLoop::run;
        }
    }

    let mut previous_time = web_time::Instant::now();

    // On native this is a result, but on wasm it's a unit type.
    #[allow(clippy::let_unit_value)]
    let _ = (event_loop_function)(
        event_loop,
        move |event: Event<T>, event_loop_window_target: &EventLoopWindowTarget<T>| {
            let mut control_flow = event_loop_window_target.control_flow();
            if let Some(suspend) = handle_surface(
                &app,
                &window,
                &event,
                &iad.instance,
                &mut surface,
                &renderer,
                &mut stored_surface_info,
            ) {
                suspended = suspend;
            }

            // We move to Wait when we get suspended so we don't spin at 50k FPS.
            match event {
                Event::Suspended => {
                    control_flow = ControlFlow::Wait;
                }
                Event::Resumed => {
                    control_flow = last_user_control_mode;
                }
                _ => {}
            }

            // Close button was clicked, we should close.
            if let winit::event::Event::WindowEvent {
                event: winit::event::WindowEvent::CloseRequested,
                ..
            } = event
            {
                event_loop_window_target.exit();
                return;
            }

            // We need to block all updates
            if let Event::WindowEvent {
                window_id: _,
                event: winit::event::WindowEvent::RedrawRequested,
            } = event
            {
                if suspended {
                    return;
                }

                let Some(surface) = surface.as_ref() else {
                    return;
                };

                if stored_surface_info.requires_reconfigure {
                    rend3::configure_surface(
                        surface,
                        &renderer.device,
                        format,
                        stored_surface_info.size,
                        stored_surface_info.present_mode,
                    );
                    stored_surface_info.requires_reconfigure = false;
                }

                let surface_texture = match surface.get_current_texture() {
                    Ok(texture) => texture,
                    Err(SurfaceError::Outdated) => {
                        stored_surface_info.requires_reconfigure = true;
                        return;
                    }
                    Err(SurfaceError::Timeout) => {
                        return;
                    }
                    Err(SurfaceError::OutOfMemory | SurfaceError::Lost) => panic!("Surface OOM"),
                };

                let current_time = web_time::Instant::now();
                let delta_t_seconds = (current_time - previous_time).as_secs_f32();
                previous_time = current_time;

                app.handle_redraw(RedrawContext {
                    window: Some(&window),
                    renderer: &renderer,
                    routines: &routines,
                    base_rendergraph: &base_rendergraph,
                    surface_texture: &surface_texture.texture,
                    resolution: stored_surface_info.size,
                    control_flow: &mut |c: ControlFlow| {
                        control_flow = c;
                        last_user_control_mode = c;
                    },
                    event_loop_window_target: Some(event_loop_window_target),
                    delta_t_seconds,
                });

                surface_texture.present();

                window.request_redraw();
            } else {
                app.handle_event(
                    EventContext {
                        window: Some(&window),
                        renderer: &renderer,
                        routines: &routines,
                        base_rendergraph: &base_rendergraph,
                        resolution: stored_surface_info.size,
                        control_flow: &mut |c: ControlFlow| {
                            control_flow = c;
                            last_user_control_mode = c;
                        },
                        event_loop_window_target,
                    },
                    event,
                );
            }
        },
    );
}

struct StoredSurfaceInfo {
    size: UVec2,
    scale_factor: f32,
    sample_count: SampleCount,
    present_mode: PresentMode,
    requires_reconfigure: bool,
}

#[allow(clippy::too_many_arguments)]
fn handle_surface<A: App<T>, T: 'static>(
    app: &A,
    window: &Window,
    event: &Event<T>,
    instance: &Instance,
    surface: &mut Option<Arc<Surface>>,
    renderer: &Arc<Renderer>,
    surface_info: &mut StoredSurfaceInfo,
) -> Option<bool> {
    match *event {
        Event::Resumed => {
            if surface.is_none() {
                *surface = Some(Arc::new(unsafe { instance.create_surface(window) }.unwrap()));
            }
            Some(false)
        }
        Event::Suspended => {
            *surface = None;
            Some(true)
        }
        Event::WindowEvent {
            event: winit::event::WindowEvent::Resized(size),
            ..
        } => {
            log::debug!("resize {:?}", size);
            let size = UVec2::new(size.width, size.height);

            if size.x == 0 || size.y == 0 {
                return Some(false);
            }

            surface_info.size = size;
            surface_info.scale_factor = app.scale_factor();
            surface_info.sample_count = app.sample_count();
            surface_info.present_mode = app.present_mode();
            surface_info.requires_reconfigure = true;

            // Tell the renderer about the new aspect ratio.
            renderer.set_aspect_ratio(size.x as f32 / size.y as f32);
            Some(false)
        }
        _ => None,
    }
}

pub fn start<A: App<T> + 'static, T: 'static>(app: A, window_builder: WindowBuilder) {
    #[cfg(target_arch = "wasm32")]
    {
        wasm_bindgen_futures::spawn_local(async_start(app, window_builder));
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        pollster::block_on(async_start(app, window_builder));
    }
}
