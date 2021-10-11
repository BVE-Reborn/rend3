use std::sync::Arc;
use wgpu::{SurfaceError, SurfaceTexture, TextureView, TextureViewDescriptor};

use crate::types::Surface;

pub enum OutputFrame {
    // A surface which has not yet been acquired. This lets rend3 acquire as late as possible.
    Surface {
        surface: Arc<Surface>,
    },
    // Pre-acquired surface. rend3 will present it.
    SurfaceAcquired {
        view: TextureView,
        surface_tex: SurfaceTexture,
    },
    // Arbitrary texture view.
    View(Arc<TextureView>),
}

impl OutputFrame {
    pub fn acquire(&mut self) -> Result<(), SurfaceError> {
        if let Self::Surface { surface } = self {
            profiling::scope!("OutputFrame::acquire");
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
            let surface_tex = retrieved_frame.expect("Swapchain acquire timed out 10 times.");

            let view = surface_tex.texture.create_view(&TextureViewDescriptor::default());

            *self = Self::SurfaceAcquired { view, surface_tex }
        }

        Ok(())
    }

    pub fn as_view(&self) -> Option<&TextureView> {
        match self {
            Self::Surface { .. } => None,
            Self::SurfaceAcquired { view, .. } => Some(view),
            Self::View(inner) => Some(&**inner),
        }
    }

    pub fn present(self) {
        if let Self::SurfaceAcquired {
            surface_tex: surface, ..
        } = self
        {
            surface.present();
        }
    }
}
