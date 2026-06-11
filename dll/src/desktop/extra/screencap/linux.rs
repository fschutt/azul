//! Linux screen capture: xdg-desktop-portal **ScreenCast** → **PipeWire**.
//!
//! `open()` runs the portal handshake over the session D-Bus (zbus, blocking):
//! `CreateSession` → `SelectSources` → `Start` (this is where the desktop
//! shows its share-picker dialog, exactly like in a browser) →
//! `OpenPipeWireRemote` (a connected PipeWire socket fd). It then dlopens
//! `libpipewire-0.3.so.0`, connects an INPUT video stream to the granted node
//! and copies each frame into a shared latest-frame slot that `read()` blocks
//! on. All SPA pods (format negotiation) are hand-assembled/parsed — the SPA
//! pod wire format is a stable public ABI and the ~hundred lines here beat a
//! bindgen dependency for a single stream.
//!
//! Every failure path returns `0` from `open()`, which makes the widget fall
//! back to its test pattern — the backend can never take the app down.

use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CString};
use std::os::fd::{IntoRawFd, OwnedFd};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

/// `AZ_SCREENCAP_DEBUG=1` traces the portal/PipeWire handshake to stderr,
/// independent of the dll's `logging` feature (the demos build without it).
macro_rules! scd {
    ($($arg:tt)*) => {{
        if std::env::var_os("AZ_SCREENCAP_DEBUG").is_some() {
            eprintln!("[screencap] {}", format!($($arg)*));
        }
    }};
}

// ---------------------------------------------------------------------------
// Shared frame state (written by the PipeWire process callback, read by read())
// ---------------------------------------------------------------------------

#[derive(Default)]
struct FrameSlot {
    /// Tightly packed pixels in the NEGOTIATED format (converted in read()).
    data: Vec<u8>,
    width: u32,
    height: u32,
    /// Negotiated spa_video_format (RGBx/BGRx/RGBA/BGRA).
    spa_format: u32,
    /// Bumped on every new frame; read() waits for a change.
    seq: u64,
    /// Stream hit an error / EOS.
    dead: bool,
}

struct Shared {
    slot: Mutex<FrameSlot>,
    cond: Condvar,
}

struct Session {
    shared: Arc<Shared>,
    /// PipeWire objects — torn down in close() (loop must stop first).
    pw: Arc<PwLib>,
    thread_loop: *mut c_void,
    context: *mut c_void,
    core: *mut c_void,
    stream: *mut c_void,
    /// Keeps the events vtable + hook + shared-ptr alive for the C side.
    _events: Box<PwStreamEvents>,
    _hook: Box<[u8; 128]>,
    _ctx: *mut StreamCtx,
    /// Keep the D-Bus session alive (the portal closes the cast when the
    /// session object drops).
    _dbus: zbus::blocking::Connection,
    session_path: String,
}

// The raw pointers are only touched from close() and the pw loop's own thread.
unsafe impl Send for Session {}

static SESSIONS: OnceLock<Mutex<HashMap<u64, Session>>> = OnceLock::new();
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

fn sessions() -> &'static Mutex<HashMap<u64, Session>> {
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

// ---------------------------------------------------------------------------
// Portal handshake (zbus, blocking)
// ---------------------------------------------------------------------------

/// The portal's request/response pattern, hand-rolled: every ScreenCast call
/// takes a `handle_token`, returns a Request object path, and the actual
/// result arrives as a `Response(u, a{sv})` signal on that path.
struct Portal {
    conn: zbus::blocking::Connection,
    proxy: zbus::blocking::Proxy<'static>,
    sender_token: String,
    counter: u32,
}

impl Portal {
    fn new(conn: &zbus::blocking::Connection) -> Option<Self> {
        let conn = conn.clone();
        let proxy = zbus::blocking::Proxy::new(
            &conn,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.ScreenCast",
        )
        .ok()?;
        let unique = conn.unique_name()?.to_string(); // ":1.123"
        let sender_token = unique.trim_start_matches(':').replace('.', "_");
        Some(Self { conn, proxy, sender_token, counter: 0 })
    }

    /// A fresh per-request handle token (the portal echoes it back via the
    /// Request object path the caller must subscribe to).
    fn next_token(&mut self) -> String {
        self.counter += 1;
        format!("azul{}", self.counter)
    }

