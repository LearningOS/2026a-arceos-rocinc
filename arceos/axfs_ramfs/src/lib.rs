//! RAM filesystem used by [ArceOS](https://github.com/arceos-org/arceos).
//!
//! The implementation is based on [`axfs_vfs`].

#![allow(unused_attributes, dead_code, unused_imports)]
#![cfg_attr(not(test), no_std)]

extern crate alloc;

mod dir;
mod file;

pub(crate) mod sync {
    use core::cell::UnsafeCell;
    use core::hint::spin_loop;
    use core::ops::{Deref, DerefMut};
    use core::sync::atomic::{AtomicUsize, Ordering};

    pub struct RwLock<T> {
        state: AtomicUsize,
        data: UnsafeCell<T>,
    }

    pub struct RwLockReadGuard<'a, T> {
        lock: &'a RwLock<T>,
    }

    pub struct RwLockWriteGuard<'a, T> {
        lock: &'a RwLock<T>,
    }

    unsafe impl<T: Send> Send for RwLock<T> {}
    unsafe impl<T: Send + Sync> Sync for RwLock<T> {}

    impl<T> RwLock<T> {
        pub const fn new(data: T) -> Self {
            Self {
                state: AtomicUsize::new(0),
                data: UnsafeCell::new(data),
            }
        }

        pub fn read(&self) -> RwLockReadGuard<'_, T> {
            loop {
                let s = self.state.load(Ordering::Relaxed);
                if s != usize::MAX
                    && self
                        .state
                        .compare_exchange_weak(s, s + 1, Ordering::Acquire, Ordering::Relaxed)
                        .is_ok()
                {
                    return RwLockReadGuard { lock: self };
                }
                spin_loop();
            }
        }

        pub fn write(&self) -> RwLockWriteGuard<'_, T> {
            while self
                .state
                .compare_exchange_weak(0, usize::MAX, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                spin_loop();
            }
            RwLockWriteGuard { lock: self }
        }
    }

    impl<T> Deref for RwLockReadGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            unsafe { &*self.lock.data.get() }
        }
    }

    impl<T> Drop for RwLockReadGuard<'_, T> {
        fn drop(&mut self) {
            self.lock.state.fetch_sub(1, Ordering::Release);
        }
    }

    impl<T> Deref for RwLockWriteGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            unsafe { &*self.lock.data.get() }
        }
    }

    impl<T> DerefMut for RwLockWriteGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut T {
            unsafe { &mut *self.lock.data.get() }
        }
    }

    impl<T> Drop for RwLockWriteGuard<'_, T> {
        fn drop(&mut self) {
            self.lock.state.store(0, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests;

pub use self::dir::DirNode;
pub use self::file::FileNode;

use self::sync::RwLock;
use alloc::sync::Arc;
use axfs_vfs::{VfsNodeRef, VfsOps, VfsResult};

/// A RAM filesystem that implements [`axfs_vfs::VfsOps`].
pub struct RamFileSystem {
    parent: RwLock<Option<VfsNodeRef>>,
    root: Arc<DirNode>,
}

impl RamFileSystem {
    /// Create a new instance.
    pub fn new() -> Self {
        Self {
            parent: RwLock::new(None),
            root: DirNode::new(None),
        }
    }

    /// Returns the root directory node in [`Arc<DirNode>`](DirNode).
    pub fn root_dir_node(&self) -> Arc<DirNode> {
        self.root.clone()
    }
}

impl VfsOps for RamFileSystem {
    fn mount(&self, _path: &str, mount_point: VfsNodeRef) -> VfsResult {
        if let Some(parent) = mount_point.parent() {
            self.root
                .set_parent(Some(self.parent.write().get_or_insert(parent)));
        } else {
            self.root.set_parent(None);
        }
        Ok(())
    }

    fn root_dir(&self) -> VfsNodeRef {
        self.root.clone()
    }
}

impl Default for RamFileSystem {
    fn default() -> Self {
        Self::new()
    }
}
