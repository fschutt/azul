//! Unified menu system using window-based approach
//!
//! Menus are implemented as regular Azul windows with:
//! - `window_type == WindowType::Menu`
//! - `size_to_content = true`
//! - Custom layout callbacks that render StyledDom
//! - RefAny data containing menu state and SystemStyle
//!
//! This approach works identically on all platforms (X11, Wayland, Windows, macOS).
//!
//! A menu window only SHOWS the menu: the item the user picks runs in the window that opened
//! it, through that window's [`MenuPicks`] mailbox, as a native menu's item does.

use alloc::{boxed::Box, sync::Arc, vec::Vec};

use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    menu::{CoreMenuCallback, Menu, MenuPopupPosition},
    refany::RefAny,
};
use azul_css::system::SystemStyle;
use azul_layout::{
    placement::{floating_frame, screen_for_anchor, AxisRule, FloatingPlacement, ScreenRects},
    window_state::WindowCreateOptions,
};

use crate::{
    desktop::{display::screen_rects, shell2::common::debug_server::LogCategory},
    log_debug,
};

/// The items picked in window-drawn menus, on their way to the window that opened them.
///
/// A native menu runs an item's callback in the window that opened it (macOS
/// `handle_menu_action`, Win32 `WM_COMMAND`, both through
/// `PlatformWindow::invoke_menu_callback`): its DOM, its window state, its timers and threads.
/// A window-drawn menu ([`show_menu`], Wayland's `create_menu_popup_options`, the headless
/// menus) is a window of its own, which closes as the item is picked - running the callback
/// THERE set a text in a field the menu does not have, the menu's title, and timers and threads
/// that died with it (AzDrive's Saved searches, 2026-10-10).
///
/// So the menu only posts the pick here ([`MenuPicks::post`]), and the window that opened the
/// menu runs it as its own (`PlatformWindow::run_menu_picks`). One mailbox per window
/// (`CommonWindowState::menu_picks`), shared by every menu it opens and by their submenus.
#[derive(Debug, Clone, Default)]
pub struct MenuPicks {
    picks: Arc<std::sync::Mutex<Vec<CoreMenuCallback>>>,
}

impl MenuPicks {
    /// A menu's item was picked: its callback is the owner's to run.
    pub fn post(&self, callback: CoreMenuCallback) {
        if let Ok(mut picks) = self.picks.lock() {
            picks.push(callback);
        }
    }

    /// Everything picked since the last call, in the order it was picked.
    #[must_use]
    pub fn take(&self) -> Vec<CoreMenuCallback> {
        self.picks
            .lock()
            .map(|mut picks| core::mem::take(&mut *picks))
            .unwrap_or_default()
    }

    /// Nothing was picked since the last [`Self::take`]: the owner's loop has nothing to run.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self.picks.lock() {
            Ok(picks) => picks.is_empty(),
            Err(_) => true,
        }
    }
}

/// Menu window data stored in RefAny
#[derive(Debug, Clone)]
pub struct MenuWindowData {
    /// The menu structure to render
    pub menu: Menu,
    /// System style for native look
    pub system_style: Arc<SystemStyle>,
    /// Parent window position (for calculating popup position)
    pub parent_window_position: LogicalPosition,
    /// Hit node rectangle (where the menu was triggered from)
    pub trigger_rect: Option<LogicalRect>,
    /// Cursor position when menu was triggered (used for AutoCursor positioning)
    pub cursor_position: Option<LogicalPosition>,
    /// Parent menu window ID (for submenus) - used to close parent when submenu closes
    pub parent_menu_id: Option<u64>,
    /// This menu window's ID (assigned after creation)
    pub menu_window_id: Option<u64>,
    /// All child submenu IDs spawned from this menu
    pub child_menu_ids: Arc<std::sync::Mutex<Vec<u64>>>,
    /// The index of the item whose submenu is currently on screen, if any.
    ///
    /// Opening a submenu creates an OS WINDOW, so it is not idempotent and
    /// the menu has to remember it already did. Shared (not cloned) with
    /// every item callback of this menu: the hover that opens a submenu and
    /// the hover that must NOT open a second one are different items of the
    /// same window. See [`should_open_submenu`].
    pub open_submenu: Arc<std::sync::Mutex<Option<usize>>>,
    /// Where this menu's picks go: the mailbox of the window that opened it, which its
    /// submenus share. See [`MenuPicks`].
    pub picks: MenuPicks,
}

impl MenuWindowData {
    /// The rule this menu is placed by, in screen coordinates: its strategy against the
    /// pointer and the control it was opened from, offset by the window that opened it.
    pub(crate) fn placement(&self) -> FloatingPlacement {
        menu_placement_on_screen(
            resolve_position_strategy(self.menu.position, self.trigger_rect.is_some()),
            self.parent_window_position,
            self.cursor_position,
            self.trigger_rect,
        )
    }
}

/// A menu opens where it ASKED to: only the `Auto*` answers hand the choice
/// back to the toolkit, and then a trigger rect means "anchor to the control"
/// and its absence "at the pointer".
///
/// `show_menu` used to overwrite every strategy with `AutoHitRect` /
/// `AutoCursor`, so a submenu's `RightOfHitRect` never survived and every
/// submenu opened UNDER its item instead of beside it.
#[must_use]
pub(crate) fn resolve_position_strategy(
    requested: MenuPopupPosition,
    has_trigger_rect: bool,
) -> MenuPopupPosition {
    match requested {
        MenuPopupPosition::AutoCursor | MenuPopupPosition::AutoHitRect => {
            if has_trigger_rect {
                MenuPopupPosition::AutoHitRect
            } else {
                MenuPopupPosition::AutoCursor
            }
        }
        explicit => explicit,
    }
}

