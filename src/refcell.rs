use std::{
    cell::{BorrowMutError, RefCell, RefMut},
    future::Future,
    marker::PhantomData,
    pin::Pin,
};

pub struct RefCellRefMutStack<'root, T: ?Sized> {
    /// Ensures this mutrefstack does not exceed the lifetime of its root.
    lifetime: PhantomData<&'root mut T>,
    /// The stack of pointers. Each one borrows from the one prior, except the first which is the `root` and may never be popped.
    /// Note: the `'root` lifetime is a "lie", only used because there's no raw pointer counterpart for `RefMut`.
    /// The `RefMut`s are not publicly accessible so this is fine.
    data: Vec<RefMut<'root, T>>,
}

pub enum MoveDecision<'root, 'this, T: ?Sized> {
    Ascend,
    Stay,
    Descend(&'this RefCell<T>),
    Inject(&'root RefCell<T>),
}

pub enum MoveError {
    AscendAtRoot,
    BorrowMutError(BorrowMutError),
}

impl<'root, T: ?Sized> RefCellRefMutStack<'root, T> {
    /// Create a new MutRefStack from a mutable reference to the root
    /// of a recursive data structure.
    pub fn new(root: &'root RefCell<T>) -> Result<Self, BorrowMutError> {
        let root: *const RefCell<T> = root;
        let borrow = unsafe { (*root).try_borrow_mut()? };
        Ok(Self {
            lifetime: PhantomData,
            data: vec![borrow],
        })
    }

    pub fn raw_top_mut(&mut self) -> *mut T {
        let refmut: *mut RefMut<T> = self.data.last_mut().unwrap();
        unsafe { &mut **refmut }
    }

    /// Obtain a shared reference to the top of the stack.
    pub fn top(&self) -> &T {
        &*self.data.last().unwrap()
    }

    /// Obtain a mutable reference to the top of the stack.
    pub fn top_mut(&mut self) -> &mut T {
        &mut *self.data.last_mut().unwrap()
    }

    /// Is this MutRefStack currently at its root?
    pub fn is_at_root(&self) -> bool {
        self.data.len() == 1
    }

    /// Inject a new reference to the top of the stack. The reference still must live
    /// as long as the root of the stack.
    pub fn inject_top(&mut self, new_top: &'root RefCell<T>) -> Result<&mut T, BorrowMutError> {
        let new_top: *const RefCell<T> = new_top;
        let borrow = unsafe { (*new_top).try_borrow_mut()? };
        self.data.push(borrow);
        Ok(self.top_mut())
    }

    /// Inject a new reference to the top of the stack. The reference still must live
    /// as long as the root of the stack.
    pub fn inject_with(
        &mut self,
        f: impl FnOnce(&mut T) -> Option<&'root RefCell<T>>,
    ) -> Option<Result<&mut T, BorrowMutError>> {
        let old_top: *mut T = self.raw_top_mut();
        let new_top: &RefCell<T> = unsafe { f(&mut *old_top)? };
        let new_top: *const RefCell<T> = new_top;
        let borrow = unsafe { (*new_top).try_borrow_mut() };
        match borrow {
            Ok(borrow) => {
                self.data.push(borrow);
                Some(Ok(self.top_mut()))
            }
            Err(err) => Some(Err(err)),
        }
    }

    /// Descend into the recursive data structure, returning a mutable reference to the new top element.
    /// Rust's borrow checker enforces that the closure cannot inject any lifetime (other than `'static`),
    /// because the closure must work for any lifetime `'node`.
    pub fn descend_with(
        &mut self,
        f: impl for<'node> FnOnce(&'node mut T) -> Option<&'node RefCell<T>>,
    ) -> Option<Result<&mut T, BorrowMutError>> {
        let old_top: *mut T = self.raw_top_mut();
        let new_top: &RefCell<T> = unsafe { f(&mut *old_top)? };
        let new_top: *const RefCell<T> = new_top;
        let borrow = unsafe { (*new_top).try_borrow_mut() };
        match borrow {
            Ok(borrow) => {
                self.data.push(borrow);
                Some(Ok(self.top_mut()))
            }
            Err(err) => Some(Err(err)),
        }
    }

    /// Ascend back up from the recursive data structure, returning a mutable reference to the new top element, if it changed.
    /// If we are not currently at the root, ascend and return a reference to the new top.
    /// If we are already at the root, returns None (the top is the root and does not change).
    pub fn ascend(&mut self) -> Option<&mut T> {
        match self.data.len() {
            0 => unreachable!("root pointer must always exist"),
            1 => None,
            _ => {
                self.data.pop();
                Some(self.top_mut())
            }
        }
    }

    /// Ascend back up from the recursive data structure while the given closure returns `true`, returning a mutable reference to the new top element.
    /// If we are not currently at the root, and the predicate returns `true`, ascend and continue.
    /// If we are already at the root, or if the predicate returned false, returns a reference to the top element.
    pub fn ascend_while<P>(&mut self, mut predicate: P) -> &mut T
    where
        P: FnMut(&mut T) -> bool,
    {
        while !self.is_at_root() && predicate(self.top_mut()) {
            let Some(_) = self.ascend() else {
                unreachable!();
            };
        }
        self.top_mut()
    }

    /// Ascend from, descend from, inject a new stack top, or stay at the current node,
    /// based on the return value of the closure.
    pub fn move_with<F>(&mut self, f: F) -> Result<&mut T, MoveError>
    where
        F: for<'a> FnOnce(&'a mut T) -> MoveDecision<'root, 'a, T>,
    {
        let old_top: *mut T = self.raw_top_mut();
        let result = unsafe { f(&mut *old_top) };
        match result {
            MoveDecision::Ascend => self.ascend().ok_or(MoveError::AscendAtRoot),
            MoveDecision::Stay => Ok(self.top_mut()),
            MoveDecision::Inject(new_top) | MoveDecision::Descend(new_top) => {
                let new_top: *const RefCell<T> = new_top;
                let borrow = unsafe { (*new_top).try_borrow_mut() };
                match borrow {
                    Ok(borrow) => {
                        self.data.push(borrow);
                        Ok(self.top_mut())
                    }
                    Err(err) => Err(MoveError::BorrowMutError(err)),
                }
            }
        }
    }

