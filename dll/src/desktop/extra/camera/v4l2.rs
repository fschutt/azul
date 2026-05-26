//! Linux V4L2 camera capture backend for the capture seam, via libv4l2 loaded
//! at runtime with `libloading` (NO build-time link, so it cross-compiles to
//! every target and only fails - gracefully - at runtime if libv4l2 is absent).
//!
//! The dlopen rule, same as `audio/alsa.rs`: `libv4l2.so.0` (fall back to
//! `libv4l2.so`) is opened lazily and dispatched through fn pointers. We drive
//! the standard V4L2 capture flow (QUERYCAP / S_FMT / REQBUFS+mmap / STREAMON /
//! QBUF+DQBUF / STREAMOFF) ourselves with hand-transcribed structs + `_IOWR`
//! ioctl numbers. We request `V4L2_PIX_FMT_RGB24`; libv4l2 transparently
//! converts most camera formats (YUYV, MJPEG, ...) to RGB24 for us, which we
//! then expand to the seam's tightly-packed RGBA8.
//!
//! Plugs into `capture_common::register_camera_backend`, so `CameraWidget`
//! shows the real camera where `/dev/video*` exists; the worker falls back to
//! its test pattern when `open` returns `0` (no libv4l2 / no device / format
//! rejected).

use core::ffi::{c_char, c_int, c_long, c_ulong, c_void};
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// libv4l2 runtime loader (dlopen, no static link).
// ---------------------------------------------------------------------------

type OffT = c_long;

/// The handful of libv4l2 symbols the capture flow needs. libv4l2 mirrors the
/// libc syscalls 1:1 but inserts its format-conversion layer, so we route
/// open/close/ioctl/mmap/munmap through it rather than calling libc directly.
struct V4l2Fns {
    open: unsafe extern "C" fn(*const c_char, c_int) -> c_int,
    close: unsafe extern "C" fn(c_int) -> c_int,
    ioctl: unsafe extern "C" fn(c_int, c_ulong, *mut c_void) -> c_int,
    mmap: unsafe extern "C" fn(*mut c_void, usize, c_int, c_int, c_int, OffT) -> *mut c_void,
    munmap: unsafe extern "C" fn(*mut c_void, usize) -> c_int,
}

static V4L2: OnceLock<Option<(libloading::Library, V4l2Fns)>> = OnceLock::new();

fn v4l2() -> Option<&'static V4l2Fns> {
    V4L2.get_or_init(|| unsafe {
        let lib = libloading::Library::new("libv4l2.so.0")
            .or_else(|_| libloading::Library::new("libv4l2.so"))
            .ok()?;
        let fns = V4l2Fns {
            open: *lib.get(b"v4l2_open\0").ok()?,
            close: *lib.get(b"v4l2_close\0").ok()?,
            ioctl: *lib.get(b"v4l2_ioctl\0").ok()?,
            mmap: *lib.get(b"v4l2_mmap\0").ok()?,
            munmap: *lib.get(b"v4l2_munmap\0").ok()?,
        };
        Some((lib, fns))
    })
    .as_ref()
    .map(|(_, f)| f)
}

// ---------------------------------------------------------------------------
// V4L2 ABI constants + ioctl numbers (stable Linux UAPI).
// ---------------------------------------------------------------------------

// open() flags.
const O_RDWR: c_int = 0o2;
const O_NONBLOCK: c_int = 0o4000;

// mmap() prot / flags.
const PROT_READ: c_int = 0x1;
const PROT_WRITE: c_int = 0x2;
const MAP_SHARED: c_int = 0x1;
const MAP_FAILED: isize = -1;

// Buffer types / memory / capabilities.
const V4L2_BUF_TYPE_VIDEO_CAPTURE: u32 = 1;
const V4L2_MEMORY_MMAP: u32 = 1;
const V4L2_FIELD_NONE: u32 = 1;
const V4L2_CAP_VIDEO_CAPTURE: u32 = 0x0000_0001;
const V4L2_CAP_STREAMING: u32 = 0x0400_0000;

