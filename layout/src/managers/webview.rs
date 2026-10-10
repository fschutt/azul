//! `<webview>`: the engine half (the node and its events are
//! `azul_core::webview`).
//!
//! A window's shell owns the native views - a `WKWebView`, a `WebView2`, a
//! recorder in the headless backend. This owns everything about them that is
//! the same on every platform:
//!
//! - WHICH views exist. A view is keyed by its node across rebuilds
//!   ([`NodeIdRemap`]) and named by a [`WebViewId`] for its lifetime, so an app
//!   rebuilding its DOM every frame keeps ONE native view - the page, its
//!   scroll and its sign-in state all live in that view. Mounting creates it,
//!   unmounting destroys it, a changed `src` on the same node navigates it
//!   ([`WebViewManager::reconcile`], at the tail of every layout pass).
//! - WHERE they are. The display lists reserve each view's content box
//!   (`DisplayListItem::WebView`); [`WebViewManager::sync_placements`] places
//!   it from `crate::headless::painted_webviews` - the live scroll offsets and
//!   every enclosing clip - each frame. A mounted view the lists did not paint
//!   this frame (`display: none` above it, `visibility: hidden`) or that is
//!   scrolled out of its box is HIDDEN, never destroyed.
//! - WHAT the backend must do: an op queue ([`WebViewOp`]) the shell drains.
//! - WHAT the app is told. A backend's report ([`WebViewReport`]) becomes a
//!   component event at the view's node ([`WebViewManager::begin_report`]);
//!   its payload is the view's last event, which `CallbackInfo::
//!   get_webview_event` reads; the answer to a navigation request
//!   ([`WebViewManager::finish_report`]) goes back to the backend.
//!
//! A window whose shell has no backend - a platform without one yet, a
//! system without the library ([`WebViewPlatform`]) - creates nothing: each
//! web view gets a `WebViewLoadFailed` with the reason, and the DOM pass
//! ([`insert_unavailable_fallback`]) puts that reason into the view's box.

use alloc::{format, string::String, vec::Vec};

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeType},
    events::SyntheticEvent,
    events::{KeyModifiers, MouseButton},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    task::Instant,
    webview::{
        create_webview_event, WebViewCommand, WebViewConfig, WebViewEvent, WebViewLoadError,
    },
};
use azul_css::AzString;

use super::{NodeIdMap, NodeIdRemap};
use crate::headless::PaintedWebView;

/// Why a window without a web view backend shows no page, when its shell
/// did not say more ([`WebViewPlatform::default`]).
pub const NO_BACKEND: &str = "this platform has no web view backend yet";

/// How many navigations a view remembers ([`MountedWebView::navigations`]).
pub const MAX_NAVIGATION_RECORDS: usize = 32;

/// The CSS of the paragraph a `<webview>` shows instead of a page.
const FALLBACK_CSS: &str = "margin: 0; padding: 8px; box-sizing: border-box; width: 100%; \
                            height: 100%; overflow: hidden; background: #f3f3f3; color: \
                            #555555; font-size: 13px;";

/// A web view's identity for its lifetime: what the backend and the debug
/// server name it by. Never reused within a window.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WebViewId(pub u64);

/// Where a web view is shown, in window logical coordinates - what the
/// backend applies to its native view.
#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct WebViewPlacement {
    /// The content box the page fills; may reach past the window.
    pub rect: LogicalRect,
    /// The part of `rect` that shows: the native view is cut to it.
    pub clip: LogicalRect,
    /// Whether anything of it shows. A hidden view keeps its page.
    pub visible: bool,
}

impl WebViewPlacement {
    /// Not shown: where a view starts, and where it goes while its node is
    /// not painted.
    pub const HIDDEN: Self = Self {
        rect: LogicalRect::zero(),
        clip: LogicalRect::zero(),
        visible: false,
    };

    /// Where the display lists put it.
    #[must_use]
    pub const fn of(painted: &PaintedWebView) -> Self {
        match painted.clip {
            Some(clip) => Self {
                rect: painted.rect,
                clip,
                visible: true,
            },
            None => Self {
                rect: painted.rect,
                clip: LogicalRect::zero(),
                visible: false,
            },
        }
    }
}

/// How a web view's page maps into its placement
/// ([`WebViewOp::Transform`]): the page's own size and the linear part of
/// the CSS transforms above its box.
///
/// A placement's `rect` is the bounding box of the page's box under the
/// whole map, so the map's translation follows from it: the backend gets
/// the size and the linear part here, and [`Self::to_window`] /
/// [`Self::to_page`] put the two together. An upright scale
/// ([`Self::is_axis_aligned`]) is what a native view shows exactly - at
/// `rect`, zoomed by [`Self::zoom`]; a turn or a skew only a backend that
/// can transform its view (or draws the page itself) shows as such.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct WebViewTransform {
    /// The page's own size: its content box, in CSS px - what it lays out
    /// at, whatever the transform.
    pub size: LogicalSize,
    /// The linear part of the page-to-window map, row-vector convention:
    /// `x' = x * sx + y * shx`, `y' = x * shy + y * sy`.
    pub sx: f32,
    pub shy: f32,
    pub shx: f32,
    pub sy: f32,
}

