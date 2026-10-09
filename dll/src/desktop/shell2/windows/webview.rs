//! `<webview>` on Windows: Microsoft Edge `WebView2`, through
//! `WebView2Loader.dll` loaded with `LoadLibraryW` when the first web view
//! appears - nothing links it, and an app without a web view never loads it.
//!
//! Two parts, from two places: `WebView2Loader.dll`, which the app ships
//! next to its executable (from the `Microsoft.Web.WebView2` package), and
//! the `WebView2` Runtime (the Edge engine), which Windows 11 has and
//! Windows 10 gets from Microsoft's Evergreen installer.
//!
//! # What is here (phase 1)
//!
//! The probe a window runs when its first web view appears ([`probe`], the
//! `WebViewPlatform` from [`platform`]): load the loader, resolve its two
//! entry points, ask it for the installed runtime's version. Its answer is
//! what the view shows and what the app's `WebViewLoadFailed` reads.
//!
//! # The embedding (phase 2)
//!
//! A `WebViewBackend`: one `CreateCoreWebView2EnvironmentWithOptions` per
//! process with a user-data folder under the app's local data directory;
//! per view a controller (`CreateCoreWebView2ControllerWithOptions`) on a
//! clip child `HWND` of the azul window (the macOS container's job:
//! `put_Bounds` is the full box, the child's rectangle the visible part),
//! `put_IsVisible` for hidden views. Ephemeral = an `InPrivate` profile
//! (`ICoreWebView2ControllerOptions::put_IsInPrivateModeEnabled`),
//! persistent = a profile named after the app. Settings: no host objects,
//! no web messages (no bridge). `NavigationStarting` is decided inside the
//! handler - its args are valid only there, on the UI thread, so the shell
//! dispatches the report from it and `put_Cancel`s what the app (or the
//! `file://` rule) refused; `NavigationCompleted` (`IsSuccess`,
//! `WebErrorStatus`) and `DocumentTitleChanged` are the other reports.
//! Creation is asynchronous (completion handlers), so a view placed before
//! its controller exists is placed when it arrives.

use core::ffi::c_void;
use std::sync::OnceLock;

use azul_css::AzString;
use azul_layout::managers::webview::WebViewPlatform;

use super::dlopen::{DynamicLibrary, HRESULT};
use crate::desktop::shell2::common::{DlError, DynamicLibrary as _};

/// The loader the app ships next to its executable.
pub const WEBVIEW2_LOADER: &str = "WebView2Loader.dll";

/// `GetAvailableCoreWebView2BrowserVersionString(browserExecutableFolder,
/// versionInfo)`: the installed runtime's version, freed with
/// `CoTaskMemFree`.
type GetAvailableBrowserVersion = unsafe extern "system" fn(*const u16, *mut *mut u16) -> HRESULT;
/// `CreateCoreWebView2EnvironmentWithOptions(browserExecutableFolder,
/// userDataFolder, environmentOptions, environmentCreatedHandler)`.
type CreateEnvironmentWithOptions =
    unsafe extern "system" fn(*const u16, *const u16, *mut c_void, *mut c_void) -> HRESULT;
/// ole32's `CoTaskMemFree`.
type CoTaskMemFree = unsafe extern "system" fn(*mut c_void);

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

/// What a Windows window can do with a `<webview>`: probe for `WebView2`
/// when the first one appears.
#[must_use]
pub fn platform() -> WebViewPlatform {
    WebViewPlatform::Probe(probe)
}

/// Why a Windows window shows no page in a web view - see
/// [`unavailable_reason`]. Looks the first time it is asked and answers the
/// same from then on.
#[must_use]
pub fn probe() -> AzString {
    static REASON: OnceLock<String> = OnceLock::new();
    AzString::from(REASON.get_or_init(|| unavailable_reason(&find())).as_str())
}

/// What a web view shows (and `WebViewLoadFailed` says) for what [`find`]
/// found: one line, what is missing and where it comes from - not the
/// loader's own error text - or the version that is there and that the
/// embedding comes later.
#[must_use]
pub fn unavailable_reason(found: &WebView2) -> String {
    match found {
        WebView2::Runtime(version) => format!(
            "Microsoft Edge WebView2 {version} is installed, but showing it inside an azul \
             window comes in a later release."
        ),
        WebView2::NoRuntime => String::from(
            "This web view needs the Microsoft Edge WebView2 Runtime, which is not installed: \
             Windows 11 has it, Windows 10 gets it from Microsoft's Evergreen installer.",
        ),
        WebView2::LoaderMissing(DlError::SymbolNotFound { symbol, .. }) => format!(
            "This web view needs Microsoft Edge WebView2, but {WEBVIEW2_LOADER} has no \
             {symbol}: it is older than azul supports."
        ),
        WebView2::LoaderMissing(_) => format!(
            "This web view needs Microsoft Edge WebView2, but {WEBVIEW2_LOADER} was not found \
             next to the program: the app ships it (from the Microsoft.Web.WebView2 package)."
        ),
    }
}

/// Load the loader, resolve its entry points, ask for the runtime.
#[must_use]
pub fn find() -> WebView2 {
    let loader = match DynamicLibrary::load(WEBVIEW2_LOADER) {
        Ok(loader) => loader,
        Err(e) => return WebView2::LoaderMissing(e),
    };
    // SAFETY: each type is the documented signature of the export named.
    let symbols = unsafe {
        (
            loader.get_symbol::<GetAvailableBrowserVersion>(
                "GetAvailableCoreWebView2BrowserVersionString",
            ),
            loader.get_symbol::<CreateEnvironmentWithOptions>(
                "CreateCoreWebView2EnvironmentWithOptions",
            ),
        )
    };
    let get_version = match symbols {
        (Ok(get_version), Ok(_)) => get_version,
        (Err(e), _) | (_, Err(e)) => return WebView2::LoaderMissing(e),
    };
    let mut version: *mut u16 = core::ptr::null_mut();
    // SAFETY: a null folder asks for the installed (Evergreen) runtime; the
    // out pointer is valid for the call.
    let hr = unsafe { get_version(core::ptr::null(), &mut version) };
    if version.is_null() {
        return WebView2::NoRuntime;
    }
    // SAFETY: on return the loader handed over a NUL-terminated UTF-16
    // string that is ours until freed.
    let text = unsafe {
        let len = (0..).take_while(|&i| *version.add(i) != 0).count();
        String::from_utf16_lossy(core::slice::from_raw_parts(version, len))
    };
    free_co_task_mem(version.cast());
    if hr < 0 || text.is_empty() {
        WebView2::NoRuntime
    } else {
        WebView2::Runtime(text)
    }
}

/// Free a string a COM API allocated (leaked if ole32 will not load - a
/// few bytes, once).
fn free_co_task_mem(ptr: *mut c_void) {
    let Ok(ole32) = DynamicLibrary::load("ole32.dll") else {
        return;
    };
    // SAFETY: `CoTaskMemFree`'s signature; `ptr` came from `CoTaskMemAlloc`.
    if let Ok(free) = unsafe { ole32.get_symbol::<CoTaskMemFree>("CoTaskMemFree") } {
        unsafe { free(ptr) };
    }
}
