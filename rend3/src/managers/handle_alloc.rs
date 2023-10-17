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
    /// We want the render routines to be able to rely on deleted handles being valid for at
    /// least one frame.
    ///
    /// To facilitate this, we first put the handle in the delay list, then at the top of
    /// every frame, we move the handles from the delay list to the freelist.
    ///
    /// We do not need to do this for everything though, only for Object handles, as these
    /// are the root handle which the renderer accesses everything.
    delay_list: Option<Mutex<Vec<usize>>>,
    _phantom: PhantomData<T>,
}

impl<T> HandleAllocator<T>
where
    RawResourceHandle<T>: DeletableRawResourceHandle,
{
    pub fn new(delay_handle_reclaimation: bool) -> Self {
        Self {
            max_allocated: AtomicUsize::new(0),
            freelist: Mutex::new(Vec::new()),
            delay_list: delay_handle_reclaimation.then(|| Mutex::new(Vec::new())),
            _phantom: PhantomData,
        }
    }

    pub fn allocate(&self, renderer: &Arc<Renderer>) -> ResourceHandle<T> {
        let maybe_idx = self.freelist.lock().pop();
        let idx = maybe_idx.unwrap_or_else(|| self.max_allocated.fetch_add(1, Ordering::Relaxed));

        let renderer = Arc::clone(renderer);
        let destroy_fn = move |handle: RawResourceHandle<T>| {
            renderer
                .instructions
                .push(handle.into_delete_instruction_kind(), *Location::caller())
        };

        ResourceHandle::new(destroy_fn, idx)
    }

    pub fn deallocate(&self, handle: RawResourceHandle<T>) {
        let idx = handle.idx;
        if let Some(ref delay_list) = self.delay_list {
            delay_list.lock().push(idx);
        } else {
            self.freelist.lock().push(idx);
        }
    }

    pub fn reclaim_delayed_handles(&self) -> Vec<RawResourceHandle<T>> {
        if let Some(ref delay_list) = self.delay_list {
            let mut locked_delay_list = delay_list.lock();

            self.freelist.lock().extend_from_slice(&locked_delay_list);
            locked_delay_list
                .drain(..)
                .map(|idx| RawResourceHandle::new(idx))
                .collect()
        } else {
            Vec::new()
        }
    }
}
