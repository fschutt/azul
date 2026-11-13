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
//! - In the popup: Escape (`dismiss=outside|escape`), or losing window focus (`dismiss=outside`) →
//!   it closes itself and posts `dismissed`.
//! - In the parent: any fresh mouse press while popups are open (`dismiss=outside`) → it closes
//!   them. The parent KNOWS such a press is outside, because the popup is a different window.
//! - The parent then marks the node dismissed (edge-triggered, so the node's still-`open` attribute
//!   cannot reopen it), and fires `ComponentEventFilter::Dismissed` on the node so the app drops
//!   its flag.
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
    geom::{LogicalPosition, LogicalRect, LogicalSize, PhysicalPosition},
    id::NodeId,
    refany::{OptionRefAny, RefAny},
    task::ThreadId,
    transient::{TransientAnchor, TransientDismiss, TransientTearoff},
    window::{
        VirtualKeyCode, WindowBackgroundMaterial, WindowDecorations, WindowPosition, WindowType,
    },
};
use azul_layout::{
    placement::{screen_for_anchor, FloatingPlacement, ScreenPoint, ScreenRects, WindowToScreen},
    thread::Thread,
    transient::{OpenTransientWindow, TransientPlacement},
    window::LayoutWindow,
    window_state::{FullWindowState, WindowCreateOptions},
};

use super::debug_server::LogCategory;
use crate::log_debug;

/// Shared between a parent window and one of its popups — see the module doc.
#[derive(Debug, Clone)]
pub struct TransientWindowData {
    /// The parent's registry id, as `WindowCreateOptions::parent_window_id`.
    pub parent_window_id: u64,
    /// Identity in the parent's manager: the id the content is laid out under.
    pub content_dom: DomId,
    /// Anchor rect + edge + dismiss policy, from the parent's last layout.
    pub placement: TransientPlacement,
    /// The rule the popup is placed by (`TransientPlacement::floating`), in the
    /// parent's coordinates: what a Wayland compositor's positioner is handed,
    /// and what `origin` is resolved from everywhere else.
    pub floating: FloatingPlacement,
    /// How many of the backend's screen units (the unit `WindowPosition`
    /// carries) one logical px of the parent is: what turns `origin` and a
    /// tear-off drag into window positions. 1 where the parent has no screen
    /// mapping.
    pub screen_scale: f32,
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
    /// MODALITY INHERITANCE. `true` when the parent opened this window from a
    /// KEYBOARD-indicated focus (Tab to a swatch, then Enter/Space), so the
    /// control the popup autofocuses must show a focus ring; `false` when a
    /// pointer opened it, where a ring would look like the user had tabbed.
    ///
    /// This is the `:focus-visible` bit of the ELEMENT THAT OPENED THE POPUP,
    /// carried across the window boundary - a popup is a separate OS window
    /// with its own `FocusManager`, so it cannot see the parent's modality any
    /// other way, and without it a keyboard-opened picker autofocused its
    /// colour plane INVISIBLY (device report, 2026-09-01).
    pub focus_visible: bool,
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
    /// Parent → popup: keyboard transitions the PARENT received while this
    /// popup held the keyboard, oldest first, for the popup to replay through
    /// its own pipeline (see [`forward_key`]).
    pub forwarded_keys: Vec<ForwardedKey>,
    /// Parent → popup: does this popup TAKE focus when it opens (a picker,
    /// anything you work in), or leave it on its invoker (a combobox's list)?
    /// See `azul_layout::transient::transient_takes_focus`. A popup that
    /// leaves it does not autofocus its first control.
    pub takes_focus: bool,
    /// Popup → popup: the autofocus of its first control has happened. It
    /// happens ONCE per window: a click on nothing that clears focus later
    /// must not be undone by the next pass.
    pub autofocused: bool,
    /// Popup → parent: a callback that ran in this popup closed "the window"
    /// (`CallbackInfo::close_window`) - the window its node belongs to, the
    /// parent ([`post_close_owner`]). The parent's next sync takes it
    /// ([`SyncOutcome::close_owner`]).
    pub close_owner: bool,
    /// Popup → parent: the threads a callback that ran in this popup started
    /// (`CallbackInfo::add_thread`) - the OWNER's work, on the owner's data,
    /// which must outlive the popup ([`post_owner_thread`]). The parent's
    /// next sync adopts them ([`SyncOutcome::threads`]).
    pub owner_threads: Vec<(ThreadId, Thread)>,
}

/// One keyboard transition a parent received while its popup held the
/// keyboard. X11 never gives an override-redirect popup the input focus, so
/// every key lands in the parent; the parent hands it over here instead of
/// running its own defaults on it (report 2: arrows moved the parent's focus
/// onto a control hidden under the colour picker, and the picker never saw a
/// key). Wayland does the same forwarding in its `handle_key`; macOS and
/// Win32 make the popup the key window, so their parents never see the key.
#[derive(Debug, Clone, PartialEq)]
pub struct ForwardedKey {
    /// The parent's whole keyboard state AFTER the transition. The popup
    /// adopts it, so its own state diff yields the KeyDown / KeyUp /
    /// ModifiersChanged - the same events it would have seen as the key
    /// window, modifiers included.
    pub keyboard: azul_core::window::KeyboardState,
    /// The key the parent's PREVIOUS state held as current. The popup's diff
    /// starts from it, so it sees exactly the transition the parent saw - an
    /// auto-repeat included, which the backends express as `None` -> the key
    /// (X11 clears the previous key for a repeat; without this a held arrow
    /// would nudge the picker once).
    pub previous_key: azul_core::window::OptionVirtualKeyCode,
    /// Text the key typed, as the parent's input method resolved it.
    pub text: Option<String>,
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

/// Did the parent open this window from a KEYBOARD-indicated focus?
///
/// Reads [`TransientWindowData::focus_visible`] out of the popup's own
/// mailbox; `false` for a window that is not a transient at all.
#[must_use]
pub fn opened_with_visible_focus(state: &FullWindowState) -> bool {
    mailbox_of(state)
        .and_then(|m| read(&m, |d| d.focus_visible))
        .unwrap_or(false)
}

/// Does this popup take focus when it opens? `true` for a window that is not
/// a transient at all. See [`TransientWindowData::takes_focus`].
#[must_use]
pub fn popup_takes_focus(state: &FullWindowState) -> bool {
    mailbox_of(state)
        .and_then(|m| read(&m, |d| d.takes_focus))
        .unwrap_or(true)
}

/// Has this popup already autofocused its first control? `true` for a
/// window that is not a transient (nothing to autofocus there).
#[must_use]
pub fn popup_autofocused(state: &FullWindowState) -> bool {
    mailbox_of(state)
        .and_then(|m| read(&m, |d| d.autofocused))
        .unwrap_or(true)
}

/// Record that this popup's one autofocus has happened.
pub fn mark_popup_autofocused(state: &FullWindowState) {
    if let Some(m) = mailbox_of(state) {
        write(&m, |d| d.autofocused = true);
    }
}

/// The mailbox of the popup that holds `lw`'s keyboard (see
/// `LayoutWindow::transient_keyboard_owner`), once its window exists.
#[must_use]
pub fn keyboard_owner_mailbox(lw: &LayoutWindow) -> Option<RefAny> {
    match &lw.transient_keyboard_owner()?.surface {
        OptionRefAny::Some(m) => Some(m.clone()),
        OptionRefAny::None => None,
    }
}

/// The mailbox of the open popup that leaves focus on its invoker (a
/// combobox's list, see `LayoutWindow::transient_list_popup`), once its
/// window exists.
#[must_use]
pub fn list_popup_mailbox(lw: &LayoutWindow) -> Option<RefAny> {
    match &lw.transient_list_popup()?.surface {
        OptionRefAny::Some(m) => Some(m.clone()),
        OptionRefAny::None => None,
    }
}

/// The keys that belong to an open LIST popup even though its invoker keeps
/// focus: the ones that walk a list and pick from it. Everything else - the
/// text, Backspace, the caret keys Left / Right - keeps editing the field.
#[must_use]
pub const fn is_list_navigation_key(key: VirtualKeyCode) -> bool {
    matches!(
        key,
        VirtualKeyCode::Up
            | VirtualKeyCode::Down
            | VirtualKeyCode::PageUp
            | VirtualKeyCode::PageDown
            | VirtualKeyCode::Home
            | VirtualKeyCode::End
            | VirtualKeyCode::Return
            | VirtualKeyCode::NumpadEnter
    )
}

/// Where a keyboard transition the PARENT received goes while popups are
/// open - the one routing rule `PlatformWindow::forward_keys_to_popup`
/// applies on every backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParentKeyRoute {
    /// The parent's own key: it runs through the parent's pass.
    Parent,
    /// The key of the popup that holds the keyboard (a picker): forwarded
    /// with the text it typed; the parent's pass does not see it.
    KeyboardOwner,
    /// A key of the open LIST popup (a combobox's options): forwarded; the
    /// field keeps its focus and every other key.
    ListPopup,
}

