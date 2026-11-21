//! macOS and iOS: `ASWebAuthenticationSession` (`AuthenticationServices.framework`; macOS
//! 10.15 and iOS 12, the presentation context provider macOS 10.15 / iOS 13) for a
//! custom-scheme redirect.
//!
//! The framework is dlopen'd at the first sign-in and every class is looked up at runtime
//! (`AnyClass::get`), the way the `<webview>` loads WebKit (`shell2::macos::webview`): an app
//! that never signs in never loads it, and nothing links it. The presentation anchor
//! ([`AnchorProvider`], `define_class!`; the `ASWebAuthenticationPresentationContextProviding`
//! protocol attached once the framework is loaded) is the calling window - the app's key
//! window when the callback has none.
//!
//! The session is created, configured and started on the main thread (the callback that asks
//! for it) and kept in a main-thread registry until it ends: the system does not retain it. Its
//! completion handler parks the answer in a slot the request pump polls
//! (`super::poll_slot`) and wakes the run loop; a session whose time is up is cancelled from
//! that poll. A cancelled session (`ASWebAuthenticationSessionErrorCodeCanceledLogin`) is
//! `Cancelled`, every other error `Failed`; a callback URL goes through
//! `auth_session::finish` like every platform's.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    ffi::{c_char, CStr, CString},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock, PoisonError,
    },
    time::{Duration, Instant},
};

use azul_core::window::RawWindowHandle;
use azul_layout::{
    auth_session::{self as auth, AuthSessionResult, AuthSessionStatus},
    request::PollFn,
};
use block2::RcBlock;
use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, AnyProtocol, Bool},
    sel, AllocAnyThread, ClassType, DefinedClass,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSString};

/// Where `AuthenticationServices` lives on macOS and iOS.
const FRAMEWORK_PATH: &str =
    "/System/Library/Frameworks/AuthenticationServices.framework/AuthenticationServices";
/// `ASWebAuthenticationSessionErrorDomain`.
const ERROR_DOMAIN: &str = "com.apple.AuthenticationServices.WebAuthenticationSession";
/// `ASWebAuthenticationSessionErrorCodeCanceledLogin`.
const CANCELED_LOGIN: isize = 1;

/// `AuthenticationServices.framework`, loaded once - at the first sign-in of the process.
fn framework() -> Option<&'static libloading::Library> {
    static LIB: OnceLock<Option<libloading::Library>> = OnceLock::new();
    LIB.get_or_init(
        || match unsafe { libloading::Library::new(FRAMEWORK_PATH) } {
            Ok(lib) => {
                crate::plog_info!("[auth_session] AuthenticationServices.framework loaded");
                Some(lib)
            }
            Err(e) => {
                crate::plog_warn!(
                    "[auth_session] AuthenticationServices.framework could not be loaded: {e}"
                );
                None
            }
        },
    )
    .as_ref()
}

fn class(name: &str) -> Option<&'static AnyClass> {
    let name = CString::new(name).ok()?;
    AnyClass::get(&name)
}

/// An `NSString *` as a Rust string; `None` for nil.
unsafe fn ns_string(ns: *mut AnyObject) -> Option<String> {
    if ns.is_null() {
        return None;
    }
    let utf8: *const c_char = unsafe { msg_send![ns, UTF8String] };
    if utf8.is_null() {
        return None;
    }
    Some(
        unsafe { CStr::from_ptr(utf8) }
            .to_string_lossy()
            .into_owned(),
    )
}

/// Ivars of [`AnchorProvider`]: the window the sheet is presented over.
struct AnchorIvars {
    window: Option<Retained<AnyObject>>,
}

