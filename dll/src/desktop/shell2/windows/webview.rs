//! `<webview>` on Windows: Microsoft Edge `WebView2`, through
//! `WebView2Loader.dll` loaded with `LoadLibraryW` when the first web view
//! appears - nothing links it, and an app without a web view never loads it.
//!
//! Two parts, from two places: `WebView2Loader.dll`, which the app ships
//! next to its executable (from the `Microsoft.Web.WebView2` package), and
//! the `WebView2` Runtime (the Edge engine), which Windows 11 has and
//! Windows 10 gets from Microsoft's Evergreen installer. Without either, a
//! web view says which ([`unavailable_reason`]).
//!
//! # How
//!
//! [`WebView2Views`] is a window's backend. Per process and store one
//! environment (`CreateCoreWebView2EnvironmentWithOptions`): an ephemeral
//! store is a user-data folder of the process's own under `%TEMP%`, a
//! persistent one the app's under `%LOCALAPPDATA%` (never the system
//! browser's). Per view a clip window - a plain `STATIC` child of the azul
//! window at the visible part of the view's box - and a controller on it
//! whose bounds are the whole box, so a view scrolled half out of its box is
//! cut where azul cuts its content; `put_ZoomFactor` keeps a scaled page at
//! its own size. Settings: no host objects, no web messages (no bridge).
//!
//! The COM objects are called through their vtables by slot (`WebView2.h`
//! order, `slots`), and the handlers it calls back are implemented here
//! ([`Handler`]). Creation is asynchronous: a view placed or navigated
//! before its controller arrived is placed and navigated when it does.
//!
//! `NavigationStarting` is decided INSIDE its handler - its arguments are
//! valid only there, on the UI thread - by dispatching the report at once
//! (`common::webview::deliver_now`) and cancelling what the app (or the
//! `file://` rule) refused. The handlers run from the window loop's message
//! dispatch, with no borrow of any window live; the window is found by its
//! `HWND` (`registry::get_window`). The other events are reported and
//! pumped the same way.

use core::{
    cell::{Cell, OnceCell, RefCell},
    ffi::c_void,
    ptr,
};
use std::collections::BTreeMap;

use azul_core::{
    geom::LogicalRect,
    webview::{WebViewConfig, WebViewEvent, WebViewLoadError, WebViewNavigation, WebViewStorage},
};
use azul_css::AzString;
use azul_layout::managers::webview::{
    WebViewId, WebViewPlacement, WebViewPlatform, WebViewReport, WebViewTransform,
};

use super::{
    dlopen::{DynamicLibrary, BOOL, HINSTANCE, HRESULT, HWND, RECT},
    registry,
};
use crate::desktop::shell2::common::{webview::WebViewBackend, DlError, DynamicLibrary as _};

/// The loader the app ships next to its executable.
pub const WEBVIEW2_LOADER: &str = "WebView2Loader.dll";

const S_OK: HRESULT = 0;
#[allow(clippy::cast_possible_wrap)]
const E_NOINTERFACE: HRESULT = 0x8000_4002_u32 as i32;
#[allow(clippy::cast_possible_wrap)]
const E_POINTER: HRESULT = 0x8000_4003_u32 as i32;
const WS_CHILD: u32 = 0x4000_0000;
const WS_CLIPSIBLINGS: u32 = 0x0400_0000;
const WS_CLIPCHILDREN: u32 = 0x0200_0000;
const SW_HIDE: i32 = 0;
const SW_SHOWNA: i32 = 8;
const SWP_NOZORDER: u32 = 0x0004;
const SWP_NOACTIVATE: u32 = 0x0010;
/// `COINIT_APARTMENTTHREADED`: `WebView2` needs a single-threaded apartment.
const COINIT_APARTMENTTHREADED: u32 = 0x2;
/// `COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED`.
const WEB_ERROR_OPERATION_CANCELED: i32 = 14;

/// Vtable slots (`WebView2.h` order; 0-2 are `IUnknown`'s).
mod slots {
    pub const ADD_REF: usize = 1;
    pub const RELEASE: usize = 2;
    pub const ENVIRONMENT_CREATE_CONTROLLER: usize = 3;
    pub const CONTROLLER_PUT_IS_VISIBLE: usize = 4;
    pub const CONTROLLER_PUT_BOUNDS: usize = 6;
    pub const CONTROLLER_PUT_ZOOM_FACTOR: usize = 8;
    pub const CONTROLLER_CLOSE: usize = 24;
    pub const CONTROLLER_GET_CORE_WEBVIEW2: usize = 25;
    pub const WEBVIEW_GET_SETTINGS: usize = 3;
    pub const WEBVIEW_GET_SOURCE: usize = 4;
    pub const WEBVIEW_NAVIGATE: usize = 5;
    pub const WEBVIEW_ADD_NAVIGATION_STARTING: usize = 7;
    pub const WEBVIEW_ADD_NAVIGATION_COMPLETED: usize = 15;
    pub const WEBVIEW_ADD_PROCESS_FAILED: usize = 25;
    pub const WEBVIEW_RELOAD: usize = 31;
    pub const WEBVIEW_GO_BACK: usize = 40;
    pub const WEBVIEW_ADD_NEW_WINDOW_REQUESTED: usize = 44;
    pub const WEBVIEW_ADD_DOCUMENT_TITLE_CHANGED: usize = 46;
    pub const WEBVIEW_GET_DOCUMENT_TITLE: usize = 48;
    pub const SETTINGS_PUT_IS_WEB_MESSAGE_ENABLED: usize = 6;
    pub const SETTINGS_PUT_ARE_HOST_OBJECTS_ALLOWED: usize = 16;
    pub const NAVIGATION_STARTING_GET_URI: usize = 3;
    pub const NAVIGATION_STARTING_GET_IS_REDIRECTED: usize = 5;
    pub const NAVIGATION_STARTING_PUT_CANCEL: usize = 8;
    pub const NAVIGATION_COMPLETED_GET_IS_SUCCESS: usize = 3;
    pub const NAVIGATION_COMPLETED_GET_WEB_ERROR_STATUS: usize = 4;
    pub const NEW_WINDOW_GET_URI: usize = 3;
    pub const NEW_WINDOW_PUT_HANDLED: usize = 6;
}

