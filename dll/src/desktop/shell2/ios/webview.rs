//! The iOS `<webview>` backend: one `WKWebView` per view - the macOS
//! backend's shape (`macos::webview`) with `UIKit` instead of `AppKit`.
//!
//! `WebKit.framework` is dlopen'd at the FIRST view and every class is
//! looked up at runtime: an app without a `<webview>` never loads `WebKit`.
//! Each view is a clip container `UIView` (`clipsToBounds`) at the visible
//! part of the view's box holding the `WKWebView` at the box's full size;
//! both are subviews of the render view (`AzulView`), whose coordinates are
//! azul's own (top-left origin, points), so nothing is flipped.
//!
//! The navigation delegate turns `WebKit`'s callbacks into reports on a
//! mailbox shared with the backend and asks the render view for a display
//! pass (`setNeedsDisplay`); the pass pumps the web views first
//! (`IOSWindow::pump_webviews_if_any`), so the views' callbacks run on the
//! main thread outside any `WebKit` call. A navigation's decision handler is
//! copied and called once the app's callbacks have run, exactly once (a
//! view or a window going away cancels what is pending).
//!
//! Policy before the app is asked, as on macOS: `file:` refused, sub-frames
//! allowed, a new window loaded in this view. One `nonPersistentDataStore`
//! per process for ephemeral views, `defaultDataStore` for persistent ones;
//! no script message handler (no bridge).
//!
//! An OAuth provider that refuses embedded views (Google, Facebook, Sign in
//! with Apple) needs `ASWebAuthenticationSession`, a different API: this
//! node is for the providers that allow a web view.

use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    ffi::{c_char, c_void, CStr, CString},
    rc::Rc,
    sync::OnceLock,
};

use azul_core::webview::{
    WebViewConfig, WebViewEvent, WebViewLoadError, WebViewNavigation, WebViewStorage,
};
use azul_css::AzString;
use azul_layout::managers::webview::{WebViewId, WebViewPlacement, WebViewReport, WebViewTransform};
use block2::{Block, RcBlock};
use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, AnyProtocol, Bool},
    AllocAnyThread, ClassType, DefinedClass,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};

use crate::desktop::shell2::common::webview::WebViewBackend;

/// Where `WebKit` lives on iOS.
const WEBKIT_PATH: &str = "/System/Library/Frameworks/WebKit.framework/WebKit";
/// `WKNavigationActionPolicyCancel`.
const POLICY_CANCEL: isize = 0;
/// `WKNavigationActionPolicyAllow`.
const POLICY_ALLOW: isize = 1;
/// `WKNavigationTypeOther`: a script's navigation, a server redirect, a load
/// the app asked for.
const NAVIGATION_TYPE_OTHER: isize = -1;
/// `NSKeyValueObservingOptionNew`.
const KVO_OPTION_NEW: usize = 1;
/// The page title, observed by key-value observing.
const TITLE_KEY: &str = "title";

/// `WebKit.framework`, loaded once - at the first web view of the process.
fn webkit() -> Option<&'static libloading::Library> {
    static LIB: OnceLock<Option<libloading::Library>> = OnceLock::new();
    LIB.get_or_init(|| match unsafe { libloading::Library::new(WEBKIT_PATH) } {
        Ok(lib) => Some(lib),
        Err(e) => {
            crate::plog_warn!("[webview] WebKit.framework could not be loaded: {e}");
            None
        }
    })
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
    Some(unsafe { CStr::from_ptr(utf8) }.to_string_lossy().into_owned())
}

/// `-[NSURL absoluteString]` of `url`; empty for nil.
unsafe fn url_string(url: *mut AnyObject) -> String {
    if url.is_null() {
        return String::new();
    }
    let absolute: *mut AnyObject = unsafe { msg_send![url, absoluteString] };
    unsafe { ns_string(absolute) }.unwrap_or_default()
}

/// The page `web_view` is on; empty before any.
unsafe fn current_url(web_view: *mut AnyObject) -> String {
    if web_view.is_null() {
        return String::new();
    }
    let url: *mut AnyObject = unsafe { msg_send![web_view, URL] };
    unsafe { url_string(url) }
}

fn is_file_url(url: &str) -> bool {
    url.trim_start()
        .get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("file:"))
}

