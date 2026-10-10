//! The thread WPE `WebKit` runs on (`azul-webview`), one per process.
//!
//! `WebKit` is driven by a GLib main context. The X11 and Wayland loops park
//! in `poll(2)` on their own descriptors and never iterate GLib, so the web
//! views live on a thread of their own that runs the default main context
//! (`g_main_loop_run`) and owns every `WebKit` object: no `WebKit` call is
//! ever made from a window's thread, and no window state is touched from
//! this one.
//!
//! The two sides talk through mailboxes. A window's backend
//! (`super::webview::WpeWebViews`) queues [`Command`]s in the [`INBOX`] and
//! wakes this thread with `g_idle_add` (thread-safe); the thread answers into
//! the window's [`Mail`] - reports for the app's callbacks, the newest frame
//! of each view - and wakes the window's loop (`loop_waker::wake`).
//!
//! Per view: a `WPEBackend-fdo` exportable view backend exporting
//! shared-memory frames (each copied into the mail, released and answered
//! with `frame_complete`), a `WebKitWebViewBackend` around it, and a
//! `WebKitWebView` in the store its config asks for. Its signals:
//! `decide-policy` holds the `WebKitPolicyDecision` until the app answered
//! (`Command::Decide`), `load-changed` / `load-failed` / `notify::title` /
//! `web-process-terminated` become reports.

use core::ptr;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    ffi::{c_char, c_int, c_ulong, c_void, CStr, CString},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, OnceLock,
    },
    time::Instant,
};

use azul_core::{
    events::{KeyModifiers, MouseButton},
    webview::{WebViewConfig, WebViewEvent, WebViewLoadError, WebViewNavigation, WebViewStorage},
};
use azul_css::AzString;
use azul_layout::managers::webview::{
    WebViewId, WebViewInput, WebViewOp, WebViewPlacement, WebViewReport, WebViewTransform,
};

use super::{
    webview::{
        bgra_from_shm, device_point, is_quiet_failure, unavailable_reason, wheel_axes, wpe_button,
        wpe_modifiers,
    },
    wpe::{
        self, GBoolean, GError, WebKitStoreApi, Wpe, WpeInputAxis2dEvent, WpeInputAxisEvent,
        WpeInputKeyboardEvent, WpeInputPointerEvent, WpeViewBackendExportableFdoClient,
        WPE_ACTIVITY_FOCUSED, WPE_ACTIVITY_IN_WINDOW, WPE_ACTIVITY_VISIBLE, WPE_AXIS_MASK_2D,
        WPE_AXIS_MOTION_SMOOTH, WPE_POINTER_BUTTON, WPE_POINTER_MOTION,
    },
};

/// `WEBKIT_POLICY_DECISION_TYPE_NAVIGATION_ACTION`.
const POLICY_NAVIGATION_ACTION: c_int = 0;
/// `WEBKIT_POLICY_DECISION_TYPE_NEW_WINDOW_ACTION`.
const POLICY_NEW_WINDOW_ACTION: c_int = 1;
/// `WEBKIT_LOAD_FINISHED`.
const LOAD_FINISHED: c_int = 3;
/// The size a view is created at, before its first placement.
const INITIAL_SIZE: (u32, u32) = (300, 150);

/// What a window's views told it since it last looked.
#[derive(Default)]
pub(super) struct Mail {
    /// For the app's callbacks, oldest first.
    pub reports: Vec<WebViewReport>,
    /// The newest frame of each view (an older one is never drawn).
    pub frames: BTreeMap<WebViewId, Frame>,
}