/// A COM interface id.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Guid {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

const fn guid(data1: u32, data2: u16, data3: u16, data4: [u8; 8]) -> Guid {
    Guid {
        data1,
        data2,
        data3,
        data4,
    }
}

const IID_IUNKNOWN: Guid = guid(0, 0, 0, [0xC0, 0, 0, 0, 0, 0, 0, 0x46]);
const IID_ENVIRONMENT_COMPLETED: Guid = guid(
    0x8B4F_98CE,
    0xDB0D,
    0x4E71,
    [0x85, 0xFD, 0xC4, 0xC4, 0xEF, 0x1F, 0x26, 0x30],
);
const IID_CONTROLLER_COMPLETED: Guid = guid(
    0x86EF_6808,
    0x3C3F,
    0x4C6F,
    [0x97, 0x5E, 0x8C, 0xE0, 0xB9, 0x8F, 0x70, 0xBA],
);
const IID_NAVIGATION_STARTING: Guid = guid(
    0x0733_37A4,
    0x64D2,
    0x4C7E,
    [0xAC, 0x9F, 0x98, 0x7F, 0x0F, 0x61, 0x34, 0x97],
);
const IID_NAVIGATION_COMPLETED: Guid = guid(
    0x9F92_1239,
    0x20C4,
    0x455F,
    [0x9E, 0x3F, 0x60, 0x47, 0xA5, 0x0E, 0x24, 0x8B],
);
const IID_DOCUMENT_TITLE_CHANGED: Guid = guid(
    0x6423_D6B1,
    0x5A57,
    0x46C5,
    [0xBA, 0x46, 0xDB, 0xB3, 0x73, 0x5E, 0xE7, 0xC9],
);
const IID_NEW_WINDOW_REQUESTED: Guid = guid(
    0xACAA_30EF,
    0xA40C,
    0x47BD,
    [0x9C, 0xB9, 0xD9, 0xC2, 0xAA, 0xDC, 0x9F, 0xCB],
);
const IID_PROCESS_FAILED: Guid = guid(
    0x7D21_83F9,
    0xCCA8,
    0x40F2,
    [0x91, 0xA9, 0xEA, 0xFA, 0xD3, 0x2C, 0x8A, 0x9B],
);

/// `GetAvailableCoreWebView2BrowserVersionString(browserExecutableFolder,
/// versionInfo)`: the installed runtime's version, freed with
/// `CoTaskMemFree`.
type GetAvailableBrowserVersion = unsafe extern "system" fn(*const u16, *mut *mut u16) -> HRESULT;
/// `CreateCoreWebView2EnvironmentWithOptions(browserExecutableFolder,
/// userDataFolder, environmentOptions, environmentCreatedHandler)`.
type CreateEnvironmentWithOptions =
    unsafe extern "system" fn(*const u16, *const u16, *mut c_void, *mut c_void) -> HRESULT;

/// What looking for `WebView2` found.
#[derive(Debug, Clone)]
pub enum WebView2 {
    /// The loader did not load, or lacks an entry point.
    LoaderMissing(DlError),
    /// The loader is there; the runtime is not.
    NoRuntime,
    /// Both, and this is the runtime's version.
    Runtime(String),
}

/// What a Windows window does with a `<webview>`: a `WebView2` controller -
/// the loader is loaded at the first one.
#[must_use]
pub const fn platform() -> WebViewPlatform {
    WebViewPlatform::Backend
}

/// What a web view shows (and `WebViewLoadFailed` says) when `WebView2` is
/// missing: one line, what is missing and where it comes from - not the
/// loader's own error text. `None` when nothing is.
#[must_use]
pub fn unavailable_reason(found: &WebView2) -> Option<String> {
    match found {
        WebView2::Runtime(_) => None,
        WebView2::NoRuntime => Some(String::from(
            "This web view needs the Microsoft Edge WebView2 Runtime, which is not installed: \
             Windows 11 has it, Windows 10 gets it from Microsoft's Evergreen installer.",
        )),
        WebView2::LoaderMissing(DlError::SymbolNotFound { symbol, .. }) => Some(format!(
            "This web view needs Microsoft Edge WebView2, but {WEBVIEW2_LOADER} has no \
             {symbol}: it is older than azul supports."
        )),
        WebView2::LoaderMissing(_) => Some(format!(
            "This web view needs Microsoft Edge WebView2, but {WEBVIEW2_LOADER} was not found \
             next to the program: the app ships it (from the Microsoft.Web.WebView2 package)."
        )),
    }
}

/// A logical rect in whole device pixels, each edge rounded on its own (so
/// boxes that touch still touch).
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn physical(rect: LogicalRect, scale: f32) -> RECT {
    let edge = |v: f32| (v * scale).round() as i32;
    RECT {
        left: edge(rect.origin.x),
        top: edge(rect.origin.y),
        right: edge(rect.origin.x + rect.size.width),
        bottom: edge(rect.origin.y + rect.size.height),
    }
}

/// Where a placed view's clip window goes (the visible part, in the azul
/// window's client pixels) and where its page goes inside it (the whole
/// box, in the clip window's pixels).
#[must_use]
pub fn layout_in_container(placement: &WebViewPlacement, scale: f32) -> (RECT, RECT) {
    let container = physical(placement.clip, scale);
    let page = physical(placement.rect, scale);
    (
        container,
        RECT {
            left: page.left - container.left,
            top: page.top - container.top,
            right: page.right - container.left,
            bottom: page.bottom - container.top,
        },
    )
}

/// Whether a failed navigation is one somebody cancelled (the app, a new
/// navigation over it) rather than a failure.
#[must_use]
pub const fn is_quiet_failure(web_error_status: i32) -> bool {
    web_error_status == WEB_ERROR_OPERATION_CANCELED
}