// fourcc helper for pixel formats.
const fn fourcc(a: u8, b: u8, c: u8, d: u8) -> u32 {
    (a as u32) | ((b as u32) << 8) | ((c as u32) << 16) | ((d as u32) << 24)
}
/// 24-bit RGB, 8 bits per component, packed (libv4l2 converts to this).
const V4L2_PIX_FMT_RGB24: u32 = fourcc(b'R', b'G', b'B', b'3');

// _IOWR-style ioctl request encoding (Linux asm-generic/ioctl.h).
const IOC_NRBITS: u32 = 8;
const IOC_TYPEBITS: u32 = 8;
const IOC_SIZEBITS: u32 = 14;
const IOC_NRSHIFT: u32 = 0;
const IOC_TYPESHIFT: u32 = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: u32 = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: u32 = IOC_SIZESHIFT + IOC_SIZEBITS;
const IOC_WRITE: u32 = 1;
const IOC_READ: u32 = 2;

const fn ioc(dir: u32, ty: u32, nr: u32, size: u32) -> c_ulong {
    (((dir) << IOC_DIRSHIFT)
        | ((ty) << IOC_TYPESHIFT)
        | ((nr) << IOC_NRSHIFT)
        | ((size) << IOC_SIZESHIFT)) as c_ulong
}
const fn iowr(ty: u32, nr: u32, size: u32) -> c_ulong {
    ioc(IOC_READ | IOC_WRITE, ty, nr, size)
}

const VIDIOC_TYPE: u32 = b'V' as u32;

// VIDIOC_* request codes, encoded from the struct sizes below.
fn vidioc_querycap() -> c_ulong {
    iowr(VIDIOC_TYPE, 0, core::mem::size_of::<v4l2_capability>() as u32)
}
fn vidioc_s_fmt() -> c_ulong {
    iowr(VIDIOC_TYPE, 5, core::mem::size_of::<v4l2_format>() as u32)
}
fn vidioc_reqbufs() -> c_ulong {
    iowr(VIDIOC_TYPE, 8, core::mem::size_of::<v4l2_requestbuffers>() as u32)
}
fn vidioc_querybuf() -> c_ulong {
    iowr(VIDIOC_TYPE, 9, core::mem::size_of::<v4l2_buffer>() as u32)
}
fn vidioc_qbuf() -> c_ulong {
    iowr(VIDIOC_TYPE, 15, core::mem::size_of::<v4l2_buffer>() as u32)
}
fn vidioc_dqbuf() -> c_ulong {
    iowr(VIDIOC_TYPE, 17, core::mem::size_of::<v4l2_buffer>() as u32)
}
// STREAMON / STREAMOFF take a plain `int` (the buffer type), via _IOW.
fn vidioc_streamon() -> c_ulong {
    ioc(IOC_WRITE, VIDIOC_TYPE, 18, core::mem::size_of::<c_int>() as u32)
}
fn vidioc_streamoff() -> c_ulong {
    ioc(IOC_WRITE, VIDIOC_TYPE, 19, core::mem::size_of::<c_int>() as u32)
}

// ---------------------------------------------------------------------------
// V4L2 ABI structs (linux/videodev2.h). Sizes/offsets must match the kernel
// UAPI exactly, since the ioctl numbers above encode `sizeof(struct)`.
// ---------------------------------------------------------------------------

