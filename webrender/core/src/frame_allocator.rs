/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

///! A custom allocator for memory allocations that have the lifetime of a frame.
///!
///! See also `internal_types::FrameVec`.
///!

use allocator_api2::alloc::{Allocator, AllocError, Layout, Global};

use std::{cell::UnsafeCell, ptr::NonNull, sync::atomic::{AtomicI32, Ordering}};

use crate::{bump_allocator::{BumpAllocator, Stats}, internal_types::{FrameId, FrameVec}};

/// A memory allocator for allocations that have the same lifetime as a built frame.
///
/// A custom allocator is used because:
/// - The frame is created on a thread and dropped on another thread, which causes
///   lock contention in jemalloc.
/// - Since all allocations have a very similar lifetime, we can implement much faster
///   allocation and deallocation with a specialized allocator than can be achieved
///   with a general purpose allocator.
///
/// If the allocator is created using `FrameAllocator::fallback()`, it is not
/// attached to a `FrameMemory` and simply falls back to the global allocator. This
/// should only be used to handle deserialization (for wrench replays) and tests.
///
/// # Safety
///
/// None of the safety restrictions below apply if the allocator is created using
/// `FrameAllocator::fallback`.
///
/// `FrameAllocator` can move between thread if and only if it does so along with
/// the `FrameMemory` it is associated to (if any). The opposite is also true: it
/// is safe to move `FrameMemory` between threads if and only if all live frame
/// allocators associated to it move along with it.
///
/// `FrameAllocator` must be dropped before the `FrameMemory` it is associated to.
///
/// In other words, `FrameAllocator` should only be used for containers that are
/// in the `Frame` data structure and not stored elsewhere. The `Frame` holds on
/// to its `FrameMemory`, allowing it all to be sent from the frame builder thread
/// to the renderer thread together.
///
/// Another way to think of it is that the frame is a large self-referential data
/// structure, holding on to its memory and a large number of containers that
/// point into the memory.
pub struct FrameAllocator {
    // If this pointer is null, fall back to the global allocator.
    inner: *mut FrameInnerAllocator,

    #[cfg(debug_assertions)]
    frame_id: Option<FrameId>,
}

impl FrameAllocator {
    /// Creates a `FrameAllocator` that defaults to the global allocator.
    ///
    /// Should only be used for testing purposes or desrialization in wrench replays.
	pub fn fallback() -> Self {
		FrameAllocator {
			inner: std::ptr::null_mut(),
            #[cfg(debug_assertions)]
            frame_id: None,
        }
	}

    /// Shorthand for creating a FrameVec.
    #[inline]
    pub fn new_vec<T>(self) -> FrameVec<T> {
        FrameVec::new_in(self)
    }

    /// Shorthand for creating a FrameVec.
    #[inline]
    pub fn new_vec_with_capacity<T>(self, cap: usize) -> FrameVec<T> {
        FrameVec::with_capacity_in(cap, self)
    }

    #[inline]
    fn allocate_impl(mem: *mut FrameInnerAllocator, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        unsafe {
            (*mem).live_alloc_count.fetch_add(1, Ordering::Relaxed);
            (*mem).bump.allocate_item(layout)
        }
    }

    #[inline]
    unsafe fn deallocate_impl(mem: *mut FrameInnerAllocator, ptr: NonNull<u8>, layout: Layout) {
        (*mem).live_alloc_count.fetch_sub(1, Ordering::Relaxed);
        (*mem).bump.deallocate_item(ptr, layout)
    }

    #[inline]
    unsafe fn grow_impl(mem: *mut FrameInnerAllocator, ptr: NonNull<u8>, old_layout: Layout, new_layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        (*mem).bump.grow_item(ptr, old_layout, new_layout)
    }

    #[inline]
    unsafe fn shrink_impl(mem: *mut FrameInnerAllocator, ptr: NonNull<u8>, old_layout: Layout, new_layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        (*mem).bump.shrink_item(ptr, old_layout, new_layout)
    }

    #[cold]
    #[inline(never)]
    fn allocate_fallback(layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        Global.allocate(layout)
    }

    #[cold]
    #[inline(never)]
    fn deallocate_fallback(ptr: NonNull<u8>, layout: Layout) {
        unsafe { Global.deallocate(ptr, layout) }
    }

    #[cold]
    #[inline(never)]
    fn grow_fallback(ptr: NonNull<u8>, old_layout: Layout, new_layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        unsafe { Global.grow(ptr, old_layout, new_layout) }
    }

    #[cfg(not(debug_assertions))]
    fn check_frame_id(&self) {}

    #[cfg(debug_assertions)]
    fn check_frame_id(&self) {
        if self.inner.is_null() {
            return;
        }
        unsafe {
            assert_eq!(self.frame_id, (*self.inner).frame_id);
        }
    }
}