define_class!(
    // No `thread_kind`: created like the other runtime delegates (`AllocAnyThread`); the
    // session asks it on the main thread.
    #[unsafe(super(NSObject))]
    #[name = "AzulAuthSessionAnchor"]
    #[ivars = AnchorIvars]
    struct AnchorProvider;

    unsafe impl NSObjectProtocol for AnchorProvider {}

    impl AnchorProvider {
        /// `-[ASWebAuthenticationPresentationContextProviding
        /// presentationAnchorForWebAuthenticationSession:]`: the window (`NSWindow` on macOS,
        /// `UIWindow` on iOS).
        #[unsafe(method(presentationAnchorForWebAuthenticationSession:))]
        fn presentation_anchor(&self, _session: *mut AnyObject) -> *mut AnyObject {
            self.ivars()
                .window
                .as_ref()
                .map_or(core::ptr::null_mut(), |window| {
                    Retained::as_ptr(window).cast_mut()
                })
        }
    }
);

impl AnchorProvider {
    fn create(window: Option<Retained<AnyObject>>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(AnchorIvars { window });
        unsafe { msg_send![super(this), init] }
    }

    /// Attach the (runtime-only) presentation-context protocol to the class, for the
    /// session's `conformsToProtocol:` check. Once, after the framework is loaded.
    fn attach_protocol() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| unsafe {
            if let Some(protocol) =
                AnyProtocol::get(c"ASWebAuthenticationPresentationContextProviding")
            {
                let cls = Self::class();
                objc2::ffi::class_addProtocol(cls as *const AnyClass as *mut AnyClass, protocol);
            }
        });
    }
}

/// The window a sheet is presented over: the calling window, else the app's key window.
fn anchor_window(window: RawWindowHandle) -> Option<Retained<AnyObject>> {
    let ptr: *mut AnyObject = match window {
        RawWindowHandle::MacOS(handle) => handle.ns_window.cast(),
        RawWindowHandle::IOS(handle) => handle.ui_window.cast(),
        _ => core::ptr::null_mut(),
    };
    let ptr = if ptr.is_null() { key_window() } else { ptr };
    unsafe { Retained::retain(ptr) }
}

/// The app's key window (`NSApplication` / `UIApplication`), for a session started outside a
/// window's callback.
fn key_window() -> *mut AnyObject {
    let app_class = if cfg!(target_os = "ios") {
        class("UIApplication")
    } else {
        class("NSApplication")
    };
    let Some(app_class) = app_class else {
        return core::ptr::null_mut();
    };
    unsafe {
        let app: *mut AnyObject = msg_send![app_class, sharedApplication];
        if app.is_null() {
            return core::ptr::null_mut();
        }
        msg_send![app, keyWindow]
    }
}

/// A running session and what has to live as long as it.
struct Running {
    session: Retained<AnyObject>,
    _anchor: Retained<AnchorProvider>,
    _handler: RcBlock<dyn Fn(*mut AnyObject, *mut AnyObject)>,
}