#[repr(C)]
struct v4l2_capability {
    driver: [u8; 16],
    card: [u8; 32],
    bus_info: [u8; 32],
    version: u32,
    capabilities: u32,
    device_caps: u32,
    reserved: [u32; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct v4l2_pix_format {
    width: u32,
    height: u32,
    pixelformat: u32,
    field: u32,
    bytesperline: u32,
    sizeimage: u32,
    colorspace: u32,
    priv_: u32,
    flags: u32,
    enc: u32,        // anonymous union { ycbcr_enc; hsv_enc; } (u32)
    quantization: u32,
    xfer_func: u32,
}

// `struct v4l2_format` holds a union `fmt` whose largest member is 200 bytes.
// We only use the `pix` member; pad the rest so `sizeof` matches the kernel.
const V4L2_FORMAT_FMT_UNION_SIZE: usize = 200;
#[repr(C)]
struct v4l2_format {
    type_: u32,
    // union fmt: pix occupies the front; padded to the union's full size.
    pix: v4l2_pix_format,
    _pad: [u8; V4L2_FORMAT_FMT_UNION_SIZE - core::mem::size_of::<v4l2_pix_format>()],
}

#[repr(C)]
struct v4l2_requestbuffers {
    count: u32,
    type_: u32,
    memory: u32,
    capabilities: u32,
    flags: u8,
    reserved: [u8; 3],
}

#[repr(C)]
struct v4l2_timeval {
    tv_sec: c_long,
    tv_usec: c_long,
}

#[repr(C)]
struct v4l2_timecode {
    type_: u32,
    flags: u32,
    frames: u8,
    seconds: u8,
    minutes: u8,
    hours: u8,
    userbits: [u8; 4],
}

#[repr(C)]
struct v4l2_buffer {
    index: u32,
    type_: u32,
    bytesused: u32,
    flags: u32,
    field: u32,
    timestamp: v4l2_timeval,
    timecode: v4l2_timecode,
    sequence: u32,
    memory: u32,
    // union m { offset: u32; userptr: c_ulong; planes: ptr; fd: i32 }
    // largest member is a pointer (c_ulong width); `offset` aliases its low word.
    m: c_ulong,
    length: u32,
    reserved2: u32,
    // union { request_fd: i32; reserved: u32 }
    request_fd_or_reserved: u32,
}

impl v4l2_buffer {
    /// MMAP buffers carry the mmap `offset` in the low word of the `m` union.
    fn offset(&self) -> u32 {
        self.m as u32
    }
}

// ---------------------------------------------------------------------------
// Capture state behind the seam's `u64` handle.
// ---------------------------------------------------------------------------

/// One mmap'd capture buffer.
struct MmapBuf {
    ptr: *mut c_void,
    len: usize,
}

/// Live capture state behind the seam's `u64` handle. Worker-thread-local (the
/// camera worker calls `open`/`read`/`close` on one thread), so no `Send`.
struct V4l2Cam {
    fd: c_int,
    width: u32,
    height: u32,
    bytesperline: u32,
    buffers: Vec<MmapBuf>,
}

/// Open `/dev/video{index}` at `width` x `height`, requesting RGB24 (libv4l2
/// converts the device's native format for us). Sets up mmap streaming and
/// starts the stream. Returns a boxed `V4l2Cam` as the opaque handle, or `0` on
/// any failure (no libv4l2, no device, format/streaming rejected) so the worker
/// falls back to the test pattern.
pub fn open(index: u32, width: u32, height: u32) -> u64 {
    let f = match v4l2() {
        Some(f) => f,
        None => return 0,
    };
    let width = if width == 0 { 640 } else { width };
    let height = if height == 0 { 480 } else { height };

    unsafe {
        // open device.
        let path = format!("/dev/video{}\0", index);
        let fd = (f.open)(path.as_ptr() as *const c_char, O_RDWR | O_NONBLOCK);
        if fd < 0 {
            return 0;
        }

        // Tear-down helper for the error paths below.
        let fail = |fd: c_int| -> u64 {
            (f.close)(fd);
            0
        };

        // VIDIOC_QUERYCAP - require a streaming-capable capture device.
        let mut cap: v4l2_capability = core::mem::zeroed();
        if (f.ioctl)(fd, vidioc_querycap(), &mut cap as *mut _ as *mut c_void) < 0 {
            return fail(fd);
        }
        // device_caps is per-node on modern kernels; fall back to capabilities.
        let caps = if cap.device_caps != 0 {
            cap.device_caps
        } else {
            cap.capabilities
        };
        if caps & V4L2_CAP_VIDEO_CAPTURE == 0 || caps & V4L2_CAP_STREAMING == 0 {
            return fail(fd);
        }

        // VIDIOC_S_FMT - ask libv4l2 for packed RGB24 at our resolution.
        let mut fmt: v4l2_format = core::mem::zeroed();
        fmt.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
        fmt.pix.width = width;
        fmt.pix.height = height;
        fmt.pix.pixelformat = V4L2_PIX_FMT_RGB24;
        fmt.pix.field = V4L2_FIELD_NONE;
        if (f.ioctl)(fd, vidioc_s_fmt(), &mut fmt as *mut _ as *mut c_void) < 0 {
            return fail(fd);
        }
        if fmt.pix.pixelformat != V4L2_PIX_FMT_RGB24 {
            // libv4l2 couldn't deliver RGB24 - bail (worker uses test pattern).
            return fail(fd);
        }
        // The driver may adjust the resolution; honor what it returned.
        let width = fmt.pix.width;
        let height = fmt.pix.height;
        let bytesperline = if fmt.pix.bytesperline != 0 {
            fmt.pix.bytesperline
        } else {
            width * 3
        };

        // VIDIOC_REQBUFS - request a small ring of mmap buffers.
        const BUF_COUNT: u32 = 4;
        let mut req: v4l2_requestbuffers = core::mem::zeroed();
        req.count = BUF_COUNT;
        req.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
        req.memory = V4L2_MEMORY_MMAP;
        if (f.ioctl)(fd, vidioc_reqbufs(), &mut req as *mut _ as *mut c_void) < 0 || req.count == 0 {
            return fail(fd);
        }

        // Query + mmap each buffer, then queue it.
        let mut buffers: Vec<MmapBuf> = Vec::with_capacity(req.count as usize);
        for i in 0..req.count {
            let mut buf: v4l2_buffer = core::mem::zeroed();
            buf.index = i;
            buf.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
            buf.memory = V4L2_MEMORY_MMAP;
            if (f.ioctl)(fd, vidioc_querybuf(), &mut buf as *mut _ as *mut c_void) < 0 {
                free_buffers(f, fd, &buffers);
                return fail(fd);
            }
            let ptr = (f.mmap)(
                core::ptr::null_mut(),
                buf.length as usize,
                PROT_READ | PROT_WRITE,
                MAP_SHARED,
                fd,
                buf.offset() as OffT,
            );
            if ptr as isize == MAP_FAILED || ptr.is_null() {
                free_buffers(f, fd, &buffers);
                return fail(fd);
            }
            buffers.push(MmapBuf {
                ptr,
                len: buf.length as usize,
            });
            // Queue the buffer so the device can fill it.
            if (f.ioctl)(fd, vidioc_qbuf(), &mut buf as *mut _ as *mut c_void) < 0 {
                free_buffers(f, fd, &buffers);
                return fail(fd);
            }
        }

        // VIDIOC_STREAMON.
        let mut ty: c_int = V4L2_BUF_TYPE_VIDEO_CAPTURE as c_int;
        if (f.ioctl)(fd, vidioc_streamon(), &mut ty as *mut _ as *mut c_void) < 0 {
            free_buffers(f, fd, &buffers);
            return fail(fd);
        }

        Box::into_raw(Box::new(V4l2Cam {
            fd,
            width,
            height,
            bytesperline,
            buffers,
        })) as u64
    }
}

/// Capture the next frame: dequeue a filled mmap buffer, expand its RGB24 rows
/// to tightly-packed RGBA8 into `out`, then re-queue the buffer. Returns the
/// frame `(width, height)`, or `(0, 0)` on error (worker stops). EAGAIN (no
/// frame ready yet on the non-blocking fd) is retried briefly.
pub fn read(handle: u64, out: &mut Vec<u8>) -> (u32, u32) {
    let f = match v4l2() {
        Some(f) => f,
        None => return (0, 0),
    };
    let cam = match unsafe { (handle as *mut V4l2Cam).as_mut() } {
        Some(c) => c,
        None => return (0, 0),
    };

    unsafe {
        // Dequeue a filled buffer, retrying on EAGAIN (non-blocking fd).
        let mut buf: v4l2_buffer = core::mem::zeroed();
        buf.type_ = V4L2_BUF_TYPE_VIDEO_CAPTURE;
        buf.memory = V4L2_MEMORY_MMAP;

        let mut attempts = 0;
        loop {
            if (f.ioctl)(cam.fd, vidioc_dqbuf(), &mut buf as *mut _ as *mut c_void) >= 0 {
                break;
            }
            // The fd is non-blocking; libv4l2 returns -1 with errno EAGAIN until
            // a frame is ready. Retry a bounded number of times with a short
            // sleep so we don't spin forever or block the worker indefinitely.
            attempts += 1;
            if attempts > 200 {
                return (0, 0);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        let idx = buf.index as usize;
        if idx >= cam.buffers.len() {
            return (0, 0);
        }
        let mbuf = &cam.buffers[idx];

        let (w, h) = (cam.width, cam.height);
        out.clear();
        out.resize((w as usize) * (h as usize) * 4, 0);
        let src = core::slice::from_raw_parts(mbuf.ptr as *const u8, mbuf.len);
        rgb24_to_rgba(src, w, h, cam.bytesperline, out);

        // Re-queue the buffer for reuse.
        let _ = (f.ioctl)(cam.fd, vidioc_qbuf(), &mut buf as *mut _ as *mut c_void);

        (w, h)
    }
}

/// Stop streaming, unmap + free buffers, close the fd, and drop the boxed
/// `V4l2Cam`.
pub fn close(handle: u64) {
    if handle == 0 {
        return;
    }
    let f = match v4l2() {
        Some(f) => f,
        None => {
            // Still reclaim the box even if the lib vanished (shouldn't happen).
            unsafe { drop(Box::from_raw(handle as *mut V4l2Cam)) };
            return;
        }
    };
    unsafe {
        let cam = Box::from_raw(handle as *mut V4l2Cam);
        // VIDIOC_STREAMOFF.
        let mut ty: c_int = V4L2_BUF_TYPE_VIDEO_CAPTURE as c_int;
        let _ = (f.ioctl)(cam.fd, vidioc_streamoff(), &mut ty as *mut _ as *mut c_void);
        free_buffers(f, cam.fd, &cam.buffers);
        (f.close)(cam.fd);
        // `cam` drops here, freeing the Vec<MmapBuf> (pointers already munmap'd).
    }
}

/// munmap every buffer (used on the error paths and on close).
fn free_buffers(f: &V4l2Fns, _fd: c_int, buffers: &[MmapBuf]) {
    for b in buffers {
        if !b.ptr.is_null() {
            unsafe {
                (f.munmap)(b.ptr, b.len);
            }
        }
    }
}

/// Expand packed/strided RGB24 (3 bytes/pixel, `stride` bytes/row) into
/// tightly-packed RGBA8 (`out` is `w*h*4`), setting alpha to 255.
fn rgb24_to_rgba(rgb: &[u8], w: u32, h: u32, stride: u32, out: &mut [u8]) {
    let w = w as usize;
    let h = h as usize;
    let stride = stride as usize;
    for y in 0..h {
        let row = y * stride;
        for x in 0..w {
            let s = row + x * 3;
            let d = (y * w + x) * 4;
            if s + 2 >= rgb.len() || d + 3 >= out.len() {
                return;
            }
            out[d] = rgb[s];
            out[d + 1] = rgb[s + 1];
            out[d + 2] = rgb[s + 2];
            out[d + 3] = 255;
        }
    }
}
