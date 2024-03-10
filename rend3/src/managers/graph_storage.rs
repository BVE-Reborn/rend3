use std::{
    any::Any,
    ops::{Deref, DerefMut},
};

use parking_lot::RwLock;
use rend3_types::{GraphDataHandle, RawGraphDataHandleUntyped, WasmNotSend};

#[derive(Default)]
pub struct GraphStorage {
    // Type under any is RwLock<T>
    #[cfg(not(target_arch = "wasm32"))]
    data: Vec<Option<Box<dyn Any + Send>>>,
    #[cfg(target_arch = "wasm32")]
    data: Vec<Option<Box<dyn Any>>>,
}

impl GraphStorage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add<T: WasmNotSend + 'static>(&mut self, handle: &RawGraphDataHandleUntyped, data: T) {
        if handle.idx >= self.data.len() {
            self.data.resize_with(handle.idx + 1, || None);
        }
        self.data[handle.idx] = Some(Box::new(RwLock::new(data)));
    }

    pub fn get<T: 'static>(&self, handle: &GraphDataHandle<T>) -> impl Deref<Target = T> + '_ {
        let rw_lock: &RwLock<T> = self.data[handle.0.idx].as_ref().unwrap().downcast_ref().unwrap();
        rw_lock.try_read().expect("Called get on the same handle that was already borrowed mutably within a renderpass")
    }

    pub fn get_mut<T: 'static>(&self, handle: &GraphDataHandle<T>) -> impl DerefMut<Target = T> + '_ {
        let rw_lock: &RwLock<T> = self.data[handle.0.idx].as_ref().unwrap().downcast_ref().unwrap();
        rw_lock.try_write().expect("Tried to call get_mut on the same handle twice within a renderpass")
    }

    pub fn remove(&mut self, handle: &RawGraphDataHandleUntyped) {
        self.data[handle.idx] = None;
    }
}