/// Decides [`ParentKeyRoute`] for one transition of `key`, given whether a
/// focus-taking popup (`keyboard_owner`) and a list popup (`list_popup`) are
/// open with a window, and whether this backend gives a focus-taking popup
/// its keys by itself (`PlatformWindow::popups_route_keys_natively`).
///
/// `native` only concerns the focus-taking popup: where it is the key /
/// active window (macOS, Win32, Wayland's grab) a key that reached the
/// parent was typed INTO the parent. A list popup is never the key window
/// on any backend - its invoker keeps focus - so its navigation keys always
/// come through here.
#[must_use]
pub fn parent_key_route(
    keyboard_owner: bool,
    list_popup: bool,
    native: bool,
    key: Option<VirtualKeyCode>,
) -> ParentKeyRoute {
    if keyboard_owner {
        return if native {
            ParentKeyRoute::Parent
        } else {
            ParentKeyRoute::KeyboardOwner
        };
    }
    if list_popup && key.is_some_and(is_list_navigation_key) {
        return ParentKeyRoute::ListPopup;
    }
    ParentKeyRoute::Parent
}

/// Parent side: hand one keyboard transition to the popup behind `mailbox`,
/// which replays it on its next pass ([`take_forwarded_keys`]). Returns
/// whether the mailbox took it - a popup the parent already closed takes
/// nothing.
pub fn forward_key(mailbox: &RefAny, key: ForwardedKey) -> bool {
    if read(mailbox, |d| d.closed).unwrap_or(true) {
        return false;
    }
    write(mailbox, |d| d.forwarded_keys.push(key))
}

/// Popup side: the keys the parent forwarded since the last call, oldest
/// first. Empty for a window that is not a transient.
pub fn take_forwarded_keys(state: &FullWindowState) -> Vec<ForwardedKey> {
    let Some(m) = mailbox_of(state) else {
        return Vec::new();
    };
    let mut taken = Vec::new();
    write(&m, |d| taken = core::mem::take(&mut d.forwarded_keys));
    taken
}

/// Popup side: are there forwarded keys this popup has not replayed yet?
#[must_use]
pub fn has_forwarded_keys(state: &FullWindowState) -> bool {
    mailbox_of(state)
        .and_then(|m| read(&m, |d| !d.forwarded_keys.is_empty()))
        .unwrap_or(false)
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
    focus_visible: bool,
    window: Option<WindowToScreen>,
) -> (WindowCreateOptions, RefAny) {
    let size = open.content_size;
    let floating = open.placement.floating(cursor);
    let origin = open.placement.resolve_within(
        size,
        cursor,
        placement_bounds(window, parent.size.dimensions, floating.anchor),
    );
    let screen_scale = window.map_or(1.0, |w| w.scale);

    let mailbox = RefAny::new(TransientWindowData {
        parent_window_id,
        content_dom: open.content_dom,
        placement: open.placement,
        floating,
        screen_scale,
        content_size: size,
        content,
        generation: 0,
        closed: false,
        dismissed: false,
        origin,
        torn: false,
        focus_visible,
        drag: None,
        drop: None,
        following: false,
        forwarded_keys: Vec::new(),
        takes_focus: true,
        autofocused: false,
        close_owner: false,
        owner_threads: Vec::new(),
    });

    // Titled as its parent until the caller knows better: a popup is part
    // of the window that opened it (the sync titles it by its node).
    let mut window_state = popup_window_state(
        parent.title.as_str(),
        "azul-transient",
        size,
        relative_position(origin, screen_scale),
    );
    window_state.size.dpi = parent.size.dpi;
    window_state.mode = parent.mode;
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
        // Seeded in the parent's mode, not the desktop's: the window is
        // cleared to that mode's background before its first frame, so a
        // dialog over a dark window does not flash white.
        mode: azul_core::window::OptionDarkLightMode::Some(parent.mode),
        create_callback: None.into(),
        hot_reload: false,
        owned: false,
        parent_window_id,
        background_color_light: azul_css::props::basic::OptionColorU::None,
        background_color_dark: azul_css::props::basic::OptionColorU::None,
    };
    (options, mailbox)
}

/// Build the window for a torn-off transient window. It is a `Menu`-type,
/// borderless, `RelativeToParentWindow`-positioned popup — the SAME window a
/// picker's popover uses — that merely happens to be `torn`. That is the one
/// path every backend already renders correctly: per-pixel alpha (so rounded
/// corners are real, not a black rectangle) and parent-relative positioning
/// (so it lands under the cursor, not at some absolute-coordinate offset). A
/// `Normal` toplevel got neither on macOS. The same mailbox layout callback
/// as a popup: it is still the one subtree.
#[must_use]
pub fn toplevel_create_options(
    parent_window_id: u64,
    parent: &FullWindowState,
    open: &OpenTransientWindow,
    content: Dom,
    title: &str,
    focus_visible: bool,
    window: Option<WindowToScreen>,
) -> (WindowCreateOptions, RefAny) {
    let size = open.content_size;
    let origin = open
        .torn
        .unwrap_or_else(|| open.placement.resolve(size, None));
    let screen_scale = window.map_or(1.0, |w| w.scale);

    let mailbox = RefAny::new(TransientWindowData {
        parent_window_id,
        content_dom: open.content_dom,
        placement: open.placement,
        floating: open.placement.floating(None),
        screen_scale,
        content_size: size,
        content,
        generation: 0,
        closed: false,
        dismissed: false,
        origin,
        torn: true,
        focus_visible,
        drag: None,
        drop: None,
        // Set true by the parent's drag handler on the first move after an
        // inline tear, so the proxy follows the cursor from drag start.
        following: false,
        forwarded_keys: Vec::new(),
        takes_focus: true,
        autofocused: false,
        close_owner: false,
        owner_threads: Vec::new(),
    });

    // A torn-off panel is exactly the popover the picker uses, only `torn`:
    // borderless, parent-owned, positioned RELATIVE to the parent (the model
    // that survives Wayland and lands under the cursor), and — through the
    // Menu window type — given per-pixel alpha by every backend, so a
    // Transparent panel keeps its real rounded corners instead of a black
    // rectangle. A `Normal` toplevel at an absolute position got neither.
    let mut window_state = popup_window_state(
        title,
        "azul-transient-torn",
        size,
        relative_position(origin, screen_scale),
    );
    window_state.size.dpi = parent.size.dpi;
    window_state.mode = parent.mode;
    window_state.flags.background_material = open.placement.material;
    // Unlike a menu/picker popup, a torn-off panel is a window the user parks
    // and works next to — it must not sit permanently above every other
    // window. (It stays borderless + parent-relative, just not pinned.)
    window_state.flags.is_always_on_top = false;
    window_state.layout_callback = LayoutCallback {
        cb: transient_layout_callback,
        ctx: OptionRefAny::Some(mailbox.clone()),
    };

    let options = WindowCreateOptions {
        window_state,
        size_to_content: false,
        renderer: None.into(),
        // Seeded in the parent's mode, not the desktop's: the window is
        // cleared to that mode's background before its first frame, so a
        // dialog over a dark window does not flash white.
        mode: azul_core::window::OptionDarkLightMode::Some(parent.mode),
        create_callback: None.into(),
        hot_reload: false,
        owned: false,
        parent_window_id,
        background_color_light: azul_css::props::basic::OptionColorU::None,
        background_color_dark: azul_css::props::basic::OptionColorU::None,
    };
    (options, mailbox)
}

