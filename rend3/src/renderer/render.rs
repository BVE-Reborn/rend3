use crate::{instruction::Instruction, statistics::RendererStatistics, Renderer};
use std::{future::Future, sync::Arc};

pub fn render_loop<TLD>(renderer: Arc<Renderer<TLD>>) -> impl Future<Output = RendererStatistics> {
    // blocks, do it before we async
    renderer.instructions.swap();

    async move {
        let mut instructions = renderer.instructions.consumer.lock();

        let mut new_options = None;

        for cmd in instructions.drain(..) {
            match cmd {
                Instruction::SetOptions { options } => new_options = Some(options),
                _ => unimplemented!(),
            }
        }

        unimplemented!()
    }
}
