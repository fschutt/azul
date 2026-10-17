//! macOS screen-capture backend via **ScreenCaptureKit** (macOS 12.3+).
//!
//! Everything is resolved AT RUNTIME — the framework is `dlopen`ed and every
//! class is looked up with `AnyClass::get` — so the dylib links and loads on
//! older macOS versions (same rule as the Linux PipeWire backend / libv4l2:
//! no build-time framework link). On macOS < 12.3 the framework is absent,
//! `open()` returns `0`, and the widget keeps its test pattern.
//!
//! Flow (the same push → pull seam as `camera/avfoundation.rs`):
//!   1. `CGPreflightScreenCaptureAccess` / `CGRequestScreenCaptureAccess` (dlsym'd from
//!      CoreGraphics, 10.15+) trigger the Screen-Recording TCC prompt. For a terminal-launched
//!      binary the grant is attributed to the *responsible process* (Terminal); detached launches
//!      are denied.
//!   2. `SCShareableContent` enumerates displays (completion-handler block).
//!   3. `SCContentFilter` (whole display) + `SCStreamConfiguration` (the widget's format - NV12
//!      '420v', what the H.264 encoder takes, or BGRA - at the size the consumers need, 15 fps
//!      unless asked otherwise: screen text wants resolution more than frames) + `SCStream` + an
//!      `SCStreamOutput` delegate registered via `define_class!` (protocol added dynamically — it
//!      only exists once the framework is loaded).
//!   4. The delegate parks each COMPLETE frame (the frame-status attachment: idle / blank /
//!      suspended frames are skipped) in a shared slot as it came, no conversion; `read` takes it.
//!      Screens only produce frames ON CHANGE, so an unchanged desktop is `Idle` and costs
//!      nothing.

use std::{
    ffi::c_void,
    sync::{mpsc, Arc, OnceLock},
    time::Duration,
};

use block2::RcBlock;
use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, AnyProtocol},
    AllocAnyThread, ClassType, DefinedClass,
};
use objc2_core_media::{CMSampleBuffer, CMTime, CMTimeFlags};
use objc2_core_video::{
    CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType, CVPixelBufferGetWidth,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSString};

/// SCStreamOutputType.screen
const SC_STREAM_OUTPUT_TYPE_SCREEN: isize = 0;
/// `SCFrameStatus.complete`: a new frame.
const SC_FRAME_STATUS_COMPLETE: isize = 0;
/// `SCFrameStatus.started`: the first frame after the stream started.
const SC_FRAME_STATUS_STARTED: isize = 4;
/// The frame rate when the request leaves it to the backend.
const DEFAULT_SCREEN_FPS: u32 = 15;

use azul_core::resources::RawImageFormat;

use crate::desktop::extra::camera::avfoundation::{cv_pixel_format_for, publish_pixel_buffer};

extern "C" {
    /// CoreMedia (linked through objc2-core-media): the per-sample
    /// attachment dictionaries (a CFArray, toll-free bridged to NSArray).
    fn CMSampleBufferGetSampleAttachmentsArray(
        sbuf: *const c_void,
        create_if_necessary: u8,
    ) -> *const c_void;
}

/// `SCStreamFrameInfoStatus` (an NSString constant of ScreenCaptureKit),
/// read once from the loaded framework. `None` when it cannot be found: then
/// every frame with pixels counts as complete.
fn frame_status_key() -> Option<*mut AnyObject> {
    static KEY: OnceLock<usize> = OnceLock::new();
    let key = *KEY.get_or_init(|| {
        let Some(Some(lib)) = SCK_LIB.get() else {
            return 0;
        };
        // SAFETY: a data symbol of type `NSString *const`; read once.
        unsafe {
            match lib.get::<*const *mut AnyObject>(b"SCStreamFrameInfoStatus\0") {
                Ok(sym) => (**sym) as usize,
                Err(_) => 0,
            }
        }
    });
    (key != 0).then_some(key as *mut AnyObject)
}