/// One frame of a page: tightly packed BGRA, premultiplied, device pixels.
pub(super) struct Frame {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

/// A window's mail, shared with this thread.
pub(super) type Mailbox = Arc<Mutex<Mail>>;

/// What a window asks of this thread.
pub(super) enum Command {
    /// An op of the window's web view manager.
    Op(WebViewOp),
    /// The app's answer to a navigation request.
    Decide {
        id: WebViewId,
        request: u64,
        allow: bool,
    },
    /// The window's scale factor (device pixels per logical one).
    Scale(f32),
    /// The window closed: its views go.
    Forget,
}

/// Commands not taken yet, with the mailbox of the window that sent each.
static INBOX: Mutex<Vec<(Mailbox, Command)>> = Mutex::new(Vec::new());
/// An idle source to drain the inbox is queued and has not run yet.
static DRAIN_QUEUED: AtomicBool = AtomicBool::new(false);
/// Whether the thread runs - or why not.
static STARTED: OnceLock<Result<(), String>> = OnceLock::new();

/// Start the thread (once per process) and wait until WPE is set up on it:
/// `Err(reason)` - for every web view of the process - when it cannot be.
pub(super) fn start() -> Result<(), String> {
    STARTED
        .get_or_init(|| {
            let (ready, wait) = mpsc::channel();
            let spawned = std::thread::Builder::new()
                .name("azul-webview".to_string())
                .spawn(move || run(&ready));
            match spawned {
                Ok(_) => wait
                    .recv()
                    .unwrap_or_else(|_| Err("the web view thread ended at its start".to_string())),
                Err(e) => Err(format!("the web view thread did not start: {e}")),
            }
        })
        .clone()
}

/// Whether the thread runs.
pub(super) fn running() -> bool {
    matches!(STARTED.get(), Some(Ok(())))
}

/// Queue `command` for `mailbox`'s window and wake the thread.
pub(super) fn send(mailbox: &Mailbox, command: Command) {
    if !running() {
        return;
    }
    if let Ok(mut inbox) = INBOX.lock() {
        inbox.push((Arc::clone(mailbox), command));
    }
    if DRAIN_QUEUED.swap(true, Ordering::AcqRel) {
        return;
    }
    // `g_idle_add` is thread-safe; GLib is loaded on this thread as well.
    if let Ok(wpe) = wpe::wpe() {
        unsafe {
            (wpe.glib.g_idle_add)(Some(drain), ptr::null_mut());
        }
    }
}

/// The thread: set WPE up, say whether that worked, run the main context.
fn run(ready: &mpsc::Sender<Result<(), String>>) {
    let wpe = match wpe::wpe() {
        Ok(wpe) => wpe,
        Err(e) => {
            let _ = ready.send(Err(unavailable_reason(&e)));
            return;
        }
    };
    unsafe {
        if !(wpe.wpe.wpe_loader_init)(c"libWPEBackend-fdo-1.0.so.1".as_ptr()) {
            let _ = ready.send(Err(String::from(
                "WPE WebKit could not load its view backend (WPEBackend-fdo)",
            )));
            return;
        }
        if !(wpe.fdo.wpe_fdo_initialize_shm)() {
            let _ = ready.send(Err(String::from(
                "WPE WebKit could not set up shared-memory rendering (WPEBackend-fdo 1.8 or \
                 newer)",
            )));
            return;
        }
    }
    crate::plog_info!("[webview] WPE WebKit {} runs the web views", wpe.version());
    let _ = ready.send(Ok(()));
    unsafe {
        let main_loop = (wpe.glib.g_main_loop_new)(ptr::null_mut(), 0);
        (wpe.glib.g_main_loop_run)(main_loop);
    }
}

thread_local! {
    /// Every view of the process, by window (the address of its mailbox).
    static STATE: RefCell<ThreadState> = RefCell::new(ThreadState::default());
}

/// The inbox's idle source: apply every queued command.
unsafe extern "C" fn drain(_data: *mut c_void) -> GBoolean {
    DRAIN_QUEUED.store(false, Ordering::Release);
    let commands = INBOX
        .lock()
        .map(|mut inbox| core::mem::take(&mut *inbox))
        .unwrap_or_default();
    let Ok(wpe) = wpe::wpe() else {
        return 0;
    };
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        for (mailbox, command) in commands {
            state.apply(wpe, &mailbox, command);
        }
    });
    // G_SOURCE_REMOVE: the next `send` queues a new one.
    0
}

#[derive(Default)]
struct ThreadState {
    windows: BTreeMap<usize, WindowViews>,
    /// The process's ephemeral store: one for every ephemeral view.
    ephemeral: Option<*mut c_void>,
    /// The app's persistent store, under its own data directory.
    persistent: Option<*mut c_void>,
}

/// One window's views.
struct WindowViews {
    scale: f32,
    views: BTreeMap<WebViewId, Box<View>>,
}

