//! MIT-SHM upload for the X11 CPU present (WAYLAND8, 2026-10-03).
//!
//! The CPU present used to pack + swizzle every damaged rect into a heap
//! buffer and `XPutImage` it: the pixels travel INTO the X protocol stream
//! (the socket), and the server copies them again into the window. With
//! MIT-SHM the damaged rects are swizzled straight into a SysV shared memory
//! segment the server has attached, and one `XShmPutImage` per rect tells the
//! server where to read - no pixel crosses the socket.
//!
//! The decisions (local display? extension? attach? segment reuse? may the
//! segment be written?) are the pure functions of `common::x11_host` (tested
//! in every build). This file is the Xlib / SysV glue around them and needs
//! an X server to run: it is verified by a Linux run, not by unit tests.
//!
//! Fallback: any failed check (`AZ_X11_SHM=0`, a remote `DISPLAY`, no
//! libXext / no extension, a refused attach) makes the window use `XPutImage`
//! for the rest of its life - the caller asks [`X11ShmUpload::probe`] once
//! and keeps the answer.

use std::{
    ffi::{c_int, c_uint, c_void},
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

use super::{
    defines::{Display, Window, XErrorEvent, XImage, XShmSegmentInfo, GC},
    dlopen::{XShm, Xlib},
};
use crate::desktop::shell2::common::x11_host::{
    display_is_local, shm_segment_reusable, x11_upload, PutImageWhy, ShmPending, X11Upload,
};

/// `ZPixmap`, the image format of a 32-bit TrueColor upload.
const Z_PIXMAP: c_int = 2;

/// One attached segment and the `XImage` that describes it.
struct ShmSegment {
    /// Boxed: `XShmCreateImage` keeps a pointer to it in the image.
    info: Box<XShmSegmentInfo>,
    image: *mut XImage,
    width: u32,
    height: u32,
    /// `image.bytes_per_line` - the segment's row pitch.
    pitch: usize,
    /// `pitch * height`.
    bytes: usize,
}

/// The MIT-SHM state of one X11 window's CPU present.
pub(super) struct X11ShmUpload {
    lib: Rc<XShm>,
    /// `XShmGetEventBase`: `ShmCompletion` arrives as this + 0.
    event_base: c_int,
    segment: Option<ShmSegment>,
    /// Puts that still read the segment (the reuse law).
    pending: ShmPending,
}

/// Set by [`shm_attach_error_trap`] while an attach is being tried.
static ATTACH_FAILED: AtomicBool = AtomicBool::new(false);

/// The error handler installed around `XShmAttach` + `XSync`: a refused attach
/// (`BadAccess` from a server in another IPC namespace) must not reach the
/// process-wide handler as a "real" error - it is the fallback signal.
unsafe extern "C" fn shm_attach_error_trap(_d: *mut Display, _e: *mut XErrorEvent) -> c_int {
    ATTACH_FAILED.store(true, Ordering::Relaxed);
    0
}

impl X11ShmUpload {
    /// Everything that can be learned before a segment exists: the switch
    /// (`AZ_X11_SHM=0`), the `DISPLAY` string, libXext, the extension. The
    /// attach is tried by the first [`Self::upload`].
    pub(super) fn probe(display: *mut Display) -> Result<Self, PutImageWhy> {
        let enabled = std::env::var("AZ_X11_SHM").map_or(true, |v| v != "0");
        let local = std::env::var("DISPLAY").is_ok_and(|d| display_is_local(&d));
        let lib = if enabled && local {
            XShm::new().ok()
        } else {
            None
        };
        let extension = lib
            .as_ref()
            .is_some_and(|l| unsafe { (l.XShmQueryExtension)(display) } != 0);
        match (x11_upload(enabled, local, extension, true), lib) {
            (X11Upload::Shm, Some(lib)) => {
                let event_base = unsafe { (lib.XShmGetEventBase)(display) };
                Ok(Self {
                    lib,
                    event_base,
                    segment: None,
                    pending: ShmPending::default(),
                })
            }
            (X11Upload::PutImage(why), _) => Err(why),
            (X11Upload::Shm, None) => Err(PutImageWhy::NoExtension),
        }
    }

    /// The event type of `ShmCompletion` on this display.
    pub(super) fn completion_event_type(&self) -> c_int {
        self.event_base + super::defines::ShmCompletion
    }

    /// A `ShmCompletion` for this window arrived: one put fewer reads the
    /// segment.
    pub(super) fn on_completion(&mut self) {
        self.pending.completed();
    }

    /// Upload `rects` (x, y, w, h in frame px) of the renderer's R,G,B,A
    /// `frame` (`frame_w` x `frame_h`, tight rows) to `window`. False when
    /// MIT-SHM cannot be used (the segment could not be made or attached):
    /// the caller uploads this frame with `XPutImage` and drops this state.
    ///
    /// # Safety
    /// `display`, `window`, `gc`, `visual` and `depth` are the live window's;
    /// called on the thread that owns the display.
    #[allow(clippy::too_many_arguments)]
    pub(super) unsafe fn upload(
        &mut self,
        xlib: &Xlib,
        display: *mut Display,
        window: Window,
        gc: GC,
        visual: *mut c_void,
        depth: c_uint,
        frame: &[u8],
        frame_w: u32,
        frame_h: u32,
        rects: &[(u32, u32, u32, u32)],
    ) -> bool {
        let reusable = self
            .segment
            .as_ref()
            .is_some_and(|s| shm_segment_reusable((s.width, s.height), (frame_w, frame_h)));
        if !reusable {
            self.destroy(xlib, display);
            match create_segment(&self.lib, xlib, display, visual, depth, frame_w, frame_h) {
                Some(seg) => self.segment = Some(seg),
                None => return false,
            }
        }
        // The reuse law: no earlier put may still be reading what we are
        // about to overwrite. XSync = the server processed every request so
        // far (their completions still arrive and are counted off later).
        if self.pending.must_sync_before_write() {
            (xlib.XSync)(display, 0);
        }
        let Some(seg) = self.segment.as_mut() else {
            return false;
        };
        let dst = core::slice::from_raw_parts_mut(seg.info.shmaddr as *mut u8, seg.bytes);
        crate::desktop::shell2::headless::copy_rgba_rects_into(
            dst,
            seg.pitch,
            frame,
            frame_w as usize * 4,
            frame_w as usize,
            frame_h as usize,
            rects,
            true, // the X visual's B,G,R,A
        );
        // One put per rect, clipped like the copy; only the LAST asks for a
        // ShmCompletion - requests are processed in order, so its completion
        // covers every earlier put of the frame.
        let clipped: Vec<(u32, u32, u32, u32)> = rects
            .iter()
            .filter_map(|&(x, y, w, h)| {
                let x1 = x.saturating_add(w).min(frame_w);
                let y1 = y.saturating_add(h).min(frame_h);
                (x < x1 && y < y1).then(|| (x, y, x1 - x, y1 - y))
            })
            .collect();
        let last = clipped.len().saturating_sub(1);
        for (i, &(x, y, w, h)) in clipped.iter().enumerate() {
            let send_event = c_int::from(i == last);
            (self.lib.XShmPutImage)(
                display, window, gc, seg.image, x as c_int, y as c_int, x as c_int, y as c_int, w,
                h, send_event,
            );
        }
        if !clipped.is_empty() {
            self.pending.put();
        }
        true
    }

    /// Detach and free the segment (resize, window teardown). Waits for the
    /// server to finish any put still reading it.
    ///
    /// # Safety
    /// `display` is the live display the segment was attached to.
    pub(super) unsafe fn destroy(&mut self, xlib: &Xlib, display: *mut Display) {
        let Some(mut seg) = self.segment.take() else {
            return;
        };
        (self.lib.XShmDetach)(display, &mut *seg.info);
        // The detach (and every put before it) must be processed before the
        // memory goes away under the server.
        (xlib.XSync)(display, 0);
        self.pending = ShmPending::default();
        libc::shmdt(seg.info.shmaddr as *const c_void);
        if !seg.image.is_null() {
            (*seg.image).data = core::ptr::null_mut();
            (xlib.XDestroyImage)(seg.image);
        }
    }
}

/// Make, map and attach a segment for a `w` x `h` frame. None (everything
/// undone) when any step fails - the attach failing is the usual reason
/// (`PutImageWhy::AttachFailed`).
unsafe fn create_segment(
    lib: &XShm,
    xlib: &Xlib,
    display: *mut Display,
    visual: *mut c_void,
    depth: c_uint,
    w: u32,
    h: u32,
) -> Option<ShmSegment> {
    let mut info = Box::new(XShmSegmentInfo {
        shmseg: 0,
        shmid: -1,
        shmaddr: core::ptr::null_mut(),
        readOnly: 1, // the server only reads it
    });
    let image = (lib.XShmCreateImage)(
        display,
        visual,
        depth,
        Z_PIXMAP,
        core::ptr::null_mut(),
        &mut *info,
        w,
        h,
    );
    if image.is_null() {
        return None;
    }
    let pitch = usize::try_from((*image).bytes_per_line).unwrap_or(0);
    let bytes = pitch.saturating_mul(h as usize);
    if pitch < w as usize * 4 || bytes == 0 {
        (xlib.XDestroyImage)(image);
        return None;
    }
    let undo_image = |image: *mut XImage| {
        (*image).data = core::ptr::null_mut();
        (xlib.XDestroyImage)(image);
    };
    let shmid = libc::shmget(libc::IPC_PRIVATE, bytes, libc::IPC_CREAT | 0o600);
    if shmid == -1 {
        undo_image(image);
        return None;
    }
    let addr = libc::shmat(shmid, core::ptr::null(), 0);
    if addr as isize == -1 {
        libc::shmctl(shmid, libc::IPC_RMID, core::ptr::null_mut());
        undo_image(image);
        return None;
    }
    info.shmid = shmid;
    info.shmaddr = addr as *mut _;
    (*image).data = addr as *mut _;

    // Attach with the error trap in place, and make the server answer now.
    ATTACH_FAILED.store(false, Ordering::Relaxed);
    let previous = (xlib.XSetErrorHandler)(Some(shm_attach_error_trap));
    let attached = (lib.XShmAttach)(display, &mut *info) != 0;
    (xlib.XSync)(display, 0);
    (xlib.XSetErrorHandler)(previous);
    // Attached by the server (or refused): either way the id can go - the
    // segment lives on until both sides detach, and never leaks on a crash.
    libc::shmctl(shmid, libc::IPC_RMID, core::ptr::null_mut());
    let ok = attached && !ATTACH_FAILED.load(Ordering::Relaxed);
    if !ok {
        libc::shmdt(addr);
        undo_image(image);
        return None;
    }
    Some(ShmSegment {
        info,
        image,
        width: w,
        height: h,
        pitch,
        bytes,
    })
}