/// Whether a sample buffer is a new frame: its `SCStreamFrameInfoStatus` is
/// complete (or started). Idle / blank / suspended / stopped status buffers
/// are skipped before any pixel is touched. `true` when the status cannot be
/// read.
///
/// # Safety
/// `sample_buffer` must be a live sample buffer.
unsafe fn is_complete_frame(sample_buffer: &CMSampleBuffer) -> bool {
    let Some(key) = frame_status_key() else {
        return true;
    };
    // SAFETY: CoreMedia on a live sample buffer; the array (if any) is
    // borrowed from it (Get rule) and read through NSArray / NSDictionary,
    // their toll-free bridged counterparts.
    unsafe {
        let array = CMSampleBufferGetSampleAttachmentsArray(
            sample_buffer as *const CMSampleBuffer as *const c_void,
            0,
        ) as *mut AnyObject;
        if array.is_null() {
            return true;
        }
        let count: usize = msg_send![array, count];
        if count == 0 {
            return true;
        }
        let dict: *mut AnyObject = msg_send![array, objectAtIndex: 0usize];
        if dict.is_null() {
            return true;
        }
        let status: *mut AnyObject = msg_send![dict, objectForKey: key];
        if status.is_null() {
            return true;
        }
        let status: isize = msg_send![status, integerValue];
        status == SC_FRAME_STATUS_COMPLETE || status == SC_FRAME_STATUS_STARTED
    }
}

use azul_layout::widgets::capture_common::{CaptureRead, CaptureRequest};

use crate::desktop::extra::capture_slot::CaptureSlot;

// ---------------------------------------------------------------------------
// Runtime framework loading
// ---------------------------------------------------------------------------

/// Keeps ScreenCaptureKit resident once loaded (classes stay registered).
static SCK_LIB: OnceLock<Option<libloading::Library>> = OnceLock::new();

/// dlopen ScreenCaptureKit; `None` on macOS < 12.3 (framework absent).
fn ensure_sck_loaded() -> bool {
    SCK_LIB
        .get_or_init(|| {
            let path = "/System/Library/Frameworks/ScreenCaptureKit.framework/ScreenCaptureKit";
            match unsafe { libloading::Library::new(path) } {
                Ok(l) => {
                    crate::plog_info!("[screencap] ScreenCaptureKit loaded (macOS 12.3+)");
                    Some(l)
                }
                Err(e) => {
                    crate::plog_warn!(
                        "[screencap] ScreenCaptureKit unavailable (needs macOS 12.3+): {}",
                        e
                    );
                    None
                }
            }
        })
        .is_some()
}

fn sck_class(name: &str) -> Option<&'static AnyClass> {
    let cname = std::ffi::CString::new(name).ok()?;
    AnyClass::get(&cname)
}

