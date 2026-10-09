//! `<webview>` on Linux (X11 and Wayland): WPE `WebKit`, every library
//! loaded with `dlopen` when the first web view appears - an app without one
//! loads nothing, and a machine without WPE `WebKit` runs the app all the same.
//!
//! # Why WPE `WebKit`, not `WebKitGTK`
//!
//! An azul window is an X11 window or a Wayland surface of its own, not a
//! GTK one. `WebKitGTK`'s view is a `GtkWidget`: it needs a GTK window tree
//! and the GTK main loop around it. Putting one into an azul window means
//! XEmbed (`GtkPlug`/`GtkSocket` - X11 only, and gone in GTK 4) or a GTK
//! toplevel kept over the view's rectangle (Wayland lets no client position
//! a toplevel, and on X11 it fights the window manager and cannot be clipped
//! by a scroll container or stacked under an azul popup).
//!
//! WPE `WebKit` is the same engine without a toolkit: it renders into
//! buffers handed to the embedder (`WPEBackend-fdo`'s exportable view
//! backend - shared memory, or EGL images on the GPU path) and takes input as
//! plain events. azul composites the buffer as an image at the view's box,
//! so clipping, scrolling, stacking and transforms are the engine's own, and
//! X11 and Wayland are one code path.
//!
//! # What is here (phase 1)
//!
//! The loading layer: every library and every function the embedding needs,
//! resolved through `dlopen`/`dlsym` ([`Wpe::load`], kept for the life of the
//! process by [`wpe`]), and the probe a window runs when its first web view
//! appears ([`probe`], the `WebViewPlatform` from [`platform`]). Its answer
//! is what the view shows and what the app's `WebViewLoadFailed` reads: the
//! libraries tried and the packages that install them, or the version found
//! and that the embedding itself comes later.
//!
//! # The embedding (phase 2)
//!
//! A `WebViewBackend` on top of [`Wpe`]: `wpe_loader_init` with
//! `WPEBackend-fdo`, `wpe_fdo_initialize_shm` (or the EGL variant on the GPU
//! path); per view an exportable backend at the box's size, a
//! `WebKitWebViewBackend` around it and a `WebKitWebView` in an ephemeral
//! network session (2.0 API; an ephemeral web context in 1.x) or a
//! persistent one under the app's data directory, `file://` access off.
//! `decide-policy` holds the `WebKitPolicyDecision` (a reference) until the
//! app's `WebViewNavigationRequested` callback ran, then `use`s or `ignore`s
//! it; `load-changed`/`load-failed`/`notify::title` become the other
//! reports. Each exported frame is copied into an image the display list
//! draws at the view's item and released with `frame_complete`; pointer,
//! wheel and key events over the view's box go to `wpe_view_backend_dispatch_*`.
//! The GLib main context is iterated from the shell's loop (non-blocking),
//! with its poll fds in the shell's poll set.

use std::{
    cell::OnceCell,
    ffi::{c_char, c_int, c_uint, c_ulong, c_void},
    fmt,
    sync::OnceLock,
};

use azul_css::AzString;
use azul_layout::managers::webview::WebViewPlatform;

use super::x11::dlopen::Library;
use crate::{
    desktop::shell2::common::{dlopen::load_first_available, DlError, DynamicLibrary as _},
    load_symbol,
};

/// WPE `WebKit`, newest API first: 2.0 (libsoup 3, network sessions), 1.1
/// (libsoup 3, web contexts), 1.0 (libsoup 2).
pub const WPE_WEBKIT: &[&str] = &[
    "libWPEWebKit-2.0.so.1",
    "libWPEWebKit-1.1.so.0",
    "libWPEWebKit-1.0.so.3",
];
/// libwpe: the view backend interface and input events.
pub const LIBWPE: &[&str] = &["libwpe-1.0.so.1"];
/// `WPEBackend-fdo`: the exportable view backend whose frames azul draws.
pub const WPE_BACKEND_FDO: &[&str] = &["libWPEBackend-fdo-1.0.so.1"];
/// libwayland-server: reads `WPEBackend-fdo`'s shared-memory frames (it
/// speaks Wayland to the web process even under X11).
pub const WAYLAND_SERVER: &[&str] = &["libwayland-server.so.0"];
/// GObject: signals and references.
pub const GOBJECT: &[&str] = &["libgobject-2.0.so.0"];
/// GLib: the main context `WebKit`'s events arrive on.
pub const GLIB: &[&str] = &["libglib-2.0.so.0"];

