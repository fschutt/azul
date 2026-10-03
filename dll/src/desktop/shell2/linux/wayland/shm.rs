//! The ONE allocator of `wl_shm` memory in the Wayland backend, and the layout
//! law of a pool of equal buffers.
//!
//! Every `wl_shm` client buffer we hand a compositor comes from here: the
//! window / popup backbuffer pool (`CpuFallbackState`), the tooltip buffer and
//! the screen-capture destination. They used to carry three copies of the same
//! memfd-then-`shm_open` code.
//!
//! # Why the layout matters (WAYLAND8, 2026-10-03)
//!
//! KWin (Plasma 6.7, MR !9178 "allow importing shm buffers through udmabuf")
//! no longer has to copy a CPU-rendered client's frame on its main thread and
//! upload it to the GPU: it wraps the client's memfd pages with the kernel's
//! `udmabuf` driver into a dma-buf and samples them directly (zero copies) when
//! the GPU accepts the row pitch, falling back to the old copy otherwise. Qt
//! 6.11.2 changed its client allocation to qualify. The import only works when
//! the CLIENT allocated the memory a particular way - it is checked in two
//! places:
//!
//! * KWin: the buffer's `offset` inside the pool is a multiple of the page size;
//!   it asks for `align_up(height * stride, page)` bytes from there.
//! * the kernel (`drivers/dma-buf/udmabuf.c`, `check_memfd_seals` and
//!   `memfd_pin_folios`): the file is a memfd (shmem) carrying `F_SEAL_SHRINK`
//!   and NOT `F_SEAL_WRITE` / `F_SEAL_FUTURE_WRITE`; offset and size are
//!   page-aligned; the range lies entirely inside the file.
//!
//! [`udmabuf_importable`] is that rule as one predicate, [`pool_layout`] lays a
//! pool out so every buffer in it passes, and [`create_shm_file`] makes a file
//! that carries the seal. None of it needs a compositor, so it is tested here.
//!
//! The row pitch stays TIGHT (`width * 4`): the CPU renderer draws straight into
//! a slot (`AzulPixmap::from_external`, no row pitch of its own). A width whose
//! rows are already a multiple of 256 bytes (every width divisible by 64 px)
//! meets the pitch every common GPU accepts and gets the zero-copy path; other
//! widths still get the udmabuf, and the driver decides whether it can sample
//! that pitch (else KWin copies, exactly as before).

use std::ffi::CString;

/// Bytes per pixel of every format we allocate (`ARGB8888` / `ABGR8888` /
/// `XRGB8888`).
pub(crate) const BYTES_PER_PIXEL: usize = 4;

/// Byte layout of a `wl_shm_pool` holding `count` equal buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShmPoolLayout {
    /// Row pitch in bytes - `wl_shm_pool.create_buffer`'s `stride`.
    pub(crate) stride: i32,
    /// Distance from one buffer's first byte to the next one's.
    pub(crate) slot_bytes: usize,
    /// Size of the file and of the pool (`wl_shm.create_pool`'s `size`).
    pub(crate) pool_bytes: usize,
}

impl ShmPoolLayout {
    /// Byte offset of buffer `slot` inside the pool.
    pub(crate) fn offset_of(&self, slot: usize) -> usize {
        slot * self.slot_bytes
    }

    /// The pixel bytes of one buffer (`stride * height`), without padding.
    pub(crate) fn pixel_bytes(&self, height: i32) -> usize {
        self.stride.max(0) as usize * height.max(1) as usize
    }

    /// One row in pixels, padding included: the width of the renderer's view
    /// of a buffer (an `AzulPixmap`'s width IS its row pitch).
    pub(crate) fn pitch_px(&self) -> u32 {
        (self.stride.max(0) as usize / BYTES_PER_PIXEL) as u32
    }

    /// Pixels of padding at the end of every row of a `width`-pixel buffer.
    pub(crate) fn padding_px(&self, width: i32) -> u32 {
        (self.pitch_px() as usize).saturating_sub(width.max(1) as usize) as u32
    }
}

/// `value` rounded up to the next multiple of `alignment` (`alignment` > 0).
/// `None` on overflow.
pub(crate) fn align_up(value: usize, alignment: usize) -> Option<usize> {
    let alignment = alignment.max(1);
    match value % alignment {
        0 => Some(value),
        rem => value.checked_add(alignment - rem),
    }
}