/// Screen-Recording TCC preflight/request via CoreGraphics (10.15+). Returns
/// whether capture access is (now) granted; `true` when the symbols are
/// missing (pre-10.15 — no TCC gate existed) so we never block older systems.
fn ensure_screen_access() -> bool {
    unsafe {
        let cg = match libloading::Library::new(
            "/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics",
        ) {
            Ok(l) => l,
            Err(_) => return true,
        };
        let preflight: libloading::Symbol<'_, unsafe extern "C" fn() -> bool> =
            match cg.get(b"CGPreflightScreenCaptureAccess\0") {
                Ok(s) => s,
                Err(_) => return true,
            };
        if preflight() {
            // MWA-C-permission: inform the PermissionManager (this backend
            // used to bypass it entirely — ScreenCapture state stayed
            // NotDetermined forever).
            azul_layout::managers::permission::push_async_result(
                azul_layout::managers::permission::Capability::ScreenCapture,
                azul_layout::managers::permission::PermissionState::Granted(
                    azul_layout::managers::permission::PermissionQuality::Full,
                ),
            );
            return true;
        }
        // Not granted yet — trigger the prompt (first time) / System-Settings
        // deep link. The user may grant asynchronously; report current state.
        if let Ok(request) =
            cg.get::<unsafe extern "C" fn() -> bool>(b"CGRequestScreenCaptureAccess\0")
        {
            let granted = request();
            crate::plog_warn!(
                "[screencap] Screen-Recording permission {} — grant it under System Settings → \
                 Privacy & Security → Screen Recording (a terminal-launched binary is listed as \
                 the terminal app), then retry",
                if granted {
                    "granted"
                } else {
                    "not yet granted"
                }
            );
            // MWA-C-permission: park the outcome for the manager. "Not yet
            // granted" after a request = the user must act in System
            // Settings → report Denied (could_re_prompt stays false, which
            // matches macOS: re-prompting is impossible until reset).
            azul_layout::managers::permission::push_async_result(
                azul_layout::managers::permission::Capability::ScreenCapture,
                if granted {
                    azul_layout::managers::permission::PermissionState::Granted(
                        azul_layout::managers::permission::PermissionQuality::Full,
                    )
                } else {
                    azul_layout::managers::permission::PermissionState::Denied
                },
            );
            return granted;
        }
        false
    }
}

// ---------------------------------------------------------------------------
// SCStreamOutput delegate
// ---------------------------------------------------------------------------

struct OutputIvars {
    slot: Arc<CaptureSlot>,
    /// The widget asked for RGBA8 (the old contract): BGRA is swizzled.
    want_rgba: bool,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "AzulScreenCapOutput"]
    #[ivars = OutputIvars]
    struct ScreenCapOutput;

    unsafe impl NSObjectProtocol for ScreenCapOutput {}

    impl ScreenCapOutput {
        /// `-[SCStreamOutput stream:didOutputSampleBuffer:ofType:]`. The
        /// protocol is attached dynamically in `attach_protocol` (it only
        /// exists at runtime, after the framework loads).
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        unsafe fn stream_did_output(
            &self,
            _stream: *mut AnyObject,
            sample_buffer: *mut CMSampleBuffer,
            of_type: isize,
        ) {
            if of_type != SC_STREAM_OUTPUT_TYPE_SCREEN || sample_buffer.is_null() {
                return;
            }
            unsafe {
                let sample_buffer = &*sample_buffer;
                // Idle / blank / suspended frames: nothing new to show or
                // send, and nothing to copy.
                if !is_complete_frame(sample_buffer) {
                    return;
                }
                let image = match sample_buffer.image_buffer() {
                    Some(i) => i,
                    // Idle/status-only sample buffers carry no pixels — skip.
                    None => return,
                };
                let pb = &*image;
                // kCVPixelBufferLock_ReadOnly (1): the planes are only read. A
                // read-write lock tells CoreVideo the CPU may have changed the
                // buffer, which invalidates its GPU / IOSurface caches for nothing.
                CVPixelBufferLockBaseAddress(pb, CVPixelBufferLockFlags(1));
                // Copy the planes (or rows) into the slot's REUSED buffer as
                // they are and wake the reader; the slot validates them.
                let ivars = self.ivars();
                if publish_pixel_buffer(&ivars.slot, pb, ivars.want_rgba) {
                    crate::plog_info!(
                        "[screencap] ScreenCaptureKit: first frame {}x{} pixel format {:#010x}",
                        CVPixelBufferGetWidth(pb),
                        CVPixelBufferGetHeight(pb),
                        CVPixelBufferGetPixelFormatType(pb)
                    );
                }
                CVPixelBufferUnlockBaseAddress(pb, CVPixelBufferLockFlags(1));
            }
        }
    }
);

impl ScreenCapOutput {
    fn new(slot: Arc<CaptureSlot>, want_rgba: bool) -> Retained<Self> {
        let this = Self::alloc().set_ivars(OutputIvars { slot, want_rgba });
        unsafe { msg_send![super(this), init] }
    }

