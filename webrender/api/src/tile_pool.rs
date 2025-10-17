// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::sync::Arc;

const NUM_TILE_BUCKETS: usize = 6;

/// A pool of blob tile buffers to mitigate the overhead of
/// allocating and deallocating blob tiles.
///
/// The pool keeps a strong reference to each allocated buffers and
/// reuses the ones with a strong count of 1.
pub struct BlobTilePool {
    largest_size_class: usize,
    buckets: [Vec<Arc<Vec<u8>>>; NUM_TILE_BUCKETS],
}

impl BlobTilePool {
    pub fn new() -> Self {
        // The default max tile size is actually 256, using 512 here
        // so that this still works when experimenting with larger
        // tile sizes. If we ever make larger adjustments, the buckets
        // should be changed accordingly.
        let max_tile_size = 512;
        BlobTilePool {
            largest_size_class: max_tile_size * max_tile_size * 4,
            buckets: [
                Vec::with_capacity(32),
                Vec::with_capacity(32),
                Vec::with_capacity(32),
                Vec::with_capacity(32),
                Vec::with_capacity(32),
                Vec::with_capacity(32),
            ],
        }
    }

    /// Get or allocate a tile buffer of the requested size.
    ///
    /// The returned buffer is zero-inizitalized.
    /// The length of the returned buffer is equal to the requested size,
    /// however the buffer may be allocated with a larger capacity to
    /// conform to the pool's corresponding bucket tile size.
    pub fn get_buffer(&mut self, requested_size: usize) -> MutableTileBuffer {
        if requested_size > self.largest_size_class {
            // If the requested size is larger than the largest size class,
            // simply return a MutableBuffer that isn't tracked/recycled by
            // the pool.
            // In Firefox this should only happen in pathological cases
            // where the blob visible area ends up so large that the tile
            // size is increased to avoid producing too many tiles.
            // See wr_resource_updates_add_blob_image.
            let mut buf = vec![0; requested_size];
            return MutableTileBuffer {
                ptr: buf.as_mut_ptr(),
                strong_ref: Arc::new(buf),
            };
        }

        let (bucket_idx, cap) = self.bucket_and_size(requested_size);
        let bucket = &mut self.buckets[bucket_idx];
        let mut selected_idx = None;
        for (buf_idx, buffer) in bucket.iter().enumerate() {
            if Arc::strong_count(buffer) == 1 {
                selected_idx = Some(buf_idx);
                break;
            }
        }

        let ptr;
        let strong_ref;
        if let Some(idx) = selected_idx {
            {
                // This works because we just ensured the pool has the only strong
                // ref to the buffer.
                let buffer = Arc::get_mut(&mut bucket[idx]).unwrap();
                debug_assert!(buffer.capacity() >= requested_size);
                // Ensure the length is equal to the requested size. It's not
                // strictly necessay for the tile pool but the texture upload
                // code relies on it.
                unsafe { buffer.set_len(requested_size); }

                // zero-initialize
                buffer.fill(0);

                ptr = buffer.as_mut_ptr();
            }
            strong_ref = Arc::clone(&bucket[idx]);
        } else {
            // Allocate a buffer with the adequate capacity for the requested
            // size's bucket.
            let mut buf = vec![0; cap];
            // Force the length to be the requested size.
            unsafe { buf.set_len(requested_size) };

            ptr = buf.as_mut_ptr();
            strong_ref = Arc::new(buf);
            // Track the new buffer.
            bucket.push(Arc::clone(&strong_ref));
        };

        MutableTileBuffer {
            ptr,
            strong_ref,
        }
    }

    fn bucket_and_size(&self, size: usize) -> (usize, usize) {
        let mut next_size_class = self.largest_size_class / 4;
        let mut idx = 0;
        while size < next_size_class && idx < NUM_TILE_BUCKETS - 1 {
            next_size_class /= 4;
            idx += 1;
        }

        (idx, next_size_class * 4)
    }

    /// Go over all allocated tile buffers. For each bucket, deallocate some buffers
    /// until the number of unused buffer is more than half of the buffers for that
    /// bucket.
    ///
    /// In practice, if called regularly, this gradually lets go of blob tiles when
    /// they are not used.
    pub fn cleanup(&mut self) {
        for bucket in &mut self.buckets {
            let threshold = bucket.len() / 2;
            let mut num_available = 0;
            bucket.retain(&mut |buffer: &Arc<Vec<u8>>| {
                if Arc::strong_count(buffer) > 1 {
                    return true;
                }

                num_available += 1;
                num_available < threshold
            });
        }
    }
}


// The role of tile buffer is to encapsulate an Arc to the underlying buffer
// with a reference count of at most 2 and a way to view the buffer's content
// as a mutable slice, even though the reference count may be more than 1.
// The safety of this relies on the other strong reference being held by the
// tile pool which never accesses the buffer's content, so the only reference
// that can access it is the `TileBuffer` itself.
pub struct MutableTileBuffer {
    strong_ref: Arc<Vec<u8>>,
    ptr: *mut u8,
}

impl MutableTileBuffer {
    pub fn as_mut_slice(&mut self) -> &mut[u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.strong_ref.len()) }
    }

    pub fn into_arc(self) -> Arc<Vec<u8>> {
        self.strong_ref
    }
}

unsafe impl Send for MutableTileBuffer {}