/// Where a popup anchored at `anchor` (parent coordinates) may go, for a parent
/// of `parent_size` that `window` maps onto the screen: the work area of the
/// screen the anchor is on (a popup is its own window and may hang out of a
/// short parent, but not off the screen). See [`popup_bounds`].
fn placement_bounds(
    window: Option<WindowToScreen>,
    parent_size: LogicalSize,
    anchor: LogicalRect,
) -> LogicalRect {
    popup_bounds(&screens_seen_from(window), parent_size, anchor)
}

/// Where a popup anchored at `anchor` may go, given every screen in the
/// parent's own coordinates: the work area of the screen the ANCHOR is on,
/// which for a parent spanning two monitors need not be the one the parent is
/// mostly on. Without any screen to go by - Wayland, which never tells - the
/// parent's own rect is the best available bound; the compositor's positioner
/// does its own constraint adjustment there anyway.
fn popup_bounds(
    screens: &[ScreenRects],
    parent_size: LogicalSize,
    anchor: LogicalRect,
) -> LogicalRect {
    screen_for_anchor(anchor, screens)
        .unwrap_or_else(|| LogicalRect::new(LogicalPosition::zero(), parent_size))
}

/// How far a popup stays off the edges of the screen's work area.
const POPUP_SCREEN_MARGIN: f32 = 8.0;

/// The most room a popup anchored at `anchor` may take, for a parent of
/// `parent_size` among `screens` (both in the parent's logical px): the work
/// area of the screen the ANCHOR is on less a margin on every side, so a
/// popover too big for the screen comes up at the screen's size and scrolls
/// (the placement then keeps it on the screen). Without a screen to go by -
/// Wayland, which never tells, or a layout pass off the main thread - the width
/// is the parent's (what a popup was always measured against there) and the
/// height is not limited; the compositor's positioner keeps the popup on
/// screen there.
#[must_use]
pub fn popup_room(
    parent_size: LogicalSize,
    screens: &[ScreenRects],
    anchor: LogicalRect,
) -> LogicalSize {
    match screen_for_anchor(anchor, screens) {
        Some(wa) => LogicalSize::new(
            (wa.size.width - 2.0 * POPUP_SCREEN_MARGIN).max(1.0),
            (wa.size.height - 2.0 * POPUP_SCREEN_MARGIN).max(1.0),
        ),
        None => LogicalSize::new(parent_size.width, f32::INFINITY),
    }
}

/// Room enough for a title bar: 28 pt on macOS, about 31 px on Windows, up
/// to 46 px for a GNOME header bar.
const TITLE_BAR_ROOM: f32 = 48.0;

/// The room a popup's content has in `room`: less a title bar, when its
/// window has one.
pub fn content_room(room: LogicalSize, titled: bool) -> LogicalSize {
    if titled {
        LogicalSize::new(room.width, (room.height - TITLE_BAR_ROOM).max(1.0))
    } else {
        room
    }
}

/// Every screen, in the logical px of the window `window` maps onto the
/// screen; none without a mapping (Wayland, headless). Off the main thread on
/// macOS the screen list is empty, so a layout pass there keeps the
/// parent-rect bound.
#[must_use]
pub fn screens_seen_from(window: Option<WindowToScreen>) -> Vec<ScreenRects> {
    window.map_or_else(Vec::new, |window| {
        window.screens(&crate::desktop::display::desktop_screens())
    })
}