    /// Invoke a request/response portal method. `args` is the FULL, concretely
    /// typed argument tuple (so zbus emits the right D-Bus signature — passing
    /// a wrapped `Value` would serialize options as `v` instead of `a{sv}` and
    /// the portal rejects the call). The caller must have put `token` into the
    /// options map. Subscribes to the predicted Request path BEFORE calling,
    /// then blocks (≤ `timeout`) for the `Response(u, a{sv})` signal.
    fn request<A>(
        &self,
        method: &str,
        token: &str,
        args: &A,
        timeout: Duration,
    ) -> Option<HashMap<String, zbus::zvariant::OwnedValue>>
    where
        A: serde::Serialize + zbus::zvariant::DynamicType,
    {
        let request_path = format!(
            "/org/freedesktop/portal/desktop/request/{}/{}",
            self.sender_token, token
        );

        // Subscribe BEFORE the call (the response can be immediate).
        let req_proxy = zbus::blocking::Proxy::new(
            &self.conn,
            "org.freedesktop.portal.Desktop",
            request_path.as_str(),
            "org.freedesktop.portal.Request",
        )
        .ok()?;
        let mut responses = req_proxy.receive_signal("Response").ok()?;

        let res: Result<zbus::zvariant::OwnedObjectPath, _> = self.proxy.call(method, args);
        if let Err(e) = &res {
            scd!("portal {} call errored: {}", method, e);
        }
        let _request_handle = res.ok()?;

        // Wait for the Response signal WITH a real timeout. `responses.next()`
        // blocks indefinitely, so a non-interacting user would hang the worker
        // forever; pull the next signal on a helper thread and bound the wait
        // with recv_timeout (the helper leaks until a signal eventually lands
        // or the process exits — fine for this one-shot handshake).
        scd!("portal {} waiting for Response (<= {:?})...", method, timeout);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            if let Some(msg) = responses.next() {
                let _ = tx.send(msg);
            }
        });
        let msg = match rx.recv_timeout(timeout) {
            Ok(m) => m,
            Err(_) => {
                crate::plog_warn!("[screencap] portal {} timed out", method);
                scd!("portal {} TIMED OUT (no Response)", method);
                return None;
            }
        };
        let body = msg.body();
        let (code, results): (u32, HashMap<String, zbus::zvariant::OwnedValue>) =
            body.deserialize().ok()?;
        if code != 0 {
            crate::plog_warn!(
                "[screencap] portal {} declined/cancelled (code {})",
                method,
                code
            );
            scd!("portal {} declined/cancelled (code {})", method, code);
            return None;
        }
        scd!("portal {} Response OK", method);
        Some(results)
    }
}

/// Full ScreenCast handshake → (pipewire fd, node_id, dbus conn, session path).
fn portal_open_stream() -> Option<(OwnedFd, u32, zbus::blocking::Connection, String)> {
    use zbus::zvariant::Value;

    let conn = zbus::blocking::Connection::session().ok()?;
    let mut portal = Portal::new(&conn)?;

    // 1. CreateSession(a{sv}) -> request path; results carry session_handle.
    let token = portal.next_token();
    let mut opts: HashMap<&'static str, Value> = HashMap::new();
    opts.insert("session_handle_token", Value::from("azulcast"));
    opts.insert("handle_token", Value::from(token.clone()));
    let results = portal.request("CreateSession", &token, &(opts,), Duration::from_secs(20))?;
    let session_handle: String = results
        .get("session_handle")
        .and_then(|v| v.downcast_ref::<zbus::zvariant::Str>().ok())
        .map(|s| s.to_string())?;
    scd!("CreateSession OK: {}", session_handle);
    let session_path =
        zbus::zvariant::OwnedObjectPath::try_from(session_handle.clone()).ok()?;

    // 2. SelectSources(o, a{sv}): monitors, single, cursor embedded in frames.
    let token = portal.next_token();
    let mut opts: HashMap<&'static str, Value> = HashMap::new();
    opts.insert("types", Value::from(1u32)); // 1 = MONITOR
    opts.insert("multiple", Value::from(false));
    opts.insert("cursor_mode", Value::from(2u32)); // 2 = EMBEDDED
    opts.insert("handle_token", Value::from(token.clone()));
    portal.request(
        "SelectSources",
        &token,
        &(session_path.clone(), opts),
        Duration::from_secs(20),
    )?;
    scd!("SelectSources OK — showing share dialog...");

    // 3. Start(o, s, a{sv}) — THIS pops the desktop's share-picker dialog.
    let token = portal.next_token();
    let mut opts: HashMap<&'static str, Value> = HashMap::new();
    opts.insert("handle_token", Value::from(token.clone()));
    let results = portal.request(
        "Start",
        &token,
        &(session_path.clone(), "", opts),
        Duration::from_secs(120),
    )?;

    // streams: a(ua{sv}) — take the first node id.
    let node_id: u32 = {
        let streams = results.get("streams")?;
        let arr: Vec<(u32, HashMap<String, zbus::zvariant::OwnedValue>)> =
            streams.try_clone().ok()?.try_into().ok()?;
        arr.first()?.0
    };

    // 4. OpenPipeWireRemote → fd (plain method, not request/response).
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.ScreenCast",
    )
    .ok()?;
    let empty: HashMap<&str, Value> = HashMap::new();
    let fd: zbus::zvariant::OwnedFd = proxy
        .call("OpenPipeWireRemote", &(session_path, empty))
        .ok()?;

    crate::plog_info!("[screencap] portal granted node {} (fd ready)", node_id);
    Some((fd.into(), node_id, conn, session_handle))
}

// ---------------------------------------------------------------------------
// PipeWire (dlopen) — minimal C ABI surface for one input video stream
// ---------------------------------------------------------------------------

#[repr(C)]
struct SpaData {
    type_: u32,
    flags: u32,
    fd: i64,
    mapoffset: u32,
    maxsize: u32,
    data: *mut c_void,
    chunk: *mut SpaChunk,
}