/// Where a menu a callback opened appears, in the window's logical
/// coordinates: `requested` is the position the callback named
/// (`open_menu_at`, `open_menu_for_node`), `None` for a bare `open_menu`.
///
/// Without one the MENU's own strategy decides, as the `Menu` docs promise
/// ("uses the position specified in the menu itself"): the cursor strategies
/// - `AutoCursor`, the default, and the four `*OfCursor` - open at the
/// pointer, the hit-rect ones off the `anchor`'s bottom-left corner (where
/// `open_menu_for_node` puts a menu opened for a control), each falling back
/// to the other, and the window's origin only when there is neither. The
/// backend then places the menu's box against that point (and the anchor).
/// The shell used to answer every bare `open_menu` with (0, 0): the menu
/// sprang open in the window's top-left corner, wherever the user clicked.
#[must_use]
pub(crate) fn resolve_open_menu_position(
    requested: Option<LogicalPosition>,
    strategy: MenuPopupPosition,
    cursor: Option<LogicalPosition>,
    anchor: Option<LogicalRect>,
) -> LogicalPosition {
    if let Some(position) = requested {
        return position;
    }
    let below_anchor =
        anchor.map(|r| LogicalPosition::new(r.origin.x, r.origin.y + r.size.height));
    let point = match strategy {
        MenuPopupPosition::AutoHitRect
        | MenuPopupPosition::BottomOfHitRect
        | MenuPopupPosition::TopOfHitRect
        | MenuPopupPosition::LeftOfHitRect
        | MenuPopupPosition::RightOfHitRect => below_anchor.or(cursor),
        MenuPopupPosition::AutoCursor
        | MenuPopupPosition::BottomLeftOfCursor
        | MenuPopupPosition::BottomRightOfCursor
        | MenuPopupPosition::TopLeftOfCursor
        | MenuPopupPosition::TopRightOfCursor => cursor.or(below_anchor),
    };
    point.unwrap_or(LogicalPosition::new(0.0, 0.0))
}

/// The rule a menu is placed by (`azul_layout::placement`), from the strategy it asked
/// for. `cursor` is the pointer and `trigger` the control the menu was opened for, both in
/// the space the menu is placed in - the screen for a window-drawn menu, the parent surface
/// for a Wayland popup. A strategy that needs what the opener did not know falls back to
/// the other (a hit-rect menu without a control opens at the pointer, a cursor menu without
/// a pointer at the control's centre), and to `fallback` with neither.
///
/// The laws: a menu opened FOR A CONTROL hangs below it with the left edges lined up (the
/// `<select>` law), flips above it with no room below and lines up the right edges with no
/// room right; a submenu opens beside its item and flips to the other side; a context menu
/// opens at the pointer and flips to its left or above it; the explicit `*OfCursor` answers
/// are kept and only slid onto the screen. A menu taller than its screen is cut to it and
/// scrolls.
#[must_use]
pub(crate) fn menu_placement(
    strategy: MenuPopupPosition,
    cursor: Option<LogicalPosition>,
    trigger: Option<LogicalRect>,
    fallback: LogicalPosition,
) -> FloatingPlacement {
    let reference = cursor
        .or_else(|| {
            trigger.map(|r| {
                LogicalPosition::new(
                    r.origin.x + r.size.width / 2.0,
                    r.origin.y + r.size.height / 2.0,
                )
            })
        })
        .unwrap_or(fallback);
    let point = LogicalRect::new(cursor.unwrap_or(reference), LogicalSize::zero());
    let control = trigger.unwrap_or_else(|| LogicalRect::new(reference, LogicalSize::zero()));
    let (anchor, x, y) = match strategy {
        MenuPopupPosition::AutoCursor => (point, AxisRule::after(true), AxisRule::after(true)),
        MenuPopupPosition::BottomRightOfCursor => {
            (point, AxisRule::after(false), AxisRule::after(false))
        }
        MenuPopupPosition::BottomLeftOfCursor => {
            (point, AxisRule::before(false), AxisRule::after(false))
        }
        MenuPopupPosition::TopRightOfCursor => {
            (point, AxisRule::after(false), AxisRule::before(false))
        }
        MenuPopupPosition::TopLeftOfCursor => {
            (point, AxisRule::before(false), AxisRule::before(false))
        }
        MenuPopupPosition::AutoHitRect => (
            control,
            AxisRule::start_aligned(true),
            AxisRule::after(true),
        ),
        MenuPopupPosition::BottomOfHitRect => (
            control,
            AxisRule::start_aligned(false),
            AxisRule::after(true),
        ),
        MenuPopupPosition::TopOfHitRect => (
            control,
            AxisRule::start_aligned(false),
            AxisRule::before(true),
        ),
        MenuPopupPosition::RightOfHitRect => (
            control,
            AxisRule::after(true),
            AxisRule::start_aligned(false),
        ),
        MenuPopupPosition::LeftOfHitRect => (
            control,
            AxisRule::before(true),
            AxisRule::start_aligned(false),
        ),
    };
    FloatingPlacement::new(anchor, x, y.with_resize())
}

/// [`menu_placement`] for a menu opened from a window at `parent` (screen coordinates),
/// from the pointer and the control in that window's own coordinates.
#[must_use]
pub(crate) fn menu_placement_on_screen(
    strategy: MenuPopupPosition,
    parent: LogicalPosition,
    cursor: Option<LogicalPosition>,
    trigger: Option<LogicalRect>,
) -> FloatingPlacement {
    let on_screen = |p: LogicalPosition| LogicalPosition::new(parent.x + p.x, parent.y + p.y);
    menu_placement(
        strategy,
        cursor.map(on_screen),
        trigger.map(|r| LogicalRect::new(on_screen(r.origin), r.size)),
        parent,
    )
}

