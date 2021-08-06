use std::sync::Arc;

use crate::{util::output::OutputFrame, Renderer};

pub trait RenderRoutine<TLD>: Send + Sync
where
    TLD: 'static,
{
    fn render(&self, renderer: Arc<Renderer<TLD>>, frame: OutputFrame);
}