impl Clone for FrameAllocator {
    fn clone(&self) -> Self {
        unsafe {
            if let Some(inner) = self.inner.as_mut() {
                // When cloning a `FrameAllocator`, we have to decrement the
                // counter of dropped references in the nner allocator to
                // balance the fact that an extra `FrameAllocator` will be
                // dropped (that hasn't been accounted in `FrameMemory`).
                inner.references_dropped.fetch_sub(1, Ordering::Relaxed);
            }
        }

        FrameAllocator {
            inner: self.inner,
            #[cfg(debug_assertions)]
            frame_id: self.frame_id,
        }
    }
}

impl Drop for FrameAllocator {
    fn drop(&mut self) {
        unsafe {
            if let Some(inner) = self.inner.as_mut() {
                inner.references_dropped.fetch_add(1, Ordering::Release);
            }
        }
    }
}

unsafe impl Send for FrameAllocator {}

unsafe impl Allocator for FrameAllocator {
    #[inline(never)]
	fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        if self.inner.is_null() {
            return FrameAllocator::allocate_fallback(layout);
        }

        self.check_frame_id();

        FrameAllocator::allocate_impl(self.inner, layout)
	}

    #[inline(never)]
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        if self.inner.is_null() {
            return FrameAllocator::deallocate_fallback(ptr, layout);
        }

        self.check_frame_id();

        FrameAllocator::deallocate_impl(self.inner, ptr, layout)
    }

    #[inline(never)]
    unsafe fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout
    ) -> Result<NonNull<[u8]>, AllocError> {
        if self.inner.is_null() {
            return FrameAllocator::grow_fallback(ptr, old_layout, new_layout);
        }

        self.check_frame_id();

        FrameAllocator::grow_impl(self.inner, ptr, old_layout, new_layout)
    }

    #[inline(never)]
    unsafe fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout
    ) -> Result<NonNull<[u8]>, AllocError> {
        if self.inner.is_null() {
            return FrameAllocator::grow_fallback(ptr, old_layout, new_layout);
        }

        self.check_frame_id();

        FrameAllocator::shrink_impl(self.inner, ptr, old_layout, new_layout)
    }
}

#[cfg(feature = "capture")]
impl serde::Serialize for FrameAllocator {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where S: serde::Serializer
	{
		().serialize(serializer)
	}
}

#[cfg(feature = "replay")]
impl<'de> serde::Deserialize<'de> for FrameAllocator {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
		let _ = <() as serde::Deserialize>::deserialize(deserializer)?;
		Ok(FrameAllocator::fallback())
    }
}

/// The default impl is required for Deserialize to work in FrameVec.
/// It's fine to fallback to the global allocator when replaying wrench
/// recording but we don't want to accidentally use `FrameAllocator::default()`
/// in regular webrender usage, so we only implement it when the replay
/// feature is enabled.
#[cfg(feature = "replay")]
impl Default for FrameAllocator {
    fn default() -> Self {
        Self::fallback()
    }
}

/// The backing storage for `FrameAllocator`
///
/// This object is meant to be stored in the built frame and must not be dropped or
/// recycled before all allocations have been deallocated and all `FrameAllocators`
/// have been dropped. In other words, drop or recycle this after dropping the rest
/// of the built frame.
pub struct FrameMemory {
    // Box would be nice but it is not adequate for this purpose because
    // it is "no-alias". So we do it the hard way and manage this pointer
    // manually.

    /// Safety: The pointed `FrameInnerAllocator` must not move or be deallocated
    /// while there are live `FrameAllocator`s pointing to it. This is ensured
    /// by respecting that the `FrameMemory` is dropped last and by the
    /// `FrameInnerAllocator` not being exposed to the outside world.
    /// It is also checked at runtime via the reference count.
    allocator: Option<NonNull<FrameInnerAllocator>>,
    /// The number of `FrameAllocator`s created during the current frame. This is
    /// used to compare aganst the inner allocator's dropped references counter
    /// to check that references have all been dropped before freeing or recycling
    /// the memory.
    references_created: UnsafeCell<i32>,
}

impl FrameMemory {
    /// Creates a fallback FrameMemory that uses the global allocator.
    ///
    /// This should only be used for testing purposes and to handle the
    /// deserialization of webrender recordings.
    #[allow(unused)]
    pub fn fallback() -> Self {
        FrameMemory {
            allocator: None,
            references_created: UnsafeCell::new(0)
        }
    }

