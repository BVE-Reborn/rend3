use winit::window::Window;

pub struct Grabber {
    grabbed: bool,
}
impl Grabber {
    pub fn new(_window: &Window) -> Self {
        Self { grabbed: false }
    }

    pub fn request_grab(&mut self, window: &Window) {
        let _ = window.set_cursor_grab(true);
        window.set_cursor_visible(false);

        self.grabbed = true;
    }

    pub fn request_ungrab(&mut self, window: &Window) {
        let _ = window.set_cursor_grab(false);
        window.set_cursor_visible(true);

        self.grabbed = false;
    }

    pub fn grabbed(&self) -> bool {
        self.grabbed
    }
}
