use std::{
    future::Future,
    marker::PhantomData,
    pin::Pin,
    sync::{Mutex, MutexGuard, PoisonError, TryLockError, TryLockResult},
};

pub struct MutexGuardStack<'root, T: ?Sized> {
    /// Ensures this mutrefstack does not exceed the lifetime of its root.
    lifetime: PhantomData<&'root mut T>,
    /// The stack of pointers. Each one borrows from the one prior, except the first which is the `root` and may never be popped.
    /// Note: the `'root` lifetime is a "lie", only used because there's no raw pointer counterpart for `MutexGuard`.
    /// The `MutexGuard`s are not publicly accessible so this is fine.
    data: Vec<MutexGuard<'root, T>>,
}

pub enum MoveDecision<'root, 'this, T: ?Sized> {
    Ascend,
    Stay,
    Descend(&'this Mutex<T>),
    Inject(&'root Mutex<T>),
}

pub enum MoveError {
    AscendAtRoot,
    Poisoned,
    WouldBlock,
}

impl<'root, T: ?Sized> MutexGuardStack<'root, T> {
    /// Create a new MutRefStack from a mutable reference to the root
    /// of a recursive data structure.
    pub fn new(root: &'root Mutex<T>) -> TryLockResult<Self> {
        let root: *const Mutex<T> = root;
        let guard = unsafe { (*root).try_lock() };
        match guard {
            Ok(guard) => Ok(Self {
                lifetime: PhantomData,
                data: vec![guard],
            }),
            Err(TryLockError::Poisoned(guard)) => {
                Err(TryLockError::Poisoned(PoisonError::new(Self {
                    lifetime: PhantomData,
                    data: vec![guard.into_inner()],
                })))
            }
            Err(TryLockError::WouldBlock) => Err(TryLockError::WouldBlock),
        }
    }

    pub fn raw_top_mut(&mut self) -> *mut T {
        let guard: *mut MutexGuard<T> = self.data.last_mut().unwrap();
        unsafe { &mut **guard }
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

    fn handle_trylock_result(
        &mut self,
        guard: TryLockResult<MutexGuard<'root, T>>,
        ignore_poison: bool,
    ) -> Result<&mut T, TryLockError<()>> {
        match (guard, ignore_poison) {
            (Ok(guard), _) => {
                self.data.push(guard);
                Ok(self.top_mut())
            }
            (Err(TryLockError::Poisoned(guard)), true) => {
                self.data.push(guard.into_inner());
                Ok(self.top_mut())
            }
            (Err(TryLockError::Poisoned(_guard)), false) => {
                Err(TryLockError::Poisoned(PoisonError::new(())))
            }
            (Err(TryLockError::WouldBlock), _) => Err(TryLockError::WouldBlock),
        }
    }

    fn handle_move_trylock_result(
        &mut self,
        guard: TryLockResult<MutexGuard<'root, T>>,
        ignore_poison: bool,
    ) -> Result<&mut T, MoveError> {
        match (guard, ignore_poison) {
            (Ok(guard), _) => {
                self.data.push(guard);
                Ok(self.top_mut())
            }
            (Err(TryLockError::Poisoned(guard)), true) => {
                self.data.push(guard.into_inner());
                Ok(self.top_mut())
            }
            (Err(TryLockError::Poisoned(_guard)), false) => Err(MoveError::Poisoned),
            (Err(TryLockError::WouldBlock), _) => Err(MoveError::WouldBlock),
        }
    }

    /// Inject a new reference to the top of the stack. The reference still must live
    /// as long as the root of the stack.
    pub fn inject_top(
        &mut self,
        new_top: &'root Mutex<T>,
        ignore_poison: bool,
    ) -> Result<&mut T, TryLockError<()>> {
        let new_top: *const Mutex<T> = new_top;
        let guard = unsafe { (*new_top).try_lock() };
        self.handle_trylock_result(guard, ignore_poison)
    }

    /// Inject a new reference to the top of the stack. The reference still must live
    /// as long as the root of the stack.
    pub fn inject_with(
        &mut self,
        f: impl FnOnce(&mut T) -> Option<&'root Mutex<T>>,
        ignore_poison: bool,
    ) -> Option<Result<&mut T, TryLockError<()>>> {
        let old_top: *mut T = self.raw_top_mut();
        let new_top: &Mutex<T> = unsafe { f(&mut *old_top)? };
        let new_top: *const Mutex<T> = new_top;
        let guard = unsafe { (*new_top).try_lock() };
        Some(self.handle_trylock_result(guard, ignore_poison))
    }

    /// Descend into the recursive data structure, returning a mutable reference to the new top element.
    /// Rust's borrow checker enforces that the closure cannot inject any lifetime (other than `'static`),
    /// because the closure must work for any lifetime `'node`.
    pub fn descend_with(
        &mut self,
        f: impl for<'node> FnOnce(&'node mut T) -> Option<&'node Mutex<T>>,
        ignore_poison: bool,
    ) -> Option<Result<&mut T, TryLockError<()>>> {
        let old_top: *mut T = self.raw_top_mut();
        let new_top: &Mutex<T> = unsafe { f(&mut *old_top)? };
        let new_top: *const Mutex<T> = new_top;
        let guard = unsafe { (*new_top).try_lock() };
        Some(self.handle_trylock_result(guard, ignore_poison))
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
    pub fn move_with<F>(&mut self, f: F, ignore_poison: bool) -> Result<&mut T, MoveError>
    where
        F: for<'a> FnOnce(&'a mut T) -> MoveDecision<'root, 'a, T>,
    {
        let old_top: *mut T = self.raw_top_mut();
        let result = unsafe { f(&mut *old_top) };
        match result {
            MoveDecision::Ascend => self.ascend().ok_or(MoveError::AscendAtRoot),
            MoveDecision::Stay => Ok(self.top_mut()),
            MoveDecision::Inject(new_top) | MoveDecision::Descend(new_top) => {
                let new_top: *const Mutex<T> = new_top;
                let guard = unsafe { (*new_top).try_lock() };
                self.handle_move_trylock_result(guard, ignore_poison)
            }
        }
    }

    pub async fn move_with_async<F>(
        &mut self,
        f: F,
        ignore_poison: bool,
    ) -> Result<&mut T, MoveError>
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
                let new_top: *const Mutex<T> = new_top;
                let guard = unsafe { (*new_top).try_lock() };
                self.handle_move_trylock_result(guard, ignore_poison)
            }
        }
    }

    /// Return reference to the top element of this stack, forgetting about the stack entirely.
    /// Note that this leaks all `MutexGuard`s above the top.
    pub fn into_top(mut self) -> MutexGuard<'root, T> {
        let ret = self.data.pop().unwrap();
        unsafe {
            // We need to not drop the parent MutexGuards, if any
            self.data.set_len(0);
        }
        ret
    }

    /// Pop all `MutexGuard`s off the stack and go back to the root.
    pub fn to_root(&mut self) -> &mut T {
        for _ in 1..self.data.len() {
            // We need to drop the MutexGuard's in the reverse order.
            // Vec::truncate does not specify drop order, but it's probably wrong anyway.
            self.data.pop();
        }
        self.top_mut()
    }
}

impl<'root, T: ?Sized> Drop for MutexGuardStack<'root, T> {
    fn drop(&mut self) {
        for _ in 0..self.data.len() {
            // We need to drop the MutexGuard's in the reverse order.
            // Vec::truncate does not specify drop order, but it's probably wrong anyway.
            self.data.pop();
        }
    }
}
