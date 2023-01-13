use std::{any::Any, ops::DerefMut};

use parking_lot::Mutex;
use rend3_types::{GraphDataHandle, RawGraphDataHandleUntyped};

#[derive(Default)]
pub struct GraphStorage {
    // Type under any is Mutex<T>
    data: Vec<Option<Box<dyn Any + Send>>>,
}

impl GraphStorage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add<T: Send + 'static>(&mut self, handle: &RawGraphDataHandleUntyped, data: T) {
        if handle.idx >= self.data.len() {
            self.data.resize_with(handle.idx + 1, || None);
        }
        self.data[handle.idx] = Some(Box::new(Mutex::new(data)));
    }

    pub fn get_mut<T: 'static>(&self, handle: &GraphDataHandle<T>) -> impl DerefMut<Target = T> + '_ {
        let mutex: &Mutex<T> = self.data[handle.0.idx].as_ref().unwrap().downcast_ref().unwrap();
        mutex
            .try_lock()
            .expect("Tried to call get_mut on the same handle twice")
    }

    pub fn remove(&mut self, handle: &RawGraphDataHandleUntyped) {
        self.data[handle.idx] = None;
    }
}
