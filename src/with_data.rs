use std::{collections::VecDeque, marker::PhantomData};

pub struct MutRefStackWithData<'root, T: ?Sized, U: 'root> {
    /// Ensures this mutrefstack does not exceed the lifetime of its root.
    lifetime: PhantomData<(&'root mut T, U)>,
    /// The stack of pointers. Each one borrows from the one prior, except the first which is the `root` and may never be popped.
    data: Vec<(*mut T, U)>,
}

pub enum MoveDecision<'root, 'this, T: ?Sized, U: 'root> {
    Ascend,
    Stay,
    Descend(&'this mut T, U),
    Inject(&'root mut T, U),
}

pub enum MoveError {
    AscendAtRoot,
}

impl<'root, T: ?Sized, U> MutRefStackWithData<'root, T, U> {
    /// Create a new MutRefStack from a mutable reference to the root
    /// of a recursive data structure.
    pub fn new(root: &'root mut T, additional_data: U) -> Self {
        Self {
            lifetime: PhantomData,
            data: vec![(root, additional_data)],
        }
    }

    /// Obtain a shared reference to the top of the stack.
    pub fn top(&self) -> (&T, &U) {
        let &(ptr, ref additional_data) = self
            .data
            .last()
            .expect("root pointer should never be popped");
        let ptr: *const T = ptr;
        (unsafe { &(*ptr) }, additional_data)
    }

    /// Obtain a mutable reference to the top of the stack.
    pub fn top_mut(&mut self) -> (&mut T, &mut U) {
        let &mut (ptr, ref mut additional_data) = self
            .data
            .last_mut()
            .expect("root pointer should never be popped");
        (unsafe { &mut (*ptr) }, additional_data)
    }

    /// Is this MutRefStack currently at its root?
    pub fn is_at_root(&self) -> bool {
        self.data.len() == 1
    }

    /// Descend into the recursive data structure, returning a mutable reference to the new top element.
    /// Rust's borrow checker enforces that the closure cannot inject any lifetime (other than `'static`),
    /// because the closure must work for any lifetime `'node`.
    pub fn descend_with(
        &mut self,
        f: impl for<'node, 'addl> FnOnce(&'node mut T, &'addl mut U) -> Option<(&'node mut T, U)>,
    ) -> Option<(&mut T, &mut U)> {
        let &mut (ptr, ref mut addl) = self
            .data
            .last_mut()
            .expect("root pointer should never be popped");
        {
            let node = unsafe { &mut *ptr };
            let (desc, new_addl) = f(node, addl)?;
            self.data.push((desc, new_addl));
        }
        Some(self.top_mut())
    }

    /// Inject a new reference to the top of the stack. The reference still must live
    /// as long as the root of the stack.
    pub fn inject_with(
        &mut self,
        f: impl for<'node, 'addl> FnOnce(&'node mut T, &'addl mut U) -> Option<(&'root mut T, U)>,
    ) -> Option<(&mut T, &mut U)> {
        let (top, addl) = self.top_mut();
        let (new_top, new_addl) = f(top, addl)?;
        self.data.push((new_top, new_addl));
        Some(self.top_mut())
    }

    /// Ascend back up from the recursive data structure, returning a mutable reference to the new top element, if it changed.
    /// If we are not currently at the root, ascend and return a reference to the new top, a reference to the new top's additional data, and the old top's additional data.
    /// If we are already the root, returns None (the top is the root and does not change).
    pub fn ascend(&mut self) -> Option<((&mut T, &mut U), U)> {
        match self.data.len() {
            0 => unreachable!("root pointer must always exist"),
            1 => None,
            _ => {
                let Some((_ptr, addl)) = self.data.pop() else { unreachable!() };
                Some((self.top_mut(), addl))
            }
        }
    }

    /// Ascend back up from the recursive data structure while the given closure returns `true`, returning a mutable reference to the new top element.
    /// If we are not currently at the root, and the predicate returns `true`, ascend and continue.
    /// If we are already at the root, or if the predicate returned false, returns a reference to the top element.
    pub fn ascend_while<P>(
        &mut self,
        mut predicate: P,
    ) -> ((&mut T, &mut U), impl IntoIterator<Item = U>)
    where
        P: FnMut(&mut T, &mut U) -> bool,
    {
        let mut items = VecDeque::new();
        while !self.is_at_root() {
            let (top, addl) = self.top_mut();
            if !predicate(top, addl) {
                break;
            }
            let Some((_top, addl)) = self.ascend() else {
                unreachable!()
            };
            items.push_front(addl);
        }
        (self.top_mut(), items)
    }

    /// Ascend from, descend from, inject above, or stay at the current node,
    /// based on the return value of the closure.
    pub fn move_with(
        &mut self,
        f: impl for<'node, 'addl> FnOnce(&'node mut T, &'addl mut U) -> MoveDecision<'root, 'node, T, U>,
    ) -> Result<((&mut T, &mut U), Option<U>), MoveError> {
        let top = self.top_mut();
        let result = f(top.0, top.1);
        match result {
            MoveDecision::Ascend => {
                let (top, old_addl) = self.ascend().ok_or(MoveError::AscendAtRoot)?;
                Ok((top, Some(old_addl)))
            }
            MoveDecision::Stay => Ok((self.top_mut(), None)),
            MoveDecision::Inject(new_top, new_addl) | MoveDecision::Descend(new_top, new_addl) => {
                let new_top: *mut T = new_top;
                self.data.push((new_top, new_addl));
                Ok((self.top_mut(), None))
            }
        }
    }

    /// Return reference to the top element of this stack, forgetting about the stack entirely.
    pub fn into_top(self) -> &'root mut T {
        let ptr = self.data.last().unwrap().0;
        unsafe { &mut *ptr }
    }
}
