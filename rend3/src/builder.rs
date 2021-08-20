use crate::{Renderer, RendererInitializationError, RendererMode, InternalSurfaceOptions};
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use std::{future::Future, sync::Arc};
use wgpu::{AdapterInfo, Backend, Device, Instance, Queue};

pub struct CustomDevice {
    pub instance: Arc<Instance>,
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    pub info: AdapterInfo,
}

pub struct DummyWindow;

unsafe impl HasRawWindowHandle for DummyWindow {
    fn raw_window_handle(&self) -> RawWindowHandle {
        unreachable!("Dummy type for generic purposes")
    }
}

pub struct RendererBuilder<'a, W = DummyWindow>
where
    W: HasRawWindowHandle,
{
    pub(crate) window: Option<&'a W>,
    pub(crate) options: InternalSurfaceOptions,
    pub(crate) device: Option<CustomDevice>,
    pub(crate) desired_backend: Option<Backend>,
    pub(crate) desired_device_name: Option<String>,
    pub(crate) desired_mode: Option<RendererMode>,
}
impl<'a> RendererBuilder<'a, DummyWindow> {
    pub fn new(options: InternalSurfaceOptions) -> Self {
        Self {
            window: None,
            options,
            device: None,
            desired_backend: None,
            desired_device_name: None,
            desired_mode: None,
        }
    }
}

impl<'a, W> RendererBuilder<'a, W>
where
    W: HasRawWindowHandle,
{
    pub fn device(mut self, device: CustomDevice) -> Self {
        self.device = Some(device);
        self
    }

    pub fn desired_device(
        mut self,
        desired_backend: Option<Backend>,
        desired_device_name: Option<String>,
        desired_mode: Option<RendererMode>,
    ) -> Self {
        self.desired_backend = desired_backend;
        self.desired_device_name = desired_device_name;
        self.desired_mode = desired_mode;
        self
    }

    pub fn window<W2: HasRawWindowHandle>(self, window: &'a W2) -> RendererBuilder<'a, W2> {
        RendererBuilder {
            window: Some(window),
            options: self.options,
            device: self.device,
            desired_backend: self.desired_backend,
            desired_device_name: self.desired_device_name,
            desired_mode: self.desired_mode,
        }
    }

    pub fn build(self) -> impl Future<Output = Result<Arc<Renderer>, RendererInitializationError>> + 'a {
        Renderer::new(self)
    }
}
