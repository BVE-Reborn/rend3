use std::sync::Arc;

mod bind_group;
mod pipeline;
mod renderbuffer;
mod shader;

pub use bind_group::*;
pub use pipeline::*;
pub use renderbuffer::*;
pub use shader::*;

struct ParentedCached<T, P> {
    inner: Arc<T>,
    parent: Arc<P>,
    epoch: usize,
}
struct Cached<T> {
    inner: Arc<T>,
    epoch: usize,
}