/// Where a window-drawn menu of `size` goes for `placement` (screen coordinates): against
/// the work area of the screen its anchor is on. With no screen known it opens where it
/// asks to.
#[must_use]
pub(crate) fn menu_frame(placement: &FloatingPlacement, size: LogicalSize) -> LogicalRect {
    frame_among(placement, size, &screen_rects())
}

/// [`menu_frame`] against an explicit list of screens.
fn frame_among(
    placement: &FloatingPlacement,
    size: LogicalSize,
    screens: &[ScreenRects],
) -> LogicalRect {
    match screen_for_anchor(placement.anchor, screens) {
        Some(work_area) => floating_frame(placement, size, work_area),
        None => LogicalRect::new(placement.preferred_origin(size), size),
    }
}

/// Calculate the menu position for a `MenuPopupPosition` strategy: [`menu_placement`]
/// placed by [`menu_frame`]. `cursor_pos` is on the screen, `trigger_rect` in the parent
/// window at `parent_window_pos`.
pub(crate) fn calculate_menu_position(
    position_strategy: MenuPopupPosition,
    cursor_pos: Option<LogicalPosition>,
    trigger_rect: Option<LogicalRect>,
    menu_size: LogicalSize,
    parent_window_pos: LogicalPosition,
) -> LogicalPosition {
    let trigger_abs = trigger_rect.map(|r| {
        LogicalRect::new(
            LogicalPosition::new(
                parent_window_pos.x + r.origin.x,
                parent_window_pos.y + r.origin.y,
            ),
            r.size,
        )
    });
    let placement = menu_placement(position_strategy, cursor_pos, trigger_abs, parent_window_pos);
    menu_frame(&placement, menu_size).origin
}

/// Where a window-drawn menu goes once its content has been MEASURED, in the screen
/// coordinates it was opened in: its frame by the rule it was opened with - flipped
/// above its control when the real menu is taller than the room below, cut to the screen
/// when it is taller than that. `show_menu` can only place a menu from an estimate of its
/// size; the backend lays the menu out, then places it again through this, so the
/// estimate never decides where a menu ends up. `None` when `ctx` is not a menu's.
#[must_use]
pub(crate) fn place_measured_menu(
    ctx: &azul_core::refany::OptionRefAny,
    measured: LogicalSize,
) -> Option<LogicalRect> {
    let azul_core::refany::OptionRefAny::Some(refany) = ctx else {
        return None;
    };
    let mut probe = refany.clone();
    let data = probe.downcast_ref::<MenuWindowData>()?;
    Some(measured_menu_frame(&data, measured, &screen_rects()))
}

/// [`place_measured_menu`] for a menu's data, against an explicit list of screens.
fn measured_menu_frame(
    data: &MenuWindowData,
    measured: LogicalSize,
    screens: &[ScreenRects],
) -> LogicalRect {
    frame_among(&data.placement(), measured, screens)
}

/// Layout callback for menu windows
extern "C" fn menu_layout_callback(_data: RefAny, info: LayoutCallbackInfo) -> azul_core::dom::Dom {
    // The menu's `MenuWindowData` is carried in the layout callback's `ctx` (set by
    // `show_menu`), NOT in `data`: `data` is the shared app data, common to every
    // window, so downcasting it to MenuWindowData always fails. Read the per-window
    // menu data via `info.get_ctx()`.
    let menu_refany = match info.get_ctx().into_option() {
        Some(r) => r,
        None => {
            crate::log_debug!(
                LogCategory::Callbacks,
                "[menu_layout_callback] menu window has no ctx (MenuWindowData)"
            );
            return azul_core::dom::Dom::create_body();
        }
    };
    let mut probe = menu_refany.clone();
    let (menu, picks) = match probe.downcast_ref::<MenuWindowData>() {
        Some(d) => (d.menu.clone(), d.picks.clone()),
        None => {
            crate::log_debug!(
                LogCategory::Callbacks,
                "[menu_layout_callback] ctx is not MenuWindowData"
            );
            return azul_core::dom::Dom::create_body();
        }
    };

    let system_style = &*info.get_system_style();
    crate::desktop::menu_renderer::create_menu_dom_with_css(
        &menu,
        system_style,
        menu_refany,
        &picks,
    )
}

