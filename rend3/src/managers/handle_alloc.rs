use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::Mutex;
use rend3_types::{RawResourceHandle, ResourceHandle};

pub struct HandleAllocator<T> {
    death_channel_sender: flume::Sender<RawResourceHandle<T>>,
    death_channel_receiver: flume::Receiver<RawResourceHandle<T>>,

    max_allocated: AtomicUsize,
    freelist: Mutex<Vec<usize>>,
}

impl<T> HandleAllocator<T> {
    pub fn new() -> Self {
        let (death_channel_sender, death_channel_receiver) = flume::unbounded();
        Self {
            death_channel_sender,
            death_channel_receiver,
            max_allocated: AtomicUsize::new(0),
            freelist: Mutex::new(Vec::new()),
        }
    }

    pub fn allocate(&self) -> ResourceHandle<T> {
        let maybe_idx = self.freelist.lock().pop();
        let idx = maybe_idx.unwrap_or_else(|| self.max_allocated.fetch_add(1, Ordering::Relaxed));

        ResourceHandle::new(self.death_channel_sender.clone(), idx)
    }

    pub fn flush_dead(&self) -> impl Iterator<Item = RawResourceHandle<T>> + '_ {
        std::iter::from_fn(|| {
            let handle = self.death_channel_receiver.try_recv().ok();
            
            // We intentionally do not add to the freelist here 

            handle
        })
    }
}