/// What the delegates of one window share with its backend.
struct Mailbox {
    reports: Vec<WebViewReport>,
    /// Copied decision handlers waiting for the app's answer, by handle.
    decisions: BTreeMap<u64, (WebViewId, RcBlock<dyn Fn(isize)>)>,
    last_request: u64,
    /// The render view: a report asks it for a display pass, which pumps
    /// the web views. Not retained (the window owns it, and the backend).
    wake_view: *mut AnyObject,
}

impl Mailbox {
    fn report(&mut self, id: WebViewId, request: u64, event: WebViewEvent) {
        self.reports.push(WebViewReport { id, request, event });
        if !self.wake_view.is_null() {
            let _: () = unsafe { msg_send![self.wake_view, setNeedsDisplay] };
        }
    }

    fn take_decisions_of(&mut self, id: Option<WebViewId>) -> Vec<RcBlock<dyn Fn(isize)>> {
        let requests: Vec<u64> = self
            .decisions
            .iter()
            .filter(|(_, (view, _))| id.is_none_or(|id| *view == id))
            .map(|(request, _)| *request)
            .collect();
        requests
            .into_iter()
            .filter_map(|request| self.decisions.remove(&request).map(|(_, handler)| handler))
            .collect()
    }
}

type SharedMailbox = Rc<RefCell<Mailbox>>;

enum Policy {
    Now(isize),
    AskTheApp { url: String, is_redirect: bool },
}

struct DelegateIvars {
    id: WebViewId,
    mailbox: SharedMailbox,
    provisional: Cell<bool>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "AzulWebViewNavigationDelegate"]
    #[ivars = DelegateIvars]
    struct NavigationDelegate;

    unsafe impl NSObjectProtocol for NavigationDelegate {}

    impl NavigationDelegate {
        #[unsafe(method(webView:decidePolicyForNavigationAction:decisionHandler:))]
        unsafe fn decide_policy(
            &self,
            web_view: *mut AnyObject,
            action: *mut AnyObject,
            handler: *mut Block<dyn Fn(isize)>,
        ) {
            let Some(handler) = (unsafe { handler.as_ref() }) else {
                return;
            };
            match unsafe { self.policy_for(web_view, action) } {
                Policy::Now(policy) => handler.call((policy,)),
                Policy::AskTheApp { url, is_redirect } => {
                    let ivars = self.ivars();
                    let mut mailbox = ivars.mailbox.borrow_mut();
                    mailbox.last_request += 1;
                    let request = mailbox.last_request;
                    mailbox.decisions.insert(request, (ivars.id, handler.copy()));
                    mailbox.report(
                        ivars.id,
                        request,
                        WebViewEvent::NavigationRequested(WebViewNavigation {
                            url: AzString::from(url),
                            is_redirect,
                        }),
                    );
                }
            }
        }

        #[unsafe(method(webView:didStartProvisionalNavigation:))]
        fn did_start_provisional_navigation(
            &self,
            _web_view: *mut AnyObject,
            _navigation: *mut AnyObject,
        ) {
            self.ivars().provisional.set(true);
        }

        #[unsafe(method(webView:didCommitNavigation:))]
        fn did_commit_navigation(&self, _web_view: *mut AnyObject, _navigation: *mut AnyObject) {
            self.ivars().provisional.set(false);
        }

        #[unsafe(method(webView:didFinishNavigation:))]
        unsafe fn did_finish_navigation(
            &self,
            web_view: *mut AnyObject,
            _navigation: *mut AnyObject,
        ) {
            self.ivars().provisional.set(false);
            let url = unsafe { current_url(web_view) };
            self.report(WebViewEvent::LoadFinished(AzString::from(url)));
        }

        #[unsafe(method(webView:didFailProvisionalNavigation:withError:))]
        unsafe fn did_fail_provisional_navigation(
            &self,
            web_view: *mut AnyObject,
            _navigation: *mut AnyObject,
            error: *mut AnyObject,
        ) {
            unsafe { self.failed(web_view, error) };
        }

        #[unsafe(method(webView:didFailNavigation:withError:))]
        unsafe fn did_fail_navigation(
            &self,
            web_view: *mut AnyObject,
            _navigation: *mut AnyObject,
            error: *mut AnyObject,
        ) {
            unsafe { self.failed(web_view, error) };
        }

        #[unsafe(method(webViewWebContentProcessDidTerminate:))]
        unsafe fn content_process_did_terminate(&self, web_view: *mut AnyObject) {
            self.ivars().provisional.set(false);
            let url = unsafe { current_url(web_view) };
            self.report(WebViewEvent::LoadFailed(WebViewLoadError {
                url: AzString::from(url),
                reason: AzString::from("the page's web content process ended"),
            }));
        }

        #[unsafe(method(observeValueForKeyPath:ofObject:change:context:))]
        unsafe fn observe_value(
            &self,
            key_path: *mut AnyObject,
            object: *mut AnyObject,
            _change: *mut AnyObject,
            _context: *mut c_void,
        ) {
            if unsafe { ns_string(key_path) }.as_deref() != Some(TITLE_KEY) || object.is_null() {
                return;
            }
            let title: *mut AnyObject = unsafe { msg_send![object, title] };
            let title = unsafe { ns_string(title) }.unwrap_or_default();
            self.report(WebViewEvent::TitleChanged(AzString::from(title)));
        }
    }
);

