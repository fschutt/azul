//! The shell half of `<webview>`: the backend trait every platform's native
//! web view implements, and the pump that connects it to the engine.
//!
//! The engine (`azul_layout::managers::webview`) decides WHICH views exist,
//! WHERE they are each frame and WHAT the app is told; a backend only turns
//! its ops into native calls and reports what the page did. The pump, run by
//! a shell once per loop turn ([`pump`]) and after every frame ([`sync`]):
//!
//! 1. syncs the placements from this frame's display lists and hands every
//!    queued op (create, navigate, place, destroy, ...) to the backend;
//! 2. takes the backend's reports - and the debug server's simulated ones -
//!    and dispatches each as a component event at its view's node, so the
//!    app's `WebViewNavigationRequested` / `LoadFinished` / `TitleChanged` /
//!    `LoadFailed` callbacks run;
//! 3. answers a navigation request with what they decided: allowed, unless a
//!    callback called `prevent_default` (how a sign-in flow catches the
//!    redirect that carries its code) or the page is `file://`.
//!
//! A backend that cannot create views at all (`create` fails: its library
//! did not load) turns the window into one WITHOUT web views
//! (`WebViewManager::set_absent`): every view's app hears
//! `WebViewLoadFailed` with the reason, and the DOM is rebuilt so each view
//! shows it.
//!
//! # Backends
//!
//! | platform | backend | state |
//! |---|---|---|
//! | headless | [`HeadlessWebViews`]: the shared recorder, no browser | done |
//! | macOS | `macos::webview` - `WKWebView`, `WebKit.framework` dlopen'd at the first view | done |
//! | Linux (X11, Wayland) | WPE `WebKit` via dlopen (`linux::webview`), the loading layer | probe |
//! | Windows | `WebView2` via `WebView2Loader.dll` (`windows::webview`) | probe |
//! | iOS | `WKWebView` in a `UIView`, the macOS shape | none yet |
//! | Android | `android.webkit.WebView` through JNI | none yet |
//!
//! A platform without a backend never reaches this module: its windows say
//! so through `WebViewPlatform` (the default `Absent`, or a `Probe` naming
//! the missing library), and the engine alone fails the views - the view
//! shows the reason and the app hears `WebViewLoadFailed`.
//!
//! # iOS and Android (design)
//!
//! - iOS: `macos::webview` with `UIKit` - a clip container `UIView`
//!   (`clipsToBounds`) holding a `WKWebView`, subviews of the render view
//!   (flipped like azul, no y conversion), the same navigation delegate,
//!   stores and decision-handler rules. `WebKit` is a system framework there
//!   too; it is loaded the same lazy way.
//! - Android: an `android.webkit.WebView` through JNI, a child of the
//!   activity's content `FrameLayout` positioned with layout params and
//!   clipped by a wrapping `FrameLayout` (`setClipChildren`). A Java
//!   `WebViewClient` subclass (`shouldOverrideUrlLoading`, `onPageFinished`,
//!   `onReceivedError`) and `WebChromeClient.onReceivedTitle` post reports to
//!   a native queue; `shouldOverrideUrlLoading` must answer synchronously,
//!   so the shell dispatches the report inside it on the UI thread. No
//!   `addJavascriptInterface` (no bridge), `setAllowFileAccess(false)`;
//!   ephemeral = clear the per-app `CookieManager` / `WebStorage` when the
//!   last ephemeral view goes (Android has one store per app process).
//!
//! Both: an OAuth provider that refuses embedded views (Google, Facebook)
//! goes through the system auth session (`ASWebAuthenticationSession`,
//! Custom Tabs) instead - a different API, not this node.

use alloc::{string::String, vec::Vec};

use azul_core::{
    callbacks::{RelayoutReason, Update},
    events::ProcessEventResult,
    resources::ImageRef,
    webview::WebViewConfig,
};
use azul_css::AzString;
use azul_layout::managers::webview::{
    WebViewId, WebViewInput, WebViewOp, WebViewPlacement, WebViewRecorder, WebViewReport,
    WebViewTransform,
};

use super::event::PlatformWindow;

/// How many op/report rounds one pump runs: a callback's command makes ops
/// that make reports; the cap keeps a callback that navigates on every
/// report from hanging the loop.
const MAX_ROUNDS: usize = 64;

