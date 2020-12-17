use crate::{JobPriorities, Renderer, RendererInitializationError, RendererMode, RendererOptions};
use raw_window_handle::HasRawWindowHandle;
use std::{future::Future, sync::Arc};
use switchyard::{
    threads::{single_pool_one_to_one, thread_info},
    Switchyard,
};
use wgpu::{AdapterInfo, Backend, Device, Instance, Queue};

pub struct CustomDevice<'a> {
    pub instance: &'a Instance,
    pub device: Arc<Device>,
    pub queue: Arc<Queue>,
    pub info: AdapterInfo,
}

pub struct RendererBuilder<'a, W, TLD = ()>
where
    TLD: 'static,
    W: HasRawWindowHandle,
{
    pub(crate) window: &'a W,
    pub(crate) options: RendererOptions,
    pub(crate) device: Option<CustomDevice<'a>>,
    pub(crate) yard: Option<Arc<Switchyard<TLD>>>,
    pub(crate) priorities: Option<JobPriorities>,
    pub(crate) desired_backend: Option<Backend>,
    pub(crate) desired_device_name: Option<String>,
    pub(crate) desired_mode: Option<RendererMode>,
}
impl<'a, W> RendererBuilder<'a, W, ()>
where
    W: HasRawWindowHandle,
{
    pub fn new(window: &'a W, options: RendererOptions) -> Self {
        Self {
            window,
            options,
            device: None,
            yard: None,
            priorities: None,
            desired_backend: None,
            desired_device_name: None,
            desired_mode: None,
        }
    }
}

impl<'a, W, TLD> RendererBuilder<'a, W, TLD>
where
    TLD: 'static,
    W: HasRawWindowHandle,
{
    pub fn device(mut self, device: CustomDevice<'a>) -> Self {
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

    pub fn yard<TLD2: 'static>(
        self,
        yard: Arc<Switchyard<TLD2>>,
        priorities: JobPriorities,
    ) -> RendererBuilder<'a, W, TLD2> {
        RendererBuilder {
            window: self.window,
            options: self.options,
            device: self.device,
            yard: Some(yard),
            priorities: Some(priorities),
            desired_backend: self.desired_backend,
            desired_device_name: self.desired_device_name,
            desired_mode: self.desired_mode,
        }
    }

    pub fn build(mut self) -> impl Future<Output = Result<Arc<Renderer<TLD>>, RendererInitializationError>> + 'a
    where
        TLD: Default,
    {
        // TODO: figure out how to deal with non-defaultable TLDs
        self.yard.get_or_insert_with(|| {
            Arc::new(Switchyard::new(1, single_pool_one_to_one(thread_info(), None), || TLD::default()).unwrap())
        });

        Renderer::new(self)
    }
}