/// `s` as a NUL-terminated UTF-16 string.
#[must_use]
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(core::iter::once(0)).collect()
}

/// The user-data folder of a store: the app's own under
/// `local_app_data` for a persistent one, the process's own under `temp`
/// for an ephemeral one (gone with the temp directory's cleanup).
#[must_use]
pub fn user_data_folder(
    storage: WebViewStorage,
    local_app_data: &str,
    temp: &str,
    app: &str,
    pid: u32,
) -> String {
    match storage {
        WebViewStorage::Persistent => format!(r"{local_app_data}\{app}\WebView2"),
        WebViewStorage::Ephemeral => format!(r"{temp}\azul-webview-{app}-{pid}"),
    }
}

/// The text of a `COREWEBVIEW2_WEB_ERROR_STATUS`.
const fn web_error_text(status: i32) -> &'static str {
    match status {
        1..=5 => "the site's certificate is not valid",
        6 | 12 => "the server could not be reached",
        7 => "the server did not answer in time",
        8 => "the server's answer was not valid",
        9..=11 => "the connection was lost",
        13 => "no such host",
        15 => "a redirect failed",
        _ => "the page did not load",
    }
}

fn is_file_url(url: &str) -> bool {
    url.trim_start()
        .get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("file:"))
}

// ---------------------------------------------------------------------------
// Loading the loader and ole32
// ---------------------------------------------------------------------------

/// The loaded loader (kept for the process: `WebView2` runs out of it).
struct Loader {
    _lib: DynamicLibrary,
    create_environment: CreateEnvironmentWithOptions,
}

/// ole32: the apartment `WebView2` needs, and the free for its strings.
struct Ole32 {
    _lib: DynamicLibrary,
    co_initialize_ex: unsafe extern "system" fn(*mut c_void, u32) -> HRESULT,
    co_task_mem_free: unsafe extern "system" fn(*mut c_void),
}

thread_local! {
    static LOADER: OnceCell<Result<&'static Loader, WebView2>> = const { OnceCell::new() };
    static OLE32: OnceCell<Option<&'static Ole32>> = const { OnceCell::new() };
    /// One environment per store, on the UI thread.
    static ENVIRONMENTS: RefCell<[Option<Environment>; 2]> = const { RefCell::new([None, None]) };
}

fn ole32() -> Option<&'static Ole32> {
    OLE32.with(|cell| {
        *cell.get_or_init(|| {
            let lib = DynamicLibrary::load("ole32.dll").ok()?;
            // SAFETY: the documented signatures of the exports named.
            let (co_initialize_ex, co_task_mem_free) = unsafe {
                (
                    lib.get_symbol("CoInitializeEx").ok()?,
                    lib.get_symbol("CoTaskMemFree").ok()?,
                )
            };
            Some(&*Box::leak(Box::new(Ole32 {
                _lib: lib,
                co_initialize_ex,
                co_task_mem_free,
            })))
        })
    })
}

/// The loader, with the runtime checked - once.
fn loader() -> Result<&'static Loader, WebView2> {
    LOADER.with(|cell| cell.get_or_init(load_loader).clone())
}

fn load_loader() -> Result<&'static Loader, WebView2> {
    let lib = DynamicLibrary::load(WEBVIEW2_LOADER).map_err(WebView2::LoaderMissing)?;
    // SAFETY: each type is the documented signature of the export named.
    let symbols = unsafe {
        (
            lib.get_symbol::<GetAvailableBrowserVersion>(
                "GetAvailableCoreWebView2BrowserVersionString",
            ),
            lib.get_symbol::<CreateEnvironmentWithOptions>(
                "CreateCoreWebView2EnvironmentWithOptions",
            ),
        )
    };
    let (get_version, create_environment) = match symbols {
        (Ok(get_version), Ok(create_environment)) => (get_version, create_environment),
        (Err(e), _) | (_, Err(e)) => return Err(WebView2::LoaderMissing(e)),
    };
    let mut version: *mut u16 = ptr::null_mut();
    // SAFETY: a null folder asks for the installed (Evergreen) runtime.
    let hr = unsafe { get_version(ptr::null(), &mut version) };
    let version = unsafe { take_wide(version) };
    if hr < 0 || version.is_empty() {
        return Err(WebView2::NoRuntime);
    }
    crate::plog_info!("[webview] Microsoft Edge WebView2 {version}");
    Ok(&*Box::leak(Box::new(Loader {
        _lib: lib,
        create_environment,
    })))
}

/// A NUL-terminated UTF-16 string a COM call allocated, as a Rust one -
/// freed. Empty for NULL.
unsafe fn take_wide(p: *mut u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let text = unsafe {
        let len = (0..).take_while(|&i| *p.add(i) != 0).count();
        String::from_utf16_lossy(core::slice::from_raw_parts(p, len))
    };
    if let Some(ole) = ole32() {
        unsafe { (ole.co_task_mem_free)(p.cast()) };
    }
    text
}

// ---------------------------------------------------------------------------
// COM calls by slot
// ---------------------------------------------------------------------------

/// The function in slot `index` of COM object `this`'s vtable, as `F` (a
/// function pointer type).
unsafe fn slot<F: Copy>(this: *mut c_void, index: usize) -> F {
    unsafe {
        let vtbl = *this.cast::<*const usize>();
        let entry = *vtbl.add(index);
        core::mem::transmute_copy(&entry)
    }
}

type Unary = unsafe extern "system" fn(*mut c_void) -> HRESULT;
type WithBool = unsafe extern "system" fn(*mut c_void, BOOL) -> HRESULT;
type GetBool = unsafe extern "system" fn(*mut c_void, *mut BOOL) -> HRESULT;
type GetI32 = unsafe extern "system" fn(*mut c_void, *mut i32) -> HRESULT;
type GetString = unsafe extern "system" fn(*mut c_void, *mut *mut u16) -> HRESULT;
type GetObject = unsafe extern "system" fn(*mut c_void, *mut *mut c_void) -> HRESULT;
type WithString = unsafe extern "system" fn(*mut c_void, *const u16) -> HRESULT;
type AddHandler = unsafe extern "system" fn(*mut c_void, *mut c_void, *mut i64) -> HRESULT;
type RefCount = unsafe extern "system" fn(*mut c_void) -> u32;
type QueryInterface =
    unsafe extern "system" fn(*mut Handler, *const Guid, *mut *mut c_void) -> HRESULT;