/// Show a menu at a specific position by creating a new menu window.
///
/// Main entry point for context menus, dropdown menus, etc.
/// Returns `WindowCreateOptions` to pass to `CallbackInfo::create_window()`.
///
/// `picks` is the mailbox of the window that opens the menu (`CommonWindowState::menu_picks`;
/// a submenu passes its parent menu's): the items picked in the menu run there.
pub fn show_menu(
    menu: Menu,
    system_style: Arc<SystemStyle>,
    parent_window_position: LogicalPosition,
    trigger_rect: Option<LogicalRect>,
    cursor_position: Option<LogicalPosition>,
    parent_menu_id: Option<u64>,
    picks: MenuPicks,
) -> WindowCreateOptions {
    // The window is created at an estimate of the menu's size, from the desktop's own
    // metrics (the ones the menu stylesheet is built from), and placed by the menu's rule
    // for that size. The backend then lays the menu out and places it again for its
    // MEASURED size (`place_measured_menu`), so the estimate only decides where the menu
    // shows for the instant before that. `cursor_position` and `trigger_rect` are in the
    // parent window, at `parent_window_position` on the screen.
    let estimated_size = crate::desktop::menu_renderer::MenuMetrics::from_system_style(
        &system_style,
    )
    .estimate_menu_size(&menu);
    let menu_data = MenuWindowData {
        menu,
        system_style,
        parent_window_position,
        trigger_rect,
        cursor_position,
        parent_menu_id,
        menu_window_id: None,
        child_menu_ids: Arc::new(std::sync::Mutex::new(Vec::new())),
        open_submenu: Arc::new(std::sync::Mutex::new(None)),
        picks,
    };
    let frame = menu_frame(&menu_data.placement(), estimated_size);
    let menu_pos = frame.origin;

    // A menu is a popup window like any other (`<transient-window>` shares
    // this builder): Menu-type, borderless, always on top, parent-relative.
    // `menu_pos` is an absolute (work-area-clamped) screen position; convert
    // it to a parent-local offset so the backend re-resolves it against the
    // parent's live origin. Wayland cannot take screen coordinates at all, and
    // it degrades to monitor-relative when there is no parent.
    let mut window_state = crate::desktop::shell2::common::transient::popup_window_state(
        "Menu",
        "azul-menu",
        frame.size,
        LogicalPosition::new(
            menu_pos.x - parent_window_position.x,
            menu_pos.y - parent_window_position.y,
        ),
    );
    window_state.layout_callback = LayoutCallback {
        cb: menu_layout_callback,
        // Carry the per-window MenuWindowData to menu_layout_callback via the callback
        // ctx (read with info.get_ctx()); the callback's `data` arg is the shared app
        // data, so the menu data cannot travel that way.
        ctx: azul_core::refany::OptionRefAny::Some(RefAny::new(menu_data)),
    };

    WindowCreateOptions {
        window_state,
        size_to_content: true,
        renderer: None.into(),
        mode: None.into(),
        create_callback: None.into(),
        hot_reload: false,
        owned: false,
        // A menu opened FROM a menu is placed AGAINST that menu: the
        // `RelativeToParentWindow` offset above is measured from
        // `parent_window_position`, and the backend needs to be told which
        // window that was or it has nothing to add the offset to. X11 then
        // fell back to the MONITOR origin and a submenu's parent-local offset
        // (parent width, item y) became its screen position — measured live
        // as 160x54+160+94 for a parent menu at +962+451.
        //
        // The parent id also decides whether the menu shares the opener's X
        // display connection (`shared_parent_display`): a menu on a
        // connection of its own is drained by nobody's pump, cannot nest its
        // pointer grab inside its parent's, and puts a second connection's
        // window ids into the one registry that routes X events by id.
        //
        // A TOP-LEVEL menu still names no parent here; its spawner
        // (`show_window_based_context_menu` / `show_fallback_menu`) knows the
        // opener's id and fills it in.
        parent_window_id: parent_menu_id.unwrap_or(0),
        background_color_light: azul_css::props::basic::OptionColorU::None,
        background_color_dark: azul_css::props::basic::OptionColorU::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: These tests require the main thread on macOS and real display hardware
    // because calculate_menu_position reads the display list.
    // They are marked as #[ignore] for regular unit testing.

    #[test]
    #[ignore = "Requires main thread and real display hardware"]
    fn test_menu_position_auto_cursor_default() {
        let cursor_pos = LogicalPosition::new(100.0, 100.0);
        let menu_size = LogicalSize::new(150.0, 200.0);
        let parent_pos = LogicalPosition::new(0.0, 0.0);

        let pos = calculate_menu_position(
            MenuPopupPosition::AutoCursor,
            Some(cursor_pos),
            None,
            menu_size,
            parent_pos,
        );

        // Should be at cursor position (right-bottom default)
        assert_eq!(pos.x, 100.0);
        assert_eq!(pos.y, 100.0);
    }

    #[test]
    #[ignore = "Requires main thread and real display hardware"]
    fn test_menu_position_auto_hit_rect_default() {
        let trigger_rect = LogicalRect::new(
            LogicalPosition::new(100.0, 100.0),
            LogicalSize::new(200.0, 30.0),
        );
        let menu_size = LogicalSize::new(150.0, 200.0);
        let parent_pos = LogicalPosition::new(0.0, 0.0);

        let pos = calculate_menu_position(
            MenuPopupPosition::AutoHitRect,
            None,
            Some(trigger_rect),
            menu_size,
            parent_pos,
        );

        // Should be at right-bottom of trigger
        assert_eq!(pos.x, 300.0); // 100 + 200
        assert_eq!(pos.y, 130.0); // 100 + 30
    }

    #[test]
    #[ignore = "Requires main thread and real display hardware"]
    fn test_menu_position_overflow_right() {
        let trigger_rect = LogicalRect::new(
            LogicalPosition::new(1800.0, 100.0),
            LogicalSize::new(100.0, 30.0),
        );
        let menu_size = LogicalSize::new(150.0, 200.0);
        let parent_pos = LogicalPosition::new(0.0, 0.0);

        let pos = calculate_menu_position(
            MenuPopupPosition::AutoHitRect,
            None,
            Some(trigger_rect),
            menu_size,
            parent_pos,
        );

        // Should flip to left side (exact value depends on display bounds)
        assert!(pos.x < 1800.0);
    }

    #[test]
    #[ignore = "Requires main thread and real display hardware"]
    fn test_menu_position_overflow_bottom() {
        let trigger_rect = LogicalRect::new(
            LogicalPosition::new(100.0, 1000.0),
            LogicalSize::new(100.0, 30.0),
        );
        let menu_size = LogicalSize::new(150.0, 200.0);
        let parent_pos = LogicalPosition::new(0.0, 0.0);

        let pos = calculate_menu_position(
            MenuPopupPosition::AutoHitRect,
            None,
            Some(trigger_rect),
            menu_size,
            parent_pos,
        );

        // Should flip to top (exact value depends on display bounds)
        assert!(pos.y < 1000.0);
    }

    #[test]
    #[ignore = "Requires main thread and real display hardware"]
    fn test_submenu_positioning_right() {
        let trigger_rect = LogicalRect::new(
            LogicalPosition::new(100.0, 50.0),
            LogicalSize::new(200.0, 30.0),
        );
        let menu_size = LogicalSize::new(150.0, 200.0);
        let parent_pos = LogicalPosition::new(0.0, 0.0);

        let pos = calculate_menu_position(
            MenuPopupPosition::RightOfHitRect,
            None,
            Some(trigger_rect),
            menu_size,
            parent_pos,
        );

        // Should be to the right of the menu item
        assert_eq!(pos.x, 300.0); // 100 + 200
        assert_eq!(pos.y, 50.0); // Aligned with menu item top
    }
}

/// The placement laws a menu obeys on every backend that places its own popup
/// (X11, Windows' and macOS' fallback menus). Pure geometry: an explicit work
/// area, no display query, so these run headless.
#[cfg(test)]
mod placement_tests {
    use super::*;

    fn work_area() -> LogicalRect {
        LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(1920.0, 1080.0),
        )
    }

    /// Where a menu of `size` opened for `control` (screen coordinates) opens on the one
    /// 1920x1080 screen.
    fn menu_origin(
        strategy: MenuPopupPosition,
        control: LogicalRect,
        size: LogicalSize,
    ) -> LogicalPosition {
        let placement = menu_placement(strategy, None, Some(control), LogicalPosition::zero());
        floating_frame(&placement, size, work_area()).origin
    }

    /// A menu opened FOR A CONTROL hangs off that control's LEFT edge, below
    /// it - the `<select>` law, and what every menu bar on every desktop does.
    /// Anchoring it at the trigger's bottom-RIGHT corner puts it one whole
    /// trigger width to the right of the control it belongs to. One rule for
    /// every backend, the compositor-placed Wayland path included.
    #[test]
    fn a_menu_anchored_to_a_control_opens_below_its_left_edge() {
        let pos = menu_origin(
            MenuPopupPosition::AutoHitRect,
            LogicalRect::new(
                LogicalPosition::new(100.0, 100.0),
                LogicalSize::new(200.0, 30.0),
            ),
            LogicalSize::new(150.0, 200.0),
        );
        assert_eq!(
            (pos.x, pos.y),
            (100.0, 130.0),
            "the menu's left edge belongs on the control's left edge, not one control width right \
             of it"
        );
    }

    /// A menu that will not fit below its trigger FLIPS above it. Sliding it
    /// up instead parks it ON TOP of the control that opened it, which is the
    /// one place a menu may never be.
    #[test]
    fn a_menu_that_would_leave_the_bottom_flips_above_its_trigger() {
        let pos = menu_origin(
            MenuPopupPosition::BottomOfHitRect,
            LogicalRect::new(
                LogicalPosition::new(100.0, 1000.0),
                LogicalSize::new(120.0, 30.0),
            ),
            LogicalSize::new(150.0, 200.0),
        );
        assert_eq!(
            pos.y, 800.0,
            "a menu with no room below its trigger opens above it (trigger top 1000 - menu height \
             200), never slid down over it"
        );
    }

    /// A menu that SAID where it wants to be opens there. `Auto*` is the only
    /// answer that hands the choice back to the toolkit - so a submenu, which
    /// asks for `RightOfHitRect`, opens beside its item and not under it.
    #[test]
    fn a_menu_opens_where_it_asked_to() {
        assert_eq!(
            resolve_position_strategy(MenuPopupPosition::RightOfHitRect, true),
            MenuPopupPosition::RightOfHitRect,
            "a submenu's stated placement must survive"
        );
        assert_eq!(
            resolve_position_strategy(MenuPopupPosition::AutoCursor, true),
            MenuPopupPosition::AutoHitRect,
            "`Auto` with a trigger rect means: anchor to the trigger"
        );
        assert_eq!(
            resolve_position_strategy(MenuPopupPosition::AutoHitRect, false),
            MenuPopupPosition::AutoCursor,
            "`Auto` with no trigger rect means: at the pointer"
        );
    }

    /// The mirror law, already held: a submenu with no room on the right
    /// opens on the left of its item. Kept as the guard for the flip above.
    #[test]
    fn a_submenu_that_would_leave_the_right_edge_flips_to_the_left_of_its_item() {
        let pos = menu_origin(
            MenuPopupPosition::RightOfHitRect,
            LogicalRect::new(
                LogicalPosition::new(1700.0, 100.0),
                LogicalSize::new(180.0, 24.0),
            ),
            LogicalSize::new(200.0, 150.0),
        );
        assert_eq!((pos.x, pos.y), (1500.0, 100.0));
    }

    /// A menu is placed once from an ESTIMATE of its size and again once it has
    /// been laid out. Placed again by its rule, a menu that turned out taller
    /// than the room below its control opens above the control. Only slid back
    /// onto the screen from where the estimate had put it, it covered the
    /// control that opened it.
    #[test]
    fn a_menu_measured_taller_than_its_estimate_opens_above_its_control() {
        let screen = ScreenRects {
            bounds: work_area(),
            work_area: work_area(),
        };
        let data = MenuWindowData {
            menu: Menu {
                items: azul_core::menu::MenuItemVec::from_vec(vec![
                    azul_core::menu::MenuItem::String(azul_core::menu::StringMenuItem::create(
                        "Open".to_string().into(),
                    )),
                ]),
                position: MenuPopupPosition::AutoHitRect,
                context_mouse_btn: azul_core::window::ContextMenuMouseButton::Right,
            },
            system_style: Arc::new(azul_css::system::defaults::kde_breeze_light()),
            parent_window_position: LogicalPosition::zero(),
            trigger_rect: Some(LogicalRect::new(
                LogicalPosition::new(100.0, 900.0),
                LogicalSize::new(120.0, 30.0),
            )),
            cursor_position: None,
            parent_menu_id: None,
            menu_window_id: None,
            child_menu_ids: Arc::new(std::sync::Mutex::new(Vec::new())),
            open_submenu: Arc::new(std::sync::Mutex::new(None)),
            picks: MenuPicks::default(),
        };
        let frame = measured_menu_frame(&data, LogicalSize::new(180.0, 300.0), &[screen]);
        assert_eq!(
            (frame.origin.x, frame.origin.y),
            (100.0, 600.0),
            "300 tall does not fit below the control (930..1080) but does above it"
        );
        assert!(
            frame.origin.y + frame.size.height <= 900.0,
            "the menu must not cover the control at 900..930: {frame:?}"
        );
    }
}

