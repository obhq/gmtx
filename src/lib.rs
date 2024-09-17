//! Mutex that grant exclusive access to a group of members.
//!
//! The [`std::sync::Mutex`] and the related types are prone to deadlock when using on a multiple
//! struct fields like this:
//!
//! ```
//! use std::sync::Mutex;
//!
//! pub struct Foo {
//!     field1: Mutex<()>,
//!     field2: Mutex<()>,
//! }
//! ```
//!
//! The order to acquire the lock must be the same everywhere otherwise the deadlock is possible.
//! Maintaining the lock order manually are cumbersome task so we invent this crate to handle this
//! instead.
//!
//! How this crate are working is simple. Any locks on any [`Gutex`] will lock the same mutex in the
//! group, which mean there are only one mutex in the group. It have the same effect as the
//! following code:
//!
//! ```
//! use std::sync::Mutex;
//!
//! pub struct Foo {
//!     data: Mutex<Data>,
//! }
//!
//! struct Data {
//!     field1: (),
//!     field2: (),
//! }
//! ```
//!
//! The bonus point of [`Gutex`] is it will allow recursive lock for read-only access so you will
//! never end up deadlock yourself. This read-only access is per [`Gutex`]. It will panic if you try
//! to acquire write access while the readers are still active the same as [`std::cell::RefCell`].
use std::cell::UnsafeCell;
use std::sync::Arc;

pub use self::group::*;
pub use self::guard::*;

mod group;
mod guard;

/// Member of a [`GutexGroup`].
///
/// Either lock on this type will lock the same mutex in the group. Lock on the group always grant
/// an exclusive access to the whole group. Let's say thread A call [`Gutex::read()`] then thread B
/// try to call this method on the same group. The result is thread B will wait for thread A to
/// unlock the group.
#[derive(Debug)]
pub struct Gutex<T> {
    group: Arc<GutexGroup>,
    active: UnsafeCell<usize>,
    value: UnsafeCell<T>,
}

impl<T> Gutex<T> {
    /// Returns a mutable reference to the underlying data.
    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    /// Locks this [`Gutex`] with read-only access.
    ///
    /// Multiple read-only accesses can be taken out at the same time.
    ///
    /// # Panics
    /// If there are an active write access to this [`Gutex`].
    pub fn read(&self) -> GutexReadGuard<T> {
        // Check if there are an active writer.
        let lock = self.group.lock();
        let active = self.active.get();

        unsafe {
            if *active == usize::MAX {
                panic!("attempt to acquire the read lock while there are an active write lock");
            } else if *active == (usize::MAX - 1) {
                // This should never happen because stack overflow should be triggering first.
                panic!("maximum number of active readers has been reached");
            }

            *active += 1;
        }

        GutexReadGuard::new(lock, self)
    }

    /// Locks this [`Gutex`] with write access.
    ///
    /// # Panics
    /// If there are any active reader or writer.
    pub fn write(&self) -> GutexWriteGuard<T> {
        // Check if there are active reader or writer.
        let lock = self.group.lock();
        let active = self.active.get();

        // SAFETY: This is safe because we own the lock that protect both active and value.
        unsafe {
            if *active != 0 {
                panic!(
                    "attempt to acquire the write lock while there are an active reader or writer"
                );
            }

            *active = usize::MAX;

            GutexWriteGuard::new(lock, active, self.value.get())
        }
    }
}

unsafe impl<T: Send> Send for Gutex<T> {}
unsafe impl<T: Send> Sync for Gutex<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;
    use std::time::Duration;

    #[test]
    fn group_lock() {
        let b = Arc::new(Barrier::new(2));
        let v = Arc::new(GutexGroup::new().spawn(0));
        let mut l = v.write();
        let t = std::thread::spawn({
            let b = b.clone();
            let v = v.clone();

            move || {
                // Wait for parent thread.
                let mut l = v.write();

                b.wait();

                assert_eq!(*l, 1);

                // Notify the parent thread.
                std::thread::sleep(Duration::from_secs(1));

                *l = 2;
            }
        });

        // Notify the inner thread.
        *l = 1;
        drop(l);

        // Wait for the inner thread value.
        b.wait();

        assert_eq!(*v.read(), 2);

        t.join().unwrap();
    }
}