impl WebViewTransform {
    /// How close to zero a coefficient counts as zero (a quarter turn's
    /// cosine is not exactly zero in `f32`).
    const EPSILON: f32 = 1e-5;

    /// A page of `size` shown at its size, upright.
    #[must_use]
    pub const fn untransformed(size: LogicalSize) -> Self {
        Self {
            size,
            sx: 1.0,
            shy: 0.0,
            shx: 0.0,
            sy: 1.0,
        }
    }

    /// Shown at its own size, upright: its placement says all.
    #[must_use]
    pub fn is_untransformed(&self) -> bool {
        self.is_axis_aligned()
            && (self.sx - 1.0).abs() < Self::EPSILON
            && (self.sy - 1.0).abs() < Self::EPSILON
    }

    /// Upright - a scale, no turn, no skew.
    #[must_use]
    pub fn is_axis_aligned(&self) -> bool {
        self.shx.abs() < Self::EPSILON && self.shy.abs() < Self::EPSILON
    }

    /// How much the page is magnified along its own x and y: the lengths
    /// the map gives its unit vectors.
    #[must_use]
    pub fn zoom(&self) -> (f32, f32) {
        (self.sx.hypot(self.shy), self.shx.hypot(self.sy))
    }

    /// How far the page is turned, in degrees, clockwise on screen (y
    /// down, as CSS `rotate()`): the angle of the page's own x axis. With
    /// [`Self::zoom`] and the corner [`Self::to_window`] maps `(0, 0)` to,
    /// it is what a native view that turns is given (a skew is not kept).
    #[must_use]
    pub fn rotation_degrees(&self) -> f32 {
        self.shy.atan2(self.sx).to_degrees()
    }

    /// The page point `p` (CSS px from its top-left corner) on screen, for
    /// a page placed at `rect`.
    #[must_use]
    pub fn to_window(&self, rect: LogicalRect, p: LogicalPosition) -> LogicalPosition {
        let origin = self.origin(rect);
        LogicalPosition::new(
            p.x.mul_add(self.sx, p.y * self.shx) + origin.x,
            p.x.mul_add(self.shy, p.y * self.sy) + origin.y,
        )
    }

    /// The page point at window point `p`, for a page placed at `rect`;
    /// `None` for a map that flattens the page (nothing of it can be hit).
    #[must_use]
    pub fn to_page(&self, rect: LogicalRect, p: LogicalPosition) -> Option<LogicalPosition> {
        let det = self.sx.mul_add(self.sy, -(self.shx * self.shy));
        if det.abs() < Self::EPSILON {
            return None;
        }
        let origin = self.origin(rect);
        let (x, y) = (p.x - origin.x, p.y - origin.y);
        Some(LogicalPosition::new(
            self.sy.mul_add(x, -(self.shx * y)) / det,
            self.sx.mul_add(y, -(self.shy * x)) / det,
        ))
    }

    /// Where the page's top-left corner lands: the translation that puts
    /// the bounds of its mapped box at `rect`.
    fn origin(&self, rect: LogicalRect) -> LogicalPosition {
        let (w, h) = (self.size.width, self.size.height);
        // The mapped corners' x and y: (0, 0), (w, 0), (0, h), (w, h).
        let across = [0.0, w * self.sx, h * self.shx, w.mul_add(self.sx, h * self.shx)];
        let down = [0.0, w * self.shy, h * self.sy, w.mul_add(self.shy, h * self.sy)];
        let left = across.iter().copied().fold(f32::INFINITY, f32::min);
        let top = down.iter().copied().fold(f32::INFINITY, f32::min);
        LogicalPosition::new(rect.origin.x - left, rect.origin.y - top)
    }
}

/// What the pointer did, as the shell reports it to
/// `LayoutWindow::route_webview_pointer`.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum WebViewPointer {
    /// It moved.
    Move,
    /// A button went down (`pressed`) or up.
    Button { button: MouseButton, pressed: bool },
    /// A wheel or a touchpad scrolled by `delta` (logical px; positive `y`
    /// scrolls the content up, as a wheel turned towards the user does).
    Wheel { delta: LogicalPosition },
}

