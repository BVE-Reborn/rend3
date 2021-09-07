use std::sync::Arc;
use wgpu::{Surface, SurfaceError, SurfaceFrame, TextureFormat, TextureView, TextureViewDescriptor};

pub const SURFACE_FORMAT: TextureFormat = TextureFormat::Bgra8Unorm;

pub enum OutputFrame {
    Surface {
        view: TextureView,
        surface: Arc<SurfaceFrame>,
    },
    View(Arc<TextureView>),
}

impl OutputFrame {
    pub fn as_view(&self) -> &TextureView {
        match self {
            Self::Surface { view, .. } => view,
            Self::View(inner) => &**inner,
        }
    }
}

pub enum RendererOutput {
    /// Use an internally configured surface. Must setup window using [`RendererBuilder::window`](crate::RendererBuilder::window) before
    /// this can be used.
    ///
    /// # Panics
    ///
    /// Rendering will panic if no window was set up.
    InternalSurface,
    /// Use an externally managed surface's frame. Surface format must be [`SURFACE_FORMAT`].
    ExternalSurface(Arc<SurfaceFrame>),
    /// Use an arbitrary texture view. Format must be [`SURFACE_FORMAT`].
    Image(Arc<TextureView>),
}
impl RendererOutput {
    pub(crate) fn acquire(self, internal: &Option<Surface>) -> OutputFrame {
        profiling::scope!("Acquire Output");
        match self {
            RendererOutput::InternalSurface => {
                let surface = internal
                    .as_ref()
                    .expect("Must setup renderer with a window in order to use internal surface");
                let mut retrieved_frame = None;
                for _ in 0..10 {
                    profiling::scope!("Inner Acquire Loop");
                    match surface.get_current_frame() {
                        Ok(frame) => {
                            retrieved_frame = Some(frame);
                            break;
                        }
                        Err(SurfaceError::Timeout) => {}
                        Err(e) => panic!("Failed to acquire swapchain due to error: {}", e),
                    }
                }
                let frame = retrieved_frame.expect("Swapchain acquire timed out 10 times.");

                let view = frame.output.texture.create_view(&TextureViewDescriptor::default());

                OutputFrame::Surface {
                    surface: Arc::new(frame),
                    view,
                }
            }
            RendererOutput::ExternalSurface(frame) => {
                let view = frame.output.texture.create_view(&TextureViewDescriptor::default());

                OutputFrame::Surface { surface: frame, view }
            }
            RendererOutput::Image(view) => OutputFrame::View(view),
        }
    }
}