impl NavigationDelegate {
    fn create(id: WebViewId, mailbox: SharedMailbox) -> Retained<Self> {
        let this = Self::alloc().set_ivars(DelegateIvars {
            id,
            mailbox,
            provisional: Cell::new(false),
        });
        unsafe { msg_send![super(this), init] }
    }

    /// Attach the runtime-only `WKNavigationDelegate` protocol, once, after
    /// `WebKit` is loaded.
    fn attach_protocol() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| unsafe {
            if let Some(protocol) = AnyProtocol::get(c"WKNavigationDelegate") {
                let cls = Self::class();
                objc2::ffi::class_addProtocol(cls as *const AnyClass as *mut AnyClass, protocol);
            }
        });
    }

    fn report(&self, event: WebViewEvent) {
        let ivars = self.ivars();
        ivars.mailbox.borrow_mut().report(ivars.id, 0, event);
    }

    unsafe fn policy_for(&self, web_view: *mut AnyObject, action: *mut AnyObject) -> Policy {
        if action.is_null() {
            return Policy::Now(POLICY_CANCEL);
        }
        let request: *mut AnyObject = unsafe { msg_send![action, request] };
        let url = if request.is_null() {
            String::new()
        } else {
            let nsurl: *mut AnyObject = unsafe { msg_send![request, URL] };
            unsafe { url_string(nsurl) }
        };
        if is_file_url(&url) {
            return Policy::Now(POLICY_CANCEL);
        }
        let frame: *mut AnyObject = unsafe { msg_send![action, targetFrame] };
        if frame.is_null() {
            // A new window: this view loads it instead.
            if !web_view.is_null() && !request.is_null() {
                let _: *mut AnyObject = unsafe { msg_send![web_view, loadRequest: request] };
            }
            return Policy::Now(POLICY_CANCEL);
        }
        let main_frame: Bool = unsafe { msg_send![frame, isMainFrame] };
        if !main_frame.as_bool() {
            return Policy::Now(POLICY_ALLOW);
        }
        let kind: isize = unsafe { msg_send![action, navigationType] };
        Policy::AskTheApp {
            url,
            is_redirect: self.ivars().provisional.get() && kind == NAVIGATION_TYPE_OTHER,
        }
    }

    unsafe fn failed(&self, web_view: *mut AnyObject, error: *mut AnyObject) {
        self.ivars().provisional.set(false);
        if error.is_null() {
            return;
        }
        let code: isize = unsafe { msg_send![error, code] };
        let domain: *mut AnyObject = unsafe { msg_send![error, domain] };
        let domain = unsafe { ns_string(domain) }.unwrap_or_default();
        // NSURLErrorCancelled, WebKitErrorFrameLoadInterruptedByPolicyChange.
        if (domain == "NSURLErrorDomain" && code == -999)
            || (domain == "WebKitErrorDomain" && code == 102)
        {
            return;
        }
        let description: *mut AnyObject = unsafe { msg_send![error, localizedDescription] };
        let reason =
            unsafe { ns_string(description) }.unwrap_or_else(|| format!("{domain} error {code}"));
        let url = unsafe { current_url(web_view) };
        self.report(WebViewEvent::LoadFailed(WebViewLoadError {
            url: AzString::from(url),
            reason: AzString::from(reason),
        }));
    }
}

thread_local! {
    /// The process's ephemeral store, shared by every ephemeral view.
    static EPHEMERAL_STORE: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
}

unsafe fn data_store(store_cls: &AnyClass, storage: WebViewStorage) -> *mut AnyObject {
    match storage {
        WebViewStorage::Persistent => unsafe { msg_send![store_cls, defaultDataStore] },
        WebViewStorage::Ephemeral => EPHEMERAL_STORE.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.is_none() {
                let store: *mut AnyObject = unsafe { msg_send![store_cls, nonPersistentDataStore] };
                *slot = unsafe { Retained::retain(store) };
            }
            slot.as_ref()
                .map_or(core::ptr::null_mut(), |s| Retained::as_ptr(s) as *mut AnyObject)
        }),
    }
}