/// The system page size (`sysconf(_SC_PAGESIZE)`; 4096 if that fails).
pub(crate) fn page_size() -> usize {
    let p = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if p > 0 {
        p as usize
    } else {
        4096
    }
}

/// Lay out a pool of `count` buffers of `width` x `height` pixels (clamped to
/// at least 1 x 1). `None` when the pool would not fit the protocol's `int32`
/// size.
///
/// Every buffer starts on a `page` boundary and owns a whole number of pages
/// (its pixels, rounded up), so each one passes [`udmabuf_importable`] - the
/// padding is less than one page per buffer. The row pitch stays tight (see
/// the module docs).
pub(crate) fn pool_layout(
    width: i32,
    height: i32,
    count: usize,
    page: usize,
) -> Option<ShmPoolLayout> {
    let w = width.max(1) as usize;
    let h = height.max(1) as usize;
    let stride = w.checked_mul(BYTES_PER_PIXEL)?;
    let slot_bytes = align_up(stride.checked_mul(h)?, page)?;
    let pool_bytes = slot_bytes.checked_mul(count.max(1))?;
    if pool_bytes > i32::MAX as usize {
        return None;
    }
    Some(ShmPoolLayout {
        stride: stride as i32,
        slot_bytes,
        pool_bytes,
    })
}

/// Whether a buffer at `offset` with `stride` x `height` inside a file of
/// `file_len` bytes carrying `seals` can be turned into a udmabuf - KWin's
/// check and the kernel's checks, in one place (see the module docs).
pub(crate) fn udmabuf_importable(
    offset: usize,
    stride: i32,
    height: i32,
    page: usize,
    file_len: usize,
    seals: i32,
) -> bool {
    let page = page.max(1);
    if stride <= 0 || height <= 0 || offset % page != 0 {
        return false;
    }
    // KWin asks for the pixel bytes rounded up to whole pages.
    let size = match (stride as usize)
        .checked_mul(height as usize)
        .and_then(|n| align_up(n, page))
    {
        Some(s) if s > 0 => s,
        _ => return false,
    };
    // memfd_pin_folios: the last byte must lie inside the file.
    match offset.checked_add(size) {
        Some(end) if end <= file_len => {}
        _ => return false,
    }
    let wanted = libc::F_SEAL_SHRINK;
    let denied = libc::F_SEAL_WRITE | libc::F_SEAL_FUTURE_WRITE;
    (seals & wanted) == wanted && (seals & denied) == 0
}