/// One live window as the platform's window registry knows it: its key, the
/// key of the window it was opened from, and whether it is a menu.
///
/// A menu chain is not a list anywhere — it only exists as this parent link
/// repeated, so tearing a chain down means walking it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MenuChainLink {
    /// The registry key (X11: the window id; Windows: the HWND; macOS: the
    /// NSWindow).
    pub id: u64,
    /// The registry key of the window this one was opened from, `0` for a
    /// toplevel.
    pub parent: u64,
    /// `true` for a `WindowType::Menu` window.
    pub is_menu: bool,
}

/// Every menu window that must close because the user left the menu at
/// `from` — deepest first, so a teardown never orphans a submenu.
///
/// A menu chain lives only while the user is IN it. Clicking away, or the
/// window that owns the chain losing focus, dismisses the WHOLE chain and not
/// just the one window the event happened to reach: X11 never gives an
/// override-redirect popup the input focus, so the owning toplevel's
/// `FocusOut` is the only event that ever says "the user went somewhere
/// else", and the popup the pointer grab delivered the click to is not
/// necessarily the popup the user wants gone.
///
/// `from` may be any window of the chain — the owning toplevel, or a menu
/// inside it. Either way the walk first climbs out of the chain to the window
/// that owns it and then collects that window's menu descendants, so the
/// answer does not depend on which end the dismissal arrived at.
#[must_use]
pub(crate) fn menus_to_dismiss(links: &[MenuChainLink], from: u64) -> Vec<u64> {
    let link_of = |id: u64| links.iter().find(|l| l.id == id).copied();

    // Climb out of the chain: while the window we are standing on is a menu,
    // step to the window that opened it. `seen` bounds the walk — a registry
    // whose parent links have gone circular (a reused window id) must not
    // spin here.
    let mut root = from;
    let mut seen = Vec::new();
    while let Some(link) = link_of(root) {
        if !link.is_menu || seen.contains(&root) {
            break;
        }
        seen.push(root);
        root = link.parent;
    }

    // Then collect that window's menu descendants, breadth-first, and hand
    // them back deepest LAST reversed — so a caller closing them in order
    // closes a submenu before the menu it hangs off.
    let mut out: Vec<u64> = Vec::new();
    let mut frontier = alloc::vec![root];
    while let Some(parent) = frontier.pop() {
        for link in links.iter().filter(|l| l.is_menu && l.parent == parent) {
            if out.contains(&link.id) {
                continue;
            }
            out.push(link.id);
            frontier.push(link.id);
        }
    }
    out.reverse();
    out
}