#[repr(C)]
struct SpaChunk {
    offset: u32,
    size: u32,
    stride: i32,
    flags: i32,
}

#[repr(C)]
struct SpaBuffer {
    n_metas: u32,
    n_datas: u32,
    metas: *mut c_void,
    datas: *mut SpaData,
}

#[repr(C)]
struct PwBuffer {
    buffer: *mut SpaBuffer,
    user_data: *mut c_void,
    size: u64,
    requested: u64,
    time: u64,
}

/// `struct pw_stream_events` (version 2, PipeWire 0.3/1.x ABI).
#[repr(C)]
struct PwStreamEvents {
    version: u32,
    destroy: Option<extern "C" fn(*mut c_void)>,
    state_changed: Option<extern "C" fn(*mut c_void, c_int, c_int, *const c_char)>,
    control_info: Option<extern "C" fn(*mut c_void, u32, *const c_void)>,
    io_changed: Option<extern "C" fn(*mut c_void, u32, *mut c_void, u32)>,
    param_changed: Option<extern "C" fn(*mut c_void, u32, *const u8)>,
    add_buffer: Option<extern "C" fn(*mut c_void, *mut PwBuffer)>,
    remove_buffer: Option<extern "C" fn(*mut c_void, *mut PwBuffer)>,
    process: Option<extern "C" fn(*mut c_void)>,
    drained: Option<extern "C" fn(*mut c_void)>,
    command: Option<extern "C" fn(*mut c_void, *const c_void)>,
    trigger_done: Option<extern "C" fn(*mut c_void)>,
}

struct PwLib {
    _lib: libloading::Library,
    pw_init: unsafe extern "C" fn(*mut c_int, *mut *mut *mut c_char),
    pw_thread_loop_new: unsafe extern "C" fn(*const c_char, *const c_void) -> *mut c_void,
    pw_thread_loop_destroy: unsafe extern "C" fn(*mut c_void),
    pw_thread_loop_start: unsafe extern "C" fn(*mut c_void) -> c_int,
    pw_thread_loop_stop: unsafe extern "C" fn(*mut c_void),
    pw_thread_loop_lock: unsafe extern "C" fn(*mut c_void),
    pw_thread_loop_unlock: unsafe extern "C" fn(*mut c_void),
    pw_thread_loop_get_loop: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pw_context_new: unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> *mut c_void,
    pw_context_destroy: unsafe extern "C" fn(*mut c_void),
    pw_context_connect_fd: unsafe extern "C" fn(*mut c_void, c_int, *mut c_void, usize) -> *mut c_void,
    pw_core_disconnect: unsafe extern "C" fn(*mut c_void) -> c_int,
    pw_stream_new: unsafe extern "C" fn(*mut c_void, *const c_char, *mut c_void) -> *mut c_void,
    pw_stream_destroy: unsafe extern "C" fn(*mut c_void),
    pw_stream_add_listener: unsafe extern "C" fn(*mut c_void, *mut c_void, *const PwStreamEvents, *mut c_void),
    pw_stream_connect: unsafe extern "C" fn(*mut c_void, c_int, u32, c_int, *mut *const u8, u32) -> c_int,
    pw_stream_disconnect: unsafe extern "C" fn(*mut c_void) -> c_int,
    pw_stream_dequeue_buffer: unsafe extern "C" fn(*mut c_void) -> *mut PwBuffer,
    pw_stream_queue_buffer: unsafe extern "C" fn(*mut c_void, *mut PwBuffer) -> c_int,
    pw_stream_update_params: unsafe extern "C" fn(*mut c_void, *mut *const u8, u32) -> c_int,
    pw_properties_new: unsafe extern "C" fn(*const c_char, ...) -> *mut c_void,
}

impl PwLib {
    fn load() -> Option<Arc<Self>> {
        static CACHE: OnceLock<Option<Arc<PwLib>>> = OnceLock::new();
        CACHE
            .get_or_init(|| unsafe {
                let lib = libloading::Library::new("libpipewire-0.3.so.0").ok()?;
                macro_rules! sym {
                    ($name:literal) => {
                        *lib.get($name).ok()?
                    };
                }
                let me = PwLib {
                    pw_init: sym!(b"pw_init"),
                    pw_thread_loop_new: sym!(b"pw_thread_loop_new"),
                    pw_thread_loop_destroy: sym!(b"pw_thread_loop_destroy"),
                    pw_thread_loop_start: sym!(b"pw_thread_loop_start"),
                    pw_thread_loop_stop: sym!(b"pw_thread_loop_stop"),
                    pw_thread_loop_lock: sym!(b"pw_thread_loop_lock"),
                    pw_thread_loop_unlock: sym!(b"pw_thread_loop_unlock"),
                    pw_thread_loop_get_loop: sym!(b"pw_thread_loop_get_loop"),
                    pw_context_new: sym!(b"pw_context_new"),
                    pw_context_destroy: sym!(b"pw_context_destroy"),
                    pw_context_connect_fd: sym!(b"pw_context_connect_fd"),
                    pw_core_disconnect: sym!(b"pw_core_disconnect"),
                    pw_stream_new: sym!(b"pw_stream_new"),
                    pw_stream_destroy: sym!(b"pw_stream_destroy"),
                    pw_stream_add_listener: sym!(b"pw_stream_add_listener"),
                    pw_stream_connect: sym!(b"pw_stream_connect"),
                    pw_stream_disconnect: sym!(b"pw_stream_disconnect"),
                    pw_stream_dequeue_buffer: sym!(b"pw_stream_dequeue_buffer"),
                    pw_stream_queue_buffer: sym!(b"pw_stream_queue_buffer"),
                    pw_stream_update_params: sym!(b"pw_stream_update_params"),
                    pw_properties_new: sym!(b"pw_properties_new"),
                    _lib: lib,
                };
                let mut argc: c_int = 0;
                (me.pw_init)(&mut argc, std::ptr::null_mut());
                Some(Arc::new(me))
            })
            .clone()
    }
}

