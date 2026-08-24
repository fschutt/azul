//! The shell half of `<transient-window>`: popups as child windows.
//!
//! The engine half (`azul_layout::transient`) decides WHAT is open — it finds
//! every `<transient-window open>` node after a layout pass, lays each one's
//! subtree out under its own `DomId`, measures it, and reconciles the set
//! against what was open before, so a popup survives a parent rebuild instead
//! of flickering. This module turns that into windows.
//!
//! # Why a popup is a full child window, not a thin surface
//!
//! Every backend's popup is already a complete window with its own
//! `LayoutWindow`, renderer and GL context — including Wayland's `xdg_popup`.
//! So a transient window is created through the one path each backend already
//! has for a `Menu`-type window (`PlatformWindow::queue_window_create`), sized
//! to the content the engine measured, and positioned from the anchor. What
//! makes it still "one tree": its layout callback returns the parent's
//! extracted subtree (resolved style baked in, see
//! `azul_core::transient::extract_subtree_as_dom`), so every callback inside
//! it is the same function pointer on the same `RefAny` as in the parent.
//!
//! # The mailbox
//!
//! Parent and popup are different windows; on Wayland the popup is not even a
//! registered window, so there is no "close window by id". The channel is the
//! popup's layout-callback ctx: a [`TransientWindowData`] inside a `RefAny`
//! that the parent's manager keeps a clone of. The parent writes content,
//! placement and `closed`; the popup writes `dismissed`. After writing, the
//! writer wakes every window (`request_regeneration_all_windows`) so the
//! reader's next layout pass sees it. `RefAny` clones share one allocation
//! with runtime borrow checking, which is exactly a mailbox.
//!
//! # Dismiss, once, for every platform
//!
//! - In the popup: Escape (`dismiss=outside|escape`), or losing window focus
//!   (`dismiss=outside`) → it closes itself and posts `dismissed`.
//! - In the parent: any fresh mouse press while popups are open
//!   (`dismiss=outside`) → it closes them. The parent KNOWS such a press is
//!   outside, because the popup is a different window.
//! - The parent then marks the node dismissed (edge-triggered, so the node's
//!   still-`open` attribute cannot reopen it), and fires
//!   `ComponentEventFilter::Dismissed` on the node so the app drops its flag.
//!
//! # Tear-off
//!
//! A `tearoff` window's `-azul-app-region: drag` strip does not hand the
//! gesture to the window manager (an override-redirect popup has no WM, an
//! `xdg_popup` cannot be moved at all): the popup's OWN pipeline runs the
//! drag. `DragStart` on the strip records the press, every `Drag` moves the
//! window by the pointer's offset from it (the window follows the pointer,
//! so the offset stays small), and `DragEnd` posts where the window and the
//! pointer ended up, in PARENT coordinates, to the mailbox. The parent's next
//! sync hands that to the engine (`LayoutWindow::drop_transient_window`),
//! which decides - dock, dock onto a zone, tear off - and the surface diff
//! that follows is an ordinary close + open: the popup becomes a
//! `WindowType::Normal` toplevel (title bar, no light-dismiss, the same
//! mailbox layout callback) or the toplevel becomes a popup again.

use alloc::vec::Vec;

use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo},
    dom::{Dom, DomId},
    geom::{LogicalPosition, LogicalSize, PhysicalPosition},
    id::NodeId,
    refany::{OptionRefAny, RefAny},
    transient::{TransientDismiss, TransientTearoff},
    window::{VirtualKeyCode, WindowDecorations, WindowPosition, WindowType},
};
use azul_layout::{
    transient::{OpenTransientWindow, TransientPlacement},
    window::LayoutWindow,
    window_state::{FullWindowState, WindowCreateOptions},
};

use super::debug_server::LogCategory;
use crate::log_debug;

/// Shared between a parent window and one of its popups — see the module doc.
#[derive(Debug)]
pub struct TransientWindowData {
    /// The parent's registry id, as `WindowCreateOptions::parent_window_id`.
    pub parent_window_id: u64,
    /// Identity in the parent's manager: the id the content is laid out under.
    pub content_dom: DomId,
    /// Anchor rect + edge + dismiss policy, from the parent's last layout.
    pub placement: TransientPlacement,
    /// What the parent measured the content at; the popup window's size.
    pub content_size: LogicalSize,
    /// The extracted subtree the popup lays out. Replaced by the parent on
    /// every pass where it changed.
    pub content: Dom,
    /// Bumped whenever the parent replaces `content`/`placement`.
    pub generation: u64,
    /// Parent → popup: close yourself on your next pass.
    pub closed: bool,
    /// Popup → parent: the user dismissed me (I am already closing).
    pub dismissed: bool,
    /// The window's top-left in the PARENT's logical coordinates: the
    /// resolved placement for a popup, the drop origin for a torn-off
    /// toplevel. Kept live by the popup's own tear-off drag.
    pub origin: LogicalPosition,
    /// This window is a torn-off toplevel, not a popup on its anchor.
    pub torn: bool,
    /// Popup → popup: a tear-off drag in progress (see [`TearDrag`]).
    pub drag: Option<TearDrag>,
    /// Popup → parent: the tear-off drag ended; decide what it meant.
    pub drop: Option<TearDropReport>,
    /// Parent → popup: this torn window is a live drag PROXY, driven by the
    /// PARENT's ongoing gesture (an inline panel torn off the moment the drag
    /// crossed the threshold — the parent owns the mouse, so the parent writes
    /// `origin` every frame). While set, the window places itself at `origin`
    /// even though it is `torn`, so it follows the cursor from drag START, not
    /// only once the pointer leaves the parent. Cleared on drop.
    pub following: bool,
}