impl ThreadState {
    fn apply(&mut self, wpe: &'static Wpe, mailbox: &Mailbox, command: Command) {
        let key = Arc::as_ptr(mailbox) as usize;
        if let Command::Forget = command {
            if let Some(window) = self.windows.remove(&key) {
                for view in window.views.into_values() {
                    view.tear_down(wpe);
                }
            }
            return;
        }
        let store = match &command {
            Command::Op(WebViewOp::Create { config, .. }) => self.store(wpe, *config),
            _ => ptr::null_mut(),
        };
        let window = self.windows.entry(key).or_insert_with(|| WindowViews {
            scale: 1.0,
            views: BTreeMap::new(),
        });
        match command {
            Command::Forget => {}
            Command::Scale(scale) => {
                window.scale = scale;
                for view in window.views.values() {
                    view.set_scale(wpe, scale);
                }
            }
            Command::Decide { id, request, allow } => {
                if let Some(view) = window.views.get(&id) {
                    view.decide(wpe, request, allow);
                }
            }
            Command::Op(op) => match op {
                WebViewOp::Create { id, config, src } => {
                    // The store came from `config` above.
                    let _ = config;
                    match View::create(wpe, mailbox, id, store, window.scale) {
                        Ok(view) => {
                            if !src.as_str().is_empty() {
                                view.navigate(wpe, src.as_str());
                            }
                            if let Some(old) = window.views.insert(id, view) {
                                old.tear_down(wpe);
                            }
                        }
                        Err(reason) => report(
                            mailbox,
                            id,
                            0,
                            WebViewEvent::LoadFailed(WebViewLoadError {
                                url: src,
                                reason: AzString::from(reason.as_str()),
                            }),
                        ),
                    }
                }
                WebViewOp::Destroy { id } => {
                    if let Some(view) = window.views.remove(&id) {
                        view.tear_down(wpe);
                    }
                }
                other => {
                    if let Some(view) = window.views.get(&other.id()) {
                        view.apply(wpe, &other);
                    }
                }
            },
        }
    }

    /// The store for `config` - made at the first view that asks for it.
    fn store(&mut self, wpe: &Wpe, config: WebViewConfig) -> *mut c_void {
        let slot = match config.storage {
            WebViewStorage::Ephemeral => &mut self.ephemeral,
            WebViewStorage::Persistent => &mut self.persistent,
        };
        if let Some(store) = *slot {
            return store;
        }
        let store = unsafe {
            match (wpe.webkit.store, config.storage) {
                (WebKitStoreApi::NetworkSession { new_ephemeral, .. }, WebViewStorage::Ephemeral)
                | (WebKitStoreApi::WebContext { new_ephemeral, .. }, WebViewStorage::Ephemeral) => {
                    new_ephemeral()
                }
                (WebKitStoreApi::NetworkSession { new, .. }, WebViewStorage::Persistent) => {
                    let (data, cache) = persistent_directories();
                    new(data.as_ptr(), cache.as_ptr())
                }
                (WebKitStoreApi::WebContext { new, .. }, WebViewStorage::Persistent) => new(),
            }
        };
        *slot = (!store.is_null()).then_some(store);
        store
    }
}

/// The app's own web data and cache directories (never the system
/// browser's): `$XDG_DATA_HOME/<app>/webview`, `$XDG_CACHE_HOME/<app>/webview`.
fn persistent_directories() -> (CString, CString) {
    let app = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| String::from("azul-app"));
    let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/tmp"));
    let base = |var: &str, fallback: &str| {
        std::env::var(var)
            .ok()
            .filter(|dir| !dir.is_empty())
            .unwrap_or_else(|| format!("{home}/{fallback}"))
    };
    let data = format!("{}/{app}/webview", base("XDG_DATA_HOME", ".local/share"));
    let cache = format!("{}/{app}/webview", base("XDG_CACHE_HOME", ".cache"));
    (
        CString::new(data).unwrap_or_default(),
        CString::new(cache).unwrap_or_default(),
    )
}

/// Put `event` into `mailbox` and wake the window's loop.
fn report(mailbox: &Mailbox, id: WebViewId, request: u64, event: WebViewEvent) {
    if let Ok(mut mail) = mailbox.lock() {
        mail.reports.push(WebViewReport { id, request, event });
    }
    crate::desktop::loop_waker::wake();
}

/// A NUL-terminated C string as a Rust one; empty for NULL.
unsafe fn c_string(s: *const c_char) -> String {
    if s.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned()
}

fn is_file_url(url: &str) -> bool {
    url.trim_start()
        .get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("file:"))
}

/// What the exportable view backend's frame callbacks get: where the frames
/// of view `id` go. Lives until `WebKit` drops the view backend
/// ([`on_backend_destroyed`]), which may be after the view.
struct FrameSink {
    id: WebViewId,
    mailbox: Mailbox,
    exportable: Cell<*mut c_void>,
}