    /// Attach the (runtime-only) `SCStreamOutput` protocol to our class so
    /// `-conformsToProtocol:` checks inside SCStream pass. Once.
    fn attach_protocol() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| unsafe {
            if let Some(proto) = AnyProtocol::get(c"SCStreamOutput") {
                let cls = Self::class();
                objc2::ffi::class_addProtocol(cls as *const AnyClass as *mut AnyClass, proto);
            }
        });
    }
}

/// `-[NSError localizedDescription]` for logs ("" when null).
unsafe fn error_desc(err: *mut AnyObject) -> String {
    if err.is_null() {
        return String::new();
    }
    unsafe {
        let desc: *mut AnyObject = msg_send![&*err, localizedDescription];
        if desc.is_null() {
            return String::new();
        }
        (*(desc as *const NSString)).to_string()
    }
}

/// Raw ObjC pointer made `Send` for the completion-handler → open() channel
/// hop (the pointee is retained before sending; released by the receiver).
struct SendPtr(*mut AnyObject);
unsafe impl Send for SendPtr {}

// ---------------------------------------------------------------------------
// Live capture handle
// ---------------------------------------------------------------------------

/// Live capture state behind the seam's `u64` handle (worker-thread-local).
struct SckScreen {
    stream: Retained<AnyObject>,
    _output: Retained<ScreenCapOutput>,
    /// The sample-handler dispatch queue must outlive the stream.
    _queue: dispatch2::DispatchRetained<dispatch2::DispatchQueue>,
    slot: Arc<CaptureSlot>,
    last_seq: u64,
    /// The captured source's size in points — the output size for a zero
    /// request (`reconfigure` needs it too).
    source_size: (usize, usize),
}

/// A fresh `SCStreamConfiguration`: the widget's pixel format (NV12 '420v'
/// for the encoder, or BGRA), cursor on, `width` x `height` output (SCK
/// scales on the GPU), `fps` (0 -> 15). Shared by `open` and `reconfigure`.
unsafe fn make_config(
    sc_config: &AnyClass,
    width: usize,
    height: usize,
    fps: u32,
    format: RawImageFormat,
) -> Option<Retained<AnyObject>> {
    let config: *mut AnyObject = msg_send![sc_config, new];
    let config = unsafe { Retained::from_raw(config)? };
    let _: () = msg_send![&*config, setWidth: width];
    let _: () = msg_send![&*config, setHeight: height];
    let _: () = msg_send![&*config, setPixelFormat: cv_pixel_format_for(format)];
    let _: () = msg_send![&*config, setShowsCursor: true];
    let _: () = msg_send![&*config, setQueueDepth: 5isize];
    let fps = if fps > 0 { fps } else { DEFAULT_SCREEN_FPS };
    let interval = CMTime {
        value: 1,
        timescale: fps as i32,
        flags: CMTimeFlags::Valid,
        epoch: 0,
    };
    let _: () = msg_send![&*config, setMinimumFrameInterval: interval];
    Some(config)
}

