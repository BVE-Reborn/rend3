use std::sync::Arc;
use wgpu::{SwapChain, SwapChainError, SwapChainFrame, TextureFormat, TextureView};

pub const SWAPCHAIN_FORMAT: TextureFormat = TextureFormat::Bgra8Unorm;

#[derive(Clone)]
pub enum OutputFrame {
    Swapchain(Arc<SwapChainFrame>),
    View(Arc<TextureView>),
}

impl OutputFrame {
    pub fn as_view(&self) -> &TextureView {
        match self {
            Self::Swapchain(inner) => &inner.output.view,
            Self::View(inner) => &**inner,
        }
    }
}

pub enum RendererOutput {
    /// Use an internally managed swapchain. Must setup window using [`RendererBuilder::window`] before
    /// this can be used.
    ///
    /// # Panics
    ///
    /// Rendering will panic if no window was set up.
    InternalSwapchain,
    /// Use an externally managed swapchain's frame. Swapchain format must be [`SWAPCHAIN_FORMAT`].
    ExternalSwapchain(Arc<SwapChainFrame>),
    /// Use an arbitrary texture view. Format must be [`SWAPCHAIN_FORMAT`].
    Image(Arc<TextureView>),
}
impl RendererOutput {
    pub(crate) fn acquire(self, internal: &Option<SwapChain>) -> OutputFrame {
        match self {
            RendererOutput::InternalSwapchain => {
                let sc = internal
                    .as_ref()
                    .expect("Must setup renderer with a window in order to use internal swapchain");
                let mut retrieved_frame = None;
                for _ in 0..10 {
                    match sc.get_current_frame() {
                        Ok(frame) => {
                            retrieved_frame = Some(frame);
                            break;
                        }
                        Err(SwapChainError::Timeout) => {}
                        Err(e) => panic!("Failed to acquire swapchain due to error: {}", e),
                    }
                }
                let frame = retrieved_frame.expect("Swapchain acquire timed out 10 times.");

                OutputFrame::Swapchain(Arc::new(frame))
            }
            RendererOutput::ExternalSwapchain(frame) => OutputFrame::Swapchain(frame),
            RendererOutput::Image(view) => OutputFrame::View(view),
        }
    }
}