/// The exportable view backend's callbacks: shared-memory frames (an EGL
/// buffer - not asked for - is released unread).
static CLIENT: WpeViewBackendExportableFdoClient = WpeViewBackendExportableFdoClient {
    export_buffer_resource: Some(on_buffer_resource),
    export_dmabuf_resource: None,
    export_shm_buffer: Some(on_shm_buffer),
    reserved0: None,
    reserved1: None,
};

/// A shared-memory frame: copied into the window's mail (replacing one it
/// has not drawn yet), released, and answered so the page draws the next.
unsafe extern "C" fn on_shm_buffer(data: *mut c_void, buffer: *mut c_void) {
    let Some(sink) = (unsafe { (data as *const FrameSink).as_ref() }) else {
        return;
    };
    let Ok(wpe) = wpe::wpe() else {
        return;
    };
    let exportable = sink.exportable.get();
    unsafe {
        let shm = (wpe.fdo.wpe_fdo_shm_exported_buffer_get_shm_buffer)(buffer);
        if !shm.is_null() {
            let wl = &wpe.wayland;
            (wl.wl_shm_buffer_begin_access)(shm);
            let width = usize::try_from((wl.wl_shm_buffer_get_width)(shm)).unwrap_or(0);
            let height = usize::try_from((wl.wl_shm_buffer_get_height)(shm)).unwrap_or(0);
            let stride = usize::try_from((wl.wl_shm_buffer_get_stride)(shm)).unwrap_or(0);
            let format = (wl.wl_shm_buffer_get_format)(shm);
            let pixels = (wl.wl_shm_buffer_get_data)(shm).cast::<u8>().cast_const();
            let copied = if pixels.is_null() || width == 0 || height == 0 {
                None
            } else {
                let bytes = core::slice::from_raw_parts(pixels, stride * height);
                bgra_from_shm(width, height, stride, format, bytes)
            };
            (wl.wl_shm_buffer_end_access)(shm);
            if let Some(pixels) = copied {
                if let Ok(mut mail) = sink.mailbox.lock() {
                    mail.frames.insert(
                        sink.id,
                        Frame {
                            width,
                            height,
                            pixels,
                        },
                    );
                }
                crate::desktop::loop_waker::wake();
            }
        }
        if !exportable.is_null() {
            (wpe.fdo.wpe_view_backend_exportable_fdo_dispatch_release_shm_exported_buffer)(
                exportable, buffer,
            );
            (wpe.fdo.wpe_view_backend_exportable_fdo_dispatch_frame_complete)(exportable);
        }
    }
}

/// An EGL buffer: released unread, so the page is not stuck waiting.
unsafe extern "C" fn on_buffer_resource(data: *mut c_void, resource: *mut c_void) {
    let Some(sink) = (unsafe { (data as *const FrameSink).as_ref() }) else {
        return;
    };
    let Ok(wpe) = wpe::wpe() else {
        return;
    };
    let exportable = sink.exportable.get();
    if !exportable.is_null() {
        unsafe {
            (wpe.fdo.wpe_view_backend_exportable_fdo_dispatch_release_buffer)(
                exportable, resource,
            );
            (wpe.fdo.wpe_view_backend_exportable_fdo_dispatch_frame_complete)(exportable);
        }
    }
}

/// `WebKit` let go of a view backend: its exportable and sink go.
unsafe extern "C" fn on_backend_destroyed(data: *mut c_void) {
    if data.is_null() {
        return;
    }
    let sink = unsafe { Box::from_raw(data.cast::<FrameSink>()) };
    let exportable = sink.exportable.get();
    if let (false, Ok(wpe)) = (exportable.is_null(), wpe::wpe()) {
        unsafe { (wpe.fdo.wpe_view_backend_exportable_fdo_destroy)(exportable) };
    }
}

/// One web view on this thread. Its signal handlers get a pointer to it
/// (it is boxed and disconnected before it goes); everything they change is
/// in a `Cell` / `RefCell`, so no `&mut` is held across a `WebKit` call.
struct View {
    id: WebViewId,
    mailbox: Mailbox,
    web_view: *mut c_void,
    view_backend: *mut c_void,
    handlers: RefCell<Vec<c_ulong>>,
    /// Policy decisions the app has not answered, by request handle (each
    /// holds a reference).
    decisions: RefCell<BTreeMap<u64, *mut c_void>>,
    last_request: Cell<u64>,
    size: Cell<(u32, u32)>,
    transform: Cell<Option<WebViewTransform>>,
    activity: Cell<u32>,
    scale: Cell<f32>,
    buttons: RefCell<Vec<MouseButton>>,
    keys: Cell<KeyModifiers>,
    epoch: Instant,
}

