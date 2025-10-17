/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

 use std::{
    alloc::Layout,
    ptr::{self, NonNull},
};

use allocator_api2::alloc::{AllocError, Allocator};

const CHUNK_ALIGNMENT: usize = 32;
const DEFAULT_CHUNK_SIZE: usize = 128 * 1024;

/// A simple bump allocator, sub-allocating from fixed size chunks that are provided
/// by a parent allocator.
///
/// If an allocation is larger than the chunk size, a chunk sufficiently large to contain
/// the allocation is added.
pub struct BumpAllocator<A: Allocator> {
    /// The chunk we are currently allocating from.
    current_chunk: NonNull<Chunk>,
    /// The defaut size for chunks.
    chunk_size: usize,
    /// For debugging.
    allocation_count: i32,
    /// the allocator that provides the chunks.
    parent_allocator: A,

    stats: Stats,
}

impl<A: Allocator> BumpAllocator<A> {
    pub fn new_in(parent_allocator: A) -> Self {
        Self::with_chunk_size_in(DEFAULT_CHUNK_SIZE, parent_allocator)
    }

    pub fn with_chunk_size_in(chunk_size: usize, parent_allocator: A) -> Self {
        let mut stats = Stats::default();
        stats.chunks = 1;
        stats.reserved_bytes += chunk_size;
        BumpAllocator {
            current_chunk: Chunk::allocate_chunk(
                chunk_size,
                None,
                &parent_allocator
            ).unwrap(),
            chunk_size,
            parent_allocator,
            allocation_count: 0,

            stats,
        }
    }

    pub fn get_stats(&mut self) -> Stats {
        self.stats.chunk_utilization = self.stats.chunks as f32 - 1.0 + Chunk::utilization(self.current_chunk);
        self.stats
    }

    pub fn reset_stats(&mut self) {
        let chunks = self.stats.chunks;
        let reserved_bytes = self.stats.reserved_bytes;
        self.stats = Stats::default();
        self.stats.chunks = chunks;
        self.stats.reserved_bytes = reserved_bytes;
    }

    pub fn allocate_item(&mut self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.stats.allocations += 1;
        self.stats.allocated_bytes += layout.size();

        if let Ok(alloc) = Chunk::allocate_item(self.current_chunk, layout) {
            self.allocation_count += 1;
            return Ok(alloc);
        }

        self.alloc_chunk(layout.size())?;

        match Chunk::allocate_item(self.current_chunk, layout) {
            Ok(alloc) => {
                self.allocation_count += 1;
                    return Ok(alloc);
            }
            Err(_) => {
                return Err(AllocError);
            }
        }
    }

    pub fn deallocate_item(&mut self, ptr: NonNull<u8>, layout: Layout) {
        self.stats.deallocations += 1;

        if Chunk::contains_item(self.current_chunk, ptr) {
            unsafe { Chunk::deallocate_item(self.current_chunk, ptr, layout); }
        }

        self.allocation_count -= 1;
        debug_assert!(self.allocation_count >= 0);
    }

    pub unsafe fn grow_item(&mut self, ptr: NonNull<u8>, old_layout: Layout, new_layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        debug_assert!(
            new_layout.size() >= old_layout.size(),
            "`new_layout.size()` must be greater than or equal to `old_layout.size()`"
        );

        self.stats.reallocations += 1;

        if Chunk::contains_item(self.current_chunk, ptr) {
            if let Ok(alloc) = Chunk::grow_item(self.current_chunk, ptr, old_layout, new_layout) {
                self.stats.in_place_reallocations += 1;
                return Ok(alloc);
            }
        }

        let new_alloc = if let Ok(alloc) = Chunk::allocate_item(self.current_chunk, new_layout) {
            alloc
        } else {
            self.alloc_chunk(new_layout.size())?;
            Chunk::allocate_item(self.current_chunk, new_layout).map_err(|_| AllocError)?
        };

        self.stats.reallocated_bytes += old_layout.size();

        unsafe {
            ptr::copy_nonoverlapping(ptr.as_ptr(), new_alloc.as_ptr().cast(), old_layout.size());
        }

        Ok(new_alloc)
    }