/// The window state every popup window starts from — a transient window
/// or a fallback (window-based) menu alike. ONE place for "what a popup is":
/// the `Menu` window type every backend already treats as borderless,
/// always-on-top and parent-owned (X11 adds override-redirect +
/// `_NET_WM_WINDOW_TYPE_POPUP_MENU`, Wayland turns it into an `xdg_popup`),
/// no decorations, not resizable, and positioned RELATIVE TO THE PARENT's
/// content origin (`position`, a `RelativeToParentWindow` offset in the
/// backend's screen units, see [`relative_position`]) — the one model that
/// works where absolute screen coordinates do not exist (Wayland) and is
/// re-resolved against the parent's live origin everywhere else.
#[must_use]
pub fn popup_window_state(
    title: &str,
    window_id: &str,
    size: LogicalSize,
    position: WindowPosition,
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
    window_state.position = position;
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

/// A popup's window state as a dialog window's (`anchor="dialog"`): the `Dialog` window type,
/// which every backend gives the system's title bar and close box and keeps above its parent
/// (a child window on macOS, an owned window on Windows, `WM_TRANSIENT_FOR` with
/// `_NET_WM_WINDOW_TYPE_DIALOG` on X11, a toplevel with a parent on Wayland) without pinning
/// it above other programs; titled by the dialog, opaque, its size the content's. X11 must
/// manage it (no override-redirect), or there is no title bar to move it by.
pub fn as_dialog_window(window_state: &mut FullWindowState, title: &str) {
    window_state.flags.window_type = WindowType::Dialog;
    window_state.flags.decorations = WindowDecorations::Normal;
    window_state.flags.is_always_on_top = false;
    window_state.flags.is_resizable = false;
    window_state.flags.background_material = WindowBackgroundMaterial::Opaque;
    window_state.title = title.into();
    window_state
        .platform_specific_options
        .linux_options
        .x11_override_redirect = false;
}

/// Whether the window of the `<transient-window>` at `node` has a title bar:
/// its `titlebar` attribute, else a dialog window (`anchor="dialog"`) has
/// one and a popover none.
pub(crate) fn transient_has_titlebar(
    styled: &azul_core::styled_dom::StyledDom,
    node: NodeId,
    anchor: TransientAnchor,
) -> bool {
    use azul_core::dom::AttributeType;
    styled
        .node_data
        .as_container()
        .get(node)
        .and_then(|nd| {
            nd.attributes().as_ref().iter().find_map(|a| match a {
                AttributeType::Titlebar(on) => Some(*on),
                _ => None,
            })
        })
        .unwrap_or(anchor == TransientAnchor::Dialog)
}

/// The window kind of a just-opened transient window: a window with a title
/// bar is a dialog window (the system's title bar, movable by it, ordered
/// with its parent); one without stays the borderless popup
/// `popup_window_state` made it.
fn style_transient_window(
    window_state: &mut FullWindowState,
    anchor: TransientAnchor,
    titled: bool,
    title: &str,
) {
    if titled {
        as_dialog_window(window_state, title);
    } else if anchor == TransientAnchor::Dialog {
        // A dialog window without its title bar: still the dialog kind (it
        // orders with its parent, the content is its whole window), only
        // borderless, as a popover is.
        as_dialog_window(window_state, title);
        window_state.flags.decorations = WindowDecorations::None;
    }
}

/// What the parent's sync step wants the shell to do afterwards.
#[derive(Debug, Default)]
pub struct SyncOutcome {
    /// Popups to create, in order.
    pub create: Vec<WindowCreateOptions>,
    /// A mailbox was written: every window must be woken to read it.
    pub wake_all: bool,
    /// A callback in one of the popups closed "the window" - the parent
    /// ([`post_close_owner`]): the shell asks the parent to close.
    pub close_owner: bool,
    /// Threads callbacks in the popups started ([`post_owner_thread`]): the
    /// shell adds them to the parent.
    pub threads: Vec<(ThreadId, Thread)>,
}

impl SyncOutcome {
    fn is_empty(&self) -> bool {
        self.create.is_empty() && !self.wake_all && !self.close_owner && self.threads.is_empty()
    }
}

/// The parent side: reconcile the engine's popup set with real windows.
///
/// 1. Popups that posted `dismissed` are closed in the manager (edge-triggered) and their
///    `Dismissed` event is queued.
/// 2. Newly opened popups get a window each.
/// 3. Still-open popups whose content or placement changed get the new content pushed.
/// 4. Closed popups' mailboxes get `closed`.
///
/// Runs after every layout pass of a window that has — or had — popups, and is
/// a no-op otherwise, so a window without `<transient-window>`s pays one empty
/// diff check per pass.
pub fn sync_parent(
    parent_window_id: u64,
    parent_state: &FullWindowState,
    lw: &mut LayoutWindow,
    window: Option<WindowToScreen>,
) -> SyncOutcome {
    let mut out = SyncOutcome::default();

    // 0. Tear-off drags that ended: the engine decides what the drop meant (dock / zone / tear off)
    //    and queues the surface changes + event.
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
        if let Some(w) = lw
            .transient_windows
            .open_windows()
            .iter()
            .find(|w| w.source_node == node)
        {
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

    // 0b. A close a callback in a popup asked for, the threads one started: the parent's
    //     (`post_close_owner`, `post_owner_thread`).
    for w in lw.transient_windows.open_windows() {
        if let OptionRefAny::Some(m) = &w.surface {
            if take_close_owner(m) {
                out.close_owner = true;
            }
            out.threads.extend(take_owner_threads(m));
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
    let mut any_dismissed = false;
    for node in dismissed {
        if let Some(closed) = lw.dismiss_transient_window(node) {
            if let OptionRefAny::Some(m) = &closed.surface {
                write(m, |d| d.closed = true);
            }
            any_dismissed = true;
            log_debug!(
                LogCategory::Window,
                "[transient] popup {:?} dismissed by the user",
                closed.content_dom
            );
        }
    }
    if any_dismissed {
        // The popup handed the keyboard back: the invoker's ring returns.
        // This runs after the pass's display list was built.
        lw.refresh_focus_ring();
    }

    // 4 (first). Closed by the app (`open=false`) or by a node going away: the manager already
    //    dropped them; their mailboxes are handed over here. BEFORE the early return below: a
    //    rebuild that drops a window's node leaves no open window and may leave no diff, and the
    //    popup was never told - a Modal whose Delete removed it from the DOM stayed open
    //    (a_modal_button_that_changes_app_state_rebuilds_its_parent_window).
    for m in lw.transient_windows.take_closed_surfaces() {
        if let OptionRefAny::Some(m) = m {
            // The answer that closed the parent also dropped its question
            // (a CloseGuard's "Don't Save"): the close is still the parent's.
            if take_close_owner(&m) {
                out.close_owner = true;
            }
            // The answer that closed the popup started work (AzPhoto's
            // export sheet): it is the parent's now.
            out.threads.extend(take_owner_threads(&m));
            if write(&m, |d| d.closed = true) {
                out.wake_all = true;
            }
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
    let root = lw
        .layout_results
        .get(&DomId::ROOT_ID)
        .map(|r| r.styled_dom.clone());
    let open: Vec<OpenTransientWindow> = lw.transient_windows.open_windows().to_vec();
    for w in open {
        if w.is_inline() {
            // Inline content of its parent / zone: laid out by the parent's
            // own pass, no surface of its own. (Its torn-off form is a
            // toplevel like any other, created below once `torn` is set.)
            continue;
        }
        let Some(styled) = root.as_ref() else { break };
        let Some(mut content) =
            azul_core::transient::extract_subtree_as_dom(styled, w.source_node)
        else {
            continue;
        };
        // Laid out in its window as it was measured: no bigger than the
        // window, the rest scrolling.
        azul_core::transient::fit_root_to(&mut content, w.content_size);
        match &w.surface {
            OptionRefAny::None => {
                // Just opened (or a surface never attached): create the
                // window - a popup on its anchor, or a toplevel if torn off.
                // Inherit the OPENER's `:focus-visible` modality: a picker
                // reached by Tab+Space rings the control it autofocuses, one
                // opened by a click does not.
                let focus_visible = lw.focus_manager.focus_is_visible;
                let title = transient_window_title(
                    styled,
                    w.source_node,
                    parent_state.title.as_str(),
                );
                let (mut options, mailbox) = if w.torn.is_some() {
                    toplevel_create_options(
                        parent_window_id,
                        parent_state,
                        &w,
                        content,
                        &title,
                        focus_visible,
                        window,
                    )
                } else {
                    popup_create_options(
                        parent_window_id,
                        parent_state,
                        &w,
                        content,
                        cursor,
                        focus_visible,
                        window,
                    )
                };
                options.window_state.title = title.as_str().into();
                if w.torn.is_none() {
                    let titled =
                        transient_has_titlebar(styled, w.source_node, w.placement.anchor);
                    style_transient_window(
                        &mut options.window_state,
                        w.placement.anchor,
                        titled,
                        &title,
                    );
                }
                // If this torn window is the panel of a live inline tear, it is
                // born FOLLOWING — mouse-transparent from its first frame — so
                // the drag the parent already owns is not interrupted the
                // instant the proxy materialises under the cursor.
                if lw.inline_tear.map(|t| t.node) == Some(w.source_node) {
                    write(&mailbox, |d| d.following = true);
                }
                // The popup's focus model, decided where the content's roles
                // are visible: a combobox's list leaves focus on the field.
                let takes_focus =
                    azul_layout::transient::transient_takes_focus(styled, w.source_node);
                write(&mailbox, |d| d.takes_focus = takes_focus);
                if let Some(slot) = lw.transient_windows.get_mut(w.content_dom) {
                    slot.surface = OptionRefAny::Some(mailbox);
                }
                if super::event::focus_trace_enabled() {
                    eprintln!(
                        "[focus] transient window created: node={:?} dom={:?} size={:?} \
                         opener_ring={focus_visible}",
                        w.source_node, w.content_dom, w.content_size
                    );
                }
                log_debug!(
                    LogCategory::Window,
                    "[transient] opening {} {:?}: {}x{} at {:?}",
                    if w.torn.is_some() {
                        "toplevel"
                    } else {
                        "popup"
                    },
                    w.content_dom,
                    w.content_size.width,
                    w.content_size.height,
                    options.window_state.position
                );
                out.create.push(options);
            }
            OptionRefAny::Some(m) => {
                let moved = diff.moved.contains(&w.content_dom);
                let changed = read(m, |d| looks_different(&d.content, &content)).unwrap_or(false);
                if !moved && !changed {
                    // The same look with the callbacks' fresh payloads: the popup lays them out
                    // on its next pass. Nothing to re-place, no window to wake.
                    write(m, |d| d.content = content);
                } else {
                    // A popup follows its anchor (the parent scrolled, the
                    // zone it docked onto moved); a torn toplevel is where
                    // the user put it and only its content refreshes.
                    let floating = w.placement.floating(cursor);
                    let origin = if w.torn.is_some() {
                        None
                    } else {
                        Some(w.placement.resolve_within(
                            w.content_size,
                            cursor,
                            placement_bounds(
                                window,
                                parent_state.size.dimensions,
                                floating.anchor,
                            ),
                        ))
                    };
                    write(m, |d| {
                        d.content = content;
                        d.placement = w.placement;
                        d.floating = floating;
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

/// Whether the popup content `new` looks different from `old`: the engine's own fingerprints
/// (structure and style, the test the window's own rebuild skips its cascade by), which read the
/// callbacks' event types but not their payloads. An app makes its payloads anew in every layout
/// (an event payload per button), so `Dom ==`, which compares payloads by identity, called every
/// rebuild a change: it woke every other window, and a dialog that owns a popup of its own woke
/// its parent, whose rebuild woke the dialog again - without end in a real window.
fn looks_different(old: &Dom, new: &Dom) -> bool {
    azul_core::diff::fingerprint_dom(old).0 != azul_core::diff::fingerprint_dom(new).0
}

/// The title of a transient window - a popup, a torn-off toplevel, a dialog
/// window: the `<transient-window>` node's `title` attribute, else its
/// accessible name (`aria-label`, what a modal names its window by), else
/// `parent_title`. A dialog the app did not name belongs to the window that
/// opened it, so it carries that window's title, as a message box does.
fn transient_window_title(
    styled: &azul_core::styled_dom::StyledDom,
    node: NodeId,
    parent_title: &str,
) -> String {
    use azul_core::dom::AttributeType;
    let nodes = styled.node_data.as_container();
    nodes
        .get(node)
        .and_then(|nd| {
            nd.attributes().as_ref().iter().find_map(|a| match a {
                AttributeType::Title(t) | AttributeType::AriaLabel(t)
                    if !t.as_str().trim().is_empty() =>
                {
                    Some(t.as_str().to_owned())
                }
                _ => None,
            })
        })
        .unwrap_or_else(|| parent_title.to_owned())
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
        /// The window position, in the backend's screen units.
        position: Option<WindowPosition>,
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
    let Some((closed, generation, origin, size, torn, dragging, following, scale)) = read(&m, |d| {
        (
            d.closed,
            d.generation,
            d.origin,
            d.content_size,
            // A dialog window, like a torn-off one, stays where the user moved it by its
            // title bar: only its size follows the content.
            d.torn || d.placement.anchor == TransientAnchor::Dialog,
            d.drag.is_some(),
            d.following,
            d.screen_scale,
        )
    }) else {
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
        let position = relative_position(origin, scale);
        if state.position != position {
            return PopupAction::Place {
                position: Some(position),
                size,
            };
        }
        return PopupAction::Nothing;
    }
    let want_pos = (!torn).then(|| relative_position(origin, scale));
    let pos_differs = want_pos.is_some_and(|p| state.position != p);
    let size_differs = state.size.dimensions != size;
    if pos_differs || size_differs {
        PopupAction::Place {
            position: want_pos,
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

/// Is this window a live drag proxy the parent is driving? A following proxy
/// must be MOUSE-TRANSPARENT: the gesture belongs to the parent (mouse-down
/// landed there), and the proxy pops up UNDER the cursor — if it ate mouse
/// events the drag would break the instant the proxy appeared, and the drop
/// would never reach the parent's zone hit-test. So events fall through it to
/// the parent, exactly as a drag ghost does. Interactive again once dropped.
#[must_use]
pub fn proxy_is_following(state: &FullWindowState) -> bool {
    mailbox_of(state)
        .and_then(|m| read(&m, |d| d.following))
        .unwrap_or(false)
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

/// The `RelativeToParentWindow` position of a popup whose top-left is `origin`
/// in its parent's logical px: the offset from the parent's content origin in
/// the backend's screen units, `screen_scale` of them per logical px (the
/// parent's DPI scale on Windows and X11, 1 on macOS and where there is no
/// screen mapping).
#[must_use]
pub fn relative_position(origin: LogicalPosition, screen_scale: f32) -> WindowPosition {
    let window = WindowToScreen::new(ScreenPoint::new(0.0, 0.0), screen_scale);
    WindowPosition::RelativeToParentWindow(window.offset_from_content(window.point(origin)))
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
    let (drag, origin, torn, screen_scale) =
        read(&m, |d| (d.drag, d.origin, d.torn, d.screen_scale))?;
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
    // The pointer moved in logical px; the window is moved in screen units.
    let (dx_screen, dy_screen) = (dx * screen_scale, dy * screen_scale);
    #[allow(clippy::cast_possible_truncation)] // whole pixels
    let position = if torn {
        match state.position {
            WindowPosition::Initialized(p) => WindowPosition::Initialized(PhysicalPosition::new(
                p.x + dx_screen.round() as i32,
                p.y + dy_screen.round() as i32,
            )),
            // No screen coordinates (Wayland): a torn toplevel cannot be
            // moved by its content; the compositor's own move does that.
            other => other,
        }
    } else {
        relative_position(new_origin, screen_scale)
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
    let local = state
        .mouse_state
        .cursor_position
        .get_position()
        .unwrap_or(LogicalPosition::zero());
    let actual = if window_follows {
        origin
    } else {
        drag.origin_at_press
    };
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
/// Escape counts for `dismiss=outside` and `dismiss=escape`; focus loss for
/// `outside` and `outside-only` (whose content answers Escape itself).
/// `dismiss=none` never dismisses — the app closes it.
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
        Some(m) => {
            let (torn, dismiss) = read(&m, |d| (d.torn, d.placement.dismiss))?;
            if torn {
                // A torn-off palette is FRAMELESS (a Menu-type drag proxy —
                // no OS close button), so Escape is the only way to close it.
                // It must NOT light-dismiss on focus loss, though: the user
                // parked it to work next to the document. Force Escape-only,
                // whatever policy the docked form carried.
                TransientDismiss::Escape
            } else {
                dismiss
            }
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
    // `outside-only`: the content answers Escape (a dialog's cancelable
    // `cancel` step), so the engine must not spend the key first.
    if fresh_escape(previous, current) && policy != TransientDismiss::OutsideOnly {
        return Some(DismissCause::Escape);
    }
    if matches!(
        policy,
        TransientDismiss::Outside | TransientDismiss::OutsideOnly
    ) && previous.window_focused
        && !current.window_focused
    {
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

/// The popup side: a callback that ran in this popup closed "the window"
/// (`CallbackInfo::close_window`). The popup only shows a subtree of its
/// owner's DOM, so that is the OWNER: post it for the parent's next sync
/// ([`SyncOutcome::close_owner`]). Returns whether this window is a popup
/// that took it - `false` for any other window, which closes itself.
pub fn post_close_owner(state: &FullWindowState) -> bool {
    mailbox_of(state).is_some_and(|m| {
        // A torn-off toplevel is a window of its own (it closes itself and
        // reports `dismissed`); a popup the parent already closed has no
        // owner to ask.
        if read(&m, |d| d.torn || d.closed).unwrap_or(true) {
            return false;
        }
        write(&m, |d| d.close_owner = true)
    })
}

/// The parent side: the close a popup posted ([`post_close_owner`]), taken
/// once.
fn take_close_owner(mailbox: &RefAny) -> bool {
    let mut asked = false;
    write(mailbox, |d| asked = core::mem::take(&mut d.close_owner));
    asked
}

/// The popup side: a callback that ran in this popup started a thread
/// (`CallbackInfo::add_thread`). Its writeback runs on the OWNER's data and
/// the popup may close before it ends - a dialog's OK closes the dialog and
/// leaves the export running - so the thread is the owner's: post it for the
/// parent's next sync ([`SyncOutcome::threads`]). Returns whether this window
/// is a popup that took it - `false` for any other window, which keeps it.
pub fn post_owner_thread(state: &FullWindowState, thread_id: ThreadId, thread: Thread) -> bool {
    mailbox_of(state).is_some_and(|m| {
        // A torn-off toplevel is a window of its own; a popup the parent
        // already closed has no owner left to take it.
        if read(&m, |d| d.torn || d.closed).unwrap_or(true) {
            return false;
        }
        write(&m, |d| d.owner_threads.push((thread_id, thread)))
    })
}

/// The parent side: the threads a popup posted ([`post_owner_thread`]).
fn take_owner_threads(mailbox: &RefAny) -> Vec<(ThreadId, Thread)> {
    let mut taken = Vec::new();
    write(mailbox, |d| taken = core::mem::take(&mut d.owner_threads));
    taken
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

/// Any mouse button held in `state`.
fn a_button_is_down(state: &FullWindowState) -> bool {
    state.mouse_state.left_down || state.mouse_state.right_down || state.mouse_state.middle_down
}

/// A fresh mouse press: no button was down, one is now.
#[must_use]
pub fn fresh_press(previous: &FullWindowState, current: &FullWindowState) -> bool {
    a_button_is_down(current) && !a_button_is_down(previous)
}

/// The press is over: a button was down, none is now.
#[must_use]
pub fn fresh_release(previous: &FullWindowState, current: &FullWindowState) -> bool {
    a_button_is_down(previous) && !a_button_is_down(current)
}

/// A fresh Escape press: Escape was not held, it is now.
#[must_use]
pub fn fresh_escape(previous: &FullWindowState, current: &FullWindowState) -> bool {
    let esc = |s: &FullWindowState| {
        s.keyboard_state
            .pressed_virtual_keycodes
            .as_ref()
            .contains(&VirtualKeyCode::Escape)
    };
    esc(current) && !esc(previous)
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
    if !fresh_escape(previous, current) {
        return false;
    }
    let targets: Vec<NodeId> = lw
        .transient_windows
        .open_windows()
        .iter()
        .filter(|w| !w.is_inline() && w.torn.is_none())
        // `none` never closes, and `outside-only` leaves Escape to the
        // content (the forwarded key reaches its handler instead).
        .filter(|w| {
            matches!(
                w.placement.dismiss,
                TransientDismiss::Outside | TransientDismiss::Escape
            )
        })
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

/// The parent side: this window is going away (its own parent's rebuild dropped it, its user
/// or its app closed it). Every popup it opened goes with it: the manager closes them all
/// ([`TransientWindowManager::close_all`]) and each popup's mailbox is told to close. A popup's
/// own popup is hoisted into the root window's children (headless `pump_children`, the
/// registries elsewhere), so nothing else would ever close it - the parent closed without
/// laying out a DOM that no longer holds it, and its mailbox was never written. Returns whether
/// a mailbox was told: the caller wakes every window so the popups read it.
///
/// [`TransientWindowManager::close_all`]: azul_layout::transient::TransientWindowManager::close_all
pub fn close_parent(lw: &mut LayoutWindow) -> bool {
    let _ = lw.transient_windows.close_all();
    let mut told = false;
    for surface in lw.transient_windows.take_closed_surfaces() {
        if let OptionRefAny::Some(m) = surface {
            told |= write(&m, |d| d.closed = true);
        }
    }
    told
}

/// The parent side: a fresh mouse press landed in the parent while popups
/// with `dismiss=outside` are open — that press is, by construction, outside
/// them. Dismisses those popups; returns whether any were.
pub fn dismiss_outside_on_press(
    previous: &FullWindowState,
    current: &FullWindowState,
    lw: &mut LayoutWindow,
) -> bool {
    if !fresh_press(previous, current) {
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
        .filter(|w| {
            matches!(
                w.placement.dismiss,
                TransientDismiss::Outside | TransientDismiss::OutsideOnly
            ) && !on_anchor(w)
        })
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

/// The parent side: this window was DEACTIVATED (the user went to another
/// window or app) while popups that leave focus on their invoker are open -
/// a combobox's list. Such a popup is never the active window, so it has no
/// focus loss of its own to close on; its parent's is the one. Dismisses
/// those with a light-dismiss policy; returns whether any were.
///
/// A popup that TAKES focus is left alone: its parent resigns the keyboard
/// TO it (macOS, Win32), which is no reason to close it - it closes on its
/// own focus loss.
pub fn dismiss_list_popups_on_deactivation(
    previous: &FullWindowState,
    current: &FullWindowState,
    lw: &mut LayoutWindow,
) -> bool {
    if !(previous.is_window_active() && !current.is_window_active()) {
        return false;
    }
    let targets: Vec<NodeId> = {
        let Some(root) = lw
            .layout_results
            .get(&DomId::ROOT_ID)
            .map(|r| &r.styled_dom)
        else {
            return false;
        };
        lw.transient_windows
            .open_windows()
            .iter()
            .filter(|w| !w.is_inline() && w.torn.is_none())
            .filter(|w| {
                matches!(
                    w.placement.dismiss,
                    TransientDismiss::Outside | TransientDismiss::OutsideOnly
                )
            })
            .filter(|w| !azul_layout::transient::transient_takes_focus(root, w.source_node))
            .map(|w| w.source_node)
            .collect()
    };
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

/// Which of a window's surfaces a Wayland `wl_keyboard.enter` / `leave`
/// names. The events for a parent AND its `xdg_popup` arrive at the parent's
/// one keyboard listener; the surface argument says which one got (or lost)
/// the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardFocusSurface {
    /// The window's own surface: the window itself was (de)activated.
    Parent,
    /// The window's open popup: the popup took (or gave up) the keyboard.
    Popup,
    /// Neither (a destroyed surface arrives as null): treated as the parent.
    Other,
}

/// Route a Wayland keyboard enter / leave by its surface. `surface`,
/// `parent` and `popup` are the `wl_surface` addresses (`popup` is `None`
/// while no popup is open).
///
/// The listener used to ignore the surface, so the popup taking the
/// keyboard read as leave(parent) + enter(parent) and the parent flickered
/// back to ACTIVE while the popup held the keyboard. Routed, the parent
/// stays inactive until the keyboard enters its own surface again - the
/// macOS / Win32 behaviour, which is what the focus ring and `:backdrop`
/// follow.
#[must_use]
pub fn keyboard_focus_surface(
    surface: usize,
    parent: usize,
    popup: Option<usize>,
) -> KeyboardFocusSurface {
    if surface == 0 {
        KeyboardFocusSurface::Other
    } else if popup == Some(surface) {
        KeyboardFocusSurface::Popup
    } else if surface == parent {
        KeyboardFocusSurface::Parent
    } else {
        KeyboardFocusSurface::Other
    }
}

/// The serial an `xdg_popup.grab` must carry: the one of the input event
/// that opened the popup. `last_input_serial` is the latest key OR button
/// serial - a keyboard-opened picker's is the key's; the pointer's own
/// serial can be an `enter` serial, which a strict compositor (Mutter)
/// answers by dismissing the popup at once. `0` = no input seen yet.
#[must_use]
pub const fn popup_grab_serial(last_input_serial: u32, pointer_serial: u32) -> u32 {
    if last_input_serial != 0 {
        last_input_serial
    } else {
        pointer_serial
    }
}

#[cfg(test)]
mod tests {
    use azul_core::{geom::LogicalRect, transient::TransientAnchor};
    use azul_layout::transient::placement_for;

    use super::*;

    fn open_window(dom: usize) -> OpenTransientWindow {
        let cfg = azul_core::transient::TransientWindowConfig::opened();
        OpenTransientWindow {
            source_node: NodeId::new(3),
            content_dom: azul_layout::transient::transient_dom_id(dom),
            placement: placement_for(
                NodeId::new(3),
                LogicalRect::new(
                    LogicalPosition::new(48.0, 48.0),
                    LogicalSize::new(60.0, 24.0),
                ),
                &cfg,
            ),
            content_size: LogicalSize::new(240.0, 160.0),
            surface: OptionRefAny::None,
            torn: None,
            anchor_override: None,
            attr_torn: false,
        }
    }

    /// A popup or dialog window starts in its parent's mode, on that mode's
    /// background. Seeded from the desktop's mode instead, a dialog opened
    /// from a dark window on a light desktop showed a white window until its
    /// first frame was drawn (the save guard's "Save changes?" flashed white).
    #[test]
    fn a_dialog_opened_from_a_dark_window_starts_on_the_dark_background() {
        use azul_core::window::{DarkLightMode, OptionDarkLightMode};
        let mut parent = FullWindowState::default();
        parent.mode = DarkLightMode::Dark;
        // SystemStyle implements Drop, so it is built and then set, not
        // filled in from a default with `..`.
        let mut light_desktop = azul_css::system::SystemStyle::default();
        light_desktop.mode = azul_css::system::DarkLightMode::Light;
        let w = open_window(0);
        let (mut opts, _mailbox) =
            popup_create_options(0xABCD, &parent, &w, Dom::create_div(), None, false, None);
        as_dialog_window(&mut opts.window_state, "Save changes?");
        assert_eq!(opts.mode, OptionDarkLightMode::Some(DarkLightMode::Dark));
        super::super::resolve_initial_background_color(&mut opts, &light_desktop);
        let dark = super::super::mode_background(
            DarkLightMode::Dark,
            &light_desktop,
            azul_css::props::basic::OptionColorU::None,
            azul_css::props::basic::OptionColorU::None,
        );
        assert_eq!(
            opts.window_state.background_color.into_option(),
            Some(dark),
            "the window is cleared to the dark background before its first frame"
        );
    }

    /// A dialog window is titled by its dialog (the `<transient-window>`
    /// node's `title` or `aria-label`), else by the window that opened it -
    /// the app's own title - and never by a made-up word ("Panel", "Popup").
    #[test]
    fn a_dialog_window_is_titled_by_its_dialog_or_else_by_its_parent_window() {
        use azul_core::{dom::AttributeType, styled_dom::StyledDom};
        let window = |label: Option<&str>| {
            let mut node = Dom::create_node(azul_core::dom::NodeType::TransientWindow(
                azul_core::transient::TransientWindowConfig::opened(),
            ));
            if let Some(l) = label {
                node = node.with_attribute(AttributeType::AriaLabel(l.into()));
            }
            StyledDom::create_from_dom(Dom::create_body().with_child(node))
        };
        let titled = window(Some("Save changes"));
        assert_eq!(
            transient_window_title(&titled, NodeId::new(1), "AzNotes"),
            "Save changes"
        );
        let untitled = window(None);
        assert_eq!(
            transient_window_title(&untitled, NodeId::new(1), "AzNotes"),
            "AzNotes",
            "an untitled dialog takes its parent window's title"
        );
    }

    /// Whether a transient window has a title bar is its node's `titlebar`
    /// attribute: a popover has none unless it asks, a dialog window one
    /// unless it says `titlebar="false"` (AzDrive's Add drive is a popover).
    #[test]
    fn a_transient_windows_title_bar_is_its_nodes_choice() {
        use azul_core::{dom::AttributeType, styled_dom::StyledDom};
        let page = |titlebar: Option<bool>| {
            let mut node = Dom::create_node(azul_core::dom::NodeType::TransientWindow(
                azul_core::transient::TransientWindowConfig::opened(),
            ));
            if let Some(t) = titlebar {
                node = node.with_attribute(AttributeType::Titlebar(t));
            }
            StyledDom::create_from_dom(Dom::create_body().with_child(node))
        };
        let decorations = |anchor: TransientAnchor, titlebar: Option<bool>| {
            let styled = page(titlebar);
            let titled = transient_has_titlebar(&styled, NodeId::new(1), anchor);
            let mut st = popup_window_state("AzDrive", "azul-transient", LogicalSize::new(200.0, 100.0), relative_position(LogicalPosition::zero(), 1.0));
            style_transient_window(&mut st, anchor, titled, "Add drive");
            st.flags.decorations
        };
        assert_eq!(decorations(TransientAnchor::Bottom, None), WindowDecorations::None, "a popover has no title bar");
        assert_eq!(decorations(TransientAnchor::Dialog, None), WindowDecorations::Normal, "a dialog window has one");
        assert_eq!(
            decorations(TransientAnchor::Dialog, Some(false)),
            WindowDecorations::None,
            "titlebar=\"false\" takes the dialog window's title bar away"
        );
        assert_eq!(
            decorations(TransientAnchor::Bottom, Some(true)),
            WindowDecorations::Normal,
            "titlebar=\"true\" gives a popover one"
        );
    }

    /// The popup window is a Menu-type, borderless, always-on-top child of
    /// the parent, exactly the engine's size, anchored below the swatch.
    #[test]
    fn the_popup_window_is_a_sized_parent_relative_menu_window() {
        let parent = FullWindowState::default();
        let w = open_window(0);
        let (opts, mailbox) =
            popup_create_options(0xABCD, &parent, &w, Dom::create_div(), None, false, None);
        assert_eq!(opts.parent_window_id, 0xABCD);
        assert!(
            !opts.size_to_content,
            "the engine measured it; no resize dance"
        );
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

    /// A popup that leaves focus on its invoker (a combobox's list) is never
    /// the key / active window, on ANY backend: the parent keeps every key
    /// (typing keeps editing the field) and hands the list its navigation
    /// keys through the mailbox. So the list route cannot depend on whether
    /// the backend gives a FOCUS-TAKING popup its keys by itself (macOS and
    /// Win32 make that one the key window; X11 never does). It used to: on
    /// macOS / Win32 every popup was the key window, the list included, and
    /// text typed while it was open went to the list and was lost.
    #[test]
    fn a_list_popup_gets_its_navigation_keys_through_the_parent_on_every_backend() {
        use azul_core::window::VirtualKeyCode::{Down, Return, A};

        for native in [false, true] {
            for key in [Down, Return] {
                assert_eq!(
                    parent_key_route(false, true, native, Some(key)),
                    ParentKeyRoute::ListPopup,
                    "{key:?} belongs to the open list, native={native}"
                );
            }
            assert_eq!(
                parent_key_route(false, true, native, Some(A)),
                ParentKeyRoute::Parent,
                "text keeps editing the field, native={native}"
            );
        }
        // A popup that TAKES focus: forwarded where it never gets the
        // keyboard itself (X11), the parent's own key where it does.
        assert_eq!(
            parent_key_route(true, false, false, Some(A)),
            ParentKeyRoute::KeyboardOwner
        );
        assert_eq!(
            parent_key_route(true, false, true, Some(A)),
            ParentKeyRoute::Parent
        );
        assert_eq!(
            parent_key_route(false, false, false, Some(Down)),
            ParentKeyRoute::Parent
        );
    }

    /// Escape dismisses under `outside` and `escape`; focus loss only under
    /// `outside`; nothing under `none`. Edges, not levels.
    #[test]
    fn dismiss_causes_follow_the_policy() {
        let mk = |dismiss: TransientDismiss| {
            let mut w = open_window(0);
            w.placement.dismiss = dismiss;
            let (opts, _) = popup_create_options(
                1,
                &FullWindowState::default(),
                &w,
                Dom::create_div(),
                None,
                false,
                None,
            );
            opts.window_state
        };
        let press_escape = |s: &mut FullWindowState| {
            s.keyboard_state.pressed_virtual_keycodes = alloc::vec![VirtualKeyCode::Escape].into();
        };

        for policy in [TransientDismiss::Outside, TransientDismiss::Escape] {
            let before = mk(policy);
            let mut after = mk(policy);
            press_escape(&mut after);
            assert_eq!(
                popup_dismiss_cause(&before, &after),
                Some(DismissCause::Escape),
                "{policy:?}"
            );
            // Held Escape is not a new press.
            let mut held = mk(policy);
            press_escape(&mut held);
            assert_eq!(popup_dismiss_cause(&held, &after), None, "{policy:?} held");
        }

        let mut focused = mk(TransientDismiss::Outside);
        focused.window_focused = true;
        let mut unfocused = mk(TransientDismiss::Outside);
        unfocused.window_focused = false;
        assert_eq!(
            popup_dismiss_cause(&focused, &unfocused),
            Some(DismissCause::FocusLost)
        );
        assert_eq!(
            popup_dismiss_cause(&unfocused, &unfocused),
            None,
            "never focused: no edge"
        );

        let mut f2 = mk(TransientDismiss::Escape);
        f2.window_focused = true;
        let mut u2 = mk(TransientDismiss::Escape);
        u2.window_focused = false;
        assert_eq!(
            popup_dismiss_cause(&f2, &u2),
            None,
            "escape-only ignores focus"
        );

        let before = mk(TransientDismiss::None);
        let mut after = mk(TransientDismiss::None);
        press_escape(&mut after);
        assert_eq!(
            popup_dismiss_cause(&before, &after),
            None,
            "none never dismisses"
        );
    }

    /// A popup reports itself dismissed through the mailbox, and the parent
    /// can see it; a closed flag travels the other way.
    /// P1-9: a keyboard enter / leave on the POPUP's surface is the popup's.
    /// The listener ignored the surface, so the popup taking the keyboard
    /// read as leave(parent) + enter(parent): the parent flickered inactive
    /// and straight back to active while the popup held the keyboard.
    #[test]
    fn a_wayland_keyboard_enter_is_routed_by_its_surface() {
        let (parent, popup) = (0x1000_usize, 0x2000_usize);
        assert_eq!(
            keyboard_focus_surface(popup, parent, Some(popup)),
            KeyboardFocusSurface::Popup,
            "the popup's surface is the popup's"
        );
        assert_eq!(
            keyboard_focus_surface(parent, parent, Some(popup)),
            KeyboardFocusSurface::Parent
        );
        assert_eq!(
            keyboard_focus_surface(parent, parent, None),
            KeyboardFocusSurface::Parent
        );
        assert_eq!(
            keyboard_focus_surface(0, parent, Some(popup)),
            KeyboardFocusSurface::Other,
            "a destroyed (null) surface is neither"
        );
        assert_eq!(
            keyboard_focus_surface(popup, parent, None),
            KeyboardFocusSurface::Other,
            "a popup that is already gone is not the popup"
        );
    }

    /// P1-9: `xdg_popup.grab` must carry the serial of the input that opened
    /// the popup - for a keyboard-opened picker that is the KEY's serial.
    /// The pointer's serial can be an `enter` serial, which a strict
    /// compositor (Mutter) rejects by dismissing the popup at once.
    #[test]
    fn a_popup_grab_carries_the_serial_of_the_last_input() {
        assert_eq!(
            popup_grab_serial(42, 7),
            42,
            "the key that opened the popup, not the pointer's older serial"
        );
        assert_eq!(
            popup_grab_serial(0, 7),
            7,
            "no input serial recorded yet: the pointer's is all there is"
        );
    }

    /// A parent window that spans two monitors opens its popover on the monitor
    /// its CONTROL is on. Bounded by the monitor the parent is mostly on, the
    /// popover was slid back across the seam, away from the control.
    #[test]
    fn a_popover_anchored_on_a_second_monitor_stays_on_that_monitor() {
        // Parent at (1000, 100) on the left monitor, 1000 wide, so its right part
        // hangs over onto the right monitor; both monitors in parent coordinates.
        let screen = |x: f32| ScreenRects {
            bounds: LogicalRect::new(
                LogicalPosition::new(x, -100.0),
                LogicalSize::new(1920.0, 1080.0),
            ),
            work_area: LogicalRect::new(
                LogicalPosition::new(x, -100.0),
                LogicalSize::new(1920.0, 1040.0),
            ),
        };
        let (left, right) = (screen(-1000.0), screen(920.0));
        let swatch = LogicalRect::new(
            LogicalPosition::new(950.0, 500.0),
            LogicalSize::new(40.0, 20.0),
        );
        let bounds = popup_bounds(&[left, right], LogicalSize::new(1000.0, 600.0), swatch);
        assert_eq!(bounds, right.work_area, "the swatch is on the right monitor");

        let picker = placement_for(
            NodeId::new(3),
            swatch,
            &azul_core::transient::TransientWindowConfig::opened(),
        );
        let origin = picker.resolve_within(LogicalSize::new(300.0, 200.0), None, bounds);
        assert_eq!(
            (origin.x, origin.y),
            (950.0, 520.0),
            "the picker opens under its swatch, not slid back onto the left monitor"
        );
    }

    /// A popup's position is an offset in the backend's SCREEN units from the
    /// parent's content origin. From a parent at twice the DPI the popup
    /// below its swatch at (48, 72) logical is 96, 144 pixels in; written
    /// as 48, 72 it opened halfway between the window's corner and its swatch.
    #[test]
    fn a_popup_on_a_scaled_window_is_offset_in_screen_pixels() {
        let parent = FullWindowState::default();
        let w = open_window(0);
        let window = WindowToScreen::new(ScreenPoint::new(500.0, 300.0), 2.0);
        let (opts, _) =
            popup_create_options(0xABCD, &parent, &w, Dom::create_div(), None, false, Some(window));
        assert_eq!(
            opts.window_state.position,
            WindowPosition::RelativeToParentWindow(PhysicalPosition::new(96, 144))
        );
        assert_eq!(
            opts.window_state.size.dimensions,
            LogicalSize::new(240.0, 160.0),
            "the size stays logical: the backend scales it like any window's"
        );
    }

    /// A popover is measured against the screen its anchor is on: for a window
    /// spanning a tall monitor and a short one, a swatch on the short one gets
    /// the short one's height as its room.
    #[test]
    fn a_popover_is_measured_against_the_screen_its_anchor_is_on() {
        let screen = |x: f32, height: f32| ScreenRects {
            bounds: LogicalRect::new(LogicalPosition::new(x, -100.0), LogicalSize::new(1920.0, height)),
            work_area: LogicalRect::new(
                LogicalPosition::new(x, -100.0),
                LogicalSize::new(1920.0, height),
            ),
        };
        let (tall, short) = (screen(-1000.0, 1440.0), screen(920.0, 900.0));
        let swatch = LogicalRect::new(LogicalPosition::new(950.0, 500.0), LogicalSize::new(40.0, 20.0));
        let room = popup_room(LogicalSize::new(1000.0, 600.0), &[tall, short], swatch);
        assert_eq!(room.height, 900.0 - 2.0 * POPUP_SCREEN_MARGIN);
    }

    /// A torn-off window follows the pointer in screen units: on a parent at
    /// twice the DPI a 20 x 10 logical drag moves it 40 x 20 pixels.
    #[test]
    fn a_torn_window_on_a_scaled_screen_follows_the_pointer() {
        let w = OpenTransientWindow {
            torn: Some(LogicalPosition::new(0.0, 0.0)),
            ..open_window(0)
        };
        let window = WindowToScreen::new(ScreenPoint::new(0.0, 0.0), 2.0);
        let (opts, mailbox) = toplevel_create_options(
            1,
            &FullWindowState::default(),
            &w,
            Dom::create_div(),
            "Palette",
            false,
            Some(window),
        );
        assert!(write(&mailbox, |d| {
            d.drag = Some(TearDrag {
                press_local: LogicalPosition::new(10.0, 10.0),
                origin_at_press: LogicalPosition::new(0.0, 0.0),
            });
        }));
        let mut state = opts.window_state;
        state.position = WindowPosition::Initialized(PhysicalPosition::new(100, 100));
        state.mouse_state.cursor_position =
            azul_core::window::CursorPosition::InWindow(LogicalPosition::new(30.0, 20.0));
        assert_eq!(
            tear_drag_move(&state, true),
            Some(WindowPosition::Initialized(PhysicalPosition::new(140, 120)))
        );
    }

    #[test]
    fn the_mailbox_carries_both_directions() {
        let w = open_window(0);
        let (opts, mailbox) = popup_create_options(
            1,
            &FullWindowState::default(),
            &w,
            Dom::create_div(),
            None,
            false,
            None,
        );
        assert_eq!(poll_popup(&opts.window_state), PopupAction::Nothing);
        assert!(post_dismissed(&opts.window_state));
        assert_eq!(read(&mailbox, |d| d.dismissed), Some(true));
        assert!(write(&mailbox, |d| d.closed = true));
        assert_eq!(poll_popup(&opts.window_state), PopupAction::Close);
        // A window that is not a popup has no mailbox and nothing to do.
        assert_eq!(
            poll_popup(&FullWindowState::default()),
            PopupAction::Nothing
        );
        assert!(!post_dismissed(&FullWindowState::default()));
    }
}