impl View {
    /// A view in `store` (null: `WebKit`'s default), hidden until placed.
    fn create(
        wpe: &'static Wpe,
        mailbox: &Mailbox,
        id: WebViewId,
        store: *mut c_void,
        scale: f32,
    ) -> Result<Box<Self>, String> {
        let (fdo, webkit) = (&wpe.fdo, &wpe.webkit);
        unsafe {
            let sink = Box::into_raw(Box::new(FrameSink {
                id,
                mailbox: Arc::clone(mailbox),
                exportable: Cell::new(ptr::null_mut()),
            }));
            let exportable = (fdo.wpe_view_backend_exportable_fdo_create)(
                &CLIENT,
                sink.cast(),
                INITIAL_SIZE.0,
                INITIAL_SIZE.1,
            );
            if exportable.is_null() {
                drop(Box::from_raw(sink));
                return Err(String::from("WPEBackend-fdo made no view backend"));
            }
            (*sink).exportable.set(exportable);
            let view_backend = (fdo.wpe_view_backend_exportable_fdo_get_view_backend)(exportable);
            (wpe.wpe.wpe_view_backend_dispatch_set_device_scale_factor)(view_backend, scale);
            let backend = (webkit.webkit_web_view_backend_new)(
                view_backend,
                Some(on_backend_destroyed),
                sink.cast(),
            );
            if backend.is_null() {
                on_backend_destroyed(sink.cast());
                return Err(String::from("WPE WebKit made no web view backend"));
            }
            let web_view = match webkit.store {
                WebKitStoreApi::NetworkSession {
                    web_view_get_type, ..
                } if !store.is_null() => (wpe.gobject.g_object_new)(
                    web_view_get_type(),
                    c"backend".as_ptr(),
                    backend,
                    c"network-session".as_ptr(),
                    store,
                    ptr::null::<c_char>(),
                ),
                WebKitStoreApi::WebContext {
                    web_view_new_with_context,
                    ..
                } if !store.is_null() => web_view_new_with_context(backend, store),
                _ => (webkit.webkit_web_view_new)(backend),
            };
            if web_view.is_null() {
                return Err(String::from("WPE WebKit made no web view"));
            }
            // No page reaches the disk through `file://`.
            let settings = (webkit.webkit_web_view_get_settings)(web_view);
            if !settings.is_null() {
                (webkit.webkit_settings_set_allow_file_access_from_file_urls)(settings, 0);
                (webkit.webkit_settings_set_allow_universal_access_from_file_urls)(settings, 0);
            }
            let view = Box::new(Self {
                id,
                mailbox: Arc::clone(mailbox),
                web_view,
                view_backend,
                handlers: RefCell::new(Vec::new()),
                decisions: RefCell::new(BTreeMap::new()),
                last_request: Cell::new(0),
                size: Cell::new(INITIAL_SIZE),
                transform: Cell::new(None),
                activity: Cell::new(0),
                scale: Cell::new(scale),
                buttons: RefCell::new(Vec::new()),
                keys: Cell::new(KeyModifiers::default()),
                epoch: Instant::now(),
            });
            view.connect(wpe);
            // In a window, hidden until placed.
            view.set_activity(wpe, WPE_ACTIVITY_IN_WINDOW, true);
            Ok(view)
        }
    }