struct IosWebView {
    container: Retained<AnyObject>,
    web_view: Retained<AnyObject>,
    delegate: Retained<NavigationDelegate>,
}

impl IosWebView {
    unsafe fn tear_down(&self) {
        let key = NSString::from_str(TITLE_KEY);
        let nil: *mut AnyObject = core::ptr::null_mut();
        unsafe {
            let _: () = msg_send![
                &*self.web_view,
                removeObserver: &*self.delegate,
                forKeyPath: &*key
            ];
            let _: () = msg_send![&*self.web_view, setNavigationDelegate: nil];
            let _: () = msg_send![&*self.web_view, stopLoading];
            let _: () = msg_send![&*self.container, removeFromSuperview];
        }
    }
}

/// The web views of the iOS window (see the module docs).
pub struct IosWebViews {
    parent: Retained<AnyObject>,
    views: BTreeMap<WebViewId, IosWebView>,
    mailbox: SharedMailbox,
}

impl IosWebViews {
    /// The backend of the window whose render view is `parent`.
    pub fn new(parent: Retained<AnyObject>) -> Self {
        let wake_view = Retained::as_ptr(&parent) as *mut AnyObject;
        Self {
            parent,
            views: BTreeMap::new(),
            mailbox: Rc::new(RefCell::new(Mailbox {
                reports: Vec::new(),
                decisions: BTreeMap::new(),
                last_request: 0,
                wake_view,
            })),
        }
    }

    fn cancel_pending(&self, id: Option<WebViewId>) {
        let pending = self.mailbox.borrow_mut().take_decisions_of(id);
        for handler in pending {
            handler.call((POLICY_CANCEL,));
        }
    }
}