/// What a Linux window (X11 or Wayland) can do with a `<webview>`: probe for
/// WPE `WebKit` when the first one appears.
#[must_use]
pub fn platform() -> WebViewPlatform {
    WebViewPlatform::Probe(probe)
}

/// Why a Linux window shows no page in a web view - see
/// [`unavailable_reason`]. Loads the libraries the first time it is asked,
/// and answers the same from then on.
#[must_use]
pub fn probe() -> AzString {
    static REASON: OnceLock<String> = OnceLock::new();
    AzString::from(
        REASON
            .get_or_init(|| unavailable_reason(&wpe().map(Wpe::version)))
            .as_str(),
    )
}

/// What a web view shows (and `WebViewLoadFailed` says) for what loading
/// WPE `WebKit` found: one line, the libraries or function that were
/// missing and what to install - not the loader's own error text - or the
/// version that is there and that the embedding comes later.
#[must_use]
pub fn unavailable_reason(found: &Result<WpeVersion, DlError>) -> String {
    match found {
        Ok(version) => format!(
            "WPE WebKit {version} is installed, but showing it inside an azul window comes \
             in a later release."
        ),
        Err(DlError::LibraryNotFound { tried, .. }) => format!(
            "This web view needs WPE WebKit, which did not load (tried {}). Install it: \
             libwpewebkit-2.0-1 and libwpebackend-fdo-1.0-1 on Debian and Ubuntu, wpewebkit \
             and wpebackend-fdo on Fedora and Arch.",
            tried.join(", ")
        ),
        Err(DlError::SymbolNotFound {
            symbol, library, ..
        }) => format!(
            "This web view needs WPE WebKit, but {library} has no {symbol}: it is older than \
             azul supports (WPE WebKit 2.22 or newer)."
        ),
        Err(other) => format!(
            "This web view needs WPE WebKit, which did not load: {}",
            other.to_string().replace('\n', " ")
        ),
    }
}

/// A WPE `WebKit` version (`webkit_get_*_version`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WpeVersion {
    pub major: u32,
    pub minor: u32,
    pub micro: u32,
}

impl fmt::Display for WpeVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.micro)
    }
}

thread_local! {
    /// The libraries, loaded on the thread that runs the windows. Never
    /// unloaded: `WebKit` registers GObject types and starts threads that
    /// outlive any handle, and `dlclose` under them crashes at exit.
    static WPE: OnceCell<Result<&'static Wpe, DlError>> = const { OnceCell::new() };
}

/// WPE `WebKit` and what it needs, loaded at the first call (on this
/// thread) and kept; the error is kept too, so a missing library is looked
/// for once.
pub fn wpe() -> Result<&'static Wpe, DlError> {
    WPE.with(|cell| {
        cell.get_or_init(|| Wpe::load().map(|wpe| &*Box::leak(Box::new(wpe))))
            .clone()
    })
}