    /// Connect the signals, each with this view as its data.
    unsafe fn connect(&self, wpe: &Wpe) {
        let data = core::ptr::from_ref(self).cast_mut().cast::<c_void>();
        let handlers: [(&CStr, unsafe extern "C" fn()); 5] = unsafe {
            [
                (
                    c"decide-policy",
                    core::mem::transmute::<
                        unsafe extern "C" fn(
                            *mut c_void,
                            *mut c_void,
                            c_int,
                            *mut c_void,
                        ) -> GBoolean,
                        unsafe extern "C" fn(),
                    >(on_decide_policy),
                ),
                (
                    c"load-changed",
                    core::mem::transmute::<
                        unsafe extern "C" fn(*mut c_void, c_int, *mut c_void),
                        unsafe extern "C" fn(),
                    >(on_load_changed),
                ),
                (
                    c"load-failed",
                    core::mem::transmute::<
                        unsafe extern "C" fn(
                            *mut c_void,
                            c_int,
                            *const c_char,
                            *mut GError,
                            *mut c_void,
                        ) -> GBoolean,
                        unsafe extern "C" fn(),
                    >(on_load_failed),
                ),
                (
                    c"notify::title",
                    core::mem::transmute::<
                        unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void),
                        unsafe extern "C" fn(),
                    >(on_title),
                ),
                (
                    c"web-process-terminated",
                    core::mem::transmute::<
                        unsafe extern "C" fn(*mut c_void, c_int, *mut c_void),
                        unsafe extern "C" fn(),
                    >(on_terminated),
                ),
            ]
        };
        let mut ids = self.handlers.borrow_mut();
        for (signal, handler) in handlers {
            let id = unsafe {
                (wpe.gobject.g_signal_connect_data)(
                    self.web_view,
                    signal.as_ptr(),
                    Some(handler),
                    data,
                    None,
                    0,
                )
            };
            if id != 0 {
                ids.push(id);
            }
        }
    }

    /// Report `event` (with the decision handle `request`) to the window.
    fn report(&self, request: u64, event: WebViewEvent) {
        report(&self.mailbox, self.id, request, event);
    }

    fn apply(&self, wpe: &Wpe, op: &WebViewOp) {
        match op {
            WebViewOp::Navigate { url, .. } => self.navigate(wpe, url.as_str()),
            WebViewOp::Reload { .. } => unsafe {
                (wpe.webkit.webkit_web_view_reload)(self.web_view);
            },
            WebViewOp::GoBack { .. } => unsafe {
                (wpe.webkit.webkit_web_view_go_back)(self.web_view);
            },
            WebViewOp::Place { placement, .. } => self.place(wpe, placement),
            WebViewOp::Transform { transform, .. } => {
                self.transform.set(Some(*transform));
                self.set_size(wpe, transform.size.width, transform.size.height);
            }
            WebViewOp::Input { input, .. } => self.input(wpe, input),
            WebViewOp::Create { .. } | WebViewOp::Destroy { .. } => {}
        }
    }

    fn navigate(&self, wpe: &Wpe, url: &str) {
        match CString::new(url) {
            Ok(url) => unsafe {
                (wpe.webkit.webkit_web_view_load_uri)(self.web_view, url.as_ptr());
            },
            Err(_) => self.report(
                0,
                WebViewEvent::LoadFailed(WebViewLoadError {
                    url: AzString::from(url),
                    reason: AzString::from("not a URL a web view can load"),
                }),
            ),
        }
    }

    /// The page lays out at its own size (its transform's, else its
    /// placement's); a hidden view is not visible to the page either.
    fn place(&self, wpe: &Wpe, placement: &WebViewPlacement) {
        let size = self
            .transform
            .get()
            .map_or(placement.rect.size, |transform| transform.size);
        self.set_size(wpe, size.width, size.height);
        self.set_activity(wpe, WPE_ACTIVITY_VISIBLE, placement.visible);
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn set_size(&self, wpe: &Wpe, width: f32, height: f32) {
        let size = (width.round().max(1.0) as u32, height.round().max(1.0) as u32);
        if size == self.size.get() {
            return;
        }
        self.size.set(size);
        unsafe { (wpe.wpe.wpe_view_backend_dispatch_set_size)(self.view_backend, size.0, size.1) };
    }

    fn set_scale(&self, wpe: &Wpe, scale: f32) {
        self.scale.set(scale);
        unsafe {
            (wpe.wpe.wpe_view_backend_dispatch_set_device_scale_factor)(self.view_backend, scale);
        }
    }

    fn set_activity(&self, wpe: &Wpe, state: u32, on: bool) {
        let now = self.activity.get();
        if (now & state != 0) == on {
            return;
        }
        unsafe {
            if on {
                (wpe.wpe.wpe_view_backend_add_activity_state)(self.view_backend, state);
            } else {
                (wpe.wpe.wpe_view_backend_remove_activity_state)(self.view_backend, state);
            }
        }
        self.activity.set(if on { now | state } else { now & !state });
    }

    /// Milliseconds since the view was made: the input events' clock.
    #[allow(clippy::cast_possible_truncation)]
    fn time(&self) -> u32 {
        self.epoch.elapsed().as_millis() as u32
    }

    fn input(&self, wpe: &Wpe, input: &WebViewInput) {
        let scale = self.scale.get();
        let backend = self.view_backend;
        let lib = &wpe.wpe;
        match input {
            WebViewInput::PointerMove { at } => {
                let (x, y) = device_point(*at, scale);
                let mut event = WpeInputPointerEvent {
                    kind: WPE_POINTER_MOTION,
                    time: self.time(),
                    x,
                    y,
                    button: 0,
                    state: 0,
                    modifiers: wpe_modifiers(self.keys.get(), &self.buttons.borrow()),
                };
                unsafe { (lib.wpe_view_backend_dispatch_pointer_event)(backend, &mut event) };
            }
            WebViewInput::PointerButton {
                at,
                button,
                pressed,
            } => {
                {
                    let mut held = self.buttons.borrow_mut();
                    held.retain(|b| b != button);
                    if *pressed {
                        held.push(*button);
                    }
                }
                let (x, y) = device_point(*at, scale);
                let mut event = WpeInputPointerEvent {
                    kind: WPE_POINTER_BUTTON,
                    time: self.time(),
                    x,
                    y,
                    button: wpe_button(*button),
                    state: u32::from(*pressed),
                    modifiers: wpe_modifiers(self.keys.get(), &self.buttons.borrow()),
                };
                unsafe { (lib.wpe_view_backend_dispatch_pointer_event)(backend, &mut event) };
            }
            WebViewInput::Wheel { at, delta } => {
                let (x, y) = device_point(*at, scale);
                let (x_axis, y_axis) = wheel_axes(*delta, scale);
                let mut event = WpeInputAxis2dEvent {
                    base: WpeInputAxisEvent {
                        kind: WPE_AXIS_MASK_2D | WPE_AXIS_MOTION_SMOOTH,
                        time: self.time(),
                        x,
                        y,
                        axis: 0,
                        value: 0,
                        modifiers: wpe_modifiers(self.keys.get(), &self.buttons.borrow()),
                    },
                    x_axis,
                    y_axis,
                };
                unsafe { (lib.wpe_view_backend_dispatch_axis_event)(backend, &mut event.base) };
            }
            // WPE has no leave event: the page keeps its last hover.
            WebViewInput::PointerLeave => {}
            WebViewInput::Key {
                native_key,
                native_scan,
                pressed,
                modifiers,
            } => {
                self.keys.set(*modifiers);
                let mut event = WpeInputKeyboardEvent {
                    time: self.time(),
                    key_code: *native_key,
                    hardware_key_code: *native_scan,
                    pressed: *pressed,
                    modifiers: wpe_modifiers(*modifiers, &self.buttons.borrow()),
                };
                unsafe { (lib.wpe_view_backend_dispatch_keyboard_event)(backend, &mut event) };
            }
            WebViewInput::Focus(focused) => self.set_activity(wpe, WPE_ACTIVITY_FOCUSED, *focused),
        }
    }

    /// The app's answer to request `request`: go ahead or stop.
    fn decide(&self, wpe: &Wpe, request: u64, allow: bool) {
        let Some(decision) = self.decisions.borrow_mut().remove(&request) else {
            return;
        };
        unsafe {
            if allow {
                (wpe.webkit.webkit_policy_decision_use)(decision);
            } else {
                (wpe.webkit.webkit_policy_decision_ignore)(decision);
            }
            (wpe.gobject.g_object_unref)(decision);
        }
    }

    /// Unhook and drop the view: no signal reaches it any more, what the app
    /// never answered is refused, and `WebKit` lets go of its view backend
    /// (whose sink goes with it, [`on_backend_destroyed`]).
    fn tear_down(self: Box<Self>, wpe: &Wpe) {
        unsafe {
            for handler in self.handlers.borrow_mut().drain(..) {
                (wpe.gobject.g_signal_handler_disconnect)(self.web_view, handler);
            }
            let pending = core::mem::take(&mut *self.decisions.borrow_mut());
            for decision in pending.into_values() {
                (wpe.webkit.webkit_policy_decision_ignore)(decision);
                (wpe.gobject.g_object_unref)(decision);
            }
            (wpe.webkit.webkit_web_view_stop_loading)(self.web_view);
            (wpe.gobject.g_object_unref)(self.web_view);
        }
    }
}