unsafe fn add_ref(this: *mut c_void) {
    if !this.is_null() {
        let f: RefCount = unsafe { slot(this, slots::ADD_REF) };
        unsafe { f(this) };
    }
}

unsafe fn release(this: *mut c_void) {
    if !this.is_null() {
        let f: RefCount = unsafe { slot(this, slots::RELEASE) };
        unsafe { f(this) };
    }
}

unsafe fn get_string(this: *mut c_void, index: usize) -> String {
    let mut out: *mut u16 = ptr::null_mut();
    unsafe {
        let f: GetString = slot(this, index);
        if f(this, &mut out) < 0 {
            return String::new();
        }
        take_wide(out)
    }
}

// ---------------------------------------------------------------------------
// The handlers WebView2 calls back
// ---------------------------------------------------------------------------

/// What a [`Handler`] is for.
#[derive(Debug, Clone, Copy)]
enum Kind {
    Environment(WebViewStorage),
    Controller,
    NavigationStarting,
    NavigationCompleted,
    TitleChanged,
    NewWindow,
    ProcessFailed,
}

/// A COM object `WebView2` calls: a completion handler (`Invoke(HRESULT,
/// object)`) or an event handler (`Invoke(sender, args)`) for view `id` of
/// the window `window`. Reference counted; called on the UI thread only.
#[repr(C)]
struct Handler {
    vtbl: *const c_void,
    refs: Cell<u32>,
    iid: Guid,
    window: HWND,
    id: WebViewId,
    kind: Kind,
}

#[repr(C)]
struct CompletedVtbl {
    query_interface: QueryInterface,
    add_ref: unsafe extern "system" fn(*mut Handler) -> u32,
    release: unsafe extern "system" fn(*mut Handler) -> u32,
    invoke: unsafe extern "system" fn(*mut Handler, HRESULT, *mut c_void) -> HRESULT,
}

#[repr(C)]
struct EventVtbl {
    query_interface: QueryInterface,
    add_ref: unsafe extern "system" fn(*mut Handler) -> u32,
    release: unsafe extern "system" fn(*mut Handler) -> u32,
    invoke: unsafe extern "system" fn(*mut Handler, *mut c_void, *mut c_void) -> HRESULT,
}

static COMPLETED_VTBL: CompletedVtbl = CompletedVtbl {
    query_interface: handler_query_interface,
    add_ref: handler_add_ref,
    release: handler_release,
    invoke: handler_invoke_completed,
};

static EVENT_VTBL: EventVtbl = EventVtbl {
    query_interface: handler_query_interface,
    add_ref: handler_add_ref,
    release: handler_release,
    invoke: handler_invoke_event,
};

impl Handler {
    /// A new handler holding one reference (the caller's, released once
    /// `WebView2` took its own).
    fn create(kind: Kind, window: HWND, id: WebViewId) -> *mut c_void {
        let completed = core::ptr::from_ref(&COMPLETED_VTBL).cast::<c_void>();
        let event = core::ptr::from_ref(&EVENT_VTBL).cast::<c_void>();
        let (vtbl, iid) = match kind {
            Kind::Environment(_) => (completed, IID_ENVIRONMENT_COMPLETED),
            Kind::Controller => (completed, IID_CONTROLLER_COMPLETED),
            Kind::NavigationStarting => (event, IID_NAVIGATION_STARTING),
            Kind::NavigationCompleted => (event, IID_NAVIGATION_COMPLETED),
            Kind::TitleChanged => (event, IID_DOCUMENT_TITLE_CHANGED),
            Kind::NewWindow => (event, IID_NEW_WINDOW_REQUESTED),
            Kind::ProcessFailed => (event, IID_PROCESS_FAILED),
        };
        Box::into_raw(Box::new(Self {
            vtbl,
            refs: Cell::new(1),
            iid,
            window,
            id,
            kind,
        }))
        .cast()
    }
}

unsafe extern "system" fn handler_query_interface(
    this: *mut Handler,
    riid: *const Guid,
    out: *mut *mut c_void,
) -> HRESULT {
    if out.is_null() || riid.is_null() {
        return E_POINTER;
    }
    unsafe {
        let wanted = *riid;
        if wanted == IID_IUNKNOWN || wanted == (*this).iid {
            handler_add_ref(this);
            *out = this.cast();
            S_OK
        } else {
            *out = ptr::null_mut();
            E_NOINTERFACE
        }
    }
}

unsafe extern "system" fn handler_add_ref(this: *mut Handler) -> u32 {
    let handler = unsafe { &*this };
    handler.refs.set(handler.refs.get() + 1);
    handler.refs.get()
}

unsafe extern "system" fn handler_release(this: *mut Handler) -> u32 {
    let left = {
        let handler = unsafe { &*this };
        let left = handler.refs.get().saturating_sub(1);
        handler.refs.set(left);
        left
    };
    if left == 0 {
        drop(unsafe { Box::from_raw(this) });
    }
    left
}

unsafe extern "system" fn handler_invoke_completed(
    this: *mut Handler,
    result: HRESULT,
    object: *mut c_void,
) -> HRESULT {
    let (kind, window, id) = {
        let handler = unsafe { &*this };
        (handler.kind, handler.window, handler.id)
    };
    unsafe {
        match kind {
            Kind::Environment(storage) => environment_created(storage, result, object),
            Kind::Controller => controller_created(window, id, result, object),
            _ => {}
        }
    }
    S_OK
}