/// A tear-off drag in progress, inside the popup window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TearDrag {
    /// Where the pointer was, in the popup's own coordinates, when the drag
    /// began. The window is moved so that the pointer stays there.
    pub press_local: LogicalPosition,
    /// The window's origin (parent coordinates) when the drag began.
    pub origin_at_press: LogicalPosition,
}

/// Where a tear-off drag ended, in the PARENT's logical coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TearDropReport {
    /// The window's top-left.
    pub origin: LogicalPosition,
    /// The pointer.
    pub cursor: LogicalPosition,
}

impl TransientWindowData {
    /// The popup's layout callback reads this out of its ctx.
    fn from_ctx(ctx: &OptionRefAny) -> Option<RefAny> {
        let OptionRefAny::Some(r) = ctx else {
            return None;
        };
        let mut probe = r.clone();
        let is_ours = probe.downcast_ref::<Self>().is_some();
        is_ours.then(|| r.clone())
    }
}

/// The popup's layout callback: the parent's extracted subtree, nothing else.
extern "C" fn transient_layout_callback(_data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let ctx = info.get_ctx();
    let Some(mut mailbox) = TransientWindowData::from_ctx(&ctx) else {
        log_debug!(
            LogCategory::Window,
            "[transient] popup layout callback has no TransientWindowData ctx"
        );
        return Dom::create_div();
    };
    let Some(data) = mailbox.downcast_ref::<TransientWindowData>() else {
        return Dom::create_div();
    };
    data.content.clone()
}

/// Is this window a transient popup? Its mailbox, if so.
#[must_use]
pub fn mailbox_of(state: &FullWindowState) -> Option<RefAny> {
    TransientWindowData::from_ctx(&state.layout_callback.ctx)
}

/// Read a field off a mailbox without holding the borrow.
fn read<T>(mailbox: &RefAny, f: impl FnOnce(&TransientWindowData) -> T) -> Option<T> {
    let mut m = mailbox.clone();
    let d = m.downcast_ref::<TransientWindowData>()?;
    Some(f(&d))
}

/// Write to a mailbox. Returns `false` if it is not a transient mailbox.
fn write(mailbox: &RefAny, f: impl FnOnce(&mut TransientWindowData)) -> bool {
    let mut m = mailbox.clone();
    let Some(mut d) = m.downcast_mut::<TransientWindowData>() else {
        return false;
    };
    f(&mut d);
    true
}

/// Build the child window for a popup the engine just opened, plus the
/// mailbox the parent keeps to talk to it.
///
/// `Menu` is the window type on purpose: it is what every backend already
/// treats as "a borderless, always-on-top, parent-owned popup" — X11 adds
/// override-redirect + `_NET_WM_WINDOW_TYPE_POPUP_MENU`, Wayland turns it
/// into an `xdg_popup`, macOS and Windows make it borderless and keep it on
/// top. The size is the engine's measurement, so there is no
/// `size_to_content` 1×1-then-resize dance — the Wayland positioner needs a
/// real size up front.
#[must_use]
pub fn popup_create_options(
    parent_window_id: u64,
    parent: &FullWindowState,
    open: &OpenTransientWindow,
    content: Dom,
    cursor: Option<LogicalPosition>,
) -> (WindowCreateOptions, RefAny) {
    let size = open.content_size;
    let origin = open.placement.resolve_within(size, cursor, placement_bounds(parent));

    let mailbox = RefAny::new(TransientWindowData {
        parent_window_id,
        content_dom: open.content_dom,
        placement: open.placement,
        content_size: size,
        content,
        generation: 0,
        closed: false,
        dismissed: false,
        origin,
        torn: false,
        drag: None,
        drop: None,
        following: false,
    });

    let mut window_state = popup_window_state("Popup", "azul-transient", size, origin);
    window_state.size.dpi = parent.size.dpi;
    window_state.theme = parent.theme;
    // `material="transparent"` (or a clip mask on the node): the frame
    // clears to transparent and whatever the content leaves at alpha 0 is
    // not window - clicks fall through, the corners are really round.
    window_state.flags.background_material = open.placement.material;
    window_state.layout_callback = LayoutCallback {
        cb: transient_layout_callback,
        ctx: OptionRefAny::Some(mailbox.clone()),
    };

    let options = WindowCreateOptions {
        window_state,
        size_to_content: false,
        renderer: None.into(),
        theme: None.into(),
        create_callback: None.into(),
        hot_reload: false,
        parent_window_id,
    };
    (options, mailbox)
}