    /// # Panics
    ///
    /// A `FrameMemory` must not be dropped until all of the associated
    /// `FrameAllocators` as well as their allocations have been dropped,
    /// otherwise the `FrameMemory::drop` will panic.
    pub fn new() -> Self {
        let layout = Layout::from_size_align(
            std::mem::size_of::<FrameInnerAllocator>(),
            std::mem::align_of::<FrameInnerAllocator>(),
        ).unwrap();

        let uninit_u8 = Global.allocate(layout).unwrap();

        unsafe {
            let allocator: NonNull<FrameInnerAllocator> = uninit_u8.cast();
            allocator.as_ptr().write(FrameInnerAllocator {
                bump: BumpAllocator::new_in(Global),

                live_alloc_count: AtomicI32::new(0),
                references_dropped: AtomicI32::new(0),
                #[cfg(debug_assertions)]
                frame_id: None,
            });

            FrameMemory {
                allocator: Some(allocator),
                references_created: UnsafeCell::new(0),
            }
        }
    }

    /// Create a `FrameAllocator` for the current frame.
    pub fn allocator(&self) -> FrameAllocator {
        if let Some(alloc) = &self.allocator {
            unsafe { *self.references_created.get() += 1 };

            return FrameAllocator {
                inner: alloc.as_ptr(),
                #[cfg(debug_assertions)]
                frame_id: unsafe { alloc.as_ref().frame_id },
            };
        }

        FrameAllocator::fallback()
    }

    /// Shorthand for creating a FrameVec.
    #[inline]
    pub fn new_vec<T>(&self) -> FrameVec<T> {
        FrameVec::new_in(self.allocator())
    }

    /// Shorthand for creating a FrameVec.
    #[inline]
    pub fn new_vec_with_capacity<T>(&self, cap: usize) -> FrameVec<T> {
        FrameVec::with_capacity_in(cap, self.allocator())
    }
    
    /// Panics if there are still live allocations or `FrameAllocator`s.
    pub fn assert_memory_reusable(&self) {
        if let Some(ptr) = self.allocator {
            unsafe {
                // If this assert blows up, it means an allocation is still alive.
                assert_eq!(ptr.as_ref().live_alloc_count.load(Ordering::Acquire), 0);
                // If this assert blows up, it means one or several FrameAllocators
                // from the previous frame are still alive.
                let references_created = *self.references_created.get();
                assert_eq!(ptr.as_ref().references_dropped.load(Ordering::Acquire), references_created);
            }
        }
    }

    /// Must be called at the beginning of each frame before creating any `FrameAllocator`.
    pub fn begin_frame(&mut self, id: FrameId) {
        self.assert_memory_reusable();

        if let Some(mut ptr) = self.allocator {
            unsafe {
                let allocator = ptr.as_mut();
                allocator.references_dropped.store(0, Ordering::Release);
                self.references_created = UnsafeCell::new(0);

                allocator.bump.reset_stats();

                allocator.set_frame_id(id);
            }
        }
    }

    #[allow(unused)]
    pub fn get_stats(&self) -> Stats {
        unsafe {
            self.allocator.map(|ptr| (*ptr.as_ptr()).bump.get_stats()).unwrap_or_else(Stats::default)
        }
    }
}

impl Drop for FrameMemory {
    fn drop(&mut self) {
        self.assert_memory_reusable();

        let layout = Layout::new::<FrameInnerAllocator>();

        unsafe {
            if let Some(ptr) = &mut self.allocator {
                std::ptr::drop_in_place(ptr.as_ptr());
                Global.deallocate(ptr.cast(), layout);
            }
        }
    }
}

unsafe impl Send for FrameMemory {}

#[cfg(feature = "capture")]
impl serde::Serialize for FrameMemory {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where S: serde::Serializer
	{
		().serialize(serializer)
	}
}

#[cfg(feature = "replay")]
impl<'de> serde::Deserialize<'de> for FrameMemory {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
		let _ = <() as serde::Deserialize>::deserialize(deserializer)?;
		Ok(FrameMemory::fallback())
    }
}

struct FrameInnerAllocator {
    bump: BumpAllocator<Global>,

    // Strictly speaking the live allocation and reference count do not need to
    // be atomic if the allocator is used correctly (the thread that
    // allocates/deallocates is also the thread where the allocator is).
    // Since the point of keeping track of the number of live allocations is to
    // check that the allocator is indeed used correctly, we stay on the safe
    // side for now.

    live_alloc_count: AtomicI32,
    /// We count the number of references dropped here and compare it against the
    /// number of references created by the `AllocatorMemory` when we need to check
    /// that the memory can be safely reused or released.
    /// This looks and is very similar to a reference counting scheme (`Arc`). The
    /// main differences are that we don't want the reference count to drive the
    /// lifetime of the allocator (only to check when we require all references to
    /// have been dropped), and we do half as many the atomic operations since we only
    /// count drops and not creations.
    references_dropped: AtomicI32,
    #[cfg(debug_assertions)]
    frame_id: Option<FrameId>,
}

impl FrameInnerAllocator {
    #[cfg(not(debug_assertions))]
    fn set_frame_id(&mut self, _: FrameId) {}

    #[cfg(debug_assertions)]
    fn set_frame_id(&mut self, id: FrameId) {
        self.frame_id = Some(id);
    }
}