unsafe extern "system" fn handler_invoke_event(
    this: *mut Handler,
    sender: *mut c_void,
    args: *mut c_void,
) -> HRESULT {
    let (kind, window, id) = {
        let handler = unsafe { &*this };
        (handler.kind, handler.window, handler.id)
    };
    unsafe {
        match kind {
            Kind::NavigationStarting => navigation_starting(window, id, args),
            Kind::NavigationCompleted => navigation_completed(window, id, sender, args),
            Kind::TitleChanged => title_changed(window, id, sender),
            Kind::NewWindow => new_window(sender, args),
            Kind::ProcessFailed => report_later(
                window,
                id,
                WebViewEvent::LoadFailed(WebViewLoadError {
                    url: AzString::from(get_string(sender, slots::WEBVIEW_GET_SOURCE)),
                    reason: AzString::from("the page's browser process ended"),
                }),
            ),
            Kind::Environment(_) | Kind::Controller => {}
        }
    }
    S_OK
}

// ---------------------------------------------------------------------------
// Environments and controllers
// ---------------------------------------------------------------------------

/// One store's environment, on the UI thread.
enum Environment {
    /// Being created: the views waiting for it - (window, view, clip window).
    Creating(Vec<(HWND, WebViewId, HWND)>),
    Ready(*mut c_void),
    Failed(String),
}

const fn store_index(storage: WebViewStorage) -> usize {
    match storage {
        WebViewStorage::Ephemeral => 0,
        WebViewStorage::Persistent => 1,
    }
}

/// Ask for a controller for view `id` of `window` on its clip window
/// `container`, in `storage`'s environment - made first if it is not.
/// `Err`: it will never come (the caller tells the view's app).
fn request_controller(
    storage: WebViewStorage,
    window: HWND,
    id: WebViewId,
    container: HWND,
) -> Result<(), String> {
    enum Next {
        Create(*mut c_void),
        StartEnvironment,
        Wait,
        Fail(String),
    }
    let index = store_index(storage);
    let next = ENVIRONMENTS.with(|envs| {
        let mut envs = envs.borrow_mut();
        match &mut envs[index] {
            Some(Environment::Ready(env)) => Next::Create(*env),
            Some(Environment::Creating(waiting)) => {
                waiting.push((window, id, container));
                Next::Wait
            }
            Some(Environment::Failed(reason)) => Next::Fail(reason.clone()),
            None => {
                envs[index] = Some(Environment::Creating(vec![(window, id, container)]));
                Next::StartEnvironment
            }
        }
    });
    match next {
        Next::Create(env) => unsafe { create_controller(env, window, id, container) },
        Next::Wait => Ok(()),
        Next::Fail(reason) => Err(reason),
        Next::StartEnvironment => start_environment(storage).inspect_err(|reason| {
            // Only this view waited for it.
            ENVIRONMENTS.with(|envs| {
                envs.borrow_mut()[index] = Some(Environment::Failed(reason.clone()));
            });
        }),
    }
}

/// `CreateCoreWebView2EnvironmentWithOptions` for `storage`; the answer
/// comes to [`environment_created`].
fn start_environment(storage: WebViewStorage) -> Result<(), String> {
    let loader = loader().map_err(|missing| {
        unavailable_reason(&missing).unwrap_or_else(|| String::from("WebView2 did not start"))
    })?;
    if let Some(ole) = ole32() {
        // S_FALSE (already) and RPC_E_CHANGED_MODE (another apartment) both
        // leave the thread usable or as it was.
        unsafe { (ole.co_initialize_ex)(ptr::null_mut(), COINIT_APARTMENTTHREADED) };
    }
    let app = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .unwrap_or_else(|| String::from("azul-app"));
    let folder = user_data_folder(
        storage,
        &std::env::var("LOCALAPPDATA").unwrap_or_else(|_| String::from(".")),
        &std::env::var("TEMP").unwrap_or_else(|_| String::from(".")),
        &app,
        std::process::id(),
    );
    let folder = wide(&folder);
    let handler = Handler::create(Kind::Environment(storage), ptr::null_mut(), WebViewId(0));
    let hr = unsafe {
        let hr = (loader.create_environment)(
            ptr::null(),
            folder.as_ptr(),
            ptr::null_mut(),
            handler,
        );
        release(handler);
        hr
    };
    if hr < 0 {
        return Err(format!("WebView2 did not start (error {hr:#x})"));
    }
    Ok(())
}

unsafe fn environment_created(storage: WebViewStorage, result: HRESULT, env: *mut c_void) {
    if result < 0 || env.is_null() {
        environment_created_with(
            storage,
            Err(format!("WebView2 did not start (error {result:#x})")),
        );
    } else {
        unsafe { add_ref(env) };
        environment_created_with(storage, Ok(env));
    }
}

/// The environment of `storage` is there (or will never be): the views
/// waiting for it get their controllers (or their failure).
fn environment_created_with(storage: WebViewStorage, env: Result<*mut c_void, String>) {
    let index = store_index(storage);
    let waiting = ENVIRONMENTS.with(|envs| {
        let mut envs = envs.borrow_mut();
        let waiting = match envs[index].take() {
            Some(Environment::Creating(waiting)) => waiting,
            _ => Vec::new(),
        };
        envs[index] = Some(match &env {
            Ok(env) => Environment::Ready(*env),
            Err(reason) => Environment::Failed(reason.clone()),
        });
        waiting
    });
    for (window, id, container) in waiting {
        let made = match &env {
            Ok(env) => unsafe { create_controller(*env, window, id, container) },
            Err(reason) => Err(reason.clone()),
        };
        if let Err(reason) = made {
            fail_view(window, id, &reason);
        }
    }
}

/// `CreateCoreWebView2Controller` on the clip window; the answer comes to
/// [`controller_created`].
unsafe fn create_controller(
    env: *mut c_void,
    window: HWND,
    id: WebViewId,
    container: HWND,
) -> Result<(), String> {
    let handler = Handler::create(Kind::Controller, window, id);
    unsafe {
        let create: unsafe extern "system" fn(*mut c_void, HWND, *mut c_void) -> HRESULT =
            slot(env, slots::ENVIRONMENT_CREATE_CONTROLLER);
        let hr = create(env, container, handler);
        release(handler);
        if hr < 0 {
            return Err(format!("WebView2 made no view (error {hr:#x})"));
        }
    }
    Ok(())
}

