//! `SharedBytes`: one immutable byte allocation shared across the C API.
//!
//! A `U8Vec` owns a `Vec`'s allocation and copies it on `clone`; an app that
//! hands azul the bytes of a font it also keeps (for its own PDF export, for
//! saving) held them twice. A `SharedBytes` is a reference to an `Arc<[u8]>`:
//! every clone - the app's, azul's font registry, every face loaded from it -
//! is the same allocation, freed when the last one is dropped. Its bytes are
//! never written through it (there is no mutator), so sharing them is sound.

use alloc::sync::Arc;
use core::{
    fmt,
    hash::{Hash, Hasher},
    mem::ManuallyDrop,
};

use crate::corety::U8Vec;

/// A reference-counted, immutable byte buffer: an `Arc<[u8]>` behind a C
/// layout (its data pointer and length). Cloning it shares the bytes.
#[repr(C)]
pub struct SharedBytes {
    /// `Arc::into_raw` of the `Arc<[u8]>`, as the data pointer; null for a
    /// zeroed handle (no bytes, nothing to free).
    ptr: *const u8,
    /// The number of bytes.
    len: usize,
}

// The bytes are an `Arc<[u8]>` (`Send + Sync`), never written through a handle.
unsafe impl Send for SharedBytes {}
unsafe impl Sync for SharedBytes {}

impl SharedBytes {
    /// Shares `bytes`: no copy.
    #[must_use]
    pub fn from_arc(bytes: Arc<[u8]>) -> Self {
        let len = bytes.len();
        let ptr = Arc::into_raw(bytes).cast::<u8>();
        Self { ptr, len }
    }

    /// The bytes of `bytes`, copied once into the one shared allocation (a
    /// `Vec`'s buffer cannot become an `Arc<[u8]>` without a copy: the counts
    /// sit in front of the bytes). Every clone afterwards shares it.
    #[must_use]
    pub fn create(bytes: U8Vec) -> Self {
        Self::from_arc(Arc::from(bytes.as_ref()))
    }

    /// Another reference to the shared allocation (no copy); an empty one for
    /// a zeroed handle.
    #[must_use]
    pub fn to_arc(&self) -> Arc<[u8]> {
        match self.raw() {
            // Safety: `raw` is the pointer `Arc::into_raw` returned for this
            // handle's reference, which stays alive while `self` does; the
            // `ManuallyDrop` keeps that reference, the clone is a new one.
            Some(raw) => {
                let held = ManuallyDrop::new(unsafe { Arc::from_raw(raw) });
                Arc::clone(&held)
            }
            None => Arc::from(&[][..]),
        }
    }

    /// The bytes.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        if self.ptr.is_null() {
            return &[];
        }
        // Safety: `ptr` / `len` are the data of the `Arc<[u8]>` this handle
        // holds a reference to, alive as long as `self`.
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }

    /// The number of bytes.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether there are no bytes.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// A copy of the bytes as a `U8Vec`.
    #[must_use]
    pub fn copy_to_vec(&self) -> U8Vec {
        U8Vec::from_vec(self.as_slice().to_vec())
    }

    /// Whether `other` shares this handle's allocation (not merely equal bytes).
    #[must_use]
    pub fn shares_with(&self, other: &Self) -> bool {
        !self.ptr.is_null() && core::ptr::eq(self.ptr, other.ptr)
    }

    fn raw(&self) -> Option<*const [u8]> {
        (!self.ptr.is_null()).then(|| core::ptr::slice_from_raw_parts(self.ptr, self.len))
    }
}

impl Clone for SharedBytes {
    fn clone(&self) -> Self {
        match self.raw() {
            Some(_) => Self::from_arc(self.to_arc()),
            None => Self {
                ptr: core::ptr::null(),
                len: 0,
            },
        }
    }
}

impl Drop for SharedBytes {
    fn drop(&mut self) {
        if let Some(raw) = self.raw() {
            // Safety: this handle's reference, given up exactly once.
            drop(unsafe { Arc::from_raw(raw) });
            self.ptr = core::ptr::null();
            self.len = 0;
        }
    }
}

impl Default for SharedBytes {
    fn default() -> Self {
        Self::from_arc(Arc::from(&[][..]))
    }
}

impl From<Arc<[u8]>> for SharedBytes {
    fn from(bytes: Arc<[u8]>) -> Self {
        Self::from_arc(bytes)
    }
}

impl fmt::Debug for SharedBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SharedBytes({} bytes)", self.len)
    }
}

impl PartialEq for SharedBytes {
    fn eq(&self, other: &Self) -> bool {
        self.shares_with(other) || self.as_slice() == other.as_slice()
    }
}

impl Eq for SharedBytes {}

impl PartialOrd for SharedBytes {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SharedBytes {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.as_slice().cmp(other.as_slice())
    }
}

impl Hash for SharedBytes {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

/// Guards of the 2026-11-21 audit, area A8 (`audit/A8.md`, A8-26).
#[cfg(test)]
mod audit_a8_tests {
    use super::*;

    /// A clone and the `Arc` handed out share the caller's allocation; the
    /// bytes outlive every handle but the last; a zeroed handle is empty and
    /// frees nothing.
    #[test]
    fn clones_share_one_allocation_that_outlives_the_first_handle() {
        let arc: Arc<[u8]> = Arc::from(&b"font bytes"[..]);
        let weak = Arc::downgrade(&arc);
        let first = SharedBytes::from_arc(Arc::clone(&arc));
        drop(arc);
        let second = first.clone();
        let held = first.to_arc();
        assert!(first.shares_with(&second));
        assert!(core::ptr::eq(held.as_ptr(), first.as_slice().as_ptr()));
        assert_eq!(Arc::strong_count(&held), 3);
        drop(first);
        assert_eq!(second.as_slice(), b"font bytes", "outlives the first handle");
        drop(second);
        assert_eq!(&held[..], b"font bytes");
        drop(held);
        assert!(weak.upgrade().is_none(), "freed with the last handle");

        let zeroed = SharedBytes {
            ptr: core::ptr::null(),
            len: 0,
        };
        assert!(zeroed.as_slice().is_empty() && zeroed.to_arc().is_empty());
        drop(zeroed.clone());
        let copied = SharedBytes::create(U8Vec::from_vec(b"abc".to_vec()));
        assert_eq!(copied.as_slice(), b"abc");
        assert_eq!(copied.copy_to_vec().as_ref(), b"abc");
    }
}