/// Build the TOPLEVEL for a torn-off transient window: a `Normal` window
/// with a title bar (so the OS gives it a close button and its own moves),
/// not always-on-top, never light-dismissed, at the engine's torn origin
/// (parent coordinates -> screen, where the parent's screen position is
/// known; the compositor places it where it is not). The same mailbox
/// layout callback as a popup: it is still the one subtree.
#[must_use]
pub fn toplevel_create_options(
    parent_window_id: u64,
    parent: &FullWindowState,
    open: &OpenTransientWindow,
    content: Dom,
    title: &str,
) -> (WindowCreateOptions, RefAny) {
    let size = open.content_size;
    let origin = open.torn.unwrap_or_else(|| open.placement.resolve(size, None));

    let mailbox = RefAny::new(TransientWindowData {
        parent_window_id,
        content_dom: open.content_dom,
        placement: open.placement,
        content_size: size,
        content,
        generation: 0,
        closed: false,
        dismissed: false,
        origin,
        torn: true,
        drag: None,
        drop: None,
        // Set true by the parent's drag handler on the first move after an
        // inline tear, so the proxy follows the cursor from drag start.
        following: false,
    });

    let mut window_state = FullWindowState::default();
    window_state.flags.window_type = WindowType::Normal;
    window_state.flags.is_visible = true;
    window_state.flags.is_resizable = false;
    // A torn-off panel is a bare, borderless surface that follows the cursor
    // and re-docks — a VS-style drag proxy, not an application window. No
    // titlebar, no traffic-light / close buttons (the popup path is already
    // frameless; the torn toplevel must match).
    window_state.flags.decorations = WindowDecorations::None;
    // Carry the configured material, exactly as the popup path does: a
    // `Transparent` panel keeps per-pixel alpha when torn off, so its rounded
    // corners are real corners instead of white window corners (the same
    // treatment the colour picker's popover gets — it was only applied to the
    // popup form, not the torn one).
    window_state.flags.background_material = open.placement.material;
    window_state.title = title.into();
    window_state.window_id = "azul-transient-torn".into();
    window_state.size.dimensions = size;
    window_state.size.dpi = parent.size.dpi;
    window_state.theme = parent.theme;
    #[allow(clippy::cast_possible_truncation)] // whole pixels
    {
        window_state.position = match parent.position {
            WindowPosition::Initialized(pp) => WindowPosition::Initialized(PhysicalPosition::new(
                pp.x + origin.x.round() as i32,
                pp.y + origin.y.round() as i32,
            )),
            _ => WindowPosition::Uninitialized,
        };
    }
    window_state.layout_callback = LayoutCallback {
        cb: transient_layout_callback,
        ctx: OptionRefAny::Some(mailbox.clone()),
    };

    let options = WindowCreateOptions {
        window_state,
        size_to_content: false,
        renderer: None.into(),
        theme: None.into(),
        create_callback: None.into(),
        hot_reload: false,
        parent_window_id,
    };
    (options, mailbox)
}

/// Where a popup of `parent` may go, in the parent's own coordinates: the
/// work area of the monitor the parent is on (a popup is its own window and
/// may hang out of a short parent, but not off the screen). When the
/// parent's screen position is unknown — Wayland, which never tells — the
/// parent's own rect is the best available bound; the compositor's
/// positioner does its own constraint adjustment there anyway.
fn placement_bounds(parent: &FullWindowState) -> azul_core::geom::LogicalRect {
    use azul_core::geom::LogicalRect;
    let own = LogicalRect::new(LogicalPosition::zero(), parent.size.dimensions);
    let WindowPosition::Initialized(pos) = parent.position else {
        return own;
    };
    #[allow(clippy::cast_precision_loss)] // screen coordinates
    let parent_origin = LogicalPosition::new(pos.x as f32, pos.y as f32);
    // macOS enumerates screens through AppKit, main thread only; a layout
    // pass off it (headless tests) keeps the parent-rect bound.
    #[cfg(target_os = "macos")]
    if objc2_foundation::MainThreadMarker::new().is_none() {
        return own;
    }
    let Some(display) = crate::desktop::display::get_window_display(parent_origin, parent.size.dimensions) else {
        return own;
    };
    let wa = display.work_area;
    LogicalRect::new(
        LogicalPosition::new(wa.origin.x - parent_origin.x, wa.origin.y - parent_origin.y),
        wa.size,
    )
}

/// The window state every popup window starts from — a transient window
/// or a fallback (window-based) menu alike. ONE place for "what a popup is":
/// the `Menu` window type every backend already treats as borderless,
/// always-on-top and parent-owned (X11 adds override-redirect +
/// `_NET_WM_WINDOW_TYPE_POPUP_MENU`, Wayland turns it into an `xdg_popup`),
/// no decorations, not resizable, and positioned RELATIVE TO THE PARENT's
/// content origin (`origin`, top-down logical px) — the one model that works
/// where absolute screen coordinates do not exist (Wayland) and is
/// re-resolved against the parent's live origin everywhere else.
#[must_use]
pub fn popup_window_state(
    title: &str,
    window_id: &str,
    size: LogicalSize,
    origin: LogicalPosition,
) -> FullWindowState {
    let mut window_state = FullWindowState::default();
    window_state.flags.window_type = WindowType::Menu;
    window_state.flags.is_always_on_top = true;
    window_state.flags.is_visible = true;
    window_state.flags.decorations = WindowDecorations::None;
    window_state.flags.is_resizable = false;
    window_state.title = title.into();
    window_state.window_id = window_id.into();
    window_state.size.dimensions = size;
    #[allow(clippy::cast_possible_truncation)] // whole logical pixels
    {
        window_state.position = WindowPosition::RelativeToParentWindow(PhysicalPosition::new(
            origin.x.round() as i32,
            origin.y.round() as i32,
        ));
    }
    {
        use azul_core::window::{AzStringPair, StringPairVec};
        let lin = &mut window_state.platform_specific_options.linux_options;
        lin.x11_override_redirect = true;
        lin.x11_wm_classes = StringPairVec::from_vec(alloc::vec![AzStringPair {
            key: window_id.into(),
            value: "Azul".into(),
        }]);
    }
    window_state
}