    pub unsafe fn shrink_item(&mut self, ptr: NonNull<u8>, old_layout: Layout, new_layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        debug_assert!(
            new_layout.size() <= old_layout.size(),
            "`new_layout.size()` must be smaller than or equal to `old_layout.size()`"
        );

        if Chunk::contains_item(self.current_chunk, ptr) {
            return unsafe { Ok(Chunk::shrink_item(self.current_chunk, ptr, old_layout, new_layout)) };
        }

        // Can't actually shrink, so return the full range of the previous allocation.
        Ok(NonNull::slice_from_raw_parts(ptr, old_layout.size()))
    }

    fn alloc_chunk(&mut self, item_size: usize) -> Result<(), AllocError> {
        let chunk_size = self.chunk_size.max(align(item_size, CHUNK_ALIGNMENT) + CHUNK_ALIGNMENT);
        self.stats.reserved_bytes += chunk_size;
        let chunk = Chunk::allocate_chunk(
            chunk_size,
            None,
            &self.parent_allocator
        )?;

        unsafe {
            (*chunk.as_ptr()).previous = Some(self.current_chunk);
        }
        self.current_chunk = chunk;

        self.stats.chunks += 1;

        Ok(())
    }
}

impl<A: Allocator> Drop for BumpAllocator<A> {
    fn drop(&mut self) {
        assert!(self.allocation_count == 0);
        let mut iter = Some(self.current_chunk);
        while let Some(chunk) = iter {
            iter = unsafe { (*chunk.as_ptr()).previous };
            Chunk::deallocate_chunk(chunk, &self.parent_allocator)
        }
    }
}

/// A Contiguous buffer of memory holding multiple sub-allocaions.
pub struct Chunk {
    previous: Option<NonNull<Chunk>>,
    /// Offset of the next allocation
    cursor: *mut u8,
    /// Points to the first byte after the chunk's buffer.
    chunk_end: *mut u8,
    /// Size of the chunk
    size: usize,
}

impl Chunk {
    pub fn allocate_chunk(
        size: usize,
        previous: Option<NonNull<Chunk>>,
        allocator: &dyn Allocator,
    ) -> Result<NonNull<Self>, AllocError> {
        assert!(size < usize::MAX / 2);

        let layout = match Layout::from_size_align(size, CHUNK_ALIGNMENT) {
            Ok(layout) => layout,
            Err(_) => {
                return Err(AllocError);
            }
        };

        let alloc = allocator.allocate(layout)?;
        let chunk: NonNull<Chunk> = alloc.cast();
        let chunk_start: *mut u8 = alloc.cast().as_ptr();

        unsafe {
            let chunk_end = chunk_start.add(size);
            let cursor = chunk_start.add(CHUNK_ALIGNMENT);
            ptr::write(
                chunk.as_ptr(),
                Chunk {
                    previous,
                    chunk_end,
                    cursor,
                    size,
                },
            );
        }

        Ok(chunk)
    }

    pub fn deallocate_chunk(this: NonNull<Chunk>, allocator: &dyn Allocator) {
        let size = unsafe { (*this.as_ptr()).size };
        let layout = Layout::from_size_align(size, CHUNK_ALIGNMENT).unwrap();

        unsafe {
            allocator.deallocate(this.cast(), layout);
        }
    }

    pub fn allocate_item(this: NonNull<Chunk>, layout: Layout) -> Result<NonNull<[u8]>, ()> {
        // Common wisdom would be to always bump address downward (https://fitzgeraldnick.com/2019/11/01/always-bump-downwards.html).
        // However, bump allocation does not show up in profiles with the current workloads
        // so we can keep things simple for now.
        debug_assert!(CHUNK_ALIGNMENT % layout.align() == 0);
        debug_assert!(layout.align() > 0);
        debug_assert!(layout.align().is_power_of_two());

        let size = align(layout.size(), CHUNK_ALIGNMENT);

        unsafe {
            let cursor = (*this.as_ptr()).cursor;
            let end = (*this.as_ptr()).chunk_end;
            let available_size = end.offset_from(cursor);

            if size as isize > available_size {
                return Err(());
            }

            let next = cursor.add(size);

            (*this.as_ptr()).cursor = next;

            let cursor = NonNull::new(cursor).unwrap();
            let suballocation: NonNull<[u8]> = NonNull::slice_from_raw_parts(cursor, size);

            Ok(suballocation)
        }
    }