/// View `id`'s controller is there (from the message dispatch: no borrow
/// of the window is live). The backend is busy while it is set up, so a
/// handler `WebView2` calls from inside does not dispatch.
unsafe fn controller_created(
    window: HWND,
    id: WebViewId,
    result: HRESULT,
    controller: *mut c_void,
) {
    let Some(views) = (unsafe { views_of(window) }) else {
        if result >= 0 {
            unsafe { close_controller(controller, ptr::null_mut()) };
        }
        return;
    };
    let was = views.busy;
    views.busy = true;
    let failure = unsafe { attach(views, window, id, result, controller) };
    if let Some(views) = unsafe { views_of(window) } {
        views.busy = was;
    }
    if let Some(reason) = failure {
        fail_view(window, id, &reason);
    }
}

/// Set view `id` up on its new controller: page, settings, handlers, and
/// what it was told before it was there. `Some(reason)`: it failed.
unsafe fn attach(
    views: &mut WebView2Views,
    window: HWND,
    id: WebViewId,
    result: HRESULT,
    controller: *mut c_void,
) -> Option<String> {
    if result < 0 || controller.is_null() {
        if let Some(view) = views.views.remove(&id) {
            view.tear_down();
        }
        return Some(format!("WebView2 made no view (error {result:#x})"));
    }
    unsafe {
        add_ref(controller);
        let mut webview: *mut c_void = ptr::null_mut();
        let get_webview: GetObject = slot(controller, slots::CONTROLLER_GET_CORE_WEBVIEW2);
        if get_webview(controller, &mut webview) < 0 || webview.is_null() {
            close_controller(controller, webview);
            if let Some(view) = views.views.remove(&id) {
                view.tear_down();
            }
            return Some(String::from("WebView2 made no page"));
        }
        let Some(view) = views.views.get_mut(&id) else {
            // Destroyed while its controller was being made.
            close_controller(controller, webview);
            return None;
        };
        view.controller = controller;
        view.webview = webview;
        configure(webview);
        for kind in [
            Kind::NavigationStarting,
            Kind::NavigationCompleted,
            Kind::TitleChanged,
            Kind::NewWindow,
            Kind::ProcessFailed,
        ] {
            let index = match kind {
                Kind::NavigationStarting => slots::WEBVIEW_ADD_NAVIGATION_STARTING,
                Kind::NavigationCompleted => slots::WEBVIEW_ADD_NAVIGATION_COMPLETED,
                Kind::TitleChanged => slots::WEBVIEW_ADD_DOCUMENT_TITLE_CHANGED,
                Kind::NewWindow => slots::WEBVIEW_ADD_NEW_WINDOW_REQUESTED,
                _ => slots::WEBVIEW_ADD_PROCESS_FAILED,
            };
            let handler = Handler::create(kind, window, id);
            let add: AddHandler = slot(webview, index);
            let mut token = 0_i64;
            add(webview, handler, &mut token);
            release(handler);
        }
        let scale = views.scale;
        if let Some(view) = views.views.get_mut(&id) {
            view.apply_placement(scale);
            view.apply_zoom();
            if let Some(url) = view.pending_url.take() {
                view.navigate_now(&url);
            }
        }
    }
    None
}

/// No script bridge: no host objects, no web messages.
unsafe fn configure(webview: *mut c_void) {
    unsafe {
        let mut settings: *mut c_void = ptr::null_mut();
        let get: GetObject = slot(webview, slots::WEBVIEW_GET_SETTINGS);
        if get(webview, &mut settings) < 0 || settings.is_null() {
            return;
        }
        let host_objects: WithBool = slot(settings, slots::SETTINGS_PUT_ARE_HOST_OBJECTS_ALLOWED);
        host_objects(settings, 0);
        let web_messages: WithBool = slot(settings, slots::SETTINGS_PUT_IS_WEB_MESSAGE_ENABLED);
        web_messages(settings, 0);
        release(settings);
    }
}

unsafe fn close_controller(controller: *mut c_void, webview: *mut c_void) {
    unsafe {
        if !controller.is_null() {
            let close: Unary = slot(controller, slots::CONTROLLER_CLOSE);
            close(controller);
        }
        release(webview);
        release(controller);
    }
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

/// The backend of the window `window`, if it is still there. The handlers
/// run from the window loop's message dispatch, with no borrow of a window
/// live.
unsafe fn views_of(window: HWND) -> Option<&'static mut WebView2Views> {
    let ptr = registry::get_window(window)?;
    unsafe { (*ptr).webviews.as_mut() }
}

/// `NavigationStarting`: decided now - the app's callbacks run inside this
/// handler (`deliver_now`), and what they refused is cancelled.
unsafe fn navigation_starting(window: HWND, id: WebViewId, args: *mut c_void) {
    unsafe {
        let url = get_string(args, slots::NAVIGATION_STARTING_GET_URI);
        let cancel: WithBool = slot(args, slots::NAVIGATION_STARTING_PUT_CANCEL);
        if is_file_url(&url) {
            cancel(args, 1);
            return;
        }
        let mut redirected: BOOL = 0;
        let get_redirected: GetBool = slot(args, slots::NAVIGATION_STARTING_GET_IS_REDIRECTED);
        get_redirected(args, &mut redirected);
        if !decide_now(window, id, url, redirected != 0) {
            cancel(args, 1);
        }
    }
}