/// `decide-policy`: a page navigation waits for the app; a new window
/// (`target=_blank`, `window.open`) loads in this view instead; anything
/// else (a response) takes `WebKit`'s default.
unsafe extern "C" fn on_decide_policy(
    web_view: *mut c_void,
    decision: *mut c_void,
    kind: c_int,
    data: *mut c_void,
) -> GBoolean {
    let Some(view) = (unsafe { (data as *const View).as_ref() }) else {
        return 0;
    };
    let Ok(wpe) = wpe::wpe() else {
        return 0;
    };
    if kind != POLICY_NAVIGATION_ACTION && kind != POLICY_NEW_WINDOW_ACTION {
        return 0;
    }
    let webkit = &wpe.webkit;
    unsafe {
        let action = (webkit.webkit_navigation_policy_decision_get_navigation_action)(decision);
        if action.is_null() {
            return 0;
        }
        let request = (webkit.webkit_navigation_action_get_request)(action);
        let url = if request.is_null() {
            String::new()
        } else {
            c_string((webkit.webkit_uri_request_get_uri)(request))
        };
        if kind == POLICY_NEW_WINDOW_ACTION {
            (webkit.webkit_policy_decision_ignore)(decision);
            if !request.is_null() {
                (webkit.webkit_web_view_load_request)(web_view, request);
            }
            return 1;
        }
        if is_file_url(&url) {
            (webkit.webkit_policy_decision_ignore)(decision);
            return 1;
        }
        let is_redirect = (webkit.webkit_navigation_action_is_redirect)(action) != 0;
        (wpe.gobject.g_object_ref)(decision);
        let handle = view.last_request.get() + 1;
        view.last_request.set(handle);
        view.decisions.borrow_mut().insert(handle, decision);
        view.report(
            handle,
            WebViewEvent::NavigationRequested(WebViewNavigation {
                url: AzString::from(url),
                is_redirect,
            }),
        );
    }
    1
}

