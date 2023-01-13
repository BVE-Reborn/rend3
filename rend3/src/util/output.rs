//! Output frame and surface acquisition.

use std::sync::Arc;

use wgpu::{SurfaceTexture, TextureView};

/// Anything that resembles a surface to render to.
pub enum OutputFrame {
    // Pre-acquired surface. rend3 will present it.
    SurfaceAcquired {
        view: TextureView,
        surface_tex: SurfaceTexture,
    },
    // Arbitrary texture view.
    View(Arc<TextureView>),
}

impl OutputFrame {
    /// Turn the given surface into a texture view, if it has one.
    pub fn as_view(&self) -> Option<&TextureView> {
        match self {
            Self::SurfaceAcquired { view, .. } => Some(view),
            Self::View(inner) => Some(&**inner),
        }
    }

    /// Present the surface, if needed.
    pub fn present(self) {
        if let Self::SurfaceAcquired {
            surface_tex: surface, ..
        } = self
        {
            surface.present();
        }
    }
}
