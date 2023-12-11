use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use wgpu::Device;

#[must_use = "All error scopes must end in a call to `end`"]
pub struct AllocationErrorScope<'a> {
    device: &'a Device,
}

impl<'a> AllocationErrorScope<'a> {
    pub fn new(device: &'a Device) -> Self {
        device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        Self { device }
    }

    pub fn end(self) -> Result<(), wgpu::Error> {
        let mut future = self.device.pop_error_scope();
        let pin = Pin::new(&mut future);
        match pin.poll(&mut Context::from_waker(&noop_waker::noop_waker())) {
            // We got an error, so return an error.
            Poll::Ready(Some(error)) => Err(error),
            // We got no error, so return nothing.
            Poll::Ready(None) => Ok(()),
            // We're on webgpu, pretend everythign always works.
            Poll::Pending => Ok(()),
        }
    }
}

impl<'a> Drop for AllocationErrorScope<'a> {
    fn drop(&mut self) {
        log::error!("AllocationErrorScope dropped without calling `end`");
    }
}