/// Input for a composited web view's page ([`WebViewOp::Input`]); points
/// are in page CSS px from its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WebViewInput {
    /// The pointer moved to `at`.
    PointerMove { at: LogicalPosition },
    /// A button went down (`pressed`) or up at `at`.
    PointerButton {
        at: LogicalPosition,
        button: MouseButton,
        pressed: bool,
    },
    /// A wheel or a touchpad scrolled by `delta` at `at`.
    Wheel {
        at: LogicalPosition,
        delta: LogicalPosition,
    },
    /// The pointer left the page (to something above it, or off it).
    PointerLeave,
    /// A key went down (`pressed`) or up, in the shell's own codes - only
    /// the backend of the same shell reads them (a keysym and a hardware
    /// keycode on Linux) - with the modifiers held.
    Key {
        native_key: u32,
        native_scan: u32,
        pressed: bool,
        modifiers: KeyModifiers,
    },
    /// The page got (`true`) or lost the keyboard focus.
    Focus(bool),
}

/// What a window's web view backend must do, in queue order
/// ([`WebViewManager::take_ops`]).
#[derive(Debug, Clone, PartialEq)]
pub enum WebViewOp {
    /// A `<webview>` mounted: create its native view (hidden until placed)
    /// with this store, and load `src` (nothing for an empty one).
    Create {
        id: WebViewId,
        config: WebViewConfig,
        src: AzString,
    },
    /// Load `url` (a changed `src`, or `CallbackInfo::webview_navigate`).
    Navigate { id: WebViewId, url: AzString },
    /// Load the current page again.
    Reload { id: WebViewId },
    /// One entry back in the view's history.
    GoBack { id: WebViewId },
    /// Move, resize, clip, show or hide the native view.
    Place {
        id: WebViewId,
        placement: WebViewPlacement,
    },
    /// How the page maps into its placement changed: a transform above it
    /// came, changed or went (an untransformed one again). Never sent for a
    /// view that was never transformed.
    Transform {
        id: WebViewId,
        transform: WebViewTransform,
    },
    /// Input aimed at a composited view's page (a native view takes its
    /// input itself).
    Input { id: WebViewId, input: WebViewInput },
    /// Its node unmounted: destroy the native view.
    Destroy { id: WebViewId },
}

impl WebViewOp {
    /// The view the op is for.
    #[must_use]
    pub const fn id(&self) -> WebViewId {
        match self {
            Self::Create { id, .. }
            | Self::Navigate { id, .. }
            | Self::Reload { id }
            | Self::GoBack { id }
            | Self::Place { id, .. }
            | Self::Transform { id, .. }
            | Self::Input { id, .. }
            | Self::Destroy { id } => *id,
        }
    }
}

/// What a backend (or the debug server's simulation) reports about one view.
#[derive(Debug, Clone, PartialEq)]
pub struct WebViewReport {
    /// The view.
    pub id: WebViewId,
    /// For a `NavigationRequested`: the backend's handle of the decision it
    /// waits for (answered with the shell's `decide_navigation`); `0` for
    /// any other report, and for a simulated one nobody waits for.
    pub request: u64,
    /// What happened.
    pub event: WebViewEvent,
}

/// One top-level navigation a view was asked about, and the answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebViewNavigationRecord {
    /// Where it went (or would have).
    pub url: AzString,
    /// A server redirect of a navigation under way.
    pub is_redirect: bool,
    /// Whether it went ahead (no callback cancelled it, no `file://`).
    pub allowed: bool,
}

/// One mounted `<webview>`.
#[derive(Debug, Clone, PartialEq)]
pub struct MountedWebView {
    /// Its identity for its lifetime.
    pub id: WebViewId,
    /// Its node, kept current across rebuilds.
    pub node: DomNodeId,
    /// The store it was created with.
    pub config: WebViewConfig,
    /// The node's `src` at the last layout.
    pub src: AzString,
    /// The page it is on (or going to): the last allowed navigation, then
    /// the page a load finished on. Empty before the first.
    pub url: AzString,
    /// The page's title, as last reported.
    pub title: AzString,
    /// A navigation went ahead and has neither finished nor failed.
    pub loading: bool,
    /// Where the backend was last told it is.
    pub placement: WebViewPlacement,
    /// The transform the backend was last told of; `None` while it shows
    /// untransformed (it never heard one, or the last one it heard said so).
    pub transform: Option<WebViewTransform>,
    /// The last [`MAX_NAVIGATION_RECORDS`] navigations it was asked about.
    pub navigations: Vec<WebViewNavigationRecord>,
    /// The last event it reported: what a callback at its node reads.
    pub last_event: Option<WebViewEvent>,
    /// The backend was told to create it (and has to be told to destroy it).
    known: bool,
}

