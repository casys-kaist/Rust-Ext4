// Copyright 2021 Computer Architecture and Systems Lab
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Simple Lock implementations.
//!
//! This might be diverse into the SpinLock and MutexLock.
use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

/// Trait that defines the operations on lock.
pub trait Dreamer
where
    Self: Send + Sync + Default,
{
    const DEFAULT: Self;

    type HookAux;

    /// Prehook that runs before try acquiring the lock.
    fn prehook(&self) -> Self::HookAux;
    /// Hook that runs after acquire the lock.
    fn acquire_hook(&self, #[cfg(feature = "track_lock")] _l: &core::panic::Location<'static>);
    /// Hook that runs after releasing the lock.
    fn release_hook(&self, #[cfg(feature = "track_lock")] _l: &core::panic::Location<'static>);
    /// Actions for sleeping.
    fn sleeping(&self, locked: &AtomicBool);
    /// Actions for waking-up.
    fn waking_up(&self);
}

/// A mutual exclusion primitive useful for protecting shared data
#[derive(Debug)]
pub struct SimpleLock<T, D>
where
    T: ?Sized + Send,
    D: Dreamer,
{
    locked: AtomicBool,
    dreamer: D,
    data: UnsafeCell<T>,
}

/// An RAII implementation of a "scoped lock" of a simple lock. When this
/// structure is dropped (falls out of scope), the lock will be unlocked.
///
/// The data protected by the spin lock can be accessed through this guard via
/// its [`Deref`] and [`DerefMut`] implementations.
///
/// This structure is created by the [`lock`](SimpleLock::lock) and
/// [`try_lock`](SimpleLock::try_lock) methods on [`SimpleLock`].
pub struct SimpleLockGuard<'a, T, D>
where
    T: ?Sized + Send,
    T: 'a,
    D: Dreamer,
{
    lock: &'a SimpleLock<T, D>,
    data: &'a mut T,
    _h: D::HookAux,
}

impl<T, D> SimpleLock<T, D>
where
    T: Send,
    D: Dreamer,
{
    /// Creates a new spin lock in an unlocked state ready for use.
    #[inline(always)]
    pub const fn new(data: T) -> SimpleLock<T, D> {
        SimpleLock {
            locked: AtomicBool::new(false),
            dreamer: D::DEFAULT,
            data: UnsafeCell::new(data),
        }
    }

    #[doc(hidden)]
    fn acquire(&self) {
        loop {
            self.dreamer.sleeping(&self.locked);
            if self
                .locked
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Acquire)
                .is_ok()
            {
                break;
            }
        }
    }

    /// Acquires a spin lock, blocking the current thread until it is able to do
    /// so.
    ///
    /// This function will block the kthread until it is available to acquire
    /// the spin lock. Upon returning, the thread is the only thread with the
    /// lock held. An RAII guard is returned to allow scoped unlock of the lock.
    /// When the guard goes out of scope, the spin lock will be unlocked.
    ///
    /// The exact behavior on locking a spin lock in the thread which already
    /// holds the lock is left unspecified. However, this function will not
    /// return on the second call (it might panic or deadlock, for example).
    #[track_caller]
    pub fn lock(&self) -> SimpleLockGuard<T, D> {
        let h = self.dreamer.prehook();

        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Acquire)
            .is_ok()
        {
            self.dreamer.acquire_hook(
                #[cfg(feature = "track_lock")]
                core::panic::Location::caller(),
            );
            return SimpleLockGuard {
                lock: self,
                data: unsafe { &mut *self.data.get() },
                _h: h,
            };
        }

        self.acquire();

        self.dreamer.acquire_hook(
            #[cfg(feature = "track_lock")]
            core::panic::Location::caller(),
        );
        SimpleLockGuard {
            lock: self,
            data: unsafe { &mut *self.data.get() },
            _h: h,
        }
    }

    /// Attempts to acquire this lock.
    ///
    /// If the lock could not be acquired at this time, then Err is returned.
    /// Otherwise, an RAII guard is returned. The lock will be unlocked when the
    /// guard is dropped.
    ///
    /// This function does not block.
    #[track_caller]
    pub fn try_lock(&self) -> Result<SimpleLockGuard<T, D>, crate::WouldBlock> {
        let h = self.dreamer.prehook();
        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Acquire)
            .is_ok()
        {
            self.dreamer.acquire_hook(
                #[cfg(feature = "track_lock")]
                core::panic::Location::caller(),
            );
            Ok(SimpleLockGuard {
                lock: self,
                data: unsafe { &mut *self.data.get() },
                _h: h,
            })
        } else {
            Err(crate::WouldBlock)
        }
    }

    /// Takes the mutable reference of the value without checking the value is locked.
    ///
    /// # Safety
    /// This is unsafe.
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn steal(&self) -> &mut T {
        &mut *self.data.get()
    }
}

unsafe impl<T, D> Sync for SimpleLock<T, D>
where
    T: ?Sized + Send,
    D: Dreamer,
{
}
unsafe impl<T, D> Send for SimpleLock<T, D>
where
    T: ?Sized + Send,
    D: Dreamer,
{
}

impl<'a, T, D> Deref for SimpleLockGuard<'a, T, D>
where
    T: Send + ?Sized,
    D: Dreamer,
{
    type Target = T;
    fn deref(&self) -> &T {
        &*self.data
    }
}

impl<'a, T, D> DerefMut for SimpleLockGuard<'a, T, D>
where
    T: Send + ?Sized,
    D: Dreamer,
{
    fn deref_mut(&mut self) -> &mut T {
        &mut *self.data
    }
}

impl<'a, T, D> Drop for SimpleLockGuard<'a, T, D>
where
    T: Send + ?Sized,
    D: Dreamer,
{
    #[track_caller]
    fn drop(&mut self) {
        debug_assert!(self.lock.locked.load(Ordering::Acquire));
        self.lock.locked.store(false, Ordering::Release);
        self.lock.dreamer.waking_up();
        self.lock.dreamer.release_hook(
            #[cfg(feature = "track_lock")]
            core::panic::Location::caller(),
        );
    }
}