/// What the parent's sync step wants the shell to do afterwards.
#[derive(Debug, Default)]
pub struct SyncOutcome {
    /// Popups to create, in order.
    pub create: Vec<WindowCreateOptions>,
    /// A mailbox was written: every window must be woken to read it.
    pub wake_all: bool,
}

impl SyncOutcome {
    fn is_empty(&self) -> bool {
        self.create.is_empty() && !self.wake_all
    }
}

/// The parent side: reconcile the engine's popup set with real windows.
///
/// 1. Popups that posted `dismissed` are closed in the manager (edge-triggered)
///    and their `Dismissed` event is queued.
/// 2. Newly opened popups get a window each.
/// 3. Still-open popups whose content or placement changed get the new
///    content pushed.
/// 4. Closed popups' mailboxes get `closed`.
///
/// Runs after every layout pass of a window that has — or had — popups, and is
/// a no-op otherwise, so a window without `<transient-window>`s pays one empty
/// diff check per pass.
pub fn sync_parent(
    parent_window_id: u64,
    parent_state: &FullWindowState,
    lw: &mut LayoutWindow,
) -> SyncOutcome {
    let mut out = SyncOutcome::default();

    // 0. Tear-off drags that ended: the engine decides what the drop meant
    //    (dock / zone / tear off) and queues the surface changes + event.
    let drops: Vec<(NodeId, TearDropReport)> = lw
        .transient_windows
        .open_windows()
        .iter()
        .filter_map(|w| match &w.surface {
            OptionRefAny::Some(m) => read(m, |d| d.drop).flatten().map(|r| (w.source_node, r)),
            OptionRefAny::None => None,
        })
        .collect();
    for (node, report) in drops {
        if let Some(w) = lw.transient_windows.open_windows().iter().find(|w| w.source_node == node) {
            if let OptionRefAny::Some(m) = &w.surface {
                write(m, |d| d.drop = None);
            }
        }
        let changed = lw.drop_transient_window(node, report.origin, report.cursor);
        log_debug!(
            LogCategory::Window,
            "[transient] drop of node {:?} at {:?} (cursor {:?}): changed={}",
            node,
            report.origin,
            report.cursor,
            changed
        );
        if changed {
            // The drop changed where the window anchors (a zone) or what
            // kind of window it is: re-place it against the parent's CURRENT
            // layout now, rather than one layout late.
            super::layout::reconcile_transient_windows(lw, parent_state);
        }
    }

    // 1. Dismissals posted by popups.
    let dismissed: Vec<NodeId> = lw
        .transient_windows
        .open_windows()
        .iter()
        .filter(|w| match &w.surface {
            OptionRefAny::Some(m) => read(m, |d| d.dismissed).unwrap_or(false),
            OptionRefAny::None => false,
        })
        .map(|w| w.source_node)
        .collect();
    for node in dismissed {
        if let Some(closed) = lw.dismiss_transient_window(node) {
            if let OptionRefAny::Some(m) = &closed.surface {
                write(m, |d| d.closed = true);
            }
            log_debug!(
                LogCategory::Window,
                "[transient] popup {:?} dismissed by the user",
                closed.content_dom
            );
        }
    }

    let diff = lw.take_transient_diff();
    if diff.is_empty() && lw.transient_windows.open_windows().is_empty() {
        return out;
    }

    // 2 + 3. Open or refresh every window that is still open.
    let cursor = match parent_state.mouse_state.cursor_position {
        azul_core::window::CursorPosition::InWindow(p) => Some(p),
        _ => None,
    };
    let root = lw.layout_results.get(&DomId::ROOT_ID).map(|r| r.styled_dom.clone());
    let open: Vec<OpenTransientWindow> = lw.transient_windows.open_windows().to_vec();
    for w in open {
        if w.is_inline() {
            // Inline content of its parent / zone: laid out by the parent's
            // own pass, no surface of its own. (Its torn-off form is a
            // toplevel like any other, created below once `torn` is set.)
            continue;
        }
        let Some(styled) = root.as_ref() else { break };
        let Some(content) = azul_core::transient::extract_subtree_as_dom(styled, w.source_node)
        else {
            continue;
        };
        match &w.surface {
            OptionRefAny::None => {
                // Just opened (or a surface never attached): create the
                // window - a popup on its anchor, or a toplevel if torn off.
                let (options, mailbox) = if w.torn.is_some() {
                    let title = transient_title(styled, w.source_node);
                    toplevel_create_options(parent_window_id, parent_state, &w, content, &title)
                } else {
                    popup_create_options(parent_window_id, parent_state, &w, content, cursor)
                };
                if let Some(slot) = lw.transient_windows.get_mut(w.content_dom) {
                    slot.surface = OptionRefAny::Some(mailbox);
                }
                log_debug!(
                    LogCategory::Window,
                    "[transient] opening {} {:?}: {}x{} at {:?}",
                    if w.torn.is_some() { "toplevel" } else { "popup" },
                    w.content_dom,
                    w.content_size.width,
                    w.content_size.height,
                    options.window_state.position
                );
                out.create.push(options);
            }
            OptionRefAny::Some(m) => {
                let moved = diff.moved.contains(&w.content_dom);
                let changed = read(m, |d| d.content != content).unwrap_or(false);
                if moved || changed {
                    // A popup follows its anchor (the parent scrolled, the
                    // zone it docked onto moved); a torn toplevel is where
                    // the user put it and only its content refreshes.
                    let origin = if w.torn.is_some() {
                        None
                    } else {
                        Some(w.placement.resolve_within(w.content_size, cursor, placement_bounds(parent_state)))
                    };
                    write(m, |d| {
                        d.content = content;
                        d.placement = w.placement;
                        d.content_size = w.content_size;
                        if let Some(o) = origin {
                            if d.drag.is_none() {
                                d.origin = o;
                            }
                        }
                        d.generation += 1;
                    });
                    out.wake_all = true;
                }
            }
        }
    }

    // 4. Closed by the app (`open=false`) or by a node going away: the
    //    manager already dropped them; their mailboxes are handed over here.
    for m in lw.transient_windows.take_closed_surfaces() {
        if let OptionRefAny::Some(m) = m {
            if write(&m, |d| d.closed = true) {
                out.wake_all = true;
            }
        }
    }

    if !out.is_empty() {
        log_debug!(
            LogCategory::Window,
            "[transient] sync: create={} wake_all={}",
            out.create.len(),
            out.wake_all
        );
    }
    out
}