/// Whether a window can show web views.
#[derive(Debug, Clone)]
pub enum WebViewPlatform {
    /// The shell has a backend: views are created through the op queue.
    Backend,
    /// The shell has a backend that renders each page offscreen: the
    /// window draws its frames (`LayoutWindow::set_webview_frame`) and
    /// routes its input to it (`WebViewOp::Input`).
    Composited,
    /// No backend: every web view shows this reason instead of a page.
    Absent(AzString),
    /// No backend, and the reason is the probe's (a missing system library,
    /// found by trying to load it). It is called only while a web view
    /// exists - an app without one loads nothing - and caches its answer.
    Probe(fn() -> AzString),
}

impl Default for WebViewPlatform {
    fn default() -> Self {
        Self::Absent(AzString::from_const_str(NO_BACKEND))
    }
}

/// One `<webview>` node a layout pass found
/// (`LayoutWindow::reconcile_webviews`).
#[derive(Debug, Clone, PartialEq)]
pub struct FoundWebView {
    /// The node.
    pub node: DomNodeId,
    /// Its config.
    pub config: WebViewConfig,
    /// Its `src` attribute (empty without one).
    pub src: AzString,
}

/// The `<webview>`s of one window - see the module docs.
#[derive(Debug, Default)]
pub struct WebViewManager {
    views: Vec<MountedWebView>,
    /// The last id handed out; ids start at 1.
    last_id: u64,
    ops: Vec<WebViewOp>,
    platform: WebViewPlatform,
    /// Reports the debug server simulated, for the shell to deliver like a
    /// backend's.
    simulated: Vec<WebViewReport>,
    /// The composited view the pointer was last routed to.
    pointer_over: Option<WebViewId>,
    /// The composited view a press went to, until its release.
    pointer_capture: Option<WebViewId>,
    /// The composited view with the keyboard focus, as last told.
    focused: Option<WebViewId>,
}

impl WebViewManager {
    /// No views, and no backend until the shell says it has one.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// What the shell can do: set once, when it creates the window.
    pub fn set_platform(&mut self, platform: WebViewPlatform) {
        self.platform = platform;
    }

    /// Whether a backend creates the views.
    #[must_use]
    pub const fn has_backend(&self) -> bool {
        matches!(self.platform, WebViewPlatform::Backend | WebViewPlatform::Composited)
    }

    /// Whether the window draws the pages and routes their input.
    #[must_use]
    pub const fn is_composited(&self) -> bool {
        matches!(self.platform, WebViewPlatform::Composited)
    }

    /// Why this window shows no page in a web view; `None` with a backend.
    /// Runs a [`WebViewPlatform::Probe`] (which caches its own answer).
    #[must_use]
    pub fn unavailable_reason(&self) -> Option<AzString> {
        match &self.platform {
            WebViewPlatform::Backend | WebViewPlatform::Composited => None,
            WebViewPlatform::Absent(reason) => Some(reason.clone()),
            WebViewPlatform::Probe(probe) => Some(probe()),
        }
    }

    /// The backend could not create a view after all (its library did not
    /// load): no web view in this window from now on. Every view is told
    /// with a `WebViewLoadFailed` (the events to dispatch are returned) and
    /// nothing more is queued for the backend; the shell rebuilds the DOM so
    /// the views show the reason.
    pub fn set_absent(&mut self, reason: &AzString, now: &Instant) -> Vec<SyntheticEvent> {
        self.platform = WebViewPlatform::Absent(reason.clone());
        self.ops.clear();
        (0..self.views.len())
            .map(|i| {
                self.views[i].known = false;
                self.fail(i, reason, now)
            })
            .collect()
    }

    /// Every mounted view.
    #[must_use]
    pub fn views(&self) -> &[MountedWebView] {
        &self.views
    }

    /// The view of the `<webview>` node `node`.
    #[must_use]
    pub fn view_at(&self, node: DomNodeId) -> Option<&MountedWebView> {
        self.views.iter().find(|v| v.node == node)
    }

    /// The view `id`.
    #[must_use]
    pub fn get(&self, id: WebViewId) -> Option<&MountedWebView> {
        self.views.iter().find(|v| v.id == id)
    }

    /// The last event the web view at `node` reported - the payload of the
    /// web view event a callback at that node is running for.
    #[must_use]
    pub fn event_of(&self, node: DomNodeId) -> Option<&WebViewEvent> {
        self.view_at(node)?.last_event.as_ref()
    }

    /// The backend's work since it last asked, in order.
    pub fn take_ops(&mut self) -> Vec<WebViewOp> {
        core::mem::take(&mut self.ops)
    }

    /// Whether the shell has anything to do: queued ops or simulated reports.
    #[must_use]
    pub const fn has_pending_work(&self) -> bool {
        !self.ops.is_empty() || !self.simulated.is_empty()
    }

