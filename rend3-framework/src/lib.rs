use std::{future::Future, pin::Pin, sync::Arc};

use rend3::{types::Surface, InstanceAdapterDevice, Renderer};
use winit::{
    event::Event,
    event_loop::{ControlFlow, EventLoop, EventLoopWindowTarget},
    window::{Window, WindowBuilder},
};

mod assets;
mod grab;
#[cfg(target_arch = "wasm32")]
mod resize_observer;

pub use assets::*;
pub use grab::*;

#[cfg(target_arch = "wasm32")]
pub trait NativeSend {}
#[cfg(target_arch = "wasm32")]
impl<T> NativeSend for T {}

#[cfg(not(target_arch = "wasm32"))]
pub trait NativeSend: Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T> NativeSend for T where T: Send {}

pub trait NativeSendFuture<O>: Future<Output = O> + NativeSend {}
impl<T, O> NativeSendFuture<O> for T where T: Future<Output = O> + NativeSend {}

pub type AsyncMutex<T> = futures_intrusive::sync::Mutex<T>;

pub trait App {
    fn register_logger(&mut self) {
        #[cfg(target_arch = "wasm32")]
        console_log::init().unwrap();

        #[cfg(not(target_arch = "wasm32"))]
        env_logger::init();
    }

    fn register_panic_hook(&mut self) {
        #[cfg(target_arch = "wasm32")]
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    }

    fn create_window(&mut self, builder: WindowBuilder) -> (EventLoop<()>, Window) {
        profiling::scope!("creating window");

        let event_loop = EventLoop::new();
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
        Box::pin(async move { Ok(rend3::create_iad(None, None, None).await?) })
    }

    fn setup<'a>(
        &'a mut self,
        window: &'a Window,
        renderer: &'a Renderer,
        routines: &'a DefaultRoutines,
        surface: &'a Surface,
    ) -> Pin<Box<dyn NativeSendFuture<()> + 'a>> {
        let _ = (window, renderer, routines, surface);
        Box::pin(async move {})
    }

    fn async_setup(
        &mut self,
        renderer: Arc<Renderer>,
        routines: Arc<DefaultRoutines>,
        surface: Arc<Surface>,
    ) -> Pin<Box<dyn NativeSendFuture<()>>> {
        let _ = (renderer, routines, surface);
        Box::pin(async move {})
    }

    fn handle_event<'a, T: NativeSend>(
        &'a mut self,
        window: &'a Window,
        renderer: &'a Arc<rend3::Renderer>,
        routines: &'a Arc<DefaultRoutines>,
        surface: &'a Arc<Surface>,
        event: Event<'a, T>,
        control_flow: &'a mut winit::event_loop::ControlFlow,
    ) -> Pin<Box<dyn NativeSendFuture<()> + 'a>> {
        let _ = (window, renderer, routines, surface, event, control_flow);
        Box::pin(async move {})
    }
}

pub struct DefaultRoutines {
    pub pbr: AsyncMutex<rend3_pbr::PbrRenderRoutine>,
    pub skybox: AsyncMutex<rend3_pbr::SkyboxRoutine>,
    pub tonemapping: AsyncMutex<rend3_pbr::TonemappingRoutine>,
}

#[cfg(not(target_arch = "wasm32"))]
pub fn spawn<Fut>(fut: Fut)
where
    Fut: Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    tokio::spawn(fut);
}

