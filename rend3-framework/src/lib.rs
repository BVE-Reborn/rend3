use std::{future::Future, pin::Pin, sync::Arc};

use glam::UVec2;
use rend3::{
    types::{Handedness, SampleCount, Surface, TextureFormat},
    InstanceAdapterDevice, Renderer,
};
use rend3_routine::base::BaseRenderGraph;
use wgpu::Instance;
use winit::{
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget},
    window::{Window, WindowBuilder, WindowId},
};

mod assets;
mod grab;
#[cfg(target_arch = "wasm32")]
mod resize_observer;

pub use assets::*;
pub use grab::*;

pub use parking_lot::{Mutex, MutexGuard};
pub type Event<'a, T> = winit::event::Event<'a, UserResizeEvent<T>>;

/// User event which the framework uses to resize on wasm.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum UserResizeEvent<T: 'static> {
    /// Used to fire off resizing on wasm
    Resize {
        window_id: WindowId,
        size: PhysicalSize<u32>,
    },
    /// Custom user event type
    Other(T),
}

pub trait App<T: 'static = ()> {
    /// The handedness of the coordinate system of the renderer.
    const HANDEDNESS: Handedness;

    fn register_logger(&mut self) {
        #[cfg(target_arch = "wasm32")]
        console_log::init().unwrap();

        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        env_logger::init();
    }

    fn register_panic_hook(&mut self) {
        #[cfg(target_arch = "wasm32")]
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    }

    fn create_window(&mut self, builder: WindowBuilder) -> (EventLoop<UserResizeEvent<T>>, Window) {
        profiling::scope!("creating window");

        let event_loop = EventLoop::with_user_event();
        let window = builder.build(&event_loop).expect("Could not build window");

        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowExtWebSys;

            let canvas = window.canvas();
            let style = canvas.style();
            style.set_property("width", "100%").unwrap();
            style.set_property("height", "100%").unwrap();

            web_sys::window()
                .and_then(|win| win.document())
                .and_then(|doc| doc.body())
                .and_then(|body| body.append_child(&canvas).ok())
                .expect("couldn't append canvas to document body");
        }

        (event_loop, window)
    }

    fn create_iad<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = anyhow::Result<InstanceAdapterDevice>> + 'a>> {
        Box::pin(async move { Ok(rend3::create_iad(None, None, None, None).await?) })
    }

    fn create_base_rendergraph(&mut self, renderer: &Renderer) -> BaseRenderGraph {
        BaseRenderGraph::new(renderer)
    }

    /// Determines the sample count used, this may change dynamically. This
    /// function is what the framework actually calls, so overriding this
    /// will always use the right values.
    ///
    /// It is called on main events cleared and things are remade if this
    /// changes.
    fn sample_count(&self) -> SampleCount;

    /// Determines the scale factor used
    fn scale_factor(&self) -> f32 {
        1.0
    }

    fn setup(
        &mut self,
        window: &Window,
        renderer: &Arc<Renderer>,
        routines: &Arc<DefaultRoutines>,
        surface_format: rend3::types::TextureFormat,
    ) {
        let _ = (window, renderer, routines, surface_format);
    }

    /// RedrawRequested/RedrawEventsCleared will only be fired if the window
    /// size is non-zero. As such you should always render
    /// in RedrawRequested and use MainEventsCleared for things that need to
    /// keep running when minimized.
    #[allow(clippy::too_many_arguments)]
    fn handle_event(
        &mut self,
        window: &Window,
        renderer: &Arc<rend3::Renderer>,
        routines: &Arc<DefaultRoutines>,
        base_rendergraph: &BaseRenderGraph,
        surface: Option<&Arc<Surface>>,
        resolution: UVec2,
        event: Event<'_, T>,
        control_flow: impl FnOnce(winit::event_loop::ControlFlow),
    ) {
        let _ = (
            window,
            renderer,
            routines,
            base_rendergraph,
            resolution,
            surface,
            event,
            control_flow,
        );
    }
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