/// The torn-off toplevel's title: the `<transient-window>` node's `title`
/// attribute, else its accessible name, else "Panel".
fn transient_title(styled: &azul_core::styled_dom::StyledDom, node: NodeId) -> String {
    use azul_core::dom::AttributeType;
    let nodes = styled.node_data.as_container();
    let Some(nd) = nodes.get(node) else {
        return "Panel".into();
    };
    nd.attributes()
        .as_ref()
        .iter()
        .find_map(|a| match a {
            AttributeType::Title(t) | AttributeType::AriaLabel(t) => Some(t.as_str().to_owned()),
            _ => None,
        })
        .unwrap_or_else(|| "Panel".into())
}

/// What the popup side found in its mailbox this pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PopupAction {
    /// Nothing to do.
    Nothing,
    /// The parent closed this popup: close the window.
    Close,
    /// The parent re-placed / re-measured this popup: move and resize the
    /// window to match (`origin` in parent coordinates, `None` for a torn
    /// toplevel, which stays where the user put it).
    Place {
        origin: Option<LogicalPosition>,
        size: LogicalSize,
    },
}

/// The popup side: act on what the parent wrote. `placed_generation` is the
/// mailbox generation the window last applied; the returned one replaces it.
#[must_use]
pub fn poll_popup(state: &FullWindowState) -> PopupAction {
    let Some(m) = mailbox_of(state) else {
        return PopupAction::Nothing;
    };
    let Some((closed, generation, origin, size, torn, dragging, following)) =
        read(&m, |d| (d.closed, d.generation, d.origin, d.content_size, d.torn, d.drag.is_some(), d.following))
    else {
        return PopupAction::Nothing;
    };
    if closed {
        return PopupAction::Close;
    }
    // The window's own state is the "last applied" record: a placement the
    // parent wrote shows up as a position/size that differs from it.
    let _ = generation;
    if dragging {
        return PopupAction::Nothing;
    }
    // A live drag proxy follows the parent's cursor: place it at `origin`
    // every frame even though it is torn (a torn window normally stays where
    // the user put it, but during the parent-driven tear the parent IS the
    // one putting it, one frame at a time).
    if following {
        if state.position != relative_position(origin) {
            return PopupAction::Place { origin: Some(origin), size };
        }
        return PopupAction::Nothing;
    }
    let want_pos = (!torn).then(|| relative_position(origin));
    let pos_differs = want_pos.is_some_and(|p| state.position != p);
    let size_differs = state.size.dimensions != size;
    if pos_differs || size_differs {
        PopupAction::Place {
            origin: (!torn).then_some(origin),
            size,
        }
    } else {
        PopupAction::Nothing
    }
}

/// The proxy window's mailbox for the inline-tear `node`, if it has a surface
/// yet (it is created a pass after the tear began). The parent talks to the
/// child through this.
#[must_use]
pub fn inline_tear_mailbox(lw: &LayoutWindow, node: NodeId) -> Option<RefAny> {
    lw.transient_windows
        .open_windows()
        .iter()
        .find(|w| w.source_node == node)
        .and_then(|w| match &w.surface {
            OptionRefAny::Some(m) => Some(m.clone()),
            OptionRefAny::None => None,
        })
}

/// Parent → proxy: slide a live inline-tear proxy to `origin` (parent logical
/// coordinates) this frame. Sets `following` so the window honours `origin`
/// even though it is torn. The parent owns the gesture, so it — not the child
/// — advances the position. Returns whether the mailbox took it.
pub fn drive_proxy(mailbox: &RefAny, origin: LogicalPosition) -> bool {
    write(mailbox, |d| {
        d.origin = origin;
        d.following = true;
    })
}

/// The inline-tear drag ended: stop driving the proxy. Whatever the drop
/// decided (docked back inline, re-docked onto a zone, or left floating), the
/// parent is no longer moving it.
pub fn release_proxy(mailbox: &RefAny) -> bool {
    write(mailbox, |d| d.following = false)
}

