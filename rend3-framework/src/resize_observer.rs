use wasm_bindgen::prelude::*;
use winit::{dpi::PhysicalSize, event_loop::EventLoopProxy, platform::web::WindowExtWebSys, window::Window};

use crate::UserResizeEvent;

//from https://github.com/seed-rs/seed/pull/534/files

#[wasm_bindgen(inline_js = r#"
export function __wasm_rend3_framework_register_resize_observer_impl(element, send_msg_resized) {
    const resizeObserver = new ResizeObserver(entries => {
        const entry = entries[0];
        let size = 0;
        // Browsers use different structures to store the size. Don't ask me why..
        if (entry.borderBoxSize instanceof ResizeObserverSize) {
            size = entry.borderBoxSize;
        } else if (entry.borderBoxSize[0] instanceof ResizeObserverSize) {
            size = entry.borderBoxSize[0];
        } else {
            console.error("Cannot get borderBoxSize from ResizeObserver entry!");
        }
        const height = size.blockSize;
        const width = size.inlineSize;
        send_msg_resized(width, height);
    });
    resizeObserver.observe(element);
}
"#)]
extern "C" {
    fn __wasm_rend3_framework_register_resize_observer_impl(element: &web_sys::Element, callback: &JsValue);
}

pub struct ResizeObserver {
    _callback: JsValue,
}
impl ResizeObserver {
    pub fn new<T: 'static>(window: &Window, proxy: EventLoopProxy<UserResizeEvent<T>>) -> Self {
        let canvas = window.canvas();
        let id = window.id();
        let callback: Box<dyn FnMut(u32, u32)> = Box::new(move |width, height| {
            let _res = proxy.send_event(UserResizeEvent::Resize {
                window_id: id,
                size: PhysicalSize { width, height },
            });

            canvas.set_width(width);
            canvas.set_height(height);
        });

        let js_value = Closure::wrap(callback).into_js_value();

        __wasm_rend3_framework_register_resize_observer_impl(&window.canvas(), &js_value);

        Self { _callback: js_value }
    }
}