std::thread_local! {
    /// The sessions of the main thread, by id, until they end.
    static RUNNING: RefCell<BTreeMap<u64, Running>> = const { RefCell::new(BTreeMap::new()) };
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Starts an `ASWebAuthenticationSession` at `url` (the authorize URL with its
/// `redirect_uri`) coming back to `scheme`; the poll answers once its completion handler ran or
/// `timeout` passed (the session cancelled then).
pub(super) fn start(
    window: RawWindowHandle,
    url: &str,
    scheme: &str,
    redirect_uri: &str,
    prefers_ephemeral: bool,
    timeout: Duration,
) -> Result<PollFn, String> {
    if framework().is_none() {
        return Err(String::from(
            "AuthenticationServices.framework could not be loaded \
             (/System/Library/Frameworks/AuthenticationServices.framework)",
        ));
    }
    let (Some(session_cls), Some(url_cls)) = (class("ASWebAuthenticationSession"), class("NSURL"))
    else {
        return Err(String::from(
            "this system has no ASWebAuthenticationSession (it needs macOS 10.15 or iOS 12)",
        ));
    };
    AnchorProvider::attach_protocol();
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let slot: Arc<Mutex<Option<AuthSessionResult>>> = Arc::default();
    let handler: RcBlock<dyn Fn(*mut AnyObject, *mut AnyObject)> = {
        let slot = Arc::clone(&slot);
        let redirect_uri = redirect_uri.to_string();
        let authorize_url = url.to_string();
        RcBlock::new(move |callback_url: *mut AnyObject, error: *mut AnyObject| {
            let result = unsafe { completion(callback_url, error, &redirect_uri, &authorize_url) };
            {
                let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
                if slot.is_none() {
                    *slot = Some(result);
                }
            }
            crate::desktop::loop_waker::wake();
        })
    };
    let anchor = AnchorProvider::create(anchor_window(window));
    let session = unsafe {
        let url_string = NSString::from_str(url);
        let ns_url: *mut AnyObject = msg_send![url_cls, URLWithString: &*url_string];
        if ns_url.is_null() {
            return Err(String::from(
                "the authorize URL is no URL the system can open",
            ));
        }
        let ns_scheme = NSString::from_str(scheme);
        let alloc: *mut AnyObject = msg_send![session_cls, alloc];
        let session: *mut AnyObject = msg_send![
            alloc,
            initWithURL: ns_url,
            callbackURLScheme: &*ns_scheme,
            completionHandler: &*handler
        ];
        let Some(session) = Retained::from_raw(session) else {
            return Err(String::from("the sign-in session could not be created"));
        };
        let can_anchor: Bool = msg_send![
            &*session,
            respondsToSelector: sel!(setPresentationContextProvider:)
        ];
        if can_anchor.as_bool() {
            let _: () = msg_send![&*session, setPresentationContextProvider: &*anchor];
        }
        let can_ephemeral: Bool = msg_send![
            &*session,
            respondsToSelector: sel!(setPrefersEphemeralWebBrowserSession:)
        ];
        if can_ephemeral.as_bool() {
            let _: () = msg_send![
                &*session,
                setPrefersEphemeralWebBrowserSession: Bool::new(prefers_ephemeral)
            ];
        }
        session
    };
    let started: Bool = unsafe { msg_send![&*session, start] };
    if !started.as_bool() {
        return Err(String::from(
            "the sign-in sheet could not be shown (is a window of the app in front?)",
        ));
    }
    RUNNING.with(|running| {
        running.borrow_mut().insert(
            id,
            Running {
                session,
                _anchor: anchor,
                _handler: handler,
            },
        );
    });
    Ok(super::poll_slot(
        slot,
        Instant::now() + timeout,
        redirect_uri.to_string(),
        move |timed_out| end(id, timed_out),
    ))
}

/// A session's end: out of the registry, cancelled first when its time is up.
fn end(id: u64, timed_out: bool) {
    let running = RUNNING.with(|running| running.borrow_mut().remove(&id));
    if let (Some(running), true) = (running, timed_out) {
        let _: () = unsafe { msg_send![&*running.session, cancel] };
    }
}

/// The completion handler's arguments as the session's answer.
unsafe fn completion(
    callback_url: *mut AnyObject,
    error: *mut AnyObject,
    redirect_uri: &str,
    authorize_url: &str,
) -> AuthSessionResult {
    if !callback_url.is_null() {
        let absolute: *mut AnyObject = unsafe { msg_send![callback_url, absoluteString] };
        let url = unsafe { ns_string(absolute) }.unwrap_or_default();
        return auth::finish(&url, redirect_uri, authorize_url);
    }
    if error.is_null() {
        return AuthSessionResult::ended(
            AuthSessionStatus::Failed,
            redirect_uri,
            "the sign-in ended without an answer",
        );
    }
    let domain: *mut AnyObject = unsafe { msg_send![error, domain] };
    let code: isize = unsafe { msg_send![error, code] };
    if unsafe { ns_string(domain) }.as_deref() == Some(ERROR_DOMAIN) && code == CANCELED_LOGIN {
        return AuthSessionResult::ended(
            AuthSessionStatus::Cancelled,
            redirect_uri,
            "the sign-in window was closed",
        );
    }
    let description: *mut AnyObject = unsafe { msg_send![error, localizedDescription] };
    let description = unsafe { ns_string(description) }.unwrap_or_else(|| format!("error {code}"));
    AuthSessionResult::ended(
        AuthSessionStatus::Failed,
        redirect_uri,
        &format!("the sign-in failed: {description}"),
    )
}