/// Open the source named by the request — display `index` (clamped) or the
/// window `request.window` — and start an SCStream at the requested size and
/// fps, BGRA, with this process's own windows left out when
/// `exclude_self`. Returns a boxed handle, or `0` on failure (test-pattern
/// fallback).
pub(super) fn open(request: &CaptureRequest) -> u64 {
    let (index, width, height) = (request.index, request.width, request.height);
    if !ensure_sck_loaded() {
        return 0;
    }
    if !ensure_screen_access() {
        return 0;
    }
    let (sc_content, sc_filter, sc_config, sc_stream) = match (
        sck_class("SCShareableContent"),
        sck_class("SCContentFilter"),
        sck_class("SCStreamConfiguration"),
        sck_class("SCStream"),
    ) {
        (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
        _ => {
            crate::plog_warn!("[screencap] ScreenCaptureKit classes missing — cannot capture");
            return 0;
        }
    };

    unsafe {
        // -- 1. Shareable content (async → block → channel) ------------------
        let (tx, rx) = mpsc::channel::<Result<SendPtr, String>>();
        let tx2 = tx.clone();
        let block = RcBlock::new(move |content: *mut AnyObject, error: *mut AnyObject| {
            if !content.is_null() {
                // Retain across the channel hop; balanced by from_raw below.
                let _: *mut AnyObject = msg_send![&*content, retain];
                let _ = tx2.send(Ok(SendPtr(content)));
            } else {
                let _ = tx2.send(Err(error_desc(error)));
            }
        });
        let _: () = msg_send![
            sc_content,
            getShareableContentExcludingDesktopWindows: true,
            onScreenWindowsOnly: true,
            completionHandler: &*block
        ];
        let content: Retained<AnyObject> = match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(ptr)) => match Retained::from_raw(ptr.0) {
                Some(c) => c,
                None => return 0,
            },
            Ok(Err(e)) => {
                crate::plog_warn!(
                    "[screencap] SCShareableContent failed: {} (Screen-Recording permission?)",
                    e
                );
                return 0;
            }
            Err(_) => {
                crate::plog_warn!("[screencap] SCShareableContent timed out");
                return 0;
            }
        };

        // -- 2. Pick the display --------------------------------------------
        let displays: *mut AnyObject = msg_send![&*content, displays];
        if displays.is_null() {
            return 0;
        }
        let displays = &*(displays as *const NSArray<AnyObject>);
        let count = displays.count();
        if count == 0 {
            crate::plog_warn!("[screencap] no shareable displays");
            return 0;
        }
        let idx = (index as usize).min(count - 1);
        let display: *mut AnyObject = msg_send![displays, objectAtIndex: idx];

        // Display size in points; SCK scales output to the config size.
        let disp_w: isize = msg_send![&*display, width];
        let disp_h: isize = msg_send![&*display, height];
        let out_w = if width > 0 {
            width as usize
        } else {
            disp_w.max(1) as usize
        };
        let out_h = if height > 0 {
            height as usize
        } else {
            disp_h.max(1) as usize
        };

        // -- 3. Filter + configuration ---------------------------------------
        //
        // A specific WINDOW (`config.source = Window(id)`) is captured on its
        // own; a display is captured with THIS PROCESS'S OWN WINDOWS EXCLUDED
        // (`exclude_self`): a shared desktop that shows the sharing app used
        // to loop — every tile repaint was a screen change, which emitted a
        // frame, which repainted the tile — at a steady 30 fps on an idle
        // desktop.
        let empty: Retained<NSArray<AnyObject>> = NSArray::new();
        let mut filter: *mut AnyObject = core::ptr::null_mut();
        let (mut out_w, mut out_h) = (out_w, out_h);
        if request.window != 0 {
            let windows: *mut AnyObject = msg_send![&*content, windows];
            if !windows.is_null() {
                let windows = &*(windows as *const NSArray<AnyObject>);
                for i in 0..windows.count() {
                    let w: *mut AnyObject = msg_send![windows, objectAtIndex: i];
                    let id: u32 = msg_send![&*w, windowID];
                    if u64::from(id) == request.window {
                        let f: *mut AnyObject = msg_send![sc_filter, alloc];
                        filter = msg_send![f, initWithDesktopIndependentWindow: w];
                        // The window's frame in points sizes a zero request.
                        if width == 0 || height == 0 {
                            let frame: objc2_foundation::NSRect = msg_send![&*w, frame];
                            out_w = (frame.size.width.max(1.0)) as usize;
                            out_h = (frame.size.height.max(1.0)) as usize;
                        }
                        break;
                    }
                }
            }
            if filter.is_null() {
                crate::plog_warn!(
                    "[screencap] window {} not shareable (closed / off-screen?) — capturing the \
                     display instead",
                    request.window
                );
            }
        }
        if filter.is_null() && request.exclude_self {
            let apps: *mut AnyObject = msg_send![&*content, applications];
            if !apps.is_null() {
                let apps = &*(apps as *const NSArray<AnyObject>);
                let me = std::process::id() as i32;
                for i in 0..apps.count() {
                    let app: *mut AnyObject = msg_send![apps, objectAtIndex: i];
                    let pid: i32 = msg_send![&*app, processID];
                    if pid == me {
                        let ours: Retained<NSArray<AnyObject>> = NSArray::from_retained_slice(&[
                            Retained::retain(app).expect("non-null app"),
                        ]);
                        let f: *mut AnyObject = msg_send![sc_filter, alloc];
                        filter = msg_send![
                            f,
                            initWithDisplay: display,
                            excludingApplications: &*ours,
                            exceptingWindows: &*empty
                        ];
                        break;
                    }
                }
            }
        }
        if filter.is_null() {
            let f: *mut AnyObject = msg_send![sc_filter, alloc];
            filter = msg_send![f, initWithDisplay: display, excludingWindows: &*empty];
        }
        let filter = match Retained::from_raw(filter) {
            Some(f) => f,
            None => return 0,
        };

        let config = match make_config(sc_config, out_w, out_h, request.fps, request.format) {
            Some(c) => c,
            None => return 0,
        };

        // -- 4. Stream + output delegate -------------------------------------
        let stream: *mut AnyObject = msg_send![sc_stream, alloc];
        let nil_delegate: *mut AnyObject = core::ptr::null_mut();
        let stream: *mut AnyObject = msg_send![
            stream,
            initWithFilter: &*filter,
            configuration: &*config,
            delegate: nil_delegate
        ];
        let stream = match Retained::from_raw(stream) {
            Some(s) => s,
            None => return 0,
        };

        ScreenCapOutput::attach_protocol();
        let slot = CaptureSlot::new();
        let output = ScreenCapOutput::new(slot.clone(), request.format == RawImageFormat::RGBA8);
        let queue = dispatch2::DispatchQueue::new("azul.screencap", None);
        let queue_ptr: *mut AnyObject = &*queue as *const _ as *mut AnyObject;

        // `type` is a Rust keyword and objc2's msg_send! registers `r#type:`
        // literally (no raw-prefix strip), so this one selector is sent by
        // hand through objc_msgSend.
        let mut err: *mut AnyObject = core::ptr::null_mut();
        let sel = objc2::runtime::Sel::register(c"addStreamOutput:type:sampleHandlerQueue:error:");
        let send: unsafe extern "C" fn(
            *mut AnyObject,
            objc2::runtime::Sel,
            *mut AnyObject,
            isize,
            *mut AnyObject,
            *mut *mut AnyObject,
        ) -> objc2::runtime::Bool = core::mem::transmute(objc2::ffi::objc_msgSend as *const c_void);
        let ok = send(
            Retained::as_ptr(&stream) as *mut AnyObject,
            sel,
            Retained::as_ptr(&output) as *const ScreenCapOutput as *mut AnyObject,
            SC_STREAM_OUTPUT_TYPE_SCREEN,
            queue_ptr,
            &mut err,
        )
        .as_bool();
        if !ok {
            crate::plog_warn!("[screencap] addStreamOutput failed: {}", error_desc(err));
            return 0;
        }

        // -- 5. Start (async → block → channel) -------------------------------
        let (stx, srx) = mpsc::channel::<String>();
        let sblock = RcBlock::new(move |error: *mut AnyObject| {
            let _ = stx.send(if error.is_null() {
                String::new()
            } else {
                error_desc(error)
            });
        });
        let _: () = msg_send![&*stream, startCaptureWithCompletionHandler: &*sblock];
        match srx.recv_timeout(Duration::from_secs(10)) {
            Ok(e) if e.is_empty() => {}
            Ok(e) => {
                crate::plog_warn!("[screencap] startCapture failed: {}", e);
                return 0;
            }
            Err(_) => {
                crate::plog_warn!("[screencap] startCapture timed out");
                return 0;
            }
        }

        crate::plog_info!(
            "[screencap] ScreenCaptureKit: display {} of {} → {}x{} {:?} @{}fps{}{}",
            idx,
            count,
            out_w,
            out_h,
            request.format,
            request.fps_or(DEFAULT_SCREEN_FPS),
            if request.window != 0 { " (window)" } else { "" },
            if request.exclude_self {
                ", own windows excluded"
            } else {
                ""
            }
        );
        Box::into_raw(Box::new(SckScreen {
            stream,
            _output: output,
            _queue: queue,
            slot,
            last_seq: 0,
            source_size: (disp_w.max(1) as usize, disp_h.max(1) as usize),
        })) as u64
    }
}