#[cfg(target_arch = "wasm32")]
pub fn spawn<Fut>(fut: Fut)
where
    Fut: Future + 'static,
    Fut::Output: 'static,
{
    wasm_bindgen_futures::spawn_local(async move {
        fut.await;
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn winit_run<F, T>(event_loop: winit::event_loop::EventLoop<T>, event_handler: F) -> !
where
    F: FnMut(Event<'_, T>, &EventLoopWindowTarget<T>, &mut ControlFlow) + 'static,
{
    event_loop.run(event_handler)
}

#[cfg(target_arch = "wasm32")]
fn winit_run<F, T>(event_loop: EventLoop<T>, event_handler: F)
where
    F: FnMut(Event<'_, T>, &EventLoopWindowTarget<T>, &mut ControlFlow) + 'static,
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

pub async fn async_start<A: App + NativeSend + 'static>(mut app: A, window_builder: WindowBuilder) {
    app.register_logger();
    app.register_panic_hook();

    let (event_loop, window) = app.create_window(window_builder);
    let window_size = window.inner_size();

    let iad = app.create_iad().await.unwrap();

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
    let routines = Arc::new(DefaultRoutines {
        pbr: AsyncMutex::new(
            rend3_pbr::PbrRenderRoutine::new(&renderer, render_texture_options),
            false,
        ),
        skybox: AsyncMutex::new(rend3_pbr::SkyboxRoutine::new(&renderer, render_texture_options), false),
        tonemapping: AsyncMutex::new(
            rend3_pbr::TonemappingRoutine::new(&renderer, render_texture_options.resolution, format),
            false,
        ),
    });

    app.setup(&window, &renderer, &routines, &surface).await;

    spawn(app.async_setup(Arc::clone(&renderer), Arc::clone(&routines), Arc::clone(&surface)));

    let (sender, reciever) = flume::unbounded();

    #[cfg(target_arch = "wasm32")]
    let observer = resize_observer::ResizeObserver::new(&window, sender.clone());

    spawn(async move {
        #[cfg(target_arch = "wasm32")]
        let _observer = observer;

        let mut flow = ControlFlow::Poll;
        let mut redraw = Vec::with_capacity(16);
        while let Ok(e) = reciever.recv_async().await {
            let mut event_opt = Some(e);
            let mut main_events_cleared = false;
            let mut redraw_events_cleared = false;
            while let Some(event) = event_opt.take() {
                match event {
                    // Window was resized, need to resize renderer.
                    Event::WindowEvent {
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
                        routines.pbr.lock().await.resize(
                            &renderer,
                            rend3_pbr::RenderTextureOptions {
                                resolution: size,
                                samples: rend3_pbr::SampleCount::One,
                            },
                        );
                        routines.tonemapping.lock().await.resize(size);
                    }
                    _ => {}
                }

                match event {
                    Event::MainEventsCleared => {
                        main_events_cleared = true;
                    }
                    Event::RedrawEventsCleared => {
                        redraw_events_cleared = true;
                    }
                    Event::RedrawRequested(w) => {
                        redraw.push(w);
                    }
                    e => {
                        app.handle_event(&window, &renderer, &routines, &surface, e, &mut flow)
                            .await;
                    }
                }

                event_opt = match reciever.try_recv() {
                    Ok(e) => Some(e),
                    Err(flume::TryRecvError::Empty) => None,
                    Err(flume::TryRecvError::Disconnected) => break,
                };
            }

            if main_events_cleared {
                app.handle_event(
                    &window,
                    &renderer,
                    &routines,
                    &surface,
                    Event::<()>::MainEventsCleared,
                    &mut flow,
                )
                .await;
            }

            for w in redraw.drain(..) {
                app.handle_event(
                    &window,
                    &renderer,
                    &routines,
                    &surface,
                    Event::<()>::RedrawRequested(w),
                    &mut flow,
                )
                .await;
            }

            if redraw_events_cleared {
                app.handle_event(
                    &window,
                    &renderer,
                    &routines,
                    &surface,
                    Event::<()>::RedrawEventsCleared,
                    &mut flow,
                )
                .await;
            }
        }
    });

    winit_run(event_loop, move |event, _event_loop, control_flow| {
        if let Some(e) = event.to_static() {
            match sender.send(e) {
                Ok(()) => {}
                Err(_) => {
                    *control_flow = ControlFlow::Exit;
                }
            }
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub fn start<A: App + Send + 'static>(app: A, window_builder: WindowBuilder) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async_start(app, window_builder));
}

#[cfg(target_arch = "wasm32")]
pub fn start<A: App + 'static>(app: A, window_builder: WindowBuilder) {
    wasm_bindgen_futures::spawn_local(async_start(app, window_builder));
}