    /// Bring the views in line with the `<webview>` nodes a layout pass
    /// `found`: a view whose node is gone (or no web view any more) is
    /// destroyed, a changed `src` navigates, a changed store makes a new view
    /// (a store is fixed when a native view is created), a new node creates
    /// one. Without a backend nothing is created: the returned events tell
    /// each new view's app why (`WebViewLoadFailed`), for the caller to queue
    /// with the other lifecycle events.
    pub fn reconcile(&mut self, found: &[FoundWebView], now: &Instant) -> Vec<SyntheticEvent> {
        let mut events = Vec::new();
        for view in core::mem::take(&mut self.views) {
            if found.iter().any(|f| f.node == view.node) {
                self.views.push(view);
            } else {
                self.destroy(view.id, view.known);
            }
        }
        if found.is_empty() {
            return events;
        }
        let absent = self.unavailable_reason();
        for f in found {
            if let Some(i) = self.views.iter().position(|v| v.node == f.node) {
                if self.views[i].config == f.config {
                    if self.views[i].src != f.src {
                        self.views[i].src = f.src.clone();
                        match &absent {
                            None => self.ops.push(WebViewOp::Navigate {
                                id: self.views[i].id,
                                url: f.src.clone(),
                            }),
                            Some(reason) => events.push(self.fail(i, reason, now)),
                        }
                    }
                    continue;
                }
                let replaced = self.views.remove(i);
                self.destroy(replaced.id, replaced.known);
            }
            self.last_id += 1;
            let id = WebViewId(self.last_id);
            self.views.push(MountedWebView {
                id,
                node: f.node,
                config: f.config,
                src: f.src.clone(),
                url: AzString::from_const_str(""),
                title: AzString::from_const_str(""),
                loading: false,
                placement: WebViewPlacement::HIDDEN,
                transform: None,
                navigations: Vec::new(),
                last_event: None,
                known: absent.is_none(),
            });
            match &absent {
                None => self.ops.push(WebViewOp::Create {
                    id,
                    config: f.config,
                    src: f.src.clone(),
                }),
                Some(reason) => {
                    let i = self.views.len() - 1;
                    events.push(self.fail(i, reason, now));
                }
            }
        }
        events
    }

    /// Place every view where the display lists put it this frame
    /// (`crate::headless::painted_webviews`); a view not among them is
    /// hidden. Only a CHANGED placement is queued for the backend, and a
    /// view's transform only when it changed - none for a view that was
    /// never transformed; a hidden view keeps the last one.
    pub fn sync_placements(&mut self, painted: &[PaintedWebView]) {
        if !self.has_backend() {
            return;
        }
        for view in &mut self.views {
            let found = painted.iter().find(|p| {
                p.dom_id == view.node.dom
                    && view.node.node.into_crate_internal() == Some(p.node_id)
            });
            let placement = found.map_or(WebViewPlacement::HIDDEN, WebViewPlacement::of);
            if placement != view.placement {
                view.placement = placement;
                self.ops.push(WebViewOp::Place {
                    id: view.id,
                    placement,
                });
            }
            if let Some(p) = found {
                let transform = (!p.transform.is_untransformed()).then_some(p.transform);
                if transform != view.transform {
                    view.transform = transform;
                    self.ops.push(WebViewOp::Transform {
                        id: view.id,
                        transform: p.transform,
                    });
                }
            }
        }
    }

    /// A callback's command (`CallbackInfo::webview_navigate`, ...) for the
    /// web view at `node`, queued for the backend. `false` when there is no
    /// such view, or no backend to drive it.
    pub fn queue_command(&mut self, node: DomNodeId, command: &WebViewCommand) -> bool {
        if !self.has_backend() {
            return false;
        }
        let Some(id) = self.view_at(node).map(|v| v.id) else {
            return false;
        };
        self.ops.push(match command {
            WebViewCommand::Navigate(url) => WebViewOp::Navigate {
                id,
                url: url.clone(),
            },
            WebViewCommand::Reload => WebViewOp::Reload { id },
            WebViewCommand::GoBack => WebViewOp::GoBack { id },
        });
        true
    }