/// May a hover on item `item` open its submenu, given that the submenu of
/// `open` is already on screen for this menu?
///
/// Opening a submenu is not idempotent — it creates an OS window — so the
/// menu has to remember that it already did. `MouseOver` is the entry event,
/// but a menu window that re-lays out (or whose hover set is rebuilt by the
/// pointer grab re-entering it) fires the entry again, and every extra firing
/// used to be another popup: one hover on a parent item produced THREE
/// identical submenu windows on X11/XFCE, each of them live, each of them
/// able to deliver its own activation for the single click the user made.
#[must_use]
pub(crate) fn should_open_submenu(open: Option<usize>, item: usize) -> bool {
    open != Some(item)
}

/// The key a window is registered under, read from the handle a callback can
/// see. The same mapping every backend's registry uses
/// (`PlatformWindow::registry_window_id`), repeated here because a callback
/// holds a `RawWindowHandle` and not the `PlatformWindow`.
#[must_use]
pub(crate) fn registry_window_id(handle: &azul_core::window::RawWindowHandle) -> u64 {
    use azul_core::window::RawWindowHandle;
    #[allow(clippy::cast_possible_truncation)]
    match handle {
        RawWindowHandle::MacOS(h) => h.ns_window as usize as u64,
        RawWindowHandle::Windows(h) => h.hwnd as usize as u64,
        RawWindowHandle::Xlib(h) => h.window,
        RawWindowHandle::Wayland(h) => h.surface as usize as u64,
        _ => 0,
    }
}

/// The laws a menu CHAIN obeys: who a menu's parent window is, when a hover
/// may open a second one, and what a dismissal takes down with it. Pure
/// bookkeeping over the window registry's own facts, so these run headless.
#[cfg(test)]
mod chain_tests {
    use alloc::vec;

    use azul_core::{
        menu::{MenuItem, MenuItemVec, MenuPopupPosition, StringMenuItem},
        window::ContextMenuMouseButton,
    };

    use super::*;

