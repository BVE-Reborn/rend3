use std::sync::Arc;

use parking_lot::{Condvar, Mutex};

// Syncronization primitive that allows waiting for all registered work to finish.
pub struct WaitGroup {
    counter: Mutex<usize>,
    condvar: Condvar,
}

impl WaitGroup {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { counter: Mutex::new(0), condvar: Condvar::new() })
    }

    pub fn increment(self: &Arc<Self>) -> DecrementGuard {
        *self.counter.lock() += 1;

        DecrementGuard { wg: self.clone() }
    }

    fn decrement(&self) {
        let mut counter = self.counter.lock();
        *counter -= 1;
        if *counter == 0 {
            self.condvar.notify_all();
        }
    }

    pub fn wait(&self) {
        let mut counter = self.counter.lock();
        while *counter != 0 {
            self.condvar.wait(&mut counter);
        }
    }
}

pub struct DecrementGuard {
    wg: Arc<WaitGroup>,
}

impl Drop for DecrementGuard {
    fn drop(&mut self) {
        self.wg.decrement();
    }
}
