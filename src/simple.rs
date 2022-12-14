use std::{future::Future, marker::PhantomData, pin::Pin};

pub struct MutRefStack<'root, T: ?Sized> {
    /// Ensures this mutrefstack does not exceed the lifetime of its root.
    lifetime: PhantomData<&'root mut T>,
    /// The stack of pointers. Each one borrows from the one prior, except the first which is the `root` and may never be popped.
    data: Vec<*mut T>,
}

pub enum MoveDecision<'root, 'this, T: ?Sized> {
    Ascend,
    Stay,
    Descend(&'this mut T),
    Inject(&'root mut T),
}

pub enum MoveError {
    AscendAtRoot,
}

impl<'root, T: ?Sized> MutRefStack<'root, T> {
    /// Create a new MutRefStack from a mutable reference to the root
    /// of a recursive data structure.
    pub fn new(root: &'root mut T) -> Self {
        Self {
            lifetime: PhantomData,
            data: vec![root],
        }
    }

    /// Helper function to get the raw top pointer.
    fn raw_top(&self) -> *mut T {
        self.data
            .last()
            .copied()
            .expect("root pointer should never be popped")
    }

    /// Obtain a shared reference to the top of the stack.
    pub fn top(&self) -> &T {
        let ptr: *const T = self.raw_top();
        unsafe { &(*ptr) }
    }

    /// Obtain a mutable reference to the top of the stack.
    pub fn top_mut(&mut self) -> &mut T {
        let ptr: *mut T = self.raw_top();
        unsafe { &mut (*ptr) }
    }

    /// Is this MutRefStack currently at its root?
    pub fn is_at_root(&self) -> bool {
        self.data.len() == 1
    }

    /// Inject a new reference to the top of the stack. The reference still must live
    /// as long as the root of the stack.
    pub fn inject_top(&mut self, new_top: &'root mut T) -> &mut T {
        self.data.push(new_top);
        self.top_mut()
    }

    /// Inject a new reference to the top of the stack. The reference still must live
    /// as long as the root of the stack.
    pub fn inject_with(
        &mut self,
        f: impl FnOnce(&mut T) -> Option<&'root mut T>,
    ) -> Option<&mut T> {
        let new_top = f(self.top_mut())?;
        self.data.push(new_top);
        Some(self.top_mut())
    }

    /// Descend into the recursive data structure, returning a mutable reference to the new top element.
    /// Rust's borrow checker enforces that the closure cannot inject any lifetime (other than `'static`),
    /// because the closure must work for any lifetime `'node`.
    pub fn descend_with(
        &mut self,
        f: impl for<'node> FnOnce(&'node mut T) -> Option<&'node mut T>,
    ) -> Option<&mut T> {
        let old_top: *mut T = self.raw_top();
        let new_top: &mut T = unsafe { f(&mut *old_top)? };
        self.data.push(new_top);
        Some(new_top)
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
        let top = self.top_mut();
        let result = f(top);
        match result {
            MoveDecision::Ascend => self.ascend().ok_or(MoveError::AscendAtRoot),
            MoveDecision::Stay => Ok(self.top_mut()),
            MoveDecision::Inject(new_top) | MoveDecision::Descend(new_top) => {
                let new_top: *mut T = new_top;
                self.data.push(new_top);
                Ok(self.top_mut())
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
        let top = self.top_mut();
        let result = f(top).await;
        match result {
            MoveDecision::Ascend => self.ascend().ok_or(MoveError::AscendAtRoot),
            MoveDecision::Stay => Ok(self.top_mut()),
            MoveDecision::Inject(new_top) | MoveDecision::Descend(new_top) => {
                let new_top: *mut T = new_top;
                self.data.push(new_top);
                Ok(self.top_mut())
            }
        }
    }

    /// Return reference to the top element of this stack, forgetting about the stack entirely.
    pub fn into_top(self) -> &'root mut T {
        let ptr = self.data.last().copied().unwrap();
        unsafe { &mut *ptr }
    }

    /// Pop all references off the stack and go back to the root.
    pub fn to_root(&mut self) -> &mut T {
        self.data.truncate(1);
        self.top_mut()
    }
}
