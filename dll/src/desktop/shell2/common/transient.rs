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

use alloc::vec::Vec;

use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo},
    dom::{Dom, DomId},
    geom::{LogicalPosition, LogicalSize, PhysicalPosition},
    id::NodeId,
    refany::{OptionRefAny, RefAny},
    transient::TransientDismiss,
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
    });

    let mut window_state = popup_window_state("Popup", "azul-transient", size, origin);
    window_state.size.dpi = parent.size.dpi;
    window_state.theme = parent.theme;
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
        let Some(styled) = root.as_ref() else { break };
        let Some(content) = azul_core::transient::extract_subtree_as_dom(styled, w.source_node)
        else {
            continue;
        };
        match &w.surface {
            OptionRefAny::None => {
                // Just opened (or a surface never attached): create the window.
                let (options, mailbox) =
                    popup_create_options(parent_window_id, parent_state, &w, content, cursor);
                if let Some(slot) = lw.transient_windows.get_mut(w.content_dom) {
                    slot.surface = OptionRefAny::Some(mailbox);
                }
                log_debug!(
                    LogCategory::Window,
                    "[transient] opening popup {:?}: {}x{} at {:?}",
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
                    write(m, |d| {
                        d.content = content;
                        d.placement = w.placement;
                        d.content_size = w.content_size;
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

/// What the popup side found in its mailbox this pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupAction {
    /// Nothing to do.
    Nothing,
    /// The parent closed this popup: close the window.
    Close,
}

/// The popup side: act on what the parent wrote.
#[must_use]
pub fn poll_popup(state: &FullWindowState) -> PopupAction {
    let Some(m) = mailbox_of(state) else {
        return PopupAction::Nothing;
    };
    if read(&m, |d| d.closed).unwrap_or(false) {
        PopupAction::Close
    } else {
        PopupAction::Nothing
    }
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
        Some(m) => read(&m, |d| d.placement.dismiss)?,
        None if current.flags.window_type == WindowType::Menu => TransientDismiss::Outside,
        None => return None,
    };
    if policy == TransientDismiss::None {
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
    mailbox_of(state).is_some_and(|m| write(&m, |d| d.dismissed = true))
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
