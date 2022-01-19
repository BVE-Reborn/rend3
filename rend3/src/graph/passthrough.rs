use std::{
    marker::PhantomData,
    sync::atomic::{AtomicUsize, Ordering},
};

pub struct PassthroughDataRef<T> {
    node_id: usize,
    index: usize,
    _phantom: PhantomData<T>,
}

pub struct PassthroughDataRefMut<T> {
    node_id: usize,
    index: usize,
    _phantom: PhantomData<T>,
}

pub struct PassthroughDataContainer<'node> {
    node_id: usize,
    data: Vec<Option<*const ()>>,
    _phantom: PhantomData<&'node ()>,
}

impl<'node> PassthroughDataContainer<'node> {
    pub(super) fn new() -> Self {
        static NODE_ID: AtomicUsize = AtomicUsize::new(0);
        Self {
            node_id: NODE_ID.fetch_add(1, Ordering::Relaxed),
            data: Vec::new(),
            _phantom: PhantomData,
        }
    }

    pub fn add_ref<T: 'node>(&mut self, data: &'node T) -> PassthroughDataRef<T> {
        let index = self.data.len();
        self.data.push(Some(<*const _>::cast(data)));
        PassthroughDataRef {
            node_id: self.node_id,
            index,
            _phantom: PhantomData,
        }
    }

    pub fn add_ref_mut<T: 'node>(&mut self, data: &'node mut T) -> PassthroughDataRefMut<T> {
        let index = self.data.len();
        self.data.push(Some(<*const _>::cast(data)));
        PassthroughDataRefMut {
            node_id: self.node_id,
            index,
            _phantom: PhantomData,
        }
    }

    pub fn get<T>(&mut self, handle: PassthroughDataRef<T>) -> &'node T {
        assert_eq!(
            handle.node_id, self.node_id,
            "Trying to use a passthrough data reference from another node"
        );
        unsafe {
            &*(self
                .data
                .get_mut(handle.index)
                .expect("internal rendergraph error: passthrough data handle corresponds to no passthrough data")
                .take()
                .expect("tried to retreve passthrough data more than once") as *const T)
        }
    }

    pub fn get_mut<T>(&mut self, handle: PassthroughDataRefMut<T>) -> &'node mut T {
        assert_eq!(
            handle.node_id, self.node_id,
            "Trying to use a passthrough data reference from another node"
        );
        unsafe {
            &mut *(self
                .data
                .get_mut(handle.index)
                .expect("internal rendergraph error: passthrough data handle corresponds to no passthrough data")
                .take()
                .expect("tried to retreve passthrough data more than once") as *const T as *mut T)
        }
    }
}
