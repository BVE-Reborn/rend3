use std::cell::RefCell;

use bumpalo::Bump;

pub struct RpassTemporaryPool<'rpass> {
    bump: Bump,
    dtors: RefCell<Vec<Box<dyn FnOnce() + 'rpass>>>,
}
impl<'rpass> RpassTemporaryPool<'rpass> {
    pub(super) fn new() -> Self {
        Self {
            bump: Bump::new(),
            dtors: RefCell::new(Vec::new()),
        }
    }

    #[allow(clippy::mut_from_ref)]
    pub fn add<T: 'rpass>(&'rpass self, v: T) -> &'rpass mut T {
        let r = self.bump.alloc(v);
        let ptr = r as *mut T;
        self.dtors
            .borrow_mut()
            .push(Box::new(move || unsafe { std::ptr::drop_in_place(ptr) }));
        r
    }

    pub(crate) unsafe fn clear(&mut self) {
        for dtor in self.dtors.get_mut().drain(..) {
            dtor()
        }
        self.bump.reset();
    }
}