// Opaque C types: only ever behind pointers.
pub type GMainContext = c_void;
pub type GType = usize;
pub type GBoolean = c_int;
pub type GCallback = Option<unsafe extern "C" fn()>;
pub type GClosureNotify = Option<unsafe extern "C" fn(data: *mut c_void, closure: *mut c_void)>;
pub type GDestroyNotify = Option<unsafe extern "C" fn(data: *mut c_void)>;
pub type WpeViewBackend = c_void;
pub type WpeViewBackendExportableFdo = c_void;
/// `struct wpe_view_backend_exportable_fdo_client`: the frame callbacks.
pub type WpeViewBackendExportableFdoClient = c_void;
pub type WpeFdoShmExportedBuffer = c_void;
pub type WlShmBuffer = c_void;
pub type WpeInputPointerEvent = c_void;
pub type WpeInputAxisEvent = c_void;
pub type WpeInputKeyboardEvent = c_void;
pub type WebKitWebViewBackend = c_void;
pub type WebKitWebView = c_void;
pub type WebKitSettings = c_void;
pub type WebKitPolicyDecision = c_void;
pub type WebKitNavigationAction = c_void;
pub type WebKitUriRequest = c_void;
/// A `WebKitNetworkSession` (2.0 API) or `WebKitWebContext` (1.x).
pub type WebKitStore = c_void;

/// Everything the embedding calls, by library. Fields are the C functions,
/// named as in C.
pub struct Wpe {
    pub webkit: WebKit,
    pub wpe: LibWpe,
    pub fdo: WpeFdo,
    pub wayland: WaylandShm,
    pub gobject: GObject,
    pub glib: GLib,
}

impl Wpe {
    /// Load every library and resolve every function; the first one
    /// missing is the error. WPE `WebKit` first: it is the one a machine
    /// usually lacks, and its absence is the answer worth giving.
    pub fn load() -> Result<Self, DlError> {
        Ok(Self {
            webkit: WebKit::load()?,
            wpe: LibWpe::load()?,
            fdo: WpeFdo::load()?,
            wayland: WaylandShm::load()?,
            gobject: GObject::load()?,
            glib: GLib::load()?,
        })
    }

    /// The `WebKit` version that loaded.
    #[must_use]
    pub fn version(&self) -> WpeVersion {
        // SAFETY: argument-less getters of constants.
        unsafe {
            WpeVersion {
                major: (self.webkit.webkit_get_major_version)(),
                minor: (self.webkit.webkit_get_minor_version)(),
                micro: (self.webkit.webkit_get_micro_version)(),
            }
        }
    }
}

/// How a view gets its store (cookies, local storage, cache).
#[derive(Clone, Copy)]
pub enum WebKitStoreApi {
    /// 2.0: `WebKitNetworkSession`, given to the view as its
    /// `network-session` property (`g_object_new`).
    NetworkSession {
        new_ephemeral: unsafe extern "C" fn() -> *mut WebKitStore,
        /// `(data_directory, cache_directory)`.
        new: unsafe extern "C" fn(*const c_char, *const c_char) -> *mut WebKitStore,
        web_view_get_type: unsafe extern "C" fn() -> GType,
    },
    /// 1.x: `WebKitWebContext`, given to `webkit_web_view_new_with_context`.
    WebContext {
        new_ephemeral: unsafe extern "C" fn() -> *mut WebKitStore,
        new: unsafe extern "C" fn() -> *mut WebKitStore,
        web_view_new_with_context: unsafe extern "C" fn(
            *mut WebKitWebViewBackend,
            *mut WebKitStore,
        ) -> *mut WebKitWebView,
    },
}