    /// The ids measured on the live X11/XFCE run (2026-09-21): the toplevel,
    /// the right-click menu it opened, and the submenu that menu opened.
    const TOPLEVEL: u64 = 0x0420_0001;
    const MENU: u64 = 0x0420_1050;
    const SUBMENU: u64 = 0x0440_0002;

    fn chain() -> Vec<MenuChainLink> {
        vec![
            MenuChainLink {
                id: TOPLEVEL,
                parent: 0,
                is_menu: false,
            },
            MenuChainLink {
                id: MENU,
                parent: TOPLEVEL,
                is_menu: true,
            },
            MenuChainLink {
                id: SUBMENU,
                parent: MENU,
                is_menu: true,
            },
        ]
    }

    fn one_item_menu() -> Menu {
        Menu {
            items: MenuItemVec::from_vec(vec![MenuItem::String(StringMenuItem::create(
                "Delete".to_string().into(),
            ))]),
            position: MenuPopupPosition::RightOfHitRect,
            context_mouse_btn: ContextMenuMouseButton::Right,
        }
    }

    /// A submenu is placed RELATIVE TO THE MENU THAT OPENED IT, so it has to
    /// name that menu as its parent window — the offset it carries is
    /// meaningless without it.
    ///
    /// On the live X11 run the parent menu sat at +962+451 and the submenu
    /// opened at +160+94: the parent-local offset (parent menu width, item y)
    /// used verbatim as a SCREEN position, because the backend had no parent
    /// to resolve it against and fell back to the monitor origin.
    #[test]
    fn a_submenu_names_the_menu_it_was_opened_from_as_its_parent_window() {
        let opts = show_menu(
            one_item_menu(),
            Arc::new(azul_css::system::defaults::kde_breeze_light()),
            LogicalPosition::new(962.0, 451.0),
            Some(LogicalRect::new(
                LogicalPosition::new(0.0, 94.0),
                LogicalSize::new(160.0, 22.0),
            )),
            None,
            Some(MENU),
            MenuPicks::default(),
        );
        assert_eq!(
            opts.parent_window_id, MENU,
            "a menu opened FROM a menu is placed against that menu; without the parent id its \
             parent-relative offset is resolved against the monitor instead"
        );
    }

    /// The item picked in a menu runs in the window that OPENED it: the menu window carries
    /// that window's mailbox, not one of its own (a submenu is opened with its parent menu's,
    /// so a whole chain posts to one window).
    #[test]
    fn a_menus_picks_go_to_the_window_that_opened_it() {
        use azul_core::{callbacks::CoreCallback, refany::OptionRefAny};

        let opener = MenuPicks::default();
        let opts = show_menu(
            one_item_menu(),
            Arc::new(azul_css::system::defaults::kde_breeze_light()),
            LogicalPosition::new(962.0, 451.0),
            Some(LogicalRect::new(
                LogicalPosition::new(0.0, 94.0),
                LogicalSize::new(160.0, 22.0),
            )),
            None,
            Some(MENU),
            opener.clone(),
        );
        let OptionRefAny::Some(mut ctx) = opts.window_state.layout_callback.ctx.clone() else {
            panic!("a menu window carries its MenuWindowData in its layout callback's ctx");
        };
        let menus_mailbox = ctx
            .downcast_ref::<MenuWindowData>()
            .map(|d| d.picks.clone())
            .expect("the ctx is the menu's MenuWindowData");

        menus_mailbox.post(CoreMenuCallback {
            refany: RefAny::new(()),
            callback: CoreCallback {
                cb: 0,
                ctx: OptionRefAny::None,
            },
        });
        assert!(
            !opener.is_empty(),
            "the pick reached the window that opened the menu"
        );
        assert_eq!(opener.take().len(), 1, "one pick, run once");
        assert!(opener.is_empty(), "a pick is taken once");
    }

    /// The other half, so the two cannot be confused: the ARITHMETIC is
    /// right. Given the parent menu's own origin, a submenu lands at its
    /// item's right edge, aligned with the item's top — (962+160, 451+94).
    /// This is the value the live run should have produced.
    #[test]
    fn a_submenu_opens_at_its_items_right_edge_in_screen_coordinates() {
        let placement = menu_placement_on_screen(
            MenuPopupPosition::RightOfHitRect,
            LogicalPosition::new(962.0, 451.0),
            None,
            Some(LogicalRect::new(
                LogicalPosition::new(0.0, 94.0),
                LogicalSize::new(160.0, 22.0),
            )),
        );
        let pos = azul_layout::placement::floating_frame(
            &placement,
            LogicalSize::new(160.0, 54.0),
            LogicalRect::new(
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(1920.0, 1080.0),
            ),
        )
        .origin;
        assert_eq!((pos.x, pos.y), (1122.0, 545.0));
    }

    /// Clicking away dismisses the CHAIN, deepest first — not only the one
    /// window the pointer grab happened to deliver the click to. On the live
    /// run a menu from an earlier right-click was still mapped alongside a
    /// new one, because nothing ever took a chain down.
    #[test]
    fn dismissing_a_chain_takes_every_menu_in_it_deepest_first() {
        assert_eq!(
            menus_to_dismiss(&chain(), TOPLEVEL),
            vec![SUBMENU, MENU],
            "the toplevel losing focus dismisses every menu it owns, submenu before parent"
        );
    }