    /// Route a pointer `event` at window point `at` to the composited page
    /// it is aimed at: the page whose node is the topmost under the pointer
    /// (`target`: that node and the hit test's point in its content box -
    /// the page point), or the page a press went to, until its release
    /// (the page point then from its placement and transform). A page the
    /// pointer moved off hears it leave. Returns whether a page took it;
    /// always `false` in a window that does not composite.
    pub fn route_pointer(
        &mut self,
        target: Option<(DomNodeId, LogicalPosition)>,
        at: LogicalPosition,
        event: WebViewPointer,
    ) -> bool {
        if !self.is_composited() {
            return false;
        }
        let hit = target.and_then(|(node, point)| self.view_at(node).map(|v| (v.id, point)));
        let aimed = match (self.pointer_capture, hit) {
            (Some(captured), Some((id, point))) if id == captured => Some((id, point)),
            (Some(captured), _) => self.get(captured).and_then(|view| {
                let transform = view
                    .transform
                    .unwrap_or_else(|| WebViewTransform::untransformed(view.placement.rect.size));
                transform
                    .to_page(view.placement.rect, at)
                    .map(|point| (captured, point))
            }),
            (None, hit) => hit,
        };
        let aimed_id = aimed.map(|(id, _)| id);
        if let Some(left) = self.pointer_over.filter(|over| Some(*over) != aimed_id) {
            self.ops.push(WebViewOp::Input {
                id: left,
                input: WebViewInput::PointerLeave,
            });
        }
        self.pointer_over = aimed_id;
        let Some((id, at)) = aimed else {
            return false;
        };
        let input = match event {
            WebViewPointer::Move => WebViewInput::PointerMove { at },
            WebViewPointer::Button { button, pressed } => {
                self.pointer_capture = pressed.then_some(id);
                WebViewInput::PointerButton {
                    at,
                    button,
                    pressed,
                }
            }
            WebViewPointer::Wheel { delta } => WebViewInput::Wheel { at, delta },
        };
        self.ops.push(WebViewOp::Input { id, input });
        true
    }

    /// Route a key to the composited page whose node has the keyboard
    /// focus (`focused`). Returns whether a page took it; always `false` in
    /// a window that does not composite.
    pub fn route_key(
        &mut self,
        focused: Option<DomNodeId>,
        native_key: u32,
        native_scan: u32,
        pressed: bool,
        modifiers: KeyModifiers,
    ) -> bool {
        if !self.is_composited() {
            return false;
        }
        let Some(id) = focused.and_then(|node| self.view_at(node)).map(|v| v.id) else {
            return false;
        };
        self.ops.push(WebViewOp::Input {
            id,
            input: WebViewInput::Key {
                native_key,
                native_scan,
                pressed,
                modifiers,
            },
        });
        true
    }

    /// Tell the composited pages the keyboard focus moved (`focused`: the
    /// node that has it now): the page that had it hears it lose it, the
    /// page that has it now hears it come. Nothing when it did not move.
    pub fn sync_focus(&mut self, focused: Option<DomNodeId>) {
        if !self.is_composited() {
            return;
        }
        let now = focused.and_then(|node| self.view_at(node)).map(|v| v.id);
        if now == self.focused {
            return;
        }
        if let Some(old) = self.focused {
            self.ops.push(WebViewOp::Input {
                id: old,
                input: WebViewInput::Focus(false),
            });
        }
        if let Some(new) = now {
            self.ops.push(WebViewOp::Input {
                id: new,
                input: WebViewInput::Focus(true),
            });
        }
        self.focused = now;
    }

    /// A report the debug server simulated (`simulate_webview_*`): delivered
    /// by the shell like a backend's.
    pub fn push_simulated(&mut self, report: WebViewReport) {
        self.simulated.push(report);
    }

    /// The simulated reports not delivered yet.
    pub fn take_simulated(&mut self) -> Vec<WebViewReport> {
        core::mem::take(&mut self.simulated)
    }

    /// Start delivering `report`: `Some(event)` to dispatch at the view's
    /// node (its payload is now the view's last event), or `None` when the
    /// app is not asked - the view is gone, or the navigation is to a
    /// `file://` page, which a web view never loads. Either way
    /// [`Self::finish_report`] answers it.
    pub fn begin_report(
        &mut self,
        report: &WebViewReport,
        now: &Instant,
    ) -> Option<SyntheticEvent> {
        if let WebViewEvent::NavigationRequested(nav) = &report.event {
            if is_file_url(nav.url.as_str()) {
                return None;
            }
        }
        let view = self.views.iter_mut().find(|v| v.id == report.id)?;
        let event = create_webview_event(&report.event, view.node, now);
        view.last_event = Some(report.event.clone());
        Some(event)
    }

    /// The report was dispatched (`dispatched`; `prevented`: a callback
    /// called `prevent_default`) or not: record what it changed on its view
    /// and, for a navigation request, return whether the navigation goes
    /// ahead - the answer the backend waits for. A navigation goes ahead when
    /// it was dispatched and nobody cancelled it.
    pub fn finish_report(
        &mut self,
        report: &WebViewReport,
        dispatched: bool,
        prevented: bool,
    ) -> Option<bool> {
        let view = self.views.iter_mut().find(|v| v.id == report.id);
        match &report.event {
            WebViewEvent::NavigationRequested(nav) => {
                let allowed = dispatched && !prevented && !is_file_url(nav.url.as_str());
                if let Some(view) = view {
                    if view.navigations.len() >= MAX_NAVIGATION_RECORDS {
                        view.navigations.remove(0);
                    }
                    view.navigations.push(WebViewNavigationRecord {
                        url: nav.url.clone(),
                        is_redirect: nav.is_redirect,
                        allowed,
                    });
                    if allowed {
                        view.url = nav.url.clone();
                        view.loading = true;
                    }
                }
                Some(allowed)
            }
            WebViewEvent::LoadFinished(url) => {
                if let Some(view) = view {
                    view.url = url.clone();
                    view.loading = false;
                }
                None
            }
            WebViewEvent::TitleChanged(title) => {
                if let Some(view) = view {
                    view.title = title.clone();
                }
                None
            }
            WebViewEvent::LoadFailed(_) => {
                if let Some(view) = view {
                    view.loading = false;
                }
                None
            }
        }
    }