/// libWPEWebKit.
pub struct WebKit {
    _lib: Library,
    pub webkit_get_major_version: unsafe extern "C" fn() -> c_uint,
    pub webkit_get_minor_version: unsafe extern "C" fn() -> c_uint,
    pub webkit_get_micro_version: unsafe extern "C" fn() -> c_uint,
    pub webkit_web_view_backend_new: unsafe extern "C" fn(
        *mut WpeViewBackend,
        GDestroyNotify,
        *mut c_void,
    ) -> *mut WebKitWebViewBackend,
    pub webkit_web_view_load_uri: unsafe extern "C" fn(*mut WebKitWebView, *const c_char),
    pub webkit_web_view_reload: unsafe extern "C" fn(*mut WebKitWebView),
    pub webkit_web_view_go_back: unsafe extern "C" fn(*mut WebKitWebView),
    pub webkit_web_view_stop_loading: unsafe extern "C" fn(*mut WebKitWebView),
    pub webkit_web_view_get_uri: unsafe extern "C" fn(*mut WebKitWebView) -> *const c_char,
    pub webkit_web_view_get_title: unsafe extern "C" fn(*mut WebKitWebView) -> *const c_char,
    pub webkit_web_view_get_settings:
        unsafe extern "C" fn(*mut WebKitWebView) -> *mut WebKitSettings,
    pub webkit_settings_set_allow_file_access_from_file_urls:
        unsafe extern "C" fn(*mut WebKitSettings, GBoolean),
    pub webkit_settings_set_allow_universal_access_from_file_urls:
        unsafe extern "C" fn(*mut WebKitSettings, GBoolean),
    pub webkit_policy_decision_use: unsafe extern "C" fn(*mut WebKitPolicyDecision),
    pub webkit_policy_decision_ignore: unsafe extern "C" fn(*mut WebKitPolicyDecision),
    pub webkit_navigation_policy_decision_get_navigation_action:
        unsafe extern "C" fn(*mut WebKitPolicyDecision) -> *mut WebKitNavigationAction,
    pub webkit_navigation_action_get_request:
        unsafe extern "C" fn(*mut WebKitNavigationAction) -> *mut WebKitUriRequest,
    pub webkit_navigation_action_is_redirect:
        unsafe extern "C" fn(*mut WebKitNavigationAction) -> GBoolean,
    pub webkit_navigation_action_get_navigation_type:
        unsafe extern "C" fn(*mut WebKitNavigationAction) -> c_int,
    pub webkit_uri_request_get_uri: unsafe extern "C" fn(*mut WebKitUriRequest) -> *const c_char,
    /// The view without a store of its own choosing (1.x: the default web
    /// context).
    pub webkit_web_view_new: unsafe extern "C" fn(*mut WebKitWebViewBackend) -> *mut WebKitWebView,
    pub store: WebKitStoreApi,
}

impl WebKit {
    fn load() -> Result<Self, DlError> {
        let lib = load_first_available::<Library>(WPE_WEBKIT)?;
        let store = Self::store_api(&lib)?;
        Ok(Self {
            webkit_get_major_version: load_symbol!(lib, _, "webkit_get_major_version"),
            webkit_get_minor_version: load_symbol!(lib, _, "webkit_get_minor_version"),
            webkit_get_micro_version: load_symbol!(lib, _, "webkit_get_micro_version"),
            webkit_web_view_backend_new: load_symbol!(lib, _, "webkit_web_view_backend_new"),
            webkit_web_view_load_uri: load_symbol!(lib, _, "webkit_web_view_load_uri"),
            webkit_web_view_reload: load_symbol!(lib, _, "webkit_web_view_reload"),
            webkit_web_view_go_back: load_symbol!(lib, _, "webkit_web_view_go_back"),
            webkit_web_view_stop_loading: load_symbol!(lib, _, "webkit_web_view_stop_loading"),
            webkit_web_view_get_uri: load_symbol!(lib, _, "webkit_web_view_get_uri"),
            webkit_web_view_get_title: load_symbol!(lib, _, "webkit_web_view_get_title"),
            webkit_web_view_get_settings: load_symbol!(lib, _, "webkit_web_view_get_settings"),
            webkit_settings_set_allow_file_access_from_file_urls: load_symbol!(
                lib,
                _,
                "webkit_settings_set_allow_file_access_from_file_urls"
            ),
            webkit_settings_set_allow_universal_access_from_file_urls: load_symbol!(
                lib,
                _,
                "webkit_settings_set_allow_universal_access_from_file_urls"
            ),
            webkit_policy_decision_use: load_symbol!(lib, _, "webkit_policy_decision_use"),
            webkit_policy_decision_ignore: load_symbol!(lib, _, "webkit_policy_decision_ignore"),
            webkit_navigation_policy_decision_get_navigation_action: load_symbol!(
                lib,
                _,
                "webkit_navigation_policy_decision_get_navigation_action"
            ),
            webkit_navigation_action_get_request: load_symbol!(
                lib,
                _,
                "webkit_navigation_action_get_request"
            ),
            webkit_navigation_action_is_redirect: load_symbol!(
                lib,
                _,
                "webkit_navigation_action_is_redirect"
            ),
            webkit_navigation_action_get_navigation_type: load_symbol!(
                lib,
                _,
                "webkit_navigation_action_get_navigation_type"
            ),
            webkit_uri_request_get_uri: load_symbol!(lib, _, "webkit_uri_request_get_uri"),
            webkit_web_view_new: load_symbol!(lib, _, "webkit_web_view_new"),
            store,
            _lib: lib,
        })
    }