/// `origin` (parent logical coordinates) as the `WindowPosition` a popup
/// carries - whole pixels, relative to the parent's content origin.
#[must_use]
pub fn relative_position(origin: LogicalPosition) -> WindowPosition {
    #[allow(clippy::cast_possible_truncation)] // whole logical pixels
    WindowPosition::RelativeToParentWindow(PhysicalPosition::new(
        origin.x.round() as i32,
        origin.y.round() as i32,
    ))
}

/// Is a tear-off drag running in this window?
#[must_use]
pub fn tear_drag_active(state: &FullWindowState) -> bool {
    mailbox_of(state).is_some_and(|m| read(&m, |d| d.drag.is_some()).unwrap_or(false))
}

/// The popup side, on `DragStart` over its `-azul-app-region: drag` strip:
/// begin a tear-off drag if the window allows one. Returns whether it did -
/// if not, the caller may hand the gesture to the window manager as for any
/// other window. `press` is where the gesture was PRESSED (the pointer has
/// moved past the drag threshold by now); the window catches up to it.
pub fn tear_drag_begin(state: &FullWindowState, press: Option<LogicalPosition>) -> bool {
    let Some(m) = mailbox_of(state) else {
        return false;
    };
    let Some(allowed) = read(&m, |d| d.placement.tearoff != TransientTearoff::None) else {
        return false;
    };
    if !allowed {
        return false;
    }
    let Some(cursor) = press.or_else(|| state.mouse_state.cursor_position.get_position()) else {
        return false;
    };
    write(&m, |d| {
        d.drag = Some(TearDrag {
            press_local: cursor,
            origin_at_press: d.origin,
        });
    })
}

/// The popup side, on every `Drag` while a tear-off drag runs: the window
/// moves so the pointer stays where it pressed. Returns the new window
/// position to apply, or `None` when nothing moved / no drag runs. The
/// mailbox `origin` (parent coordinates) moves with it, so the drop can be
/// reported without knowing the screen.
///
/// `window_follows`: whether this backend actually moves the window when
/// the position is applied. Where it does (macOS, Windows, X11), the
/// pointer's offset from the press point is the RESIDUAL after the last
/// move, and the origin advances by it. Where it cannot (Wayland at
/// `xdg_wm_base` v1 has no `xdg_popup.reposition`), the offset is the whole
/// drag so far, and the origin is the press origin plus it.
#[must_use]
pub fn tear_drag_move(state: &FullWindowState, window_follows: bool) -> Option<WindowPosition> {
    let m = mailbox_of(state)?;
    let (drag, origin, torn) = read(&m, |d| (d.drag, d.origin, d.torn))?;
    let drag = drag?;
    let cursor = state.mouse_state.cursor_position.get_position()?;
    let dx = cursor.x - drag.press_local.x;
    let dy = cursor.y - drag.press_local.y;
    if dx.abs() < 0.5 && dy.abs() < 0.5 {
        return None;
    }
    let new_origin = if window_follows {
        LogicalPosition::new(origin.x + dx, origin.y + dy)
    } else {
        LogicalPosition::new(drag.origin_at_press.x + dx, drag.origin_at_press.y + dy)
    };
    write(&m, |d| d.origin = new_origin);
    #[allow(clippy::cast_possible_truncation)] // whole pixels
    let position = if torn {
        match state.position {
            WindowPosition::Initialized(p) => {
                WindowPosition::Initialized(PhysicalPosition::new(p.x + dx.round() as i32, p.y + dy.round() as i32))
            }
            // No screen coordinates (Wayland): a torn toplevel cannot be
            // moved by its content; the compositor's own move does that.
            other => other,
        }
    } else {
        relative_position(new_origin)
    };
    Some(position)
}

/// The popup side, on `DragEnd`: report where the window and the pointer
/// ended up (parent coordinates) for the parent to decide. Returns whether a
/// report was posted (the caller wakes all windows). `window_follows` as in
/// [`tear_drag_move`]: the pointer is `local` from wherever the window
/// ACTUALLY is - the moved origin, or the press origin if it never moved.
pub fn tear_drag_end(state: &FullWindowState, window_follows: bool) -> bool {
    let Some(m) = mailbox_of(state) else {
        return false;
    };
    let Some((drag, origin)) = read(&m, |d| (d.drag, d.origin)) else {
        return false;
    };
    let Some(drag) = drag else {
        return false;
    };
    let local = state.mouse_state.cursor_position.get_position().unwrap_or(LogicalPosition::zero());
    let actual = if window_follows { origin } else { drag.origin_at_press };
    let cursor = LogicalPosition::new(actual.x + local.x, actual.y + local.y);
    write(&m, |d| {
        d.drag = None;
        d.drop = Some(TearDropReport { origin, cursor });
    })
}

/// Why a popup dismissed itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DismissCause {
    /// Escape was pressed in the popup.
    Escape,
    /// The popup lost window focus (the user went somewhere else).
    FocusLost,
}

