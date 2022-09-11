use std::ops::{Index, IndexMut};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FreelistIndex(pub usize);

pub struct FreelistVec<T> {
    data: Vec<Option<T>>,
    freelist: Vec<usize>,
}

impl<T> FreelistVec<T> {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            freelist: Vec::new(),
        }
    }

    pub fn push(&mut self, value: T) -> FreelistIndex {
        if let Some(index) = self.freelist.pop() {
            debug_assert!(self.data[index].is_none());
            self.data[index] = Some(value);
            FreelistIndex(index)
        } else {
            let index = self.data.len();
            self.data.push(Some(value));
            FreelistIndex(index)
        }
    }

    pub fn remove(&mut self, index: FreelistIndex) {
        debug_assert!(self.data[index.0].is_some());
        self.data[index.0] = None;
        self.freelist.push(index.0);
    }
}

impl<T> Index<FreelistIndex> for FreelistVec<T> {
    type Output = T;

    fn index(&self, index: FreelistIndex) -> &Self::Output {
        self.data[index.0].as_ref().unwrap()
    }
}

impl<T> IndexMut<FreelistIndex> for FreelistVec<T> {
    fn index_mut(&mut self, index: FreelistIndex) -> &mut Self::Output {
        self.data[index.0].as_mut().unwrap()
    }
}