    /// The 2.0 network session API, else the 1.x web context one.
    fn store_api(lib: &Library) -> Result<WebKitStoreApi, DlError> {
        // SAFETY: each type is the C signature of the function named.
        let session = unsafe {
            (
                lib.get_symbol("webkit_network_session_new_ephemeral"),
                lib.get_symbol("webkit_network_session_new"),
                lib.get_symbol("webkit_web_view_get_type"),
            )
        };
        if let (Ok(new_ephemeral), Ok(new), Ok(web_view_get_type)) = session {
            return Ok(WebKitStoreApi::NetworkSession {
                new_ephemeral,
                new,
                web_view_get_type,
            });
        }
        Ok(WebKitStoreApi::WebContext {
            new_ephemeral: load_symbol!(lib, _, "webkit_web_context_new_ephemeral"),
            new: load_symbol!(lib, _, "webkit_web_context_new"),
            web_view_new_with_context: load_symbol!(lib, _, "webkit_web_view_new_with_context"),
        })
    }
}

/// libwpe.
pub struct LibWpe {
    _lib: Library,
    /// Names the backend implementation (`WPEBackend-fdo`) before any view.
    pub wpe_loader_init: unsafe extern "C" fn(*const c_char) -> bool,
    pub wpe_view_backend_dispatch_set_size: unsafe extern "C" fn(*mut WpeViewBackend, u32, u32),
    pub wpe_view_backend_dispatch_set_device_scale_factor:
        unsafe extern "C" fn(*mut WpeViewBackend, f32),
    pub wpe_view_backend_dispatch_pointer_event:
        unsafe extern "C" fn(*mut WpeViewBackend, *mut WpeInputPointerEvent),
    pub wpe_view_backend_dispatch_axis_event:
        unsafe extern "C" fn(*mut WpeViewBackend, *mut WpeInputAxisEvent),
    pub wpe_view_backend_dispatch_keyboard_event:
        unsafe extern "C" fn(*mut WpeViewBackend, *mut WpeInputKeyboardEvent),
    /// Visible / focused / in-window flags (`wpe_view_activity_state`).
    pub wpe_view_backend_add_activity_state: unsafe extern "C" fn(*mut WpeViewBackend, u32),
    pub wpe_view_backend_remove_activity_state: unsafe extern "C" fn(*mut WpeViewBackend, u32),
}

impl LibWpe {
    fn load() -> Result<Self, DlError> {
        let lib = load_first_available::<Library>(LIBWPE)?;
        Ok(Self {
            wpe_loader_init: load_symbol!(lib, _, "wpe_loader_init"),
            wpe_view_backend_dispatch_set_size: load_symbol!(
                lib,
                _,
                "wpe_view_backend_dispatch_set_size"
            ),
            wpe_view_backend_dispatch_set_device_scale_factor: load_symbol!(
                lib,
                _,
                "wpe_view_backend_dispatch_set_device_scale_factor"
            ),
            wpe_view_backend_dispatch_pointer_event: load_symbol!(
                lib,
                _,
                "wpe_view_backend_dispatch_pointer_event"
            ),
            wpe_view_backend_dispatch_axis_event: load_symbol!(
                lib,
                _,
                "wpe_view_backend_dispatch_axis_event"
            ),
            wpe_view_backend_dispatch_keyboard_event: load_symbol!(
                lib,
                _,
                "wpe_view_backend_dispatch_keyboard_event"
            ),
            wpe_view_backend_add_activity_state: load_symbol!(
                lib,
                _,
                "wpe_view_backend_add_activity_state"
            ),
            wpe_view_backend_remove_activity_state: load_symbol!(
                lib,
                _,
                "wpe_view_backend_remove_activity_state"
            ),
            _lib: lib,
        })
    }
}

