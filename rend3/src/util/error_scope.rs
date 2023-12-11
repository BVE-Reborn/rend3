//! Helpers for working with wgpu error scopes.

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use wgpu::Device;

/// Helper for working with allocation failure error scopes.
///
/// Because WebGPU uses asynchronous allocation, we cannot handle allocation failures
/// on WebGPU. This will always return success on WebGPU.
#[must_use = "All error scopes must end in a call to `end`"]
pub struct AllocationErrorScope<'a> {
    device: &'a Device,
    /// Used to communicate with the destructor if `end` was called on this or not.
    ended: bool,
}

impl<'a> AllocationErrorScope<'a> {
    /// Create a new AllocationErrorScope on this device.
    pub fn new(device: &'a Device) -> Self {
        device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        Self { device, ended: false }
    }

    pub fn end(mut self) -> Result<(), wgpu::Error> {
        // End has been called, no need to error.
        self.ended = true;

        // The future we get from wgpu will always be immedately ready on webgl/native. We can't
        // reasonably handle failures on webgpu. As such we don't want to wait
        // for the future to complete, just manually poll it once.

        let mut future = self.device.pop_error_scope();
        let pin = Pin::new(&mut future);
        match pin.poll(&mut Context::from_waker(&noop_waker::noop_waker())) {
            // We got an error, so return an error.
            Poll::Ready(Some(error)) => Err(error),
            // We got no error, so return success.
            Poll::Ready(None) => Ok(()),
            // We're on webgpu, pretend everything always works.
            Poll::Pending => Ok(()),
        }
    }
}

impl<'a> Drop for AllocationErrorScope<'a> {
    fn drop(&mut self) {
        if !self.ended {
            log::error!("AllocationErrorScope dropped without calling `end`");
        }
    }
}
