use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use js_sys::Function;
use once_cell::race::OnceBox;
use wasm_bindgen::{prelude::Closure, JsCast};
use winit::{platform::web::WindowExtWebSys, window::Window};

struct GrabberInner {
    grabbed: AtomicBool,
    callback: OnceBox<Function>,
}

pub struct Grabber {
    inner: Arc<GrabberInner>,
}
impl Grabber {
    pub fn new(window: &Window) -> Self {
        let inner = Arc::new(GrabberInner {
            grabbed: AtomicBool::new(false),
            callback: OnceBox::new(),
        });

        let inner_clone = Arc::clone(&inner);

        let canvas = window.canvas();
        let document = canvas.owner_document().unwrap();

        let function: Box<dyn FnMut()> = Box::new(move || {
            if document.pointer_lock_element().as_ref() == Some(&*canvas) {
                log::info!("true");
                inner_clone.grabbed.store(true, Ordering::Release);
            } else {
                log::info!("false");
                document
                    .remove_event_listener_with_callback("pointerlockchange", inner_clone.callback.get().unwrap())
                    .unwrap();
                inner_clone.grabbed.store(false, Ordering::Release);
            }
        });

        let closure = Closure::wrap(function);
        let closure_function = closure.into_js_value().dyn_into::<Function>().unwrap();

        inner.callback.set(Box::new(closure_function)).unwrap();

        Self { inner }
    }

    pub fn request_grab(&mut self, window: &Window) {
        let canvas = window.canvas();
        let document = canvas.owner_document().unwrap();
        canvas.request_pointer_lock();

        document
            .add_event_listener_with_callback("pointerlockchange", self.inner.callback.get().unwrap())
            .unwrap();

        self.inner.grabbed.store(true, Ordering::Relaxed);
    }

    pub fn request_ungrab(&mut self, window: &Window) {
        let canvas = window.canvas();
        let document = canvas.owner_document().unwrap();

        document
            .remove_event_listener_with_callback("pointerlockchange", self.inner.callback.get().unwrap())
            .unwrap();

        document.exit_pointer_lock();
        self.inner.grabbed.store(false, Ordering::Relaxed);
    }

    pub fn grabbed(&self) -> bool {
        self.inner.grabbed.load(Ordering::Acquire)
    }
}
