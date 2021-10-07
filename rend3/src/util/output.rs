use std::sync::Arc;
use wgpu::{SurfaceError, SurfaceTexture, TextureView, TextureViewDescriptor};

use crate::types::Surface;

pub enum OutputFrame {
    Surface { view: TextureView, surface: SurfaceTexture },
    View(Arc<TextureView>),
}

impl OutputFrame {
    pub fn from_surface(surface: &Surface) -> Result<Self, SurfaceError> {
        profiling::scope!("OutputFrame::from_surface");
        let mut retrieved_frame = None;
        for _ in 0..10 {
            profiling::scope!("Inner Acquire Loop");
            match surface.get_current_texture() {
                Ok(frame) => {
                    retrieved_frame = Some(frame);
                    break;
                }
                Err(SurfaceError::Timeout) => {}
                Err(e) => return Err(e),
            }
        }
        let frame = retrieved_frame.expect("Swapchain acquire timed out 10 times.");

        let view = frame.texture.create_view(&TextureViewDescriptor::default());

        Ok(OutputFrame::Surface { surface: frame, view })
    }

    pub fn as_view(&self) -> &TextureView {
        match self {
            Self::Surface { view, .. } => view,
            Self::View(inner) => &**inner,
        }
    }

    pub fn present(self) {
        if let Self::Surface { surface, .. } = self {
            surface.present();
        }
    }
}