/// The popup side: should this input transition dismiss the popup?
///
/// Escape counts for `dismiss=outside` and `dismiss=escape`; focus loss only
/// for `outside`. `dismiss=none` never dismisses — the app closes it.
#[must_use]
pub fn popup_dismiss_cause(
    previous: &FullWindowState,
    current: &FullWindowState,
) -> Option<DismissCause> {
    // A transient window carries its policy in the mailbox. A window-based
    // MENU (no mailbox, `Menu` type) light-dismisses by definition — this is
    // the one dismiss implementation serving both, which is what the plan's
    // "lift Menu's window into TransientWindow" step is for: Escape and
    // focus loss close a fallback menu on every backend the same way.
    let policy = match mailbox_of(current) {
        // A torn-off toplevel is a palette: it closes by its close button.
        Some(m) => {
            let (torn, dismiss) = read(&m, |d| (d.torn, d.placement.dismiss))?;
            if torn {
                return None;
            }
            dismiss
        }
        None if current.flags.window_type == WindowType::Menu => TransientDismiss::Outside,
        None => return None,
    };
    if policy == TransientDismiss::None {
        return None;
    }
    // The eyedropper's loupe window (or the system sampler) takes the
    // focus; the popup that asked must survive that to hear the answer.
    if azul_layout::managers::eyedropper::in_flight_anywhere() {
        return None;
    }
    let escape_now = current
        .keyboard_state
        .pressed_virtual_keycodes
        .as_ref()
        .contains(&VirtualKeyCode::Escape);
    let escape_before = previous
        .keyboard_state
        .pressed_virtual_keycodes
        .as_ref()
        .contains(&VirtualKeyCode::Escape);
    if escape_now && !escape_before {
        return Some(DismissCause::Escape);
    }
    if policy == TransientDismiss::Outside && previous.window_focused && !current.window_focused {
        return Some(DismissCause::FocusLost);
    }
    None
}

/// The popup side: post `dismissed` to the parent. The caller closes the
/// window and wakes all windows.
pub fn post_dismissed(state: &FullWindowState) -> bool {
    mailbox_of(state).is_some_and(|m| {
        // A window the parent already closed has nothing to report.
        if read(&m, |d| d.closed).unwrap_or(true) {
            return false;
        }
        write(&m, |d| d.dismissed = true)
    })
}

/// The popup side: the window is closing for ANY reason the parent did not
/// cause (the torn-off toplevel's close button, the app) - tell the parent,
/// so the node closes too and the app hears `Dismissed`. A no-op for a
/// window the parent closed.
pub fn post_dismissed_on_close(previous: &FullWindowState, current: &FullWindowState) -> bool {
    if previous.flags.close_requested || !current.flags.close_requested {
        return false;
    }
    post_dismissed(current)
}

/// The parent side: Escape was pressed while popups are open. The popup
/// handles its own Escape when it has keyboard focus; on a platform (or in a
/// moment) where the parent still has it, the parent closes every popup
/// whose policy allows Escape. Returns whether any were.
pub fn dismiss_on_escape(
    previous: &FullWindowState,
    current: &FullWindowState,
    lw: &mut LayoutWindow,
) -> bool {
    let esc = |s: &FullWindowState| {
        s.keyboard_state.pressed_virtual_keycodes.as_ref().contains(&VirtualKeyCode::Escape)
    };
    if !(esc(current) && !esc(previous)) {
        return false;
    }
    let targets: Vec<NodeId> = lw
        .transient_windows
        .open_windows()
        .iter()
        .filter(|w| !w.is_inline() && w.torn.is_none())
        .filter(|w| w.placement.dismiss != TransientDismiss::None)
        .map(|w| w.source_node)
        .collect();
    let mut any = false;
    for node in targets {
        if let Some(closed) = lw.dismiss_transient_window(node) {
            if let OptionRefAny::Some(m) = &closed.surface {
                write(m, |d| d.closed = true);
            }
            any = true;
        }
    }
    any
}

/// The parent side: a fresh mouse press landed in the parent while popups
/// with `dismiss=outside` are open — that press is, by construction, outside
/// them. Dismisses those popups; returns whether any were.
pub fn dismiss_outside_on_press(
    previous: &FullWindowState,
    current: &FullWindowState,
    lw: &mut LayoutWindow,
) -> bool {
    let was_down = |s: &FullWindowState| {
        s.mouse_state.left_down || s.mouse_state.right_down || s.mouse_state.middle_down
    };
    if !(was_down(current) && !was_down(previous)) {
        return false;
    }
    // A press on the popup's own ANCHOR is not "outside": the anchor is the
    // invoker (a swatch, a button), and its own click handler decides — a
    // toggle there closes the popup itself. Dismissing here as well would
    // let the release re-open it, the press/release flip HTML popovers
    // avoid by exempting the invoker from light dismiss.
    let press_at = match current.mouse_state.cursor_position {
        azul_core::window::CursorPosition::InWindow(p) => Some(p),
        _ => None,
    };
    let on_anchor = |w: &OpenTransientWindow| {
        press_at.is_some_and(|p| {
            let a = w.placement.anchor_rect;
            p.x >= a.origin.x
                && p.x <= a.origin.x + a.size.width
                && p.y >= a.origin.y
                && p.y <= a.origin.y + a.size.height
        })
    };
    let targets: Vec<NodeId> = lw
        .transient_windows
        .open_windows()
        .iter()
        // Light-dismiss is a POPUP's behaviour: an inline-docked panel is
        // content, a torn-off palette is a window of its own - a press in
        // the parent is not "outside" either.
        .filter(|w| !w.is_inline() && w.torn.is_none())
        .filter(|w| w.placement.dismiss == TransientDismiss::Outside && !on_anchor(w))
        .map(|w| w.source_node)
        .collect();
    let mut any = false;
    for node in targets {
        if let Some(closed) = lw.dismiss_transient_window(node) {
            if let OptionRefAny::Some(m) = &closed.surface {
                write(m, |d| d.closed = true);
            }
            any = true;
        }
    }
    any
}