    /// Tell the view at `index` it cannot load its `src`: its last event
    /// becomes the failure, and the event to dispatch is returned.
    fn fail(&mut self, index: usize, reason: &AzString, now: &Instant) -> SyntheticEvent {
        let view = &mut self.views[index];
        let event = WebViewEvent::LoadFailed(WebViewLoadError {
            url: view.src.clone(),
            reason: reason.clone(),
        });
        view.loading = false;
        let synthetic = create_webview_event(&event, view.node, now);
        view.last_event = Some(event);
        synthetic
    }

    /// Queue the destruction of view `id` - unless the backend never saw it:
    /// a creation still queued is dropped with everything else queued for it
    /// (a view created and destroyed before the backend looked never
    /// existed), and a view a backendless window never created needs none.
    fn destroy(&mut self, id: WebViewId, known: bool) {
        let created_unseen = self
            .ops
            .iter()
            .any(|op| matches!(op, WebViewOp::Create { id: created, .. } if *created == id));
        self.ops.retain(|op| op.id() != id);
        if known && !created_unseen {
            self.ops.push(WebViewOp::Destroy { id });
        }
        for routed in [
            &mut self.pointer_over,
            &mut self.pointer_capture,
            &mut self.focused,
        ] {
            if *routed == Some(id) {
                *routed = None;
            }
        }
    }
}

/// A view is keyed by its node, and node ids are arena indices that shift
/// when the DOM is rebuilt: without this the view would follow a live but
/// WRONG node after a rebuild that inserted a sibling above it. A node the
/// rebuild unmounted takes its view with it.
impl NodeIdRemap for WebViewManager {
    fn remap_node_ids(&mut self, dom: DomId, map: &NodeIdMap) {
        for mut view in core::mem::take(&mut self.views) {
            if let Some(node) = map.resolve_dom_node_id(dom, view.node) {
                view.node = node;
                self.views.push(view);
            } else {
                self.destroy(view.id, view.known);
            }
        }
    }
}

/// The web view "browser" of a window without one: the headless backend
/// (`AZ_BACKEND=headless`) and the in-crate E2E runner.
///
/// It loads nothing. It keeps what a backend is told - which views exist,
/// where they are, which page each is on and its history - and answers the
/// ops the way an engine does as far as the APP can see: every load it is
/// asked for (the initial `src`, a navigation, a reload, a step back) is
/// first reported as a navigation request, and a page counts as the view's
/// once that request was allowed. What a real page would do on its own -
/// finish loading, change its title, redirect - is what a test or a
/// scenario simulates (`WebViewManager::push_simulated`). One recorder for
/// both hosts, so a scenario means the same thing on either.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct WebViewRecorder {
    views: Vec<RecordedWebView>,
    /// The last request handle handed out; handles start at 1 (`0` is a
    /// simulated report nobody waits for).
    last_request: u64,
    reports: Vec<WebViewReport>,
}

/// One view as the recorder knows it.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordedWebView {
    /// The view.
    pub id: WebViewId,
    /// The store it was created with.
    pub config: WebViewConfig,
    /// Where it was last placed.
    pub placement: WebViewPlacement,
    /// The last transform it was told of (`None` before any).
    pub transform: Option<WebViewTransform>,
    /// The last [`MAX_NAVIGATION_RECORDS`] inputs routed to its page,
    /// oldest first.
    pub inputs: Vec<WebViewInput>,
    /// The pages it went to, oldest first; the last is the current one.
    pub history: Vec<AzString>,
    /// Requests reported and not answered yet: (handle, page, a step back).
    pending: Vec<(u64, AzString, bool)>,
}

impl WebViewRecorder {
    /// No views.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Every view it was told to create and not to destroy.
    #[must_use]
    pub fn views(&self) -> &[RecordedWebView] {
        &self.views
    }

    /// The view `id`.
    #[must_use]
    pub fn get(&self, id: WebViewId) -> Option<&RecordedWebView> {
        self.views.iter().find(|v| v.id == id)
    }