/// Ask the app about a navigation of view `id` at once. A handler that ran
/// inside the backend's own call (`busy`) has no window to ask: it goes
/// ahead.
unsafe fn decide_now(window: HWND, id: WebViewId, url: String, is_redirect: bool) -> bool {
    let Some(ptr) = registry::get_window(window) else {
        return true;
    };
    let win = unsafe { &mut *ptr };
    let request = {
        let Some(views) = win.webviews.as_mut() else {
            return true;
        };
        if views.busy {
            return true;
        }
        views.busy = true;
        views.last_request += 1;
        views.last_request
    };
    let report = WebViewReport {
        id,
        request,
        event: WebViewEvent::NavigationRequested(WebViewNavigation {
            url: AzString::from(url),
            is_redirect,
        }),
    };
    let result = crate::desktop::shell2::common::webview::deliver_now(win, &report);
    if result != azul_core::events::ProcessEventResult::DoNothing {
        win.request_redraw();
    }
    let Some(views) = win.webviews.as_mut() else {
        return true;
    };
    views.busy = false;
    views.decided.remove(&request).unwrap_or(true)
}

unsafe fn navigation_completed(
    window: HWND,
    id: WebViewId,
    webview: *mut c_void,
    args: *mut c_void,
) {
    unsafe {
        let url = AzString::from(get_string(webview, slots::WEBVIEW_GET_SOURCE));
        let mut success: BOOL = 0;
        let get_success: GetBool = slot(args, slots::NAVIGATION_COMPLETED_GET_IS_SUCCESS);
        get_success(args, &mut success);
        if success != 0 {
            report_later(window, id, WebViewEvent::LoadFinished(url));
            return;
        }
        let mut status = 0_i32;
        let get_status: GetI32 = slot(args, slots::NAVIGATION_COMPLETED_GET_WEB_ERROR_STATUS);
        get_status(args, &mut status);
        if !is_quiet_failure(status) {
            report_later(
                window,
                id,
                WebViewEvent::LoadFailed(WebViewLoadError {
                    url,
                    reason: AzString::from(web_error_text(status)),
                }),
            );
        }
    }
}

unsafe fn title_changed(window: HWND, id: WebViewId, webview: *mut c_void) {
    let title = unsafe { get_string(webview, slots::WEBVIEW_GET_DOCUMENT_TITLE) };
    unsafe { report_later(window, id, WebViewEvent::TitleChanged(AzString::from(title))) };
}

/// A new window (`target=_blank`, `window.open`): loaded in this view
/// instead - its own `NavigationStarting` asks the app.
unsafe fn new_window(webview: *mut c_void, args: *mut c_void) {
    unsafe {
        let url = get_string(args, slots::NEW_WINDOW_GET_URI);
        let handled: WithBool = slot(args, slots::NEW_WINDOW_PUT_HANDLED);
        handled(args, 1);
        if !url.is_empty() {
            let wide_url = wide(&url);
            let navigate: WithString = slot(webview, slots::WEBVIEW_NAVIGATE);
            navigate(webview, wide_url.as_ptr());
        }
    }
}

/// Report `event` for view `id` and run the window's web view pump now
/// (unless the backend is inside a call of its own).
unsafe fn report_later(window: HWND, id: WebViewId, event: WebViewEvent) {
    let Some(ptr) = registry::get_window(window) else {
        return;
    };
    let win = unsafe { &mut *ptr };
    let busy = match win.webviews.as_mut() {
        Some(views) => {
            views.reports.push(WebViewReport {
                id,
                request: 0,
                event,
            });
            views.busy
        }
        None => return,
    };
    if !busy {
        win.pump_webviews_if_any();
    }
}

/// View `id` cannot show a page: its app hears why.
fn fail_view(window: HWND, id: WebViewId, reason: &str) {
    unsafe {
        report_later(
            window,
            id,
            WebViewEvent::LoadFailed(WebViewLoadError {
                url: AzString::from(""),
                reason: AzString::from(reason),
            }),
        );
    }
}

// ---------------------------------------------------------------------------
// The backend
// ---------------------------------------------------------------------------

/// The user32 entry points the backend needs (copied from the window's).
#[derive(Clone, Copy)]
pub struct User32 {
    pub create_window_ex_w: unsafe extern "system" fn(
        u32,
        *const u16,
        *const u16,
        u32,
        i32,
        i32,
        i32,
        i32,
        HWND,
        *mut c_void,
        HINSTANCE,
        *mut c_void,
    ) -> HWND,
    pub destroy_window: unsafe extern "system" fn(HWND) -> BOOL,
    pub show_window: unsafe extern "system" fn(HWND, i32) -> BOOL,
    pub set_window_pos: unsafe extern "system" fn(HWND, HWND, i32, i32, i32, i32, u32) -> BOOL,
}

/// One view.
struct View2 {
    user32: User32,
    /// Its clip window (the visible part of its box).
    container: HWND,
    /// Null until `WebView2` made it.
    controller: *mut c_void,
    webview: *mut c_void,
    /// Where to go once the controller is there.
    pending_url: Option<String>,
    placement: WebViewPlacement,
    zoom: f64,
}