#[cfg(test)]
mod tests {
    use super::*;
    use azul_core::{
        geom::LogicalRect,
        transient::TransientAnchor,
    };
    use azul_layout::transient::placement_for;

    fn open_window(dom: usize) -> OpenTransientWindow {
        let cfg = azul_core::transient::TransientWindowConfig::opened();
        OpenTransientWindow {
            source_node: NodeId::new(3),
            content_dom: azul_layout::transient::transient_dom_id(dom),
            placement: placement_for(
                NodeId::new(3),
                LogicalRect::new(LogicalPosition::new(48.0, 48.0), LogicalSize::new(60.0, 24.0)),
                &cfg,
            ),
            content_size: LogicalSize::new(240.0, 160.0),
            surface: OptionRefAny::None,
            torn: None,
            anchor_override: None,
            attr_torn: false,
        }
    }

    /// The popup window is a Menu-type, borderless, always-on-top child of
    /// the parent, exactly the engine's size, anchored below the swatch.
    #[test]
    fn the_popup_window_is_a_sized_parent_relative_menu_window() {
        let parent = FullWindowState::default();
        let w = open_window(0);
        let (opts, mailbox) = popup_create_options(0xABCD, &parent, &w, Dom::create_div(), None);
        assert_eq!(opts.parent_window_id, 0xABCD);
        assert!(!opts.size_to_content, "the engine measured it; no resize dance");
        let st = &opts.window_state;
        assert_eq!(st.flags.window_type, WindowType::Menu);
        assert_eq!(st.flags.decorations, WindowDecorations::None);
        assert!(st.flags.is_always_on_top && !st.flags.is_resizable);
        assert_eq!(st.size.dimensions, LogicalSize::new(240.0, 160.0));
        // Bottom edge of the anchor (48+24=72), left-aligned with it.
        assert_eq!(
            st.position,
            WindowPosition::RelativeToParentWindow(PhysicalPosition::new(48, 72))
        );
        assert_eq!(w.placement.anchor, TransientAnchor::Bottom);
        assert!(mailbox_of(st).is_some(), "the ctx IS the mailbox");
        assert_eq!(read(&mailbox, |d| d.generation), Some(0));
    }

    /// Escape dismisses under `outside` and `escape`; focus loss only under
    /// `outside`; nothing under `none`. Edges, not levels.
    #[test]
    fn dismiss_causes_follow_the_policy() {
        let mk = |dismiss: TransientDismiss| {
            let mut w = open_window(0);
            w.placement.dismiss = dismiss;
            let (opts, _) =
                popup_create_options(1, &FullWindowState::default(), &w, Dom::create_div(), None);
            opts.window_state
        };
        let press_escape = |s: &mut FullWindowState| {
            s.keyboard_state.pressed_virtual_keycodes =
                alloc::vec![VirtualKeyCode::Escape].into();
        };

        for policy in [TransientDismiss::Outside, TransientDismiss::Escape] {
            let before = mk(policy);
            let mut after = mk(policy);
            press_escape(&mut after);
            assert_eq!(popup_dismiss_cause(&before, &after), Some(DismissCause::Escape), "{policy:?}");
            // Held Escape is not a new press.
            let mut held = mk(policy);
            press_escape(&mut held);
            assert_eq!(popup_dismiss_cause(&held, &after), None, "{policy:?} held");
        }

        let mut focused = mk(TransientDismiss::Outside);
        focused.window_focused = true;
        let mut unfocused = mk(TransientDismiss::Outside);
        unfocused.window_focused = false;
        assert_eq!(popup_dismiss_cause(&focused, &unfocused), Some(DismissCause::FocusLost));
        assert_eq!(popup_dismiss_cause(&unfocused, &unfocused), None, "never focused: no edge");

        let mut f2 = mk(TransientDismiss::Escape);
        f2.window_focused = true;
        let mut u2 = mk(TransientDismiss::Escape);
        u2.window_focused = false;
        assert_eq!(popup_dismiss_cause(&f2, &u2), None, "escape-only ignores focus");

        let before = mk(TransientDismiss::None);
        let mut after = mk(TransientDismiss::None);
        press_escape(&mut after);
        assert_eq!(popup_dismiss_cause(&before, &after), None, "none never dismisses");
    }

    /// A popup reports itself dismissed through the mailbox, and the parent
    /// can see it; a closed flag travels the other way.
    #[test]
    fn the_mailbox_carries_both_directions() {
        let w = open_window(0);
        let (opts, mailbox) =
            popup_create_options(1, &FullWindowState::default(), &w, Dom::create_div(), None);
        assert_eq!(poll_popup(&opts.window_state), PopupAction::Nothing);
        assert!(post_dismissed(&opts.window_state));
        assert_eq!(read(&mailbox, |d| d.dismissed), Some(true));
        assert!(write(&mailbox, |d| d.closed = true));
        assert_eq!(poll_popup(&opts.window_state), PopupAction::Close);
        // A window that is not a popup has no mailbox and nothing to do.
        assert_eq!(poll_popup(&FullWindowState::default()), PopupAction::Nothing);
        assert!(!post_dismissed(&FullWindowState::default()));
    }
}
