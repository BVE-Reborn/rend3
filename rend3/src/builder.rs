use crate::{JobPriorities, Renderer, RendererInitializationError, RendererMode, RendererOptions};
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
use std::{future::Future, sync::Arc};
use switchyard::{
    threads::{single_pool_one_to_one, thread_info},
    Switchyard,
};
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

pub struct RendererBuilder<'a, W = DummyWindow, TLD = ()>
where
    TLD: 'static,
    W: HasRawWindowHandle,
{
    pub(crate) window: Option<&'a W>,
    pub(crate) options: RendererOptions,
    pub(crate) device: Option<CustomDevice>,
    pub(crate) yard: Option<Arc<Switchyard<TLD>>>,
    pub(crate) priorities: Option<JobPriorities>,
    pub(crate) desired_backend: Option<Backend>,
    pub(crate) desired_device_name: Option<String>,
    pub(crate) desired_mode: Option<RendererMode>,
}
impl<'a> RendererBuilder<'a, DummyWindow, ()> {
    pub fn new(options: RendererOptions) -> Self {
        Self {
            window: None,
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

    pub fn window<W2: HasRawWindowHandle>(self, window: &'a W2) -> RendererBuilder<'a, W2, TLD> {
        RendererBuilder {
            window: Some(window),
            options: self.options,
            device: self.device,
            yard: self.yard,
            priorities: self.priorities,
            desired_backend: self.desired_backend,
            desired_device_name: self.desired_device_name,
            desired_mode: self.desired_mode,
        }
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
            Arc::new(Switchyard::new(1, single_pool_one_to_one(thread_info(), None), TLD::default).unwrap())
        });

        Renderer::new(self)
    }
}