/// Take the newest frame into `out` (swapping buffers with the slot, no copy)
/// in the format it was published in. Screens only emit on CHANGE,
/// so after the bounded wait an idle desktop is `Idle` — NOT end-of-stream,
/// and NOT the previous frame re-served as a new buffer (that made an
/// unchanged picture repaint the tile once a second).
pub(super) fn read(handle: u64, out: &mut Vec<u8>) -> CaptureRead {
    let scr = match unsafe { (handle as *mut SckScreen).as_mut() } {
        Some(s) => s,
        None => return CaptureRead::Ended,
    };
    match scr
        .slot
        .take_newer(&mut scr.last_seq, out, Duration::from_millis(1000))
    {
        Some((width, height, format)) => CaptureRead::FrameIn {
            width,
            height,
            format,
        },
        None => CaptureRead::Idle,
    }
}

/// Change the output size / fps of the RUNNING stream
/// (`updateConfiguration:completionHandler:`), so a resized tile or a new
/// consumer does not restart the capture. `false` on failure (the worker
/// reopens).
pub(super) fn reconfigure(handle: u64, request: &CaptureRequest) -> bool {
    let scr = match unsafe { (handle as *mut SckScreen).as_mut() } {
        Some(s) => s,
        None => return false,
    };
    let Some(sc_config) = sck_class("SCStreamConfiguration") else {
        return false;
    };
    let out_w = if request.width > 0 {
        request.width as usize
    } else {
        scr.source_size.0
    };
    let out_h = if request.height > 0 {
        request.height as usize
    } else {
        scr.source_size.1
    };
    unsafe {
        let Some(config) = make_config(sc_config, out_w, out_h, request.fps, request.format) else {
            return false;
        };
        let (tx, rx) = mpsc::channel::<String>();
        let block = RcBlock::new(move |error: *mut AnyObject| {
            let _ = tx.send(if error.is_null() {
                String::new()
            } else {
                error_desc(error)
            });
        });
        let _: () =
            msg_send![&*scr.stream, updateConfiguration: &*config, completionHandler: &*block];
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(e) if e.is_empty() => {
                crate::plog_info!(
                    "[screencap] ScreenCaptureKit: reconfigured to {}x{} @{}fps (live)",
                    out_w,
                    out_h,
                    request.fps_or(DEFAULT_SCREEN_FPS)
                );
                true
            }
            Ok(e) => {
                crate::plog_warn!("[screencap] updateConfiguration failed: {}", e);
                false
            }
            Err(_) => {
                crate::plog_warn!("[screencap] updateConfiguration timed out");
                false
            }
        }
    }
}

/// Stop the stream + free the capture (drops the boxed `SckScreen`).
pub(super) fn close(handle: u64) {
    if handle != 0 {
        unsafe {
            let scr = Box::from_raw(handle as *mut SckScreen);
            let noop = RcBlock::new(move |_error: *mut AnyObject| {});
            let _: () = msg_send![&*scr.stream, stopCaptureWithCompletionHandler: &*noop];
            // Give the stop a moment so the sample queue drains before drop.
            std::thread::sleep(Duration::from_millis(50));
            drop(scr);
        }
    }
}