// ---------------------------------------------------------------------------
// SPA pods, hand-rolled (stable wire format)
// ---------------------------------------------------------------------------

// spa type ids
const SPA_TYPE_ID: u32 = 3;
const SPA_TYPE_INT: u32 = 4;
const SPA_TYPE_RECTANGLE: u32 = 10;
const SPA_TYPE_FRACTION: u32 = 11;
const SPA_TYPE_OBJECT: u32 = 15;
const SPA_TYPE_CHOICE: u32 = 19;
// choice kinds
const SPA_CHOICE_NONE: u32 = 0;
const SPA_CHOICE_RANGE: u32 = 1;
const SPA_CHOICE_ENUM: u32 = 2;
// objects + params
const SPA_TYPE_OBJECT_FORMAT: u32 = 0x0004_0003;
const SPA_TYPE_OBJECT_PARAM_BUFFERS: u32 = 0x0004_0005;
const SPA_PARAM_ENUM_FORMAT: u32 = 3;
const SPA_PARAM_FORMAT: u32 = 4;
const SPA_PARAM_BUFFERS: u32 = 5;
// format keys
const SPA_FORMAT_MEDIA_TYPE: u32 = 1;
const SPA_FORMAT_MEDIA_SUBTYPE: u32 = 2;
const SPA_FORMAT_VIDEO_FORMAT: u32 = 0x0002_0001;
const SPA_FORMAT_VIDEO_SIZE: u32 = 0x0002_0003;
const SPA_FORMAT_VIDEO_FRAMERATE: u32 = 0x0002_0004;
// media type/subtype
const SPA_MEDIA_TYPE_VIDEO: u32 = 2;
const SPA_MEDIA_SUBTYPE_RAW: u32 = 1;
// video formats we accept (all 32-bit RGB orderings)
const SPA_VIDEO_FORMAT_RGBX: u32 = 7;
const SPA_VIDEO_FORMAT_BGRX: u32 = 8;
const SPA_VIDEO_FORMAT_RGBA: u32 = 11;
const SPA_VIDEO_FORMAT_BGRA: u32 = 12;
// buffers keys
const SPA_PARAM_BUFFERS_BUFFERS: u32 = 1;
const SPA_PARAM_BUFFERS_DATATYPE: u32 = 6;
// spa_data types bitmask: MemPtr=1<<1, MemFd=1<<2
const DATA_TYPE_MASK_PTR_FD: u32 = (1 << 1) | (1 << 2);

/// Little-endian pod writer. Every pod is `(u32 size, u32 type, body…)` padded
/// to 8 bytes; object properties are `(u32 key, u32 flags, pod)`.
struct PodWriter {
    buf: Vec<u8>,
}

impl PodWriter {
    fn new() -> Self {
        Self { buf: Vec::with_capacity(256) }
    }
    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn pad8(&mut self) {
        while self.buf.len() % 8 != 0 {
            self.buf.push(0);
        }
    }
    /// Write a primitive pod (size covers the body only).
    fn pod(&mut self, type_: u32, body: &[u32]) {
        self.u32((body.len() * 4) as u32);
        self.u32(type_);
        for v in body {
            self.u32(*v);
        }
        self.pad8();
    }
    /// `Choice` pod wrapping `n` child values of `child_type`.
    fn choice(&mut self, kind: u32, child_type: u32, child_words: u32, values: &[u32]) {
        // choice body: (u32 kind, u32 flags, child pod header, raw values…)
        let body_words = 2 + 2 + values.len() as u32;
        self.u32(body_words * 4);
        self.u32(SPA_TYPE_CHOICE);
        self.u32(kind);
        self.u32(0); // flags
        self.u32(child_words * 4); // child size
        self.u32(child_type);
        for v in values {
            self.u32(*v);
        }
        self.pad8();
    }
    fn prop(&mut self, key: u32) {
        self.u32(key);
        self.u32(0); // flags
    }
    /// Wrap everything written so far as an object pod.
    fn into_object(self, obj_type: u32, obj_id: u32) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.buf.len() + 16);
        out.extend_from_slice(&((self.buf.len() as u32 + 8).to_le_bytes()));
        out.extend_from_slice(&SPA_TYPE_OBJECT.to_le_bytes());
        out.extend_from_slice(&obj_type.to_le_bytes());
        out.extend_from_slice(&obj_id.to_le_bytes());
        out.extend_from_slice(&self.buf);
        out
    }
}

