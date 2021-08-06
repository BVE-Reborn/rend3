use std::sync::Arc;

use crate::{util::output::OutputFrame, Renderer};

pub trait RenderRoutine<TLD>: Send + Sync
where
    TLD: 'static,
{
    fn render(&self, context: Arc<Renderer<TLD>>, frame: OutputFrame);
}