    /// Apply one op.
    pub fn apply(&mut self, op: &WebViewOp) {
        match op {
            WebViewOp::Create { id, config, src } => {
                self.views.retain(|v| v.id != *id);
                self.views.push(RecordedWebView {
                    id: *id,
                    config: *config,
                    placement: WebViewPlacement::HIDDEN,
                    transform: None,
                    inputs: Vec::new(),
                    history: Vec::new(),
                    pending: Vec::new(),
                });
                if !src.as_str().is_empty() {
                    self.request(*id, src.clone(), false);
                }
            }
            WebViewOp::Navigate { id, url } => self.request(*id, url.clone(), false),
            WebViewOp::Reload { id } => {
                if let Some(url) = self.get(*id).and_then(|v| v.history.last().cloned()) {
                    self.request(*id, url, false);
                }
            }
            WebViewOp::GoBack { id } => {
                let previous = self.get(*id).and_then(|v| {
                    let n = v.history.len();
                    (n >= 2).then(|| v.history[n - 2].clone())
                });
                if let Some(url) = previous {
                    self.request(*id, url, true);
                }
            }
            WebViewOp::Place { id, placement } => {
                if let Some(view) = self.views.iter_mut().find(|v| v.id == *id) {
                    view.placement = *placement;
                }
            }
            WebViewOp::Transform { id, transform } => {
                if let Some(view) = self.views.iter_mut().find(|v| v.id == *id) {
                    view.transform = Some(*transform);
                }
            }
            WebViewOp::Input { id, input } => {
                if let Some(view) = self.views.iter_mut().find(|v| v.id == *id) {
                    if view.inputs.len() >= MAX_NAVIGATION_RECORDS {
                        view.inputs.remove(0);
                    }
                    view.inputs.push(*input);
                }
            }
            WebViewOp::Destroy { id } => self.views.retain(|v| v.id != *id),
        }
    }

    /// The answer to a navigation request: an allowed page becomes the
    /// view's (a step back drops the page it left). A handle it did not
    /// hand out - a simulated request - changes nothing here.
    pub fn decide(&mut self, id: WebViewId, request: u64, allow: bool) {
        let Some(view) = self.views.iter_mut().find(|v| v.id == id) else {
            return;
        };
        let Some(i) = view.pending.iter().position(|(r, _, _)| *r == request) else {
            return;
        };
        let (_, url, back) = view.pending.remove(i);
        if !allow {
            return;
        }
        if back {
            drop(view.history.pop());
        } else {
            view.history.push(url);
        }
    }

    /// What it reported since the last call.
    pub fn take_reports(&mut self) -> Vec<WebViewReport> {
        core::mem::take(&mut self.reports)
    }

    /// Report a navigation of `id` to `url` and wait for its answer.
    fn request(&mut self, id: WebViewId, url: AzString, back: bool) {
        let Some(view) = self.views.iter_mut().find(|v| v.id == id) else {
            return;
        };
        self.last_request += 1;
        let request = self.last_request;
        view.pending.push((request, url.clone(), back));
        self.reports.push(WebViewReport {
            id,
            request,
            event: WebViewEvent::NavigationRequested(azul_core::webview::WebViewNavigation {
                url,
                is_redirect: false,
            }),
        });
    }
}

/// Whether `url` is a `file:` URL - a page a web view never loads.
fn is_file_url(url: &str) -> bool {
    url.trim_start()
        .get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("file:"))
}

/// Whether `dom` holds a `<webview>` anywhere.
#[must_use]
pub fn dom_has_webview(dom: &Dom) -> bool {
    matches!(dom.root.get_node_type(), NodeType::WebView(_))
        || dom.children.iter().any(dom_has_webview)
}

/// The text a `<webview>` shows when this window cannot show its page.
#[must_use]
pub fn unavailable_text(reason: &str) -> String {
    format!("This page cannot be shown here: {reason}")
}

/// Give every `<webview>` of `dom` that has no content of its own a
/// paragraph saying why it shows no page ([`unavailable_text`]), laid out in
/// its box (a replaced element's children are an overlay of its box). A web
/// view the app gave content keeps it: that IS its fallback. Returns whether
/// anything was inserted.
pub fn insert_unavailable_fallback(dom: &mut Dom, reason: &str) -> bool {
    fn walk(dom: &mut Dom, text: &str) -> bool {
        if matches!(dom.root.get_node_type(), NodeType::WebView(_)) {
            if !dom.children.is_empty() {
                return false;
            }
            let fallback =
                Dom::create_p_with_text(AzString::from(text)).with_css(FALLBACK_CSS);
            dom.set_children(vec![fallback].into());
            return true;
        }
        let mut inserted = false;
        for child in &mut dom.children {
            inserted |= walk(child, text);
        }
        inserted
    }
    let text = unavailable_text(reason);
    let inserted = walk(dom, &text);
    if inserted {
        // The counts of every ancestor of a web view that got a child.
        let _ = dom.fixup_children_estimated();
    }
    inserted
}
