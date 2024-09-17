use crate::{GroupGuard, Gutex};
use std::fmt::{Display, Formatter};
use std::ops::{Deref, DerefMut};

/// RAII structure used to release the shared read access of a lock when dropped.
#[derive(Debug)]
pub struct GutexReadGuard<'a, T> {
    #[allow(dead_code)]
    lock: GroupGuard<'a>,
    mtx: &'a Gutex<T>,
}

impl<'a, T> GutexReadGuard<'a, T> {
    pub(crate) fn new(lock: GroupGuard<'a>, mtx: &'a Gutex<T>) -> Self {
        Self { lock, mtx }
    }
}

impl<'a, T> Drop for GutexReadGuard<'a, T> {
    fn drop(&mut self) {
        unsafe { *self.mtx.active.get() -= 1 };
    }
}

impl<'a, T> Deref for GutexReadGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.mtx.value.get() }
    }
}

impl<'a, T: Display> Display for GutexReadGuard<'a, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.deref().fmt(f)
    }
}

unsafe impl<'a, T: Sync> Sync for GutexReadGuard<'a, T> {}

/// RAII structure used to release the exclusive write access of a lock when dropped.
#[derive(Debug)]
pub struct GutexWriteGuard<'a, T> {
    #[allow(dead_code)]
    lock: GroupGuard<'a>,
    active: *mut usize,
    value: *mut T,
}

impl<'a, T> GutexWriteGuard<'a, T> {
    /// # Safety
    /// `active` and `value` must be protected by `lock`.
    pub(crate) unsafe fn new(lock: GroupGuard<'a>, active: *mut usize, value: *mut T) -> Self {
        Self {
            lock,
            active,
            value,
        }
    }
}

impl<'a, T> Drop for GutexWriteGuard<'a, T> {
    fn drop(&mut self) {
        unsafe { *self.active = 0 };
    }
}

impl<'a, T> Deref for GutexWriteGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.value }
    }
}

impl<'a, T> DerefMut for GutexWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.value }
    }
}

impl<'a, T: Display> Display for GutexWriteGuard<'a, T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.deref().fmt(f)
    }
}

unsafe impl<'a, T: Sync> Sync for GutexWriteGuard<'a, T> {}