/// EnumFormat: video/raw, any of {BGRx,RGBx,BGRA,RGBA}, size 1x1..16Kx16K.
fn build_enum_format_pod() -> Vec<u8> {
    let mut w = PodWriter::new();
    w.prop(SPA_FORMAT_MEDIA_TYPE);
    w.pod(SPA_TYPE_ID, &[SPA_MEDIA_TYPE_VIDEO]);
    w.prop(SPA_FORMAT_MEDIA_SUBTYPE);
    w.pod(SPA_TYPE_ID, &[SPA_MEDIA_SUBTYPE_RAW]);
    w.prop(SPA_FORMAT_VIDEO_FORMAT);
    w.choice(
        SPA_CHOICE_ENUM,
        SPA_TYPE_ID,
        1,
        &[
            SPA_VIDEO_FORMAT_BGRX, // default/preferred first
            SPA_VIDEO_FORMAT_BGRX,
            SPA_VIDEO_FORMAT_RGBX,
            SPA_VIDEO_FORMAT_BGRA,
            SPA_VIDEO_FORMAT_RGBA,
        ],
    );
    w.prop(SPA_FORMAT_VIDEO_SIZE);
    w.choice(
        SPA_CHOICE_RANGE,
        SPA_TYPE_RECTANGLE,
        2,
        &[1920, 1080, 1, 1, 16384, 16384], // default, min, max
    );
    w.prop(SPA_FORMAT_VIDEO_FRAMERATE);
    w.choice(
        SPA_CHOICE_RANGE,
        SPA_TYPE_FRACTION,
        2,
        &[30, 1, 0, 1, 240, 1],
    );
    w.into_object(SPA_TYPE_OBJECT_FORMAT, SPA_PARAM_ENUM_FORMAT)
}

/// Buffers param: force MemPtr/MemFd (mmap-able) — no DmaBuf, so MAP_BUFFERS
/// always gives us a CPU pointer.
fn build_buffers_pod() -> Vec<u8> {
    let mut w = PodWriter::new();
    w.prop(SPA_PARAM_BUFFERS_BUFFERS);
    w.choice(SPA_CHOICE_RANGE, SPA_TYPE_INT, 1, &[4, 2, 16]);
    w.prop(SPA_PARAM_BUFFERS_DATATYPE);
    w.choice(SPA_CHOICE_NONE, SPA_TYPE_INT, 1, &[DATA_TYPE_MASK_PTR_FD]);
    w.into_object(SPA_TYPE_OBJECT_PARAM_BUFFERS, SPA_PARAM_BUFFERS)
}