impl View2 {
    /// Clip window to the visible part, the page at its whole box inside it.
    fn apply_placement(&self, scale: f32) {
        let (container, page) = layout_in_container(&self.placement, scale);
        unsafe {
            if self.placement.visible {
                (self.user32.set_window_pos)(
                    self.container,
                    ptr::null_mut(),
                    container.left,
                    container.top,
                    container.right - container.left,
                    container.bottom - container.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                (self.user32.show_window)(self.container, SW_SHOWNA);
            } else {
                (self.user32.show_window)(self.container, SW_HIDE);
            }
            if self.controller.is_null() {
                return;
            }
            let put_bounds: unsafe extern "system" fn(*mut c_void, RECT) -> HRESULT =
                slot(self.controller, slots::CONTROLLER_PUT_BOUNDS);
            put_bounds(self.controller, page);
            let put_visible: WithBool = slot(self.controller, slots::CONTROLLER_PUT_IS_VISIBLE);
            put_visible(self.controller, BOOL::from(self.placement.visible));
        }
    }

    fn apply_zoom(&self) {
        if self.controller.is_null() {
            return;
        }
        unsafe {
            let put_zoom: unsafe extern "system" fn(*mut c_void, f64) -> HRESULT =
                slot(self.controller, slots::CONTROLLER_PUT_ZOOM_FACTOR);
            put_zoom(self.controller, self.zoom);
        }
    }

    fn navigate_now(&self, url: &str) {
        let url = wide(url);
        unsafe {
            let navigate: WithString = slot(self.webview, slots::WEBVIEW_NAVIGATE);
            navigate(self.webview, url.as_ptr());
        }
    }

    fn tear_down(self) {
        unsafe {
            close_controller(self.controller, self.webview);
            (self.user32.destroy_window)(self.container);
        }
    }
}

/// The web views of one Windows window (see the module docs).
pub struct WebView2Views {
    window: HWND,
    instance: HINSTANCE,
    user32: User32,
    scale: f32,
    views: BTreeMap<WebViewId, View2>,
    /// Reports for the pump.
    reports: Vec<WebViewReport>,
    /// The app's answers to the requests `decide_now` asked, by handle.
    decided: BTreeMap<u64, bool>,
    last_request: u64,
    /// Inside a call of the backend's own, or a decision: a handler run
    /// from there must not dispatch.
    busy: bool,
}

impl WebView2Views {
    /// The backend of the window `window`. Loads nothing: the loader is
    /// loaded by the first `create`.
    #[must_use]
    pub const fn new(window: HWND, instance: HINSTANCE, user32: User32, scale: f32) -> Self {
        Self {
            window,
            instance,
            user32,
            scale,
            views: BTreeMap::new(),
            reports: Vec::new(),
            decided: BTreeMap::new(),
            last_request: 0,
            busy: false,
        }
    }

    /// The window's scale changed (it moved to another monitor).
    pub fn set_scale(&mut self, scale: f32) {
        if (scale - self.scale).abs() > f32::EPSILON {
            self.scale = scale;
            for view in self.views.values() {
                view.apply_placement(scale);
            }
        }
    }

    /// Run `f` with the backend marked busy: a handler `WebView2` calls from
    /// inside it does not dispatch.
    fn guarded<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let was = self.busy;
        self.busy = true;
        let out = f(self);
        self.busy = was;
        out
    }
}

impl WebViewBackend for WebView2Views {
    fn create(&mut self, id: WebViewId, config: WebViewConfig, src: &str) -> Result<(), String> {
        if let Err(missing) = loader() {
            return Err(unavailable_reason(&missing)
                .unwrap_or_else(|| String::from("WebView2 did not start")));
        }
        let class = wide("STATIC");
        let container = unsafe {
            (self.user32.create_window_ex_w)(
                0,
                class.as_ptr(),
                ptr::null(),
                WS_CHILD | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
                0,
                0,
                1,
                1,
                self.window,
                ptr::null_mut(),
                self.instance,
                ptr::null_mut(),
            )
        };
        if container.is_null() {
            return Err(String::from("the web view's window could not be created"));
        }
        self.views.insert(
            id,
            View2 {
                user32: self.user32,
                container,
                controller: ptr::null_mut(),
                webview: ptr::null_mut(),
                pending_url: (!src.is_empty()).then(|| src.to_string()),
                placement: WebViewPlacement::HIDDEN,
                zoom: 1.0,
            },
        );
        let window = self.window;
        let requested =
            self.guarded(|_| request_controller(config.storage, window, id, container));
        if let Err(reason) = requested {
            if let Some(view) = self.views.remove(&id) {
                view.tear_down();
            }
            self.reports.push(WebViewReport {
                id,
                request: 0,
                event: WebViewEvent::LoadFailed(WebViewLoadError {
                    url: AzString::from(src),
                    reason: AzString::from(reason.as_str()),
                }),
            });
        }
        Ok(())
    }
    fn place(&mut self, id: WebViewId, placement: &WebViewPlacement) {
        let scale = self.scale;
        self.guarded(|views| {
            if let Some(view) = views.views.get_mut(&id) {
                view.placement = *placement;
                view.apply_placement(scale);
            }
        });
    }
    fn transform(&mut self, id: WebViewId, transform: &WebViewTransform) {
        let (x, y) = transform.zoom();
        let zoom = f64::from(x.min(y));
        if !zoom.is_finite() || zoom <= 0.0 {
            return;
        }
        self.guarded(|views| {
            if let Some(view) = views.views.get_mut(&id) {
                view.zoom = zoom;
                view.apply_zoom();
            }
        });
    }
    fn navigate(&mut self, id: WebViewId, url: &str) {
        self.guarded(|views| {
            if let Some(view) = views.views.get_mut(&id) {
                if view.webview.is_null() {
                    view.pending_url = Some(url.to_string());
                } else {
                    view.navigate_now(url);
                }
            }
        });
    }
    fn reload(&mut self, id: WebViewId) {
        self.guarded(|views| {
            if let Some(view) = views.views.get(&id).filter(|v| !v.webview.is_null()) {
                unsafe {
                    let reload: Unary = slot(view.webview, slots::WEBVIEW_RELOAD);
                    reload(view.webview);
                }
            }
        });
    }
    fn go_back(&mut self, id: WebViewId) {
        self.guarded(|views| {
            if let Some(view) = views.views.get(&id).filter(|v| !v.webview.is_null()) {
                unsafe {
                    let back: Unary = slot(view.webview, slots::WEBVIEW_GO_BACK);
                    back(view.webview);
                }
            }
        });
    }
    fn decide_navigation(&mut self, _id: WebViewId, request: u64, allow: bool) {
        // Read by `decide_now` once the callbacks ran; a simulated request
        // nobody waits for is dropped at the next decision.
        self.decided.insert(request, allow);
    }
    fn destroy(&mut self, id: WebViewId) {
        if let Some(view) = self.views.remove(&id) {
            self.guarded(|_| view.tear_down());
        }
    }
    fn poll_reports(&mut self) -> Vec<WebViewReport> {
        core::mem::take(&mut self.reports)
    }
}

impl Drop for WebView2Views {
    fn drop(&mut self) {
        self.busy = true;
        for view in core::mem::take(&mut self.views).into_values() {
            view.tear_down();
        }
    }
}
