//! macOS screen-capture backend via **ScreenCaptureKit** (macOS 12.3+).
//!
//! Everything is resolved AT RUNTIME — the framework is `dlopen`ed and every
//! class is looked up with `AnyClass::get` — so the dylib links and loads on
//! older macOS versions (same rule as the Linux PipeWire backend / libv4l2:
//! no build-time framework link). On macOS < 12.3 the framework is absent,
//! `open()` returns `0`, and the widget keeps its test pattern.
//!
//! Flow (the same push → pull seam as `camera/avfoundation.rs`):
//!   1. `CGPreflightScreenCaptureAccess` / `CGRequestScreenCaptureAccess`
//!      (dlsym'd from CoreGraphics, 10.15+) trigger the Screen-Recording TCC
//!      prompt. For a terminal-launched binary the grant is attributed to the
//!      *responsible process* (Terminal); detached launches are denied.
//!   2. `SCShareableContent` enumerates displays (completion-handler block).
//!   3. `SCContentFilter` (whole display) + `SCStreamConfiguration` (BGRA,
//!      ~30 fps) + `SCStream` + an `SCStreamOutput` delegate registered via
//!      `define_class!` (protocol added dynamically — it only exists once the
//!      framework is loaded).
//!   4. The delegate parks BGRA→RGBA frames in a shared slot; `read` drains
//!      it. Screens only produce frames ON CHANGE, so `read` re-returns the
//!      last frame on timeout instead of `(0,0)` (which would stop the worker).

use std::ffi::c_void;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, AnyProtocol};
use objc2::{define_class, msg_send, AllocAnyThread, ClassType, DefinedClass};
use objc2_core_media::{CMSampleBuffer, CMTime, CMTimeFlags};
use objc2_core_video::{
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSString};

/// kCVPixelFormatType_32BGRA ('BGRA'), same as the camera backend.
const PIXEL_FORMAT_32BGRA: u32 = 0x42475241;
/// SCStreamOutputType.screen
const SC_STREAM_OUTPUT_TYPE_SCREEN: isize = 0;

/// Latest captured frame (RGBA + dims), filled by the delegate, drained by read.
#[derive(Default)]
struct FrameSlot {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    /// Bumped each frame; `read` returns the newest available.
    seq: u64,
}

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
            return true;
        }
        // Not granted yet — trigger the prompt (first time) / System-Settings
        // deep link. The user may grant asynchronously; report current state.
        if let Ok(request) = cg.get::<unsafe extern "C" fn() -> bool>(b"CGRequestScreenCaptureAccess\0") {
            let granted = request();
            crate::plog_warn!(
                "[screencap] Screen-Recording permission {} — grant it under System Settings → \
                 Privacy & Security → Screen Recording (a terminal-launched binary is listed as \
                 the terminal app), then retry",
                if granted { "granted" } else { "not yet granted" }
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
    slot: Arc<Mutex<FrameSlot>>,
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
                let image = match sample_buffer.image_buffer() {
                    Some(i) => i,
                    // Idle/status-only sample buffers carry no pixels — skip.
                    None => return,
                };
                let pb = &*image;
                CVPixelBufferLockBaseAddress(pb, CVPixelBufferLockFlags(0));
                let w = CVPixelBufferGetWidth(pb) as usize;
                let h = CVPixelBufferGetHeight(pb) as usize;
                let stride = CVPixelBufferGetBytesPerRow(pb);
                let base = CVPixelBufferGetBaseAddress(pb) as *const u8;
                if !base.is_null() && w > 0 && h > 0 && stride >= w * 4 {
                    let mut rgba = vec![0u8; w * h * 4];
                    for y in 0..h {
                        let row = base.add(y * stride);
                        for x in 0..w {
                            let s = row.add(x * 4); // BGRA
                            let o = (y * w + x) * 4;
                            rgba[o] = *s.add(2); // R
                            rgba[o + 1] = *s.add(1); // G
                            rgba[o + 2] = *s; // B
                            rgba[o + 3] = 255;
                        }
                    }
                    if let Ok(mut slot) = self.ivars().slot.lock() {
                        if slot.seq == 0 {
                            crate::plog_info!(
                                "[screencap] ScreenCaptureKit: first frame {}x{} stride={} BGRA→RGBA ok",
                                w, h, stride
                            );
                        }
                        slot.rgba = rgba;
                        slot.width = w as u32;
                        slot.height = h as u32;
                        slot.seq = slot.seq.wrapping_add(1);
                    }
                }
                CVPixelBufferUnlockBaseAddress(pb, CVPixelBufferLockFlags(0));
            }
        }
    }
);