    pub unsafe fn deallocate_item(this: NonNull<Chunk>, item: NonNull<u8>, layout: Layout) {
        debug_assert!(Chunk::contains_item(this, item));

        unsafe {
            let size = align(layout.size(), CHUNK_ALIGNMENT);
            let item_end = item.as_ptr().add(size);

            // If the item is the last allocation, then move the cursor back
            // to reuse its memory.
            if item_end == (*this.as_ptr()).cursor {
                (*this.as_ptr()).cursor = item.as_ptr();
            }

            // Otherwise, deallocation is a no-op
        }
    }

    pub unsafe fn grow_item(this: NonNull<Chunk>, item: NonNull<u8>, old_layout: Layout, new_layout: Layout) -> Result<NonNull<[u8]>, ()> {
        debug_assert!(Chunk::contains_item(this, item));

        let old_size = align(old_layout.size(), CHUNK_ALIGNMENT);
        let new_size = align(new_layout.size(), CHUNK_ALIGNMENT);
        let old_item_end = item.as_ptr().add(old_size);

        if old_item_end != (*this.as_ptr()).cursor {
            return Err(());
        }

        // The item is the last allocation. we can attempt to just move
        // the cursor if the new size fits.

        let chunk_end = (*this.as_ptr()).chunk_end;
        let available_size = chunk_end.offset_from(item.as_ptr());

        if new_size as isize > available_size {
            // Does not fit.
            return Err(());
        }

        let new_item_end = item.as_ptr().add(new_size);
        (*this.as_ptr()).cursor = new_item_end;

        Ok(NonNull::slice_from_raw_parts(item, new_size))
    }

    pub unsafe fn shrink_item(this: NonNull<Chunk>, item: NonNull<u8>, old_layout: Layout, new_layout: Layout) -> NonNull<[u8]> {
        debug_assert!(Chunk::contains_item(this, item));

        let old_size = align(old_layout.size(), CHUNK_ALIGNMENT);
        let new_size = align(new_layout.size(), CHUNK_ALIGNMENT);
        let old_item_end = item.as_ptr().add(old_size);

        // The item is the last allocation. we can attempt to just move
        // the cursor if the new size fits.

        if old_item_end == (*this.as_ptr()).cursor {
            let new_item_end = item.as_ptr().add(new_size);
            (*this.as_ptr()).cursor = new_item_end;
        }

        NonNull::slice_from_raw_parts(item, new_size)
    }

    pub fn contains_item(this: NonNull<Chunk>, item: NonNull<u8>) -> bool {
        unsafe {
            let start: *mut u8 = this.cast::<u8>().as_ptr().add(CHUNK_ALIGNMENT);
            let end: *mut u8 = (*this.as_ptr()).chunk_end;
            let item = item.as_ptr();

            start <= item && item < end
        }
    }

    fn available_size(this: NonNull<Chunk>) -> usize {
        unsafe {
            let this = this.as_ptr();
            (*this).chunk_end.offset_from((*this).cursor) as usize
        }
    }

    fn utilization(this: NonNull<Chunk>) -> f32 {
        let size = unsafe { (*this.as_ptr()).size } as f32;
        (size - Chunk::available_size(this) as f32) / size
    }
}

fn align(val: usize, alignment: usize) -> usize {
    let rem = val % alignment;
    if rem == 0 {
        return val;
    }

    val.checked_add(alignment).unwrap() - rem
}

#[derive(Copy, Clone, Debug, Default)]
pub struct Stats {
    pub chunks: u32,
    pub chunk_utilization: f32,
    pub allocations: u32,
    pub deallocations: u32,
    pub reallocations: u32,
    pub in_place_reallocations: u32,

    pub reallocated_bytes: usize,
    pub allocated_bytes: usize,
    pub reserved_bytes: usize,
}