/// Minimal pod reader: extract `VIDEO_format` + `VIDEO_size` from the
/// negotiated Format object the `param_changed` event hands us.
fn parse_format_pod(pod: *const u8) -> Option<(u32, u32, u32)> {
    if pod.is_null() {
        return None;
    }
    unsafe {
        let size = *(pod as *const u32);
        let type_ = *(pod.add(4) as *const u32);
        if type_ != SPA_TYPE_OBJECT || size < 8 {
            return None;
        }
        let mut off = 16usize; // skip object header (size,type,objtype,objid)
        let end = 8 + size as usize;
        let mut format = 0u32;
        let mut wdt = 0u32;
        let mut hgt = 0u32;
        while off + 16 <= end {
            let key = *(pod.add(off) as *const u32);
            // flags at off+4
            let vsize = *(pod.add(off + 8) as *const u32) as usize;
            let vtype = *(pod.add(off + 12) as *const u32);
            let vbody = off + 16;
            match (key, vtype) {
                (SPA_FORMAT_VIDEO_FORMAT, SPA_TYPE_ID) => {
                    format = *(pod.add(vbody) as *const u32);
                }
                (SPA_FORMAT_VIDEO_SIZE, SPA_TYPE_RECTANGLE) => {
                    wdt = *(pod.add(vbody) as *const u32);
                    hgt = *(pod.add(vbody + 4) as *const u32);
                }
                _ => {}
            }
            // advance: prop header (8) + pod header (8) + padded body
            let padded = (vsize + 7) & !7;
            off = vbody + padded;
        }
        if format != 0 && wdt != 0 {
            Some((format, wdt, hgt))
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Stream event callbacks (run on the pw thread loop)
// ---------------------------------------------------------------------------

extern "C" fn on_param_changed(data: *mut c_void, id: u32, param: *const u8) {
    let ctx = unsafe { &*(data as *const StreamCtx) };
    scd!("param_changed id={} (format id={})", id, SPA_PARAM_FORMAT);
    if id != SPA_PARAM_FORMAT || param.is_null() {
        return;
    }
    if let Some((fmt, w, h)) = parse_format_pod(param) {
        crate::plog_info!("[screencap] negotiated format {} {}x{}", fmt, w, h);
        scd!("negotiated format {} {}x{}", fmt, w, h);
        {
            let mut slot = ctx.shared.slot.lock().unwrap();
            slot.spa_format = fmt;
            slot.width = w;
            slot.height = h;
        }
        // Respond with the buffer params — this is what drives the stream from
        // "paused" (format fixed) into "streaming" (buffers allocated, process
        // fires). Without it the stream connects but never produces frames.
        let buffers = build_buffers_pod();
        let mut params: [*const u8; 1] = [buffers.as_ptr()];
        unsafe {
            (ctx.pw.pw_stream_update_params)(ctx.stream, params.as_mut_ptr(), 1);
        }
        scd!("update_params(buffers) sent");
    } else {
        scd!("param_changed: format pod did not parse");
    }
}

extern "C" fn on_state_changed(data: *mut c_void, _old: c_int, state: c_int, err: *const c_char) {
    // PW_STREAM_STATE: error=-1, unconnected=0, connecting=1, paused=2, streaming=3
    let name = match state {
        -1 => "error",
        0 => "unconnected",
        1 => "connecting",
        2 => "paused",
        3 => "streaming",
        _ => "?",
    };
    scd!("state -> {} ({})", name, state);
    if state == -1 {
        let ctx = unsafe { &*(data as *const StreamCtx) };
        let msg = if err.is_null() {
            "unknown".to_string()
        } else {
            unsafe { std::ffi::CStr::from_ptr(err) }.to_string_lossy().into_owned()
        };
        crate::plog_warn!("[screencap] stream error: {}", msg);
        let mut slot = ctx.shared.slot.lock().unwrap();
        slot.dead = true;
        ctx.shared.cond.notify_all();
    }
}

/// `process`: copy the newest buffer into the shared slot. The stream pointer
/// lives in the same allocation as the Shared arc (see `open`), so the
/// callback only needs `data`.
extern "C" fn on_process(data: *mut c_void) {
    let ctx = unsafe { &*(data as *const StreamCtx) };
    let pw = &ctx.pw;
    unsafe {
        let buf = (pw.pw_stream_dequeue_buffer)(ctx.stream);
        if buf.is_null() {
            return;
        }
        let spa_buf = (*buf).buffer;
        if !spa_buf.is_null() && (*spa_buf).n_datas > 0 {
            let d = &*(*spa_buf).datas;
            if !d.data.is_null() && !d.chunk.is_null() {
                let chunk = &*d.chunk;
                let stride = chunk.stride.unsigned_abs() as usize;
                let size = chunk.size as usize;
                let src = (d.data as *const u8).add(chunk.offset as usize);
                let mut slot = ctx.shared.slot.lock().unwrap();
                let (w, h) = (slot.width as usize, slot.height as usize);
                if w > 0 && h > 0 && stride >= w * 4 && size >= stride * h {
                    // De-stride into tightly packed rows.
                    slot.data.resize(w * h * 4, 0);
                    for row in 0..h {
                        let s = std::slice::from_raw_parts(src.add(row * stride), w * 4);
                        slot.data[row * w * 4..(row + 1) * w * 4].copy_from_slice(s);
                    }
                    if slot.seq == 0 {
                        scd!("first frame received ({}x{}, fmt {})", w, h, slot.spa_format);
                    }
                    slot.seq = slot.seq.wrapping_add(1);
                    ctx.shared.cond.notify_all();
                }
            }
        }
        (pw.pw_stream_queue_buffer)(ctx.stream, buf);
    }
}

/// Context handed to EVERY stream callback (param/state/process): the pw vtable
/// (for update_params + dequeue/queue), the stream pointer, and the shared
/// frame slot. One allocation, one listener — kept alive in `Session._ctx`.
struct StreamCtx {
    pw: Arc<PwLib>,
    stream: *mut c_void,
    shared: Arc<Shared>,
}
unsafe impl Send for StreamCtx {}
unsafe impl Sync for StreamCtx {}

// ---------------------------------------------------------------------------
// VTable: open / read / close
// ---------------------------------------------------------------------------

pub fn open(_index: u32, _width: u32, _height: u32) -> u64 {
    scd!("open() called — starting portal handshake");
    // 1. Portal handshake (may block on the user's share dialog).
    let Some((fd, node_id, dbus, session_path)) = portal_open_stream() else {
        scd!("portal handshake FAILED — falling back to test pattern");
        return 0;
    };
    scd!("portal OK: node={} session={}", node_id, session_path);
    // 2. PipeWire.
    let Some(pw) = PwLib::load() else { scd!("libpipewire load FAILED"); return 0 };
    scd!("libpipewire loaded");

    let shared = Arc::new(Shared {
        slot: Mutex::new(FrameSlot::default()),
        cond: Condvar::new(),
    });

    unsafe {
        let name = CString::new("azul-screencap").unwrap();
        let thread_loop = (pw.pw_thread_loop_new)(name.as_ptr(), std::ptr::null());
        if thread_loop.is_null() {
            return 0;
        }
        let loop_ = (pw.pw_thread_loop_get_loop)(thread_loop);
        let context = (pw.pw_context_new)(loop_, std::ptr::null_mut(), 0);
        if context.is_null() {
            (pw.pw_thread_loop_destroy)(thread_loop);
            return 0;
        }
        if (pw.pw_thread_loop_start)(thread_loop) != 0 {
            (pw.pw_context_destroy)(context);
            (pw.pw_thread_loop_destroy)(thread_loop);
            return 0;
        }

        (pw.pw_thread_loop_lock)(thread_loop);
        let core = (pw.pw_context_connect_fd)(
            context,
            fd.into_raw_fd(), // pw takes ownership of the fd
            std::ptr::null_mut(),
            0,
        );
        if core.is_null() {
            (pw.pw_thread_loop_unlock)(thread_loop);
            crate::plog_warn!("[screencap] pw_context_connect_fd failed");
            (pw.pw_thread_loop_stop)(thread_loop);
            (pw.pw_context_destroy)(context);
            (pw.pw_thread_loop_destroy)(thread_loop);
            return 0;
        }

        let key_media_type = CString::new("media.type").unwrap();
        let v_video = CString::new("Video").unwrap();
        let key_category = CString::new("media.category").unwrap();
        let v_capture = CString::new("Capture").unwrap();
        let key_role = CString::new("media.role").unwrap();
        let v_screen = CString::new("Screen").unwrap();
        let props = (pw.pw_properties_new)(
            key_media_type.as_ptr(),
            v_video.as_ptr(),
            key_category.as_ptr(),
            v_capture.as_ptr(),
            key_role.as_ptr(),
            v_screen.as_ptr(),
            std::ptr::null::<c_char>(),
        );

        let stream_name = CString::new("azul-screen").unwrap();
        let stream = (pw.pw_stream_new)(core, stream_name.as_ptr(), props);
        if stream.is_null() {
            (pw.pw_thread_loop_unlock)(thread_loop);
            (pw.pw_core_disconnect)(core);
            (pw.pw_thread_loop_stop)(thread_loop);
            (pw.pw_context_destroy)(context);
            (pw.pw_thread_loop_destroy)(thread_loop);
            return 0;
        }

        // One ctx + one listener carrying all three callbacks.
        let ctx_ptr = Box::into_raw(Box::new(StreamCtx {
            pw: pw.clone(),
            stream,
            shared: shared.clone(),
        }));
        let events = Box::new(PwStreamEvents {
            version: 2,
            destroy: None,
            state_changed: Some(on_state_changed),
            control_info: None,
            io_changed: None,
            param_changed: Some(on_param_changed),
            add_buffer: None,
            remove_buffer: None,
            process: Some(on_process),
            drained: None,
            command: None,
            trigger_done: None,
        });
        let mut hook = Box::new([0u8; 128]);
        (pw.pw_stream_add_listener)(
            stream,
            hook.as_mut_ptr() as *mut c_void,
            &*events,
            ctx_ptr as *mut c_void,
        );

        // Connect: input stream to the portal's node, autoconnect + mmap.
        let format_pod = build_enum_format_pod();
        let buffers_pod = build_buffers_pod();
        let mut params: [*const u8; 2] = [format_pod.as_ptr(), buffers_pod.as_ptr()];
        const PW_DIRECTION_INPUT: c_int = 0;
        const FLAGS_AUTOCONNECT_MAP: c_int = (1 << 0) | (1 << 2);
        let rc = (pw.pw_stream_connect)(
            stream,
            PW_DIRECTION_INPUT,
            node_id,
            FLAGS_AUTOCONNECT_MAP,
            params.as_mut_ptr(),
            2,
        );
        (pw.pw_thread_loop_unlock)(thread_loop);
        scd!("pw_stream_connect rc={}", rc);
        if rc != 0 {
            crate::plog_warn!("[screencap] pw_stream_connect failed ({})", rc);
            (pw.pw_thread_loop_lock)(thread_loop);
            (pw.pw_stream_destroy)(stream);
            (pw.pw_core_disconnect)(core);
            (pw.pw_thread_loop_unlock)(thread_loop);
            (pw.pw_thread_loop_stop)(thread_loop);
            (pw.pw_context_destroy)(context);
            (pw.pw_thread_loop_destroy)(thread_loop);
            drop(Box::from_raw(ctx_ptr));
            return 0;
        }

        // Keep the param/buffer pods alive for the duration of connect's use:
        // PipeWire copies them during pw_stream_connect, so dropping them now
        // is fine (they're Vec<u8> locals).

        let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
        // The events boxes + hooks + ctx must outlive the stream.
        let session = Session {
            shared,
            pw,
            thread_loop,
            context,
            core,
            stream,
            _events: events,
            _hook: hook,
            _ctx: ctx_ptr,
            _dbus: dbus,
            session_path,
        };
        sessions().lock().unwrap().insert(handle, session);
        crate::plog_info!("[screencap] session {} open (node {})", handle, node_id);
        scd!("session {} CONNECTED (node {}) — awaiting frames", handle, node_id);
        handle
    }
}


pub fn read(handle: u64, out: &mut Vec<u8>) -> (u32, u32) {
    let shared = {
        let map = sessions().lock().unwrap();
        let Some(s) = map.get(&handle) else { return (0, 0) };
        s.shared.clone()
    };

    let mut slot = shared.slot.lock().unwrap();
    let start_seq = slot.seq;
    // Wait for the NEXT frame (≤2s; on timeout re-deliver the last one so a
    // static screen keeps the widget alive instead of ending the stream).
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while slot.seq == start_seq && !slot.dead {
        let timeout = deadline.saturating_duration_since(std::time::Instant::now());
        if timeout.is_zero() {
            break;
        }
        let (guard, _) = shared.cond.wait_timeout(slot, timeout).unwrap();
        slot = guard;
    }
    if slot.dead {
        return (0, 0);
    }
    if slot.data.is_empty() || slot.width == 0 {
        // No frame yet (still negotiating): brief blank frame keeps the
        // worker polling without tripping its EOS handling.
        out.clear();
        out.resize(4, 0);
        return (1, 1);
    }

    let (w, h) = (slot.width as usize, slot.height as usize);
    out.resize(w * h * 4, 0);
    match slot.spa_format {
        SPA_VIDEO_FORMAT_RGBA => out.copy_from_slice(&slot.data),
        SPA_VIDEO_FORMAT_RGBX => {
            out.copy_from_slice(&slot.data);
            for px in out.chunks_exact_mut(4) {
                px[3] = 255;
            }
        }
        SPA_VIDEO_FORMAT_BGRA | SPA_VIDEO_FORMAT_BGRX => {
            for (dst, src) in out.chunks_exact_mut(4).zip(slot.data.chunks_exact(4)) {
                dst[0] = src[2];
                dst[1] = src[1];
                dst[2] = src[0];
                dst[3] = 255;
            }
        }
        _ => {
            for (dst, src) in out.chunks_exact_mut(4).zip(slot.data.chunks_exact(4)) {
                dst.copy_from_slice(src);
                dst[3] = 255;
            }
        }
    }
    (slot.width, slot.height)
}

pub fn close(handle: u64) {
    let Some(session) = sessions().lock().unwrap().remove(&handle) else {
        return;
    };
    unsafe {
        let pw = &session.pw;
        (pw.pw_thread_loop_lock)(session.thread_loop);
        (pw.pw_stream_disconnect)(session.stream);
        (pw.pw_stream_destroy)(session.stream);
        (pw.pw_core_disconnect)(session.core);
        (pw.pw_thread_loop_unlock)(session.thread_loop);
        (pw.pw_thread_loop_stop)(session.thread_loop);
        (pw.pw_context_destroy)(session.context);
        (pw.pw_thread_loop_destroy)(session.thread_loop);
        drop(Box::from_raw(session._ctx));
    }
    // Close the portal session so the desktop's "is being shared" indicator
    // goes away.
    let proxy = zbus::blocking::Proxy::new(
        &session._dbus,
        "org.freedesktop.portal.Desktop",
        session.session_path.as_str(),
        "org.freedesktop.portal.Session",
    );
    if let Ok(p) = proxy {
        let _: Result<(), _> = p.call("Close", &());
    }
    crate::plog_info!("[screencap] session {} closed", handle);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hand-rolled pods must round-trip through our own parser — catches
    /// header/padding mistakes without needing a live PipeWire.
    #[test]
    fn enum_format_pod_parses_back() {
        let pod = build_enum_format_pod();
        // The parser reads a FORMAT object; EnumFormat differs only in obj id.
        let parsed = parse_format_pod(pod.as_ptr());
        // A Choice value is not a plain Id/Rectangle, so format/size stay 0 —
        // but the walk must terminate without crashing and find nothing.
        assert!(parsed.is_none());
        // Sanity: object header.
        assert_eq!(&pod[4..8], &SPA_TYPE_OBJECT.to_le_bytes());
        assert_eq!(&pod[8..12], &SPA_TYPE_OBJECT_FORMAT.to_le_bytes());
        assert_eq!(pod.len() % 8, 0, "pods must be 8-byte aligned");
    }

    /// A synthetic NEGOTIATED format pod (plain values, the shape
    /// param_changed receives) must parse into format+size.
    #[test]
    fn negotiated_format_pod_parses() {
        let mut w = PodWriter::new();
        w.prop(SPA_FORMAT_MEDIA_TYPE);
        w.pod(SPA_TYPE_ID, &[SPA_MEDIA_TYPE_VIDEO]);
        w.prop(SPA_FORMAT_MEDIA_SUBTYPE);
        w.pod(SPA_TYPE_ID, &[SPA_MEDIA_SUBTYPE_RAW]);
        w.prop(SPA_FORMAT_VIDEO_FORMAT);
        w.pod(SPA_TYPE_ID, &[SPA_VIDEO_FORMAT_BGRX]);
        w.prop(SPA_FORMAT_VIDEO_SIZE);
        w.pod(SPA_TYPE_RECTANGLE, &[2560, 1440]);
        let pod = w.into_object(SPA_TYPE_OBJECT_FORMAT, SPA_PARAM_FORMAT);

        let (fmt, wdt, hgt) = parse_format_pod(pod.as_ptr()).expect("must parse");
        assert_eq!(fmt, SPA_VIDEO_FORMAT_BGRX);
        assert_eq!((wdt, hgt), (2560, 1440));
    }
}
