use crate::Gutex;
use std::cell::UnsafeCell;
use std::io::Error;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// Group of [`Gutex`].
#[derive(Debug)]
pub struct GutexGroup {
    owning: ThreadId,
    active: UnsafeCell<usize>,
}

impl GutexGroup {
    /// Create a new group.
    ///
    /// All members spawn within the same group will share a single mutex.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            owning: ThreadId::new(0),
            active: UnsafeCell::new(0),
        })
    }

    /// Spawn a new member for this group.
    pub fn spawn<T>(self: &Arc<Self>, value: T) -> Gutex<T> {
        Gutex {
            group: self.clone(),
            active: UnsafeCell::new(0),
            value: UnsafeCell::new(value),
        }
    }

    #[inline(never)]
    pub(crate) fn lock(&self) -> GroupGuard {
        // Check if the calling thread already own the lock.
        let current = Self::current_thread();

        if current == self.owning.load(Ordering::Relaxed) {
            // SAFETY: This is safe because the current thread own the lock.
            return unsafe { GroupGuard::new(self) };
        }

        // Acquire the lock.
        while let Err(owning) =
            self.owning
                .compare_exchange(0, current, Ordering::Acquire, Ordering::Relaxed)
        {
            // Wait for the lock to unlock.
            unsafe { Self::wait_unlock(self.owning.as_ptr(), owning) };
        }

        // SAFETY: This is safe because the current thread acquire the lock successfully by the
        // above compare_exchange().
        unsafe { GroupGuard::new(self) }
    }

    #[cfg(target_os = "linux")]
    fn current_thread() -> i32 {
        unsafe { libc::gettid() }
    }

    #[cfg(target_os = "macos")]
    fn current_thread() -> u64 {
        let mut id = 0;
        assert_eq!(unsafe { libc::pthread_threadid_np(0, &mut id) }, 0);
        id
    }

    #[cfg(target_os = "windows")]
    fn current_thread() -> u32 {
        unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() }
    }

    #[cfg(target_os = "linux")]
    unsafe fn wait_unlock(addr: *mut i32, owning: i32) {
        use libc::{syscall, SYS_futex, EAGAIN, FUTEX_PRIVATE_FLAG, FUTEX_WAIT};

        if unsafe { syscall(SYS_futex, addr, FUTEX_WAIT | FUTEX_PRIVATE_FLAG, owning, 0) } < 0 {
            let e = Error::last_os_error();

            if e.raw_os_error().unwrap() != EAGAIN {
                panic!("FUTEX_WAIT failed: {e}");
            }
        }
    }

    #[cfg(target_os = "macos")]
    unsafe fn wait_unlock(addr: *mut u64, owning: u64) {
        use ulock_sys::__ulock_wait;
        use ulock_sys::darwin19::UL_COMPARE_AND_WAIT64;

        if __ulock_wait(UL_COMPARE_AND_WAIT64, addr.cast(), owning, 0) != 0 {
            panic!("__ulock_wait() failed: {}", Error::last_os_error());
        }
    }

    #[cfg(target_os = "windows")]
    unsafe fn wait_unlock(addr: *mut u32, owning: u32) {
        use windows_sys::Win32::System::Threading::{WaitOnAddress, INFINITE};

        if unsafe { WaitOnAddress(addr.cast(), &owning as *const u32 as _, 4, INFINITE) } == 0 {
            panic!("WaitOnAddress() failed: {}", Error::last_os_error());
        }
    }

    #[cfg(target_os = "linux")]
    unsafe fn wake_one(addr: *mut i32) {
        use libc::{syscall, SYS_futex, FUTEX_PRIVATE_FLAG, FUTEX_WAKE};

        if unsafe { syscall(SYS_futex, addr, FUTEX_WAKE | FUTEX_PRIVATE_FLAG, 1) } < 0 {
            panic!("FUTEX_WAKE failed: {}", Error::last_os_error());
        }
    }

    #[cfg(target_os = "macos")]
    unsafe fn wake_one(addr: *mut u64) {
        use libc::ENOENT;
        use ulock_sys::__ulock_wake;
        use ulock_sys::darwin19::UL_COMPARE_AND_WAIT64;

        if __ulock_wake(UL_COMPARE_AND_WAIT64, addr.cast(), 0) != 0 {
            // __ulock_wake will return ENOENT if no other threads being waiting on the address.
            let e = Error::last_os_error();

            if e.raw_os_error().unwrap() != ENOENT {
                panic!("__ulock_wake() failed: {e}");
            }
        }
    }

    #[cfg(target_os = "windows")]
    unsafe fn wake_one(addr: *mut u32) {
        use windows_sys::Win32::System::Threading::WakeByAddressSingle;

        unsafe { WakeByAddressSingle(addr.cast()) };
    }
}

unsafe impl Send for GutexGroup {}
unsafe impl Sync for GutexGroup {}

/// An RAII object used to release a lock on [`GutexGroup`]. This type cannot be send because it
/// will cause data race on the group when dropping if more than one [`GroupGuard`] are active.
#[derive(Debug)]
pub(crate) struct GroupGuard<'a> {
    group: &'a GutexGroup,
    phantom: PhantomData<Rc<()>>, // For !Send and !Sync.
}

impl<'a> GroupGuard<'a> {
    /// # Safety
    /// The group must be locked by the calling thread with no active references to any of its
    /// field.
    unsafe fn new(group: &'a GutexGroup) -> Self {
        *group.active.get() += 1;

        Self {
            group,
            phantom: PhantomData,
        }
    }
}

impl<'a> Drop for GroupGuard<'a> {
    #[inline(never)]
    fn drop(&mut self) {
        // Decrease the active lock.
        unsafe {
            let active = self.group.active.get();

            *active -= 1;

            if *active != 0 {
                return;
            }
        }

        // Release the lock.
        self.group.owning.store(0, Ordering::Release);

        unsafe { GutexGroup::wake_one(self.group.owning.as_ptr()) };
    }
}

#[cfg(target_os = "linux")]
type ThreadId = std::sync::atomic::AtomicI32;

#[cfg(target_os = "macos")]
type ThreadId = std::sync::atomic::AtomicU64;

#[cfg(target_os = "windows")]
type ThreadId = std::sync::atomic::AtomicU32;
