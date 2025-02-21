use std::{
    marker::PhantomData,
    panic::Location,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use parking_lot::Mutex;
use rend3_types::{RawResourceHandle, ResourceHandle};

use crate::{instruction::DeletableRawResourceHandle, Renderer};

pub(crate) struct HandleAllocator<T>
where
    RawResourceHandle<T>: DeletableRawResourceHandle,
{
    max_allocated: AtomicUsize,
    freelist: Mutex<Vec<usize>>,
    _phantom: PhantomData<T>,
}

impl<T> HandleAllocator<T>
where
    RawResourceHandle<T>: DeletableRawResourceHandle,
{
    pub fn new() -> Self {
        Self { max_allocated: AtomicUsize::new(0), freelist: Mutex::new(Vec::new()), _phantom: PhantomData }
    }

    /// Allocate a handle.
    pub fn allocate(&self, renderer: &Arc<Renderer>) -> ResourceHandle<T> {
        let maybe_idx = self.freelist.lock().pop();
        let idx = maybe_idx.unwrap_or_else(|| self.max_allocated.fetch_add(1, Ordering::Relaxed));

        let renderer = Arc::clone(renderer);
        //  Note copy of raw resource handle. May result in resource handle outliving its index.
        let destroy_fn = move |handle: &RawResourceHandle<T>| {
            renderer.instructions.push(handle.into_delete_instruction_kind(), *Location::caller())
        };

        ResourceHandle::new(destroy_fn, idx)
    }

    /// Deallocate a raw handle. This consumes the handle.
    pub fn deallocate(&self, handle: RawResourceHandle<T>) {
        let idx = handle.idx;
        self.freelist.lock().push(idx);
    }
}