#[cfg(not(target_arch = "wasm32"))]
fn winit_run<F, T>(event_loop: winit::event_loop::EventLoop<T>, event_handler: F) -> !
where
    F: FnMut(winit::event::Event<'_, T>, &EventLoopWindowTarget<T>, &mut ControlFlow) + 'static,
    T: 'static,
{
    event_loop.run(event_handler)
}

#[cfg(target_arch = "wasm32")]
fn winit_run<F, T>(event_loop: EventLoop<T>, event_handler: F)
where
    F: FnMut(winit::event::Event<'_, T>, &EventLoopWindowTarget<T>, &mut ControlFlow) + 'static,
    T: 'static,
{
    use wasm_bindgen::{prelude::*, JsCast};

    let winit_closure = Closure::once_into_js(move || event_loop.run(event_handler));

    // make sure to handle JS exceptions thrown inside start.
    // Otherwise wasm_bindgen_futures Queue would break and never handle any tasks
    // again. This is required, because winit uses JS exception for control flow
    // to escape from `run`.
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

pub async fn async_start<A: App + 'static>(mut app: A, window_builder: WindowBuilder) {
    app.register_logger();
    app.register_panic_hook();

    // Create the window invisible until we are rendering
    let (event_loop, window) = app.create_window(window_builder.with_visible(false));
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
        Some(Arc::new(unsafe { iad.instance.create_surface(&window) }))
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
        let format = s.get_preferred_format(&iad.adapter).unwrap();

        // Configure the surface to be ready for rendering.
        rend3::configure_surface(
            s,
            &iad.device,
            format,
            glam::UVec2::new(window_size.width, window_size.height),
            rend3::types::PresentMode::Mailbox,
        );

        format
    });

    let base_rendergraph = app.create_base_rendergraph(&renderer);
    let mut data_core = renderer.data_core.lock();
    let routines = Arc::new(DefaultRoutines {
        pbr: Mutex::new(rend3_routine::pbr::PbrRoutine::new(
            &renderer,
            &mut data_core,
            &base_rendergraph.interfaces,
        )),
        skybox: Mutex::new(rend3_routine::skybox::SkyboxRoutine::new(
            &renderer,
            &base_rendergraph.interfaces,
        )),
        tonemapping: Mutex::new(rend3_routine::tonemapping::TonemappingRoutine::new(
            &renderer,
            &base_rendergraph.interfaces,
            format,
        )),
    });
    drop(data_core);

    app.setup(&window, &renderer, &routines, format);

    #[cfg(target_arch = "wasm32")]
    let _observer = resize_observer::ResizeObserver::new(&window, event_loop.create_proxy());

    // We're ready, so lets make things visible
    window.set_visible(true);

    let mut suspended = cfg!(target_os = "android");
    let mut last_user_control_mode = ControlFlow::Poll;
    let mut stored_surface_info = StoredSurfaceInfo {
        size: glam::UVec2::new(window_size.width, window_size.height),
        scale_factor: app.scale_factor(),
        sample_count: app.sample_count(),
    };

    winit_run(event_loop, move |event, _event_loop, control_flow| {
        let event = match event {
            Event::UserEvent(UserResizeEvent::Resize { size, window_id }) => Event::WindowEvent {
                window_id,
                event: WindowEvent::Resized(size),
            },
            e => e,
        };

        if let Some(suspend) = handle_surface(
            &app,
            &window,
            &event,
            &iad.instance,
            &mut surface,
            &renderer,
            format,
            &mut stored_surface_info,
        ) {
            suspended = suspend;
        }

        // We move to Wait when we get suspended so we don't spin at 50k FPS.
        match event {
            Event::Suspended => {
                *control_flow = ControlFlow::Wait;
            }
            Event::Resumed => {
                *control_flow = last_user_control_mode;
            }
            _ => {}
        }

        // We need to block all updates
        if let Event::RedrawRequested(_) | Event::RedrawEventsCleared | Event::MainEventsCleared = event {
            if suspended {
                return;
            }
        }

        app.handle_event(
            &window,
            &renderer,
            &routines,
            &base_rendergraph,
            surface.as_ref(),
            stored_surface_info.size,
            event,
            |c: ControlFlow| {
                *control_flow = c;
                last_user_control_mode = c;
            },
        )
    });
}

struct StoredSurfaceInfo {
    size: UVec2,
    scale_factor: f32,
    sample_count: SampleCount,
}

#[allow(clippy::too_many_arguments)]
fn handle_surface<A: App, T: 'static>(
    app: &A,
    window: &Window,
    event: &Event<T>,
    instance: &Instance,
    surface: &mut Option<Arc<Surface>>,
    renderer: &Arc<Renderer>,
    format: rend3::types::TextureFormat,
    surface_info: &mut StoredSurfaceInfo,
) -> Option<bool> {
    match *event {
        Event::Resumed => {
            *surface = Some(Arc::new(unsafe { instance.create_surface(window) }));
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

            // Reconfigure the surface for the new size.
            rend3::configure_surface(
                surface.as_ref().unwrap(),
                &renderer.device,
                format,
                glam::UVec2::new(size.x, size.y),
                rend3::types::PresentMode::Mailbox,
            );
            // Tell the renderer about the new aspect ratio.
            renderer.set_aspect_ratio(size.x as f32 / size.y as f32);
            Some(false)
        }
        _ => None,
    }
}

pub fn start<A: App + 'static>(app: A, window_builder: WindowBuilder) {
    #[cfg(target_arch = "wasm32")]
    {
        wasm_bindgen_futures::spawn_local(async_start(app, window_builder));
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        pollster::block_on(async_start(app, window_builder));
    }
}
