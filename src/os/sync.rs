//! Semaphore and mutex

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::ptr::{self, null};
use core::sync::atomic::{AtomicI32, Ordering::Relaxed};

use super::{port, running, with_kernel, State, FOREVER};

/// Counting semaphore. Can live directly in a static: `static S: Semaphore = Semaphore::new(0);`
pub struct Semaphore {
    count: AtomicI32, // only read/written inside with_kernel; atomic just for interior mutability
}

impl Semaphore {
    pub const fn new(initial: i32) -> Self {
        Semaphore { count: AtomicI32::new(initial) }
    }

    /// Wait until the semaphore is acquired. Task context only.
    pub fn wait(&self) {
        self.wait_timeout(FOREVER);
    }

    /// Don't wait: returns true if acquired. Callable from tasks and interrupts.
    pub fn try_wait(&self) -> bool {
        self.wait_timeout(0)
    }

    /// Wait at most `ticks` ticks; returns true if acquired, false on timeout.
    pub fn wait_timeout(&self, ticks: u32) -> bool {
        enum Outcome {
            Done(bool),
            Blocked(usize),
        }

        let outcome = with_kernel(|k| {
            let c = self.count.load(Relaxed);
            if c > 0 {
                self.count.store(c - 1, Relaxed);
                return Outcome::Done(true);
            }
            if ticks == 0 {
                return Outcome::Done(false);
            }
            k.block_current(self, ticks);
            Outcome::Blocked(k.current)
        });

        match outcome {
            Outcome::Done(got) => got,
            // We were woken up: either post handed the count directly to us, or we timed out
            Outcome::Blocked(me) => with_kernel(|k| !k.tasks[me].timed_out),
        }
    }

    /// Release the semaphore. Callable from tasks and interrupts.
    pub fn post(&self) {
        with_kernel(|k| {
            let me: *const Semaphore = self;

            // Find the highest-priority task waiting on this semaphore and hand the count
            // directly to it (no count++)
            let mut waiter: Option<usize> = None;
            for (i, t) in k.tasks[..k.count].iter().enumerate() {
                if t.state == State::Blocked
                    && ptr::eq(t.waiting_on, me)
                    && waiter.is_none_or(|w| t.prio > k.tasks[w].prio)
                {
                    waiter = Some(i);
                }
            }

            match waiter {
                Some(i) => {
                    let t = &mut k.tasks[i];
                    t.waiting_on = null();
                    t.timed_wait = false;
                    t.state = State::Ready;
                    if running() && k.tasks[i].prio > k.tasks[k.current].prio {
                        port::pend_switch(); // the woken task is more important: preempt now
                    }
                }
                None => {
                    self.count.fetch_add(1, Relaxed);
                }
            }
        });
    }
}

/// A semaphore-based mutex protecting a value of type `T`.
///
/// `lock()` returns a guard that unlocks when it goes out of scope — you cannot forget to unlock,
/// and you cannot touch the data without holding the lock. Task context only.
/// (No priority inheritance, so priority inversion is possible.)
pub struct Mutex<T> {
    sem: Semaphore,
    data: UnsafeCell<T>,
}

// SAFETY: data is only accessible through a MutexGuard, which holds the lock
unsafe impl<T: Send> Sync for Mutex<T> {}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self {
        Mutex { sem: Semaphore::new(1), data: UnsafeCell::new(value) }
    }

    pub fn lock(&self) -> MutexGuard<'_, T> {
        self.sem.wait();
        MutexGuard { mutex: self }
    }
}

pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.sem.post();
    }
}