    pub async fn move_with_async<F>(&mut self, f: F) -> Result<&mut T, MoveError>
    where
        F: for<'a> FnOnce(
            &'a mut T,
        )
            -> Pin<Box<dyn Future<Output = MoveDecision<'root, 'a, T>> + 'a>>,
    {
        let old_top: *mut T = self.raw_top_mut();
        let result = unsafe { f(&mut *old_top) }.await;
        match result {
            MoveDecision::Ascend => self.ascend().ok_or(MoveError::AscendAtRoot),
            MoveDecision::Stay => Ok(self.top_mut()),
            MoveDecision::Inject(new_top) | MoveDecision::Descend(new_top) => {
                let new_top: *const RefCell<T> = new_top;
                let borrow = unsafe { (*new_top).try_borrow_mut() };
                match borrow {
                    Ok(borrow) => {
                        self.data.push(borrow);
                        Ok(self.top_mut())
                    }
                    Err(err) => Err(MoveError::BorrowMutError(err)),
                }
            }
        }
    }

    /// Return reference to the top element of this stack, forgetting about the stack entirely.
    /// Note that this leaks all `RefMut`s above the top.
    pub fn into_top(mut self) -> RefMut<'root, T> {
        let ret = self.data.pop().unwrap();
        unsafe {
            // We need to not drop the parent RefMuts, if any
            self.data.set_len(0);
        }
        ret
    }

    /// Pop all `RefMut`s off the stack and go back to the root.
    pub fn to_root(&mut self) -> &mut T {
        for _ in 1..self.data.len() {
            // We need to drop the RefMut's in the reverse order.
            // Vec::truncate does not specify drop order, but it's probably wrong anyway.
            self.data.pop();
        }
        self.top_mut()
    }
}

impl<'root, T: ?Sized> Drop for RefCellRefMutStack<'root, T> {
    fn drop(&mut self) {
        for _ in 0..self.data.len() {
            // We need to drop the RefMut's in the reverse order.
            // Vec::truncate does not specify drop order, but it's probably wrong anyway.
            self.data.pop();
        }
    }
}