/// `WPEBackend-fdo`: the exportable view backend (shared-memory frames).
pub struct WpeFdo {
    _lib: Library,
    pub wpe_fdo_initialize_shm: unsafe extern "C" fn() -> bool,
    /// `(client callbacks, their data, width, height)`.
    pub wpe_view_backend_exportable_fdo_create: unsafe extern "C" fn(
        *const WpeViewBackendExportableFdoClient,
        *mut c_void,
        u32,
        u32,
    ) -> *mut WpeViewBackendExportableFdo,
    pub wpe_view_backend_exportable_fdo_get_view_backend:
        unsafe extern "C" fn(*mut WpeViewBackendExportableFdo) -> *mut WpeViewBackend,
    pub wpe_view_backend_exportable_fdo_dispatch_frame_complete:
        unsafe extern "C" fn(*mut WpeViewBackendExportableFdo),
    pub wpe_view_backend_exportable_fdo_dispatch_release_shm_exported_buffer:
        unsafe extern "C" fn(*mut WpeViewBackendExportableFdo, *mut WpeFdoShmExportedBuffer),
    pub wpe_view_backend_exportable_fdo_destroy:
        unsafe extern "C" fn(*mut WpeViewBackendExportableFdo),
    pub wpe_fdo_shm_exported_buffer_get_shm_buffer:
        unsafe extern "C" fn(*mut WpeFdoShmExportedBuffer) -> *mut WlShmBuffer,
}

impl WpeFdo {
    fn load() -> Result<Self, DlError> {
        let lib = load_first_available::<Library>(WPE_BACKEND_FDO)?;
        Ok(Self {
            wpe_fdo_initialize_shm: load_symbol!(lib, _, "wpe_fdo_initialize_shm"),
            wpe_view_backend_exportable_fdo_create: load_symbol!(
                lib,
                _,
                "wpe_view_backend_exportable_fdo_create"
            ),
            wpe_view_backend_exportable_fdo_get_view_backend: load_symbol!(
                lib,
                _,
                "wpe_view_backend_exportable_fdo_get_view_backend"
            ),
            wpe_view_backend_exportable_fdo_dispatch_frame_complete: load_symbol!(
                lib,
                _,
                "wpe_view_backend_exportable_fdo_dispatch_frame_complete"
            ),
            wpe_view_backend_exportable_fdo_dispatch_release_shm_exported_buffer: load_symbol!(
                lib,
                _,
                "wpe_view_backend_exportable_fdo_dispatch_release_shm_exported_buffer"
            ),
            wpe_view_backend_exportable_fdo_destroy: load_symbol!(
                lib,
                _,
                "wpe_view_backend_exportable_fdo_destroy"
            ),
            wpe_fdo_shm_exported_buffer_get_shm_buffer: load_symbol!(
                lib,
                _,
                "wpe_fdo_shm_exported_buffer_get_shm_buffer"
            ),
            _lib: lib,
        })
    }
}

