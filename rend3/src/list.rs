use crate::renderer::context::RenderContext;

pub trait RenderList<TLD>: Send + Sync where TLD: 'static {
    fn render(&self, context: RenderContext<TLD>);
}