/// Create the anonymous shared-memory file behind a `wl_shm_pool`, `size`
/// bytes long, sealed against shrinking where the kernel allows it. The
/// caller owns the returned fd (pass it to `wl_shm.create_pool`, then keep or
/// close it).
pub(crate) fn create_shm_file(name: &str, size: usize) -> Result<libc::c_int, &'static str> {
    if size == 0 || size > i32::MAX as usize {
        return Err("shared memory size out of range");
    }
    let cname = CString::new(name).unwrap_or_default();
    // memfd_create (Linux 3.17+) through the raw syscall: glibc only grew the
    // wrapper in 2.27. MFD_ALLOW_SEALING so the file can carry F_SEAL_SHRINK
    // (the udmabuf import refuses an unsealed memfd). MFD_NOEXEC_SEAL first
    // (6.3+): it implies sealing, silences the kernel's "neither MFD_EXEC nor
    // MFD_NOEXEC_SEAL" warning and is REQUIRED under vm.memfd_noexec=2; older
    // kernels reject the unknown flag with EINVAL, then plain sealing is used.
    let memfd = |flags: libc::c_uint| -> libc::c_int {
        // SAFETY: a NUL-terminated name that outlives the call, plain flags.
        unsafe {
            libc::syscall(libc::SYS_memfd_create, cname.as_ptr(), flags as libc::c_int)
                as libc::c_int
        }
    };
    let mut fd = memfd(libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING | libc::MFD_NOEXEC_SEAL);
    if fd == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EINVAL) {
        fd = memfd(libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING);
    }
    if fd == -1 {
        // Older systems: a POSIX shm object, unlinked at once so it dies with
        // the last fd.
        let path = CString::new(format!("/{}-{}", name, std::process::id())).unwrap_or_default();
        // SAFETY: a NUL-terminated path that outlives both calls.
        unsafe {
            fd = libc::shm_open(
                path.as_ptr(),
                libc::O_CREAT | libc::O_RDWR | libc::O_EXCL,
                0o600,
            );
            if fd != -1 {
                libc::shm_unlink(path.as_ptr());
            }
        }
    }
    if fd == -1 {
        return Err("Failed to create shared memory");
    }
    // SAFETY: `fd` is the file just created and owned here.
    unsafe {
        if libc::ftruncate(fd, size as libc::off_t) == -1 {
            libc::close(fd);
            return Err("ftruncate failed");
        }
        // Seal AFTER sizing: the file may never shrink (the udmabuf rule; it
        // also lets libwayland-server skip its SIGBUS guard for this pool), and
        // F_SEAL_SEAL keeps anyone holding the fd - the compositor included -
        // from adding F_SEAL_WRITE / F_SEAL_FUTURE_WRITE later. Growing stays
        // allowed. Best effort: the shm_open fallback cannot be sealed and
        // still works as a plain (copied) wl_shm pool.
        let _ = libc::fcntl(
            fd,
            libc::F_ADD_SEALS,
            libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL,
        );
    }
    Ok(fd)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Window sizes that are NOT multiples of anything convenient, plus the
    /// common full-screen ones.
    const SIZES: &[(i32, i32)] = &[
        (1, 1),
        (3, 5),
        (333, 77),
        (801, 601),
        (1023, 767),
        (1920, 1080),
        (2561, 1441),
        (3840, 2160),
    ];
    /// 4 KiB (x86-64), 16 KiB and 64 KiB (aarch64 kernels).
    const PAGES: &[usize] = &[4096, 16384, 65536];
    const SEALED: i32 = libc::F_SEAL_SHRINK | libc::F_SEAL_SEAL;

    #[test]
    fn every_buffer_of_a_two_buffer_pool_starts_on_a_page_boundary() {
        for &page in PAGES {
            for &(w, h) in SIZES {
                let l = pool_layout(w, h, 2, page).expect("fits");
                for slot in 0..2 {
                    assert_eq!(
                        l.offset_of(slot) % page,
                        0,
                        "{w}x{h}, page {page}: buffer {slot} starts at {}",
                        l.offset_of(slot)
                    );
                }
            }
        }
    }

    #[test]
    fn each_buffer_owns_whole_pages_that_cover_its_pixels_and_no_more_than_one_extra() {
        for &page in PAGES {
            for &(w, h) in SIZES {
                let l = pool_layout(w, h, 2, page).expect("fits");
                let px = l.pixel_bytes(h);
                assert_eq!(l.slot_bytes % page, 0, "{w}x{h}, page {page}");
                assert!(l.slot_bytes >= px, "{w}x{h}: slot smaller than its pixels");
                assert!(
                    l.slot_bytes < px + page,
                    "{w}x{h}: more than one page of padding"
                );
                assert_eq!(l.pool_bytes, 2 * l.slot_bytes, "{w}x{h}: pool = 2 slots");
            }
        }
    }

    #[test]
    fn every_buffer_of_our_pool_passes_the_kernel_and_kwin_udmabuf_rule() {
        for &page in PAGES {
            for &(w, h) in SIZES {
                let l = pool_layout(w, h, 2, page).expect("fits");
                for slot in 0..2 {
                    assert!(
                        udmabuf_importable(
                            l.offset_of(slot),
                            l.stride,
                            h,
                            page,
                            l.pool_bytes,
                            SEALED
                        ),
                        "{w}x{h}, page {page}: buffer {slot} is not importable as a udmabuf"
                    );
                }
            }
        }
    }

    #[test]
    fn the_row_pitch_is_the_row_rounded_up_to_256_bytes_so_any_gpu_can_sample_it() {
        for &(w, h) in SIZES {
            let l = pool_layout(w, h, 2, 4096).expect("fits");
            let row = w as usize * BYTES_PER_PIXEL;
            let pitch = l.stride as usize;
            assert_eq!(pitch % 256, 0, "{w}x{h}: pitch {pitch}");
            assert!(pitch >= row, "{w}x{h}: pitch {pitch} < row {row}");
            assert!(pitch < row + 256, "{w}x{h}: more than one step of padding");
            // The padding is whole pixels (the renderer's view of the slot is
            // a pixmap `pitch / 4` pixels wide).
            assert_eq!(pitch % BYTES_PER_PIXEL, 0);
            assert_eq!(l.padding_px(w) as usize, (pitch - row) / BYTES_PER_PIXEL);
        }
        // Widths that are multiples of 64 px need no padding at all.
        assert_eq!(pool_layout(1920, 1080, 2, 4096).unwrap().stride, 1920 * 4);
        assert_eq!(pool_layout(1921, 1080, 2, 4096).unwrap().stride, 1984 * 4);
    }

    #[test]
    fn a_pool_larger_than_the_protocol_s_int32_size_is_refused() {
        assert_eq!(pool_layout(40_000, 40_000, 2, 4096), None);
        assert!(pool_layout(4096, 4096, 2, 4096).is_some());
    }

    #[test]
    fn a_zero_or_negative_size_is_laid_out_as_one_pixel() {
        let l = pool_layout(0, -5, 2, 4096).expect("fits");
        assert_eq!(l.stride, 256);
        assert_eq!(l.slot_bytes, 4096);
        assert_eq!(l.padding_px(0), 63);
    }

    #[test]
    fn the_udmabuf_rule_refuses_the_old_tight_two_buffer_layout_and_an_unsealed_file() {
        // 801x601: the second buffer of a tightly packed pool started at
        // 801 * 4 * 601 = 1 925 604 bytes - not a page boundary.
        let stride = 801 * 4;
        let tight_offset = stride as usize * 601;
        let file = 2 * tight_offset;
        assert!(!udmabuf_importable(
            tight_offset,
            stride,
            601,
            4096,
            file,
            SEALED
        ));
        // Page-aligned but the file can shrink (memfd made without
        // MFD_ALLOW_SEALING reports F_SEAL_SEAL only).
        assert!(!udmabuf_importable(
            0,
            stride,
            601,
            4096,
            4096 * 1000,
            libc::F_SEAL_SEAL
        ));
        // Write-sealed memory cannot back a writable dma-buf.
        assert!(!udmabuf_importable(
            0,
            stride,
            601,
            4096,
            4096 * 1000,
            libc::F_SEAL_SHRINK | libc::F_SEAL_WRITE
        ));
        // The page-rounded range runs past the end of the file.
        assert!(!udmabuf_importable(
            0,
            stride,
            601,
            4096,
            tight_offset,
            SEALED
        ));
        // The same buffer, page-aligned, sealed, inside the file: accepted.
        assert!(udmabuf_importable(
            0,
            stride,
            601,
            4096,
            4096 * 1000,
            SEALED
        ));
    }

    #[test]
    fn the_shm_file_is_a_memfd_sealed_against_shrinking_and_open_for_writing() {
        unsafe {
            let fd = create_shm_file("azul-shm-test", 3 * 4096).expect("shared memory");
            let seals = libc::fcntl(fd, libc::F_GET_SEALS);
            let mut st: libc::stat = core::mem::zeroed();
            assert_eq!(libc::fstat(fd, &mut st), 0);
            let shrink = libc::ftruncate(fd, 4096);
            libc::close(fd);
            assert!(seals >= 0, "F_GET_SEALS failed: not a memfd");
            assert_ne!(
                seals & libc::F_SEAL_SHRINK,
                0,
                "seals {seals:#x} lack F_SEAL_SHRINK"
            );
            assert_eq!(
                seals & (libc::F_SEAL_WRITE | libc::F_SEAL_FUTURE_WRITE),
                0,
                "seals {seals:#x} forbid writing"
            );
            assert_eq!(st.st_size as usize, 3 * 4096);
            assert_eq!(shrink, -1, "a sealed file must refuse to shrink");
        }
    }

    #[test]
    fn the_shm_file_maps_shared_and_writable_at_its_full_size() {
        unsafe {
            let size = 2 * 4096;
            let fd = create_shm_file("azul-shm-test-map", size).expect("shared memory");
            let p = libc::mmap(
                core::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            );
            assert_ne!(p, libc::MAP_FAILED);
            let bytes = core::slice::from_raw_parts_mut(p as *mut u8, size);
            bytes[0] = 0xAB;
            bytes[size - 1] = 0xCD;
            assert_eq!((bytes[0], bytes[size - 1]), (0xAB, 0xCD));
            libc::munmap(p, size);
            libc::close(fd);
        }
    }

    #[test]
    fn an_empty_or_oversized_shm_file_is_refused() {
        assert!(create_shm_file("azul-shm-test-0", 0).is_err());
        assert!(create_shm_file("azul-shm-test-big", i32::MAX as usize + 1).is_err());
    }
}