/// `load-changed`: a finished load is reported.
unsafe extern "C" fn on_load_changed(web_view: *mut c_void, event: c_int, data: *mut c_void) {
    if event != LOAD_FINISHED {
        return;
    }
    let (Some(view), Ok(wpe)) = (unsafe { (data as *const View).as_ref() }, wpe::wpe()) else {
        return;
    };
    let url = unsafe { c_string((wpe.webkit.webkit_web_view_get_uri)(web_view)) };
    view.report(0, WebViewEvent::LoadFinished(AzString::from(url)));
}

/// `load-failed`: a failure - not a navigation somebody cancelled.
unsafe extern "C" fn on_load_failed(
    _web_view: *mut c_void,
    _event: c_int,
    failing_uri: *const c_char,
    error: *mut GError,
    data: *mut c_void,
) -> GBoolean {
    let (Some(view), Ok(wpe)) = (unsafe { (data as *const View).as_ref() }, wpe::wpe()) else {
        return 0;
    };
    let (domain, code, message) = match unsafe { error.as_ref() } {
        Some(error) => unsafe {
            (
                c_string((wpe.glib.g_quark_to_string)(error.domain)),
                error.code,
                c_string(error.message),
            )
        },
        None => (String::new(), 0, String::from("the page did not load")),
    };
    if is_quiet_failure(&domain, code) {
        return 0;
    }
    let url = unsafe { c_string(failing_uri) };
    view.report(
        0,
        WebViewEvent::LoadFailed(WebViewLoadError {
            url: AzString::from(url),
            reason: AzString::from(message),
        }),
    );
    0
}

/// `notify::title`: the page's title changed.
unsafe extern "C" fn on_title(web_view: *mut c_void, _pspec: *mut c_void, data: *mut c_void) {
    let (Some(view), Ok(wpe)) = (unsafe { (data as *const View).as_ref() }, wpe::wpe()) else {
        return;
    };
    let title = unsafe { c_string((wpe.webkit.webkit_web_view_get_title)(web_view)) };
    view.report(0, WebViewEvent::TitleChanged(AzString::from(title)));
}

/// `web-process-terminated`: the page is gone.
unsafe extern "C" fn on_terminated(web_view: *mut c_void, _reason: c_int, data: *mut c_void) {
    let (Some(view), Ok(wpe)) = (unsafe { (data as *const View).as_ref() }, wpe::wpe()) else {
        return;
    };
    let url = unsafe { c_string((wpe.webkit.webkit_web_view_get_uri)(web_view)) };
    view.report(
        0,
        WebViewEvent::LoadFailed(WebViewLoadError {
            url: AzString::from(url),
            reason: AzString::from("the page's web process ended"),
        }),
    );
}
