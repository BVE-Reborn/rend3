use std::{future::Future, pin::Pin, sync::Arc};

use futures_intrusive::sync::Mutex;
use rend3::{InstanceAdapterDevice, Renderer};
use winit::{
    event_loop::EventLoop,
    window::{Window, WindowBuilder},
};

#[cfg(target_arch = "wasm32")]
pub trait NativeSendFuture<O>: Future<Output = O> {}
#[cfg(target_arch = "wasm32")]
impl<T, O> NativeSendFuture<O> for T where T: Future<Output = O> {}

#[cfg(not(target_arch = "wasm32"))]
pub trait NativeSendFuture<O>: Future<Output = O> + Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T, O> NativeSendFuture<O> for T where T: Future<Output = O> + Send {}

pub trait App {
    type RoutineContainer;

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
            web_sys::window()
                .and_then(|win| win.document())
                .and_then(|doc| doc.body())
                .and_then(|body| body.append_child(&web_sys::Element::from(window.canvas())).ok())
                .expect("couldn't append canvas to document body");
        }

        (event_loop, window)
    }

    fn create_iad<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = anyhow::Result<InstanceAdapterDevice>> + 'a>> {
        Box::pin(async move { Ok(rend3::create_iad(None, None, None).await?) })
    }

    fn setup<'a>(
        &'a mut self,
        renderer: &'a Renderer,
        routines: &'a DefaultRoutines,
    ) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
        let _ = (renderer, routines);
        Box::pin(async move {})
    }

    fn async_setup(
        &mut self,
        renderer: Arc<Renderer>,
        routines: Arc<DefaultRoutines>,
    ) -> Pin<Box<dyn NativeSendFuture<()>>> {
        let _ = (renderer, routines);
        Box::pin(async move {})
    }
}

pub struct DefaultRoutines {
    pub pbr: Mutex<rend3_pbr::PbrRenderRoutine>,
    pub skybox: Mutex<rend3_pbr::SkyboxRoutine>,
    pub tonemapping: Mutex<rend3_pbr::TonemappingRoutine>,
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

pub async fn async_start<A: App>(mut app: A, window_builder: WindowBuilder) {
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
        pbr: Mutex::new(
            rend3_pbr::PbrRenderRoutine::new(&renderer, render_texture_options),
            false,
        ),
        skybox: Mutex::new(rend3_pbr::SkyboxRoutine::new(&renderer, render_texture_options), false),
        tonemapping: Mutex::new(
            rend3_pbr::TonemappingRoutine::new(&renderer, render_texture_options.resolution, format),
            false,
        ),
    });

    app.setup(&renderer, &routines).await;

    spawn(app.async_setup(Arc::clone(&renderer), Arc::clone(&routines)));
}

#[cfg(not(target_arch = "wasm32"))]
pub fn start<A: App>(app: A, window_builder: WindowBuilder) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async_start(app, window_builder));
}

#[cfg(target_arch = "wasm32")]
pub fn start<A: App + 'static>(app: A, window_builder: WindowBuilder) {
    wasm_bindgen_futures::spawn_local(async_start(app, window_builder));
}