impl WebViewBackend for IosWebViews {
    fn create(&mut self, id: WebViewId, config: WebViewConfig, src: &str) -> Result<(), String> {
        if webkit().is_none() {
            return Err(String::from("WebKit.framework could not be loaded"));
        }
        let (Some(web_view_cls), Some(config_cls), Some(store_cls), Some(view_cls)) = (
            class("WKWebView"),
            class("WKWebViewConfiguration"),
            class("WKWebsiteDataStore"),
            class("UIView"),
        ) else {
            return Err(String::from("WebKit has no WKWebView here"));
        };
        NavigationDelegate::attach_protocol();
        let zero = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0));
        let view = unsafe {
            let configuration: *mut AnyObject = msg_send![config_cls, new];
            let Some(configuration) = Retained::from_raw(configuration) else {
                return Err(String::from("WKWebViewConfiguration could not be created"));
            };
            let store = data_store(store_cls, config.storage);
            if !store.is_null() {
                let _: () = msg_send![&*configuration, setWebsiteDataStore: store];
            }
            let alloc: *mut AnyObject = msg_send![web_view_cls, alloc];
            let web_view: *mut AnyObject =
                msg_send![alloc, initWithFrame: zero, configuration: &*configuration];
            let Some(web_view) = Retained::from_raw(web_view) else {
                return Err(String::from("WKWebView could not be created"));
            };
            let alloc: *mut AnyObject = msg_send![view_cls, alloc];
            let container: *mut AnyObject = msg_send![alloc, initWithFrame: zero];
            let Some(container) = Retained::from_raw(container) else {
                return Err(String::from("the web view's container could not be created"));
            };
            // Cut the web view to the container: the visible part of its box.
            let _: () = msg_send![&*container, setClipsToBounds: Bool::YES];
            let _: () = msg_send![&*container, setHidden: Bool::YES];

            let delegate = NavigationDelegate::create(id, Rc::clone(&self.mailbox));
            let _: () = msg_send![&*web_view, setNavigationDelegate: &*delegate];
            let key = NSString::from_str(TITLE_KEY);
            let no_context: *mut c_void = core::ptr::null_mut();
            let _: () = msg_send![
                &*web_view,
                addObserver: &*delegate,
                forKeyPath: &*key,
                options: KVO_OPTION_NEW,
                context: no_context
            ];
            let _: () = msg_send![&*container, addSubview: &*web_view];
            let _: () = msg_send![&*self.parent, addSubview: &*container];
            IosWebView {
                container,
                web_view,
                delegate,
            }
        };
        self.views.insert(id, view);
        if !src.is_empty() {
            self.navigate(id, src);
        }
        Ok(())
    }

    fn place(&mut self, id: WebViewId, placement: &WebViewPlacement) {
        let Some(view) = self.views.get(&id) else {
            return;
        };
        unsafe {
            if !placement.visible {
                let _: () = msg_send![&*view.container, setHidden: Bool::YES];
                return;
            }
            // UIKit's coordinates are azul's: top-left origin, points.
            let (clip, rect) = (placement.clip, placement.rect);
            let container_frame = NSRect::new(
                NSPoint::new(f64::from(clip.origin.x), f64::from(clip.origin.y)),
                NSSize::new(f64::from(clip.size.width), f64::from(clip.size.height)),
            );
            let web_view_frame = NSRect::new(
                NSPoint::new(
                    f64::from(rect.origin.x - clip.origin.x),
                    f64::from(rect.origin.y - clip.origin.y),
                ),
                NSSize::new(f64::from(rect.size.width), f64::from(rect.size.height)),
            );
            let _: () = msg_send![&*view.container, setFrame: container_frame];
            let _: () = msg_send![&*view.web_view, setFrame: web_view_frame];
            let _: () = msg_send![&*view.container, setHidden: Bool::NO];
        }
    }

    /// A scaled page keeps its own size in the scaled box (`pageZoom`,
    /// iOS 14+); a turn shows upright in its bounds.
    fn transform(&mut self, id: WebViewId, transform: &WebViewTransform) {
        let Some(view) = self.views.get(&id) else {
            return;
        };
        let (zoom_x, zoom_y) = transform.zoom();
        let zoom = f64::from(zoom_x.min(zoom_y));
        if !zoom.is_finite() || zoom <= 0.0 {
            return;
        }
        unsafe {
            let zooms: Bool =
                msg_send![&*view.web_view, respondsToSelector: objc2::sel!(setPageZoom:)];
            if zooms.as_bool() {
                let _: () = msg_send![&*view.web_view, setPageZoom: zoom];
            }
        }
    }

    fn navigate(&mut self, id: WebViewId, url: &str) {
        let Some(view) = self.views.get(&id) else {
            return;
        };
        let (Some(url_cls), Some(request_cls)) = (class("NSURL"), class("NSURLRequest")) else {
            return;
        };
        let loaded = unsafe {
            let string = NSString::from_str(url);
            let nsurl: *mut AnyObject = msg_send![url_cls, URLWithString: &*string];
            if nsurl.is_null() {
                false
            } else {
                let request: *mut AnyObject = msg_send![request_cls, requestWithURL: nsurl];
                let _: *mut AnyObject = msg_send![&*view.web_view, loadRequest: request];
                true
            }
        };
        if !loaded {
            self.mailbox.borrow_mut().report(
                id,
                0,
                WebViewEvent::LoadFailed(WebViewLoadError {
                    url: AzString::from(url),
                    reason: AzString::from("not a URL a web view can load"),
                }),
            );
        }
    }

    fn reload(&mut self, id: WebViewId) {
        if let Some(view) = self.views.get(&id) {
            let _: *mut AnyObject = unsafe { msg_send![&*view.web_view, reload] };
        }
    }

    fn go_back(&mut self, id: WebViewId) {
        if let Some(view) = self.views.get(&id) {
            let _: *mut AnyObject = unsafe { msg_send![&*view.web_view, goBack] };
        }
    }

    fn decide_navigation(&mut self, _id: WebViewId, request: u64, allow: bool) {
        // Out of the mailbox BEFORE the call: WebKit may call the delegate
        // back from inside it.
        let handler = self.mailbox.borrow_mut().decisions.remove(&request);
        if let Some((_, handler)) = handler {
            handler.call((if allow { POLICY_ALLOW } else { POLICY_CANCEL },));
        }
    }

    fn destroy(&mut self, id: WebViewId) {
        self.cancel_pending(Some(id));
        if let Some(view) = self.views.remove(&id) {
            unsafe { view.tear_down() };
        }
    }

    fn poll_reports(&mut self) -> Vec<WebViewReport> {
        core::mem::take(&mut self.mailbox.borrow_mut().reports)
    }
}

impl Drop for IosWebViews {
    fn drop(&mut self) {
        self.cancel_pending(None);
        for view in self.views.values() {
            unsafe { view.tear_down() };
        }
        self.views.clear();
    }
}