/// One platform's native web views. Every method is for one view, named by
/// the engine's [`WebViewId`]; an id the backend does not know is ignored.
pub trait WebViewBackend {
    /// Create view `id` - hidden until it is placed - with `config`'s store
    /// (an ephemeral one: in memory, the app's alone; a persistent one: the
    /// app's own on disk; never the system browser's), no script bridge and
    /// no `file://` access, and start loading `src` (nothing when empty).
    /// Every top-level navigation must be reported as a `NavigationRequested`
    /// and wait for [`Self::decide_navigation`]. `Err(reason)`: this backend
    /// cannot show web views at all (its library did not load).
    fn create(&mut self, id: WebViewId, config: WebViewConfig, src: &str) -> Result<(), String>;
    /// Move, resize, clip, show or hide view `id` (window logical px).
    fn place(&mut self, id: WebViewId, placement: &WebViewPlacement);
    /// How view `id`'s page maps into its placement changed (a CSS
    /// transform above it): show the page at its own `size`, zoomed - or
    /// turned, where the native view can turn. Not called for a view that
    /// was never transformed. The default shows it untransformed at its
    /// placement (a native view that cannot zoom).
    fn transform(&mut self, id: WebViewId, transform: &WebViewTransform) {
        let _ = (id, transform);
    }
    /// Input aimed at view `id`'s page - only for a backend whose window
    /// composites its pages (`WebViewPlatform::Composited`); a native view
    /// takes its input itself.
    fn input(&mut self, id: WebViewId, input: &WebViewInput) {
        let _ = (id, input);
    }
    /// Load `url` in view `id`.
    fn navigate(&mut self, id: WebViewId, url: &str);
    /// Load view `id`'s page again.
    fn reload(&mut self, id: WebViewId);
    /// One entry back in view `id`'s history.
    fn go_back(&mut self, id: WebViewId);
    /// The app's answer to the `NavigationRequested` report with handle
    /// `request`. Every request is answered exactly once.
    fn decide_navigation(&mut self, id: WebViewId, request: u64, allow: bool);
    /// Destroy view `id`.
    fn destroy(&mut self, id: WebViewId);
    /// What the views reported since the last call, oldest first.
    fn poll_reports(&mut self) -> Vec<WebViewReport>;
    /// The newest frame of each composited view since the last call, for
    /// the window to draw (`LayoutWindow::set_webview_frame`). A native
    /// view draws itself: none.
    fn poll_frames(&mut self) -> Vec<(WebViewId, ImageRef)> {
        Vec::new()
    }

    /// Apply one engine op. `Err` only from [`Self::create`].
    fn apply(&mut self, op: &WebViewOp) -> Result<(), String> {
        match op {
            WebViewOp::Create { id, config, src } => return self.create(*id, *config, src.as_str()),
            WebViewOp::Navigate { id, url } => self.navigate(*id, url.as_str()),
            WebViewOp::Reload { id } => self.reload(*id),
            WebViewOp::GoBack { id } => self.go_back(*id),
            WebViewOp::Place { id, placement } => self.place(*id, placement),
            WebViewOp::Transform { id, transform } => self.transform(*id, transform),
            WebViewOp::Input { id, input } => self.input(*id, input),
            WebViewOp::Destroy { id } => self.destroy(*id),
        }
        Ok(())
    }
}

/// The headless backend: no browser, the shared recorder
/// (`WebViewRecorder` - the in-crate E2E runner drives the same one), so
/// `AZ_BACKEND=headless` and the runner mean the same thing by a scenario.
#[derive(Debug, Default)]
pub struct HeadlessWebViews {
    recorder: WebViewRecorder,
}

impl HeadlessWebViews {
    /// What the backend was told: its views, their placements and history.
    #[must_use]
    pub const fn recorder(&self) -> &WebViewRecorder {
        &self.recorder
    }
}

impl WebViewBackend for HeadlessWebViews {
    fn create(&mut self, id: WebViewId, config: WebViewConfig, src: &str) -> Result<(), String> {
        self.recorder.apply(&WebViewOp::Create {
            id,
            config,
            src: AzString::from(src),
        });
        Ok(())
    }
    fn place(&mut self, id: WebViewId, placement: &WebViewPlacement) {
        self.recorder.apply(&WebViewOp::Place {
            id,
            placement: *placement,
        });
    }
    fn transform(&mut self, id: WebViewId, transform: &WebViewTransform) {
        self.recorder.apply(&WebViewOp::Transform {
            id,
            transform: *transform,
        });
    }
    fn input(&mut self, id: WebViewId, input: &WebViewInput) {
        self.recorder.apply(&WebViewOp::Input {
            id,
            input: input.clone(),
        });
    }
    fn navigate(&mut self, id: WebViewId, url: &str) {
        self.recorder.apply(&WebViewOp::Navigate {
            id,
            url: AzString::from(url),
        });
    }
    fn reload(&mut self, id: WebViewId) {
        self.recorder.apply(&WebViewOp::Reload { id });
    }
    fn go_back(&mut self, id: WebViewId) {
        self.recorder.apply(&WebViewOp::GoBack { id });
    }
    fn decide_navigation(&mut self, id: WebViewId, request: u64, allow: bool) {
        self.recorder.decide(id, request, allow);
    }
    fn destroy(&mut self, id: WebViewId) {
        self.recorder.apply(&WebViewOp::Destroy { id });
    }
    fn poll_reports(&mut self) -> Vec<WebViewReport> {
        self.recorder.take_reports()
    }
    fn apply(&mut self, op: &WebViewOp) -> Result<(), String> {
        self.recorder.apply(op);
        Ok(())
    }
}

