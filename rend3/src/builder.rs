use crate::{Renderer, RendererInitializationError, RendererMode, RendererOptions};
use raw_window_handle::HasRawWindowHandle;
use std::{future::Future, sync::Arc};
use switchyard::{
    threads::{single_pool_one_to_one, thread_info},
    Switchyard,
};
use wgpu::{AdapterInfo, Backend, Device, Queue};

struct CustomDevice {
    device: Arc<Device>,
    queue: Arc<Queue>,
    info: AdapterInfo,
}

pub struct RendererBuilder<TLD = ()>
where
    TLD: 'static,
{
    device: Option<CustomDevice>,
    yard: Option<Arc<Switchyard<TLD>>>,
    desired_backend: Option<Backend>,
    desired_device_name: Option<String>,
    desired_mode: Option<RendererMode>,
}
impl<TLD> RendererBuilder<TLD>
where
    TLD: 'static + Default,
{
    pub fn new() -> Self {
        Self {
            device: None,
            yard: None,
            desired_backend: None,
            desired_device_name: None,
            desired_mode: None,
        }
    }

    pub fn device(mut self, device: Arc<Device>, queue: Arc<Queue>, info: AdapterInfo) -> Self {
        self.device = Some(CustomDevice { device, queue, info });
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

    pub fn yard<TLD2: 'static>(self, yard: Arc<Switchyard<TLD2>>) -> RendererBuilder<TLD2> {
        RendererBuilder {
            device: self.device,
            yard: Some(yard),
            desired_backend: self.desired_backend,
            desired_device_name: self.desired_device_name,
            desired_mode: self.desired_mode,
        }
    }

    pub fn build<'a, W: HasRawWindowHandle>(
        self,
        window: &'a W,
        option: RendererOptions,
    ) -> impl Future<Output = Result<Arc<Renderer<TLD>>, RendererInitializationError>> + 'a {
        let yard = self.yard.unwrap_or_else(|| {
            Arc::new(Switchyard::new(1, single_pool_one_to_one(thread_info(), None), || TLD::default()).unwrap())
        });

        Renderer::new(
            window,
            yard,
            self.desired_backend,
            self.desired_device_name,
            self.desired_mode,
            option,
        )
    }
}