/// libwayland-server's shared-memory buffer accessors.
pub struct WaylandShm {
    _lib: Library,
    pub wl_shm_buffer_begin_access: unsafe extern "C" fn(*mut WlShmBuffer),
    pub wl_shm_buffer_end_access: unsafe extern "C" fn(*mut WlShmBuffer),
    pub wl_shm_buffer_get_data: unsafe extern "C" fn(*mut WlShmBuffer) -> *mut c_void,
    pub wl_shm_buffer_get_stride: unsafe extern "C" fn(*mut WlShmBuffer) -> i32,
    pub wl_shm_buffer_get_width: unsafe extern "C" fn(*mut WlShmBuffer) -> i32,
    pub wl_shm_buffer_get_height: unsafe extern "C" fn(*mut WlShmBuffer) -> i32,
    pub wl_shm_buffer_get_format: unsafe extern "C" fn(*mut WlShmBuffer) -> u32,
}

impl WaylandShm {
    fn load() -> Result<Self, DlError> {
        let lib = load_first_available::<Library>(WAYLAND_SERVER)?;
        Ok(Self {
            wl_shm_buffer_begin_access: load_symbol!(lib, _, "wl_shm_buffer_begin_access"),
            wl_shm_buffer_end_access: load_symbol!(lib, _, "wl_shm_buffer_end_access"),
            wl_shm_buffer_get_data: load_symbol!(lib, _, "wl_shm_buffer_get_data"),
            wl_shm_buffer_get_stride: load_symbol!(lib, _, "wl_shm_buffer_get_stride"),
            wl_shm_buffer_get_width: load_symbol!(lib, _, "wl_shm_buffer_get_width"),
            wl_shm_buffer_get_height: load_symbol!(lib, _, "wl_shm_buffer_get_height"),
            wl_shm_buffer_get_format: load_symbol!(lib, _, "wl_shm_buffer_get_format"),
            _lib: lib,
        })
    }
}

/// GObject.
pub struct GObject {
    _lib: Library,
    /// Variadic: `(type, first property name, value, ..., NULL)`.
    pub g_object_new: unsafe extern "C" fn(GType, *const c_char, ...) -> *mut c_void,
    pub g_object_ref: unsafe extern "C" fn(*mut c_void) -> *mut c_void,
    pub g_object_unref: unsafe extern "C" fn(*mut c_void),
    /// `(instance, "signal", handler, data, data destroy, flags)` -> handler id.
    pub g_signal_connect_data: unsafe extern "C" fn(
        *mut c_void,
        *const c_char,
        GCallback,
        *mut c_void,
        GClosureNotify,
        c_uint,
    ) -> c_ulong,
    pub g_signal_handler_disconnect: unsafe extern "C" fn(*mut c_void, c_ulong),
}

impl GObject {
    fn load() -> Result<Self, DlError> {
        let lib = load_first_available::<Library>(GOBJECT)?;
        Ok(Self {
            g_object_new: load_symbol!(lib, _, "g_object_new"),
            g_object_ref: load_symbol!(lib, _, "g_object_ref"),
            g_object_unref: load_symbol!(lib, _, "g_object_unref"),
            g_signal_connect_data: load_symbol!(lib, _, "g_signal_connect_data"),
            g_signal_handler_disconnect: load_symbol!(lib, _, "g_signal_handler_disconnect"),
            _lib: lib,
        })
    }
}

/// GLib: the default main context, iterated from the shell's loop.
pub struct GLib {
    _lib: Library,
    pub g_main_context_default: unsafe extern "C" fn() -> *mut GMainContext,
    /// `(context, may_block)` -> whether anything was dispatched.
    pub g_main_context_iteration: unsafe extern "C" fn(*mut GMainContext, GBoolean) -> GBoolean,
    pub g_free: unsafe extern "C" fn(*mut c_void),
}

impl GLib {
    fn load() -> Result<Self, DlError> {
        let lib = load_first_available::<Library>(GLIB)?;
        Ok(Self {
            g_main_context_default: load_symbol!(lib, _, "g_main_context_default"),
            g_main_context_iteration: load_symbol!(lib, _, "g_main_context_iteration"),
            g_free: load_symbol!(lib, _, "g_free"),
            _lib: lib,
        })
    }
}