/// Placements and ops only: where the views are this frame, and every
/// queued op handed to the backend. No callback runs, so a shell may call
/// it inside its frame (after the frame's layout and scroll). Returns
/// whether the window lost its web views (the backend could not create
/// one): the DOM rebuild that shows why is already requested.
pub fn sync<W: PlatformWindow + ?Sized>(window: &mut W) -> bool {
    let ops = match window.get_layout_window_mut() {
        Some(lw) => {
            if lw.webviews.views().is_empty() && !lw.webviews.has_pending_work() {
                return false;
            }
            lw.sync_webview_placements();
            lw.webviews.take_ops()
        }
        None => return false,
    };
    if ops.is_empty() {
        return false;
    }
    let failure = match window.webview_backend() {
        Some(backend) => ops.iter().find_map(|op| backend.apply(op).err()),
        None => Some(String::from("this window's shell has no web view backend")),
    };
    let Some(reason) = failure else {
        return false;
    };
    crate::plog_warn!("[webview] no web views in this window: {reason}");
    let now = azul_core::task::Instant::from(std::time::Instant::now());
    if let Some(lw) = window.get_layout_window_mut() {
        // Delivered with the other lifecycle events after the rebuild below,
        // which also puts the reason into every view's box.
        let events = lw.webviews.set_absent(&AzString::from(reason.as_str()), &now);
        lw.pending_lifecycle_events.extend(events);
    }
    window
        .get_common_mut()
        .request_regeneration(RelayoutReason::RefreshDom);
    true
}

/// One turn of the web views: [`sync`], then every report - the backend's
/// and the debug server's simulated ones - dispatched at its view's node and
/// a navigation request answered. Rounds, because a callback's command makes
/// ops that make reports. Returns what the callbacks asked for; a DOM
/// rebuild is already requested.
pub fn pump<W: PlatformWindow + ?Sized>(window: &mut W) -> ProcessEventResult {
    let mut result = ProcessEventResult::DoNothing;
    for _ in 0..MAX_ROUNDS {
        if sync(window) {
            result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
        }
        let mut reports = window
            .webview_backend()
            .map(|backend| backend.poll_reports())
            .unwrap_or_default();
        if let Some(lw) = window.get_layout_window_mut() {
            reports.extend(lw.webviews.take_simulated());
        }
        if reports.is_empty() {
            break;
        }
        for report in reports {
            result = result.max(deliver(window, &report));
        }
    }
    // A composited backend's new frames, drawn by the window: what each
    // costs is a content change's (a repaint; the first frame a display
    // list), never a layout.
    let frames = window
        .webview_backend()
        .map(|backend| backend.poll_frames())
        .unwrap_or_default();
    if !frames.is_empty() {
        let tier = window.get_layout_window_mut().and_then(|lw| {
            frames
                .iter()
                .map(|(id, frame)| lw.set_webview_frame(*id, frame))
                .max()
        });
        result = result.max(window.content_change_result(tier));
    }
    if matches!(
        result,
        ProcessEventResult::ShouldRegenerateDomCurrentWindow
            | ProcessEventResult::ShouldRegenerateDomAllWindows
            | ProcessEventResult::ShouldIncrementalRelayout
            | ProcessEventResult::UpdateHitTesterAndProcessAgain
    ) {
        window
            .get_common_mut()
            .request_regeneration(RelayoutReason::RefreshDom);
    }
    result
}

/// Dispatch one report at its view's node and answer it.
fn deliver<W: PlatformWindow + ?Sized>(
    window: &mut W,
    report: &WebViewReport,
) -> ProcessEventResult {
    let now = azul_core::task::Instant::from(std::time::Instant::now());
    let event = window
        .get_layout_window_mut()
        .and_then(|lw| lw.webviews.begin_report(report, &now));
    let dispatched = event.is_some();
    let mut prevented = false;
    let mut result = ProcessEventResult::DoNothing;
    if let Some(event) = event {
        let (r, update, any_prevented, _) = window.dispatch_events_propagated(&[event]);
        result = r;
        if matches!(update, Update::RefreshDom | Update::RefreshDomAllWindows) {
            result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
        }
        prevented = any_prevented;
    }
    let allowed = window
        .get_layout_window_mut()
        .and_then(|lw| lw.webviews.finish_report(report, dispatched, prevented));
    if let (Some(allow), true) = (allowed, report.request != 0) {
        if let Some(backend) = window.webview_backend() {
            backend.decide_navigation(report.id, report.request, allow);
        }
    }
    result
}