impl ScreenCapOutput {
    fn new(slot: Arc<Mutex<FrameSlot>>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(OutputIvars { slot });
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
    slot: Arc<Mutex<FrameSlot>>,
    last_seq: u64,
}

/// Open display `index` (clamped) and start an SCStream at ~30 fps BGRA.
/// Returns a boxed handle, or `0` on failure (test-pattern fallback).
pub fn open(index: u32, width: u32, height: u32) -> u64 {
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
        let out_w = if width > 0 { width as usize } else { disp_w.max(1) as usize };
        let out_h = if height > 0 { height as usize } else { disp_h.max(1) as usize };

        // -- 3. Filter + configuration ---------------------------------------
        let empty: Retained<NSArray<AnyObject>> = NSArray::new();
        let filter: *mut AnyObject = msg_send![sc_filter, alloc];
        let filter: *mut AnyObject = msg_send![
            filter,
            initWithDisplay: display,
            excludingWindows: &*empty
        ];
        let filter = match Retained::from_raw(filter) {
            Some(f) => f,
            None => return 0,
        };

        let config: *mut AnyObject = msg_send![sc_config, new];
        let config = match Retained::from_raw(config) {
            Some(c) => c,
            None => return 0,
        };
        let _: () = msg_send![&*config, setWidth: out_w];
        let _: () = msg_send![&*config, setHeight: out_h];
        let _: () = msg_send![&*config, setPixelFormat: PIXEL_FORMAT_32BGRA];
        let _: () = msg_send![&*config, setShowsCursor: true];
        let _: () = msg_send![&*config, setQueueDepth: 5isize];
        let interval = CMTime {
            value: 1,
            timescale: 30,
            flags: CMTimeFlags::Valid,
            epoch: 0,
        };
        let _: () = msg_send![&*config, setMinimumFrameInterval: interval];

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
        let slot = Arc::new(Mutex::new(FrameSlot::default()));
        let output = ScreenCapOutput::new(slot.clone());
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
        ) -> objc2::runtime::Bool =
            core::mem::transmute(objc2::ffi::objc_msgSend as *const c_void);
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
            "[screencap] ScreenCaptureKit: display {} of {} → {}x{} BGRA @30fps",
            idx, count, out_w, out_h
        );
        Box::into_raw(Box::new(SckScreen {
            stream,
            _output: output,
            _queue: queue,
            slot,
            last_seq: 0,
        })) as u64
    }
}

/// Drain the newest frame into `out` (RGBA8). Screens only emit on CHANGE, so
/// after a bounded wait the *previous* frame is re-returned (returning `(0,0)`
/// would make the worker treat an idle desktop as end-of-stream).
pub fn read(handle: u64, out: &mut Vec<u8>) -> (u32, u32) {
    let scr = match unsafe { (handle as *mut SckScreen).as_mut() } {
        Some(s) => s,
        None => return (0, 0),
    };
    for _ in 0..120 {
        {
            let slot = match scr.slot.lock() {
                Ok(s) => s,
                Err(_) => return (0, 0),
            };
            if slot.seq != scr.last_seq && slot.width > 0 {
                scr.last_seq = slot.seq;
                out.clear();
                out.extend_from_slice(&slot.rgba);
                return (slot.width, slot.height);
            }
        }
        std::thread::sleep(Duration::from_millis(8));
    }
    // No new frame (idle screen) — re-serve the last one, if any.
    if let Ok(slot) = scr.slot.lock() {
        if slot.width > 0 {
            out.clear();
            out.extend_from_slice(&slot.rgba);
            return (slot.width, slot.height);
        }
    }
    (0, 0)
}

/// Stop the stream + free the capture (drops the boxed `SckScreen`).
pub fn close(handle: u64) {
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