    /// The same answer from either end: a click outside that reached the
    /// SUBMENU dismisses its parent too, because the user left the menu, not
    /// one window of it.
    #[test]
    fn dismissing_from_inside_the_chain_still_takes_the_whole_chain() {
        assert_eq!(menus_to_dismiss(&chain(), SUBMENU), vec![SUBMENU, MENU]);

        // …and the ACTIVATION path asks it. A menu item's click sets
        // `close_requested` on the window the ITEM is in - the submenu - and
        // the shell closes exactly that one, so the parent used to stay on
        // screen, mapped and grabbed, after the user had already chosen.
        // Measured live: the 160x133 parent was still there seconds after a
        // click on "Delete" in its submenu.
        let run = include_str!("shell2/run.rs");
        let close_site = run
            .split_once("if window.close_requested() {")
            .expect("the shell honours close_requested")
            .1;
        let close_site = &close_site[..close_site.find("}\n").unwrap_or(close_site.len())];
        assert!(
            close_site.contains("dismiss_chain_if_menu"),
            "closing a menu must take its chain with it, not just itself"
        );
    }

    /// A dismissal never names the window that OWNS the chain: the toplevel
    /// must survive its own menus. (The live run lost its toplevel while the
    /// process stayed alive.)
    #[test]
    fn a_dismissal_never_names_the_window_that_owns_the_chain() {
        for from in [TOPLEVEL, MENU, SUBMENU] {
            assert!(
                !menus_to_dismiss(&chain(), from).contains(&TOPLEVEL),
                "dismissing from {from:#x} must not close the toplevel"
            );
        }
    }

    /// Teardown is idempotent: once the chain is gone there is nothing left
    /// to close, so a second dismissal (a double click, or the focus change
    /// that follows the click) is a no-op rather than a second teardown of
    /// already-freed windows.
    #[test]
    fn dismissing_an_already_empty_chain_is_a_no_op() {
        let alone = vec![MenuChainLink {
            id: TOPLEVEL,
            parent: 0,
            is_menu: false,
        }];
        assert!(menus_to_dismiss(&alone, TOPLEVEL).is_empty());
        assert!(menus_to_dismiss(&[], TOPLEVEL).is_empty());
    }

    /// A hover opens a submenu ONCE. Re-entering the same item while its
    /// submenu is up opens nothing; moving to a different parent item does.
    #[test]
    fn a_hover_opens_one_submenu_and_re_hovering_the_same_item_opens_none() {
        assert!(
            should_open_submenu(None, 3),
            "nothing open yet: the hover opens the submenu"
        );
        assert!(
            !should_open_submenu(Some(3), 3),
            "item 3's submenu is already on screen; this hover must not open a second window"
        );
        assert!(
            should_open_submenu(Some(3), 5),
            "a different parent item opens its own submenu"
        );
    }
}

/// Where a menu a callback opened appears when the callback named no
/// position (`CallbackInfo::open_menu`, `TimerCallbackInfo::open_menu`).
/// Pure: the cursor and the anchor are handed in, so these run headless.
#[cfg(test)]
mod open_position_tests {
    use super::*;

    fn cursor() -> Option<LogicalPosition> {
        Some(LogicalPosition::new(300.0, 200.0))
    }

    /// `open_menu(menu)` with the menu's default strategy (`AutoCursor`):
    /// the menu opens at the pointer. It used to open at the window's
    /// top-left corner, (0, 0), wherever the user had clicked.
    #[test]
    fn a_menu_opened_without_a_position_opens_at_the_cursor() {
        let pos =
            resolve_open_menu_position(None, MenuPopupPosition::AutoCursor, cursor(), None);
        assert_eq!((pos.x, pos.y), (300.0, 200.0));
        // Every cursor-relative strategy starts from the pointer too; the
        // backend decides the side.
        for strategy in [
            MenuPopupPosition::BottomLeftOfCursor,
            MenuPopupPosition::BottomRightOfCursor,
            MenuPopupPosition::TopLeftOfCursor,
            MenuPopupPosition::TopRightOfCursor,
        ] {
            let pos = resolve_open_menu_position(None, strategy, cursor(), None);
            assert_eq!((pos.x, pos.y), (300.0, 200.0), "{strategy:?}");
        }
    }

    /// A position the callback named (`open_menu_at`, `open_menu_for_node`)
    /// is where the menu opens, whatever the menu's own strategy and
    /// wherever the pointer is.
    #[test]
    fn a_menu_opened_at_an_explicit_position_keeps_it() {
        let named = Some(LogicalPosition::new(40.0, 60.0));
        for strategy in [
            MenuPopupPosition::AutoCursor,
            MenuPopupPosition::AutoHitRect,
            MenuPopupPosition::BottomOfHitRect,
        ] {
            let pos = resolve_open_menu_position(named, strategy, cursor(), None);
            assert_eq!((pos.x, pos.y), (40.0, 60.0), "{strategy:?}");
        }
    }

    /// A hit-rect strategy hangs the menu off the anchor's bottom-left
    /// corner, as `open_menu_for_node` does; without an anchor it falls back
    /// to the pointer rather than to the window's corner.
    #[test]
    fn a_hit_rect_menu_without_a_position_hangs_off_its_anchor_or_the_cursor() {
        let anchor = Some(LogicalRect::new(
            LogicalPosition::new(100.0, 50.0),
            LogicalSize::new(120.0, 24.0),
        ));
        let pos =
            resolve_open_menu_position(None, MenuPopupPosition::AutoHitRect, cursor(), anchor);
        assert_eq!((pos.x, pos.y), (100.0, 74.0));
        let pos =
            resolve_open_menu_position(None, MenuPopupPosition::BottomOfHitRect, cursor(), None);
        assert_eq!((pos.x, pos.y), (300.0, 200.0));
    }

    /// No pointer in the window and nothing to anchor to: the corner is all
    /// that is left.
    #[test]
    fn a_menu_with_nothing_to_open_at_opens_at_the_origin() {
        let pos = resolve_open_menu_position(None, MenuPopupPosition::AutoCursor, None, None);
        assert_eq!((pos.x, pos.y), (0.0, 0.0));
    }
}
