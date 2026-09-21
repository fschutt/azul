//! Unified menu system using window-based approach
//!
//! Menus are implemented as regular Azul windows with:
//! - `window_type == WindowType::Menu`
//! - `size_to_content = true`
//! - Custom layout callbacks that render StyledDom
//! - RefAny data containing menu state and SystemStyle
//!
//! This approach works identically on all platforms (X11, Wayland, Windows, macOS).

use alloc::{boxed::Box, sync::Arc, vec::Vec};

use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    menu::{Menu, MenuPopupPosition},
    refany::RefAny,
};
use azul_css::system::SystemStyle;
use azul_layout::window_state::WindowCreateOptions;

use crate::{
    desktop::{
        display::{get_display_at_point, get_primary_display},
        shell2::common::debug_server::LogCategory,
    },
    log_debug,
};

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

/// Calculate optimal menu position based on MenuPopupPosition strategy
///
/// Algorithm depends on the position strategy:
/// - AutoCursor/AutoHitRect: Tries right-bottom, then flips on overflow
/// - Explicit positions: Uses specified direction, clamps on overflow
pub(crate) fn calculate_menu_position(
    position_strategy: MenuPopupPosition,
    cursor_pos: Option<LogicalPosition>,
    trigger_rect: Option<LogicalRect>,
    menu_size: LogicalSize,
    parent_window_pos: LogicalPosition,
) -> LogicalPosition {
    // Get display containing the trigger point or cursor
    let reference_point = cursor_pos
        .or_else(|| {
            trigger_rect.map(|r| {
                LogicalPosition::new(
                    parent_window_pos.x + r.origin.x + r.size.width / 2.0,
                    parent_window_pos.y + r.origin.y + r.size.height / 2.0,
                )
            })
        })
        .unwrap_or(parent_window_pos);

    let display = match get_display_at_point(reference_point).or_else(get_primary_display) {
        Some(d) => d,
        None => return LogicalPosition::new(0.0, 0.0),
    };

    let work_area = display.work_area;

    match position_strategy {
        MenuPopupPosition::AutoCursor => {
            calculate_auto_position(
                cursor_pos.unwrap_or(reference_point),
                menu_size,
                work_area,
                true, // prefer_right
                true, // prefer_bottom
            )
        }
        MenuPopupPosition::AutoHitRect => {
            let rect = trigger_rect
                .unwrap_or_else(|| LogicalRect::new(reference_point, LogicalSize::new(1.0, 1.0)));
            let trigger_abs = LogicalPosition::new(
                parent_window_pos.x + rect.origin.x,
                parent_window_pos.y + rect.origin.y,
            );
            calculate_auto_position_from_rect(trigger_abs, rect.size, menu_size, work_area)
        }
        MenuPopupPosition::BottomRightOfCursor => {
            position_relative_to_cursor(
                cursor_pos.unwrap_or(reference_point),
                menu_size,
                work_area,
                true, // right
                true, // bottom
            )
        }
        MenuPopupPosition::BottomLeftOfCursor => {
            position_relative_to_cursor(
                cursor_pos.unwrap_or(reference_point),
                menu_size,
                work_area,
                false, // left
                true,  // bottom
            )
        }
        MenuPopupPosition::TopRightOfCursor => {
            position_relative_to_cursor(
                cursor_pos.unwrap_or(reference_point),
                menu_size,
                work_area,
                true,  // right
                false, // top
            )
        }
        MenuPopupPosition::TopLeftOfCursor => {
            position_relative_to_cursor(
                cursor_pos.unwrap_or(reference_point),
                menu_size,
                work_area,
                false, // left
                false, // top
            )
        }
        MenuPopupPosition::BottomOfHitRect => {
            position_relative_to_rect(
                parent_window_pos,
                trigger_rect.unwrap_or_else(|| {
                    LogicalRect::new(reference_point, LogicalSize::new(1.0, 1.0))
                }),
                menu_size,
                work_area,
                0.0, // below
            )
        }
        MenuPopupPosition::TopOfHitRect => {
            position_relative_to_rect(
                parent_window_pos,
                trigger_rect.unwrap_or_else(|| {
                    LogicalRect::new(reference_point, LogicalSize::new(1.0, 1.0))
                }),
                menu_size,
                work_area,
                // ABOVE the trigger means clearing it: the offset is measured
                // from the below-the-trigger position, so it has to undo the
                // trigger's own height as well. Without it the menu's bottom
                // sat on the trigger's bottom and covered the control whole.
                -(menu_size.height + trigger_rect.map_or(1.0, |r| r.size.height)),
            )
        }
        MenuPopupPosition::RightOfHitRect => position_submenu_right(
            parent_window_pos,
            trigger_rect
                .unwrap_or_else(|| LogicalRect::new(reference_point, LogicalSize::new(1.0, 1.0))),
            menu_size,
            work_area,
        ),
        MenuPopupPosition::LeftOfHitRect => position_submenu_left(
            parent_window_pos,
            trigger_rect
                .unwrap_or_else(|| LogicalRect::new(reference_point, LogicalSize::new(1.0, 1.0))),
            menu_size,
            work_area,
        ),
    }
}

/// Auto-position menu relative to cursor with overflow detection
fn calculate_auto_position(
    cursor_pos: LogicalPosition,
    menu_size: LogicalSize,
    work_area: LogicalRect,
    prefer_right: bool,
    prefer_bottom: bool,
) -> LogicalPosition {
    let mut pos = cursor_pos;

    // Try preferred horizontal direction
    if prefer_right {
        pos.x = cursor_pos.x;
        if pos.x + menu_size.width > work_area.origin.x + work_area.size.width {
            // Flip to left
            pos.x = cursor_pos.x - menu_size.width;
        }
    } else {
        pos.x = cursor_pos.x - menu_size.width;
        if pos.x < work_area.origin.x {
            // Flip to right
            pos.x = cursor_pos.x;
        }
    }

    // Try preferred vertical direction
    if prefer_bottom {
        pos.y = cursor_pos.y;
        if pos.y + menu_size.height > work_area.origin.y + work_area.size.height {
            // Flip to top
            pos.y = cursor_pos.y - menu_size.height;
        }
    } else {
        pos.y = cursor_pos.y - menu_size.height;
        if pos.y < work_area.origin.y {
            // Flip to bottom
            pos.y = cursor_pos.y;
        }
    }

    // Final clamp to work area
    clamp_to_work_area(pos, menu_size, work_area)
}

/// Auto-position menu below/right of rect with overflow detection
fn calculate_auto_position_from_rect(
    trigger_abs: LogicalPosition,
    trigger_size: LogicalSize,
    menu_size: LogicalSize,
    work_area: LogicalRect,
) -> LogicalPosition {
    // LEFT edges aligned, below the control — the `<select>` law, and what
    // every menu bar on every desktop does. Hanging the menu off the trigger's
    // bottom-RIGHT corner put it one whole trigger width to the right of the
    // control it belongs to; Wayland's `menu_edge_for` was fixed for exactly
    // this and the self-placing backends were left behind.
    let mut pos = LogicalPosition::new(trigger_abs.x, trigger_abs.y + trigger_size.height);

    // Too wide to stay inside: align the RIGHT edges instead, which is still
    // attached to the control. Jumping to its left is what a submenu does.
    if pos.x + menu_size.width > work_area.origin.x + work_area.size.width {
        pos.x = trigger_abs.x + trigger_size.width - menu_size.width;
    }

    // Check bottom edge overflow
    if pos.y + menu_size.height > work_area.origin.y + work_area.size.height {
        // Try top instead
        pos.y = trigger_abs.y - menu_size.height;
    }

    // Final clamp
    clamp_to_work_area(pos, menu_size, work_area)
}

/// Position menu relative to cursor in specified direction
fn position_relative_to_cursor(
    cursor_pos: LogicalPosition,
    menu_size: LogicalSize,
    work_area: LogicalRect,
    right: bool,
    bottom: bool,
) -> LogicalPosition {
    let x = if right {
        cursor_pos.x
    } else {
        cursor_pos.x - menu_size.width
    };

    let y = if bottom {
        cursor_pos.y
    } else {
        cursor_pos.y - menu_size.height
    };

    clamp_to_work_area(LogicalPosition::new(x, y), menu_size, work_area)
}

/// Position menu relative to rect (above/below)
fn position_relative_to_rect(
    parent_window_pos: LogicalPosition,
    trigger_rect: LogicalRect,
    menu_size: LogicalSize,
    work_area: LogicalRect,
    y_offset: f32,
) -> LogicalPosition {
    let trigger_abs = LogicalPosition::new(
        parent_window_pos.x + trigger_rect.origin.x,
        parent_window_pos.y + trigger_rect.origin.y,
    );

    let below = trigger_abs.y + trigger_rect.size.height;
    let above = trigger_abs.y - menu_size.height;
    let mut y = below + y_offset;

    // A menu with no room where it asked to go FLIPS to the other side of its
    // trigger. Clamping alone slid it back up OVER the control that opened it
    // — the one place a menu may never be.
    let wa_top = work_area.origin.y;
    let wa_bottom = work_area.origin.y + work_area.size.height;
    if y + menu_size.height > wa_bottom && above >= wa_top {
        y = above;
    } else if y < wa_top && below + menu_size.height <= wa_bottom {
        y = below;
    }

    clamp_to_work_area(
        LogicalPosition::new(trigger_abs.x, y),
        menu_size,
        work_area,
    )
}

/// Position submenu to the right of menu item (typical for submenus)
fn position_submenu_right(
    parent_window_pos: LogicalPosition,
    trigger_rect: LogicalRect,
    menu_size: LogicalSize,
    work_area: LogicalRect,
) -> LogicalPosition {
    let trigger_abs = LogicalPosition::new(
        parent_window_pos.x + trigger_rect.origin.x,
        parent_window_pos.y + trigger_rect.origin.y,
    );

    let mut pos = LogicalPosition::new(
        trigger_abs.x + trigger_rect.size.width,
        trigger_abs.y, // Align top of submenu with menu item
    );

    // If overflows right, try left instead
    if pos.x + menu_size.width > work_area.origin.x + work_area.size.width {
        pos.x = trigger_abs.x - menu_size.width;
    }

    clamp_to_work_area(pos, menu_size, work_area)
}

/// Position submenu to the left of menu item
fn position_submenu_left(
    parent_window_pos: LogicalPosition,
    trigger_rect: LogicalRect,
    menu_size: LogicalSize,
    work_area: LogicalRect,
) -> LogicalPosition {
    let trigger_abs = LogicalPosition::new(
        parent_window_pos.x + trigger_rect.origin.x,
        parent_window_pos.y + trigger_rect.origin.y,
    );

    let mut pos = LogicalPosition::new(trigger_abs.x - menu_size.width, trigger_abs.y);

    // If overflows left, try right instead
    if pos.x < work_area.origin.x {
        pos.x = trigger_abs.x + trigger_rect.size.width;
    }

    clamp_to_work_area(pos, menu_size, work_area)
}

/// Clamp position to work area bounds
fn clamp_to_work_area(
    pos: LogicalPosition,
    menu_size: LogicalSize,
    work_area: LogicalRect,
) -> LogicalPosition {
    LogicalPosition::new(
        pos.x
            .max(work_area.origin.x)
            .min(work_area.origin.x + work_area.size.width - menu_size.width),
        pos.y
            .max(work_area.origin.y)
            .min(work_area.origin.y + work_area.size.height - menu_size.height),
    )
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
    let menu = match probe.downcast_ref::<MenuWindowData>() {
        Some(d) => d.menu.clone(),
        None => {
            crate::log_debug!(
                LogCategory::Callbacks,
                "[menu_layout_callback] ctx is not MenuWindowData"
            );
            return azul_core::dom::Dom::create_body();
        }
    };

    let system_style = &*info.get_system_style();
    crate::desktop::menu_renderer::create_menu_dom_with_css(&menu, system_style, menu_refany)
}

/// Show a menu at a specific position by creating a new menu window.
///
/// Main entry point for context menus, dropdown menus, etc.
/// Returns `WindowCreateOptions` to pass to `CallbackInfo::create_window()`.
pub fn show_menu(
    menu: Menu,
    system_style: Arc<SystemStyle>,
    parent_window_position: LogicalPosition,
    trigger_rect: Option<LogicalRect>,
    cursor_position: Option<LogicalPosition>,
    parent_menu_id: Option<u64>,
) -> WindowCreateOptions {
    // Position the popup at the cursor / trigger with edge-flip + work-area clamp
    // (was hard-coded to (0,0), so menus opened in the top-left corner).
    // size_to_content later resizes the WINDOW to its true content; this size
    // estimate only drives the flip/clamp so the menu opens at the cursor and
    // stays on-screen. cursor_position is parent-window-relative, but
    // calculate_menu_position wants an absolute (screen) cursor, so offset it by
    // the parent's position. (DPI=1 assumption: logical ~= physical; HiDPI
    // repositioning is a follow-up.)
    // The desktop's own metrics, and the same ones the menu stylesheet is
    // built from — not a second set of literals (220 x items*28) that agreed
    // with neither the stylesheet nor the Wayland estimate.
    let estimated_size = crate::desktop::menu_renderer::MenuMetrics::from_system_style(
        &system_style,
    )
    .estimate_menu_size(&menu);
    let abs_cursor = cursor_position.map(|c| {
        LogicalPosition::new(
            parent_window_position.x + c.x,
            parent_window_position.y + c.y,
        )
    });
    let menu_pos = calculate_menu_position(
        resolve_position_strategy(menu.position, trigger_rect.is_some()),
        abs_cursor,
        trigger_rect,
        estimated_size,
        parent_window_position,
    );

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
    };

    // A menu is a popup window like any other (`<transient-window>` shares
    // this builder): Menu-type, borderless, always on top, parent-relative.
    // `menu_pos` is an absolute (work-area-clamped) screen position; convert
    // it to a parent-local offset so the backend re-resolves it against the
    // parent's live origin. Wayland cannot take screen coordinates at all, and
    // it degrades to monitor-relative when there is no parent.
    let mut window_state = crate::desktop::shell2::common::transient::popup_window_state(
        "Menu",
        "azul-menu",
        estimated_size,
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
        theme: None.into(),
        create_callback: None.into(),
        hot_reload: false,
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
    // because calculate_menu_position calls get_display_at_point() internally.
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

    /// A menu opened FOR A CONTROL hangs off that control's LEFT edge, below
    /// it - the `<select>` law, and what every menu bar on every desktop does.
    /// Anchoring it at the trigger's bottom-RIGHT corner puts it one whole
    /// trigger width to the right of the control it belongs to. Wayland's
    /// `menu_edge_for` already restored this law for the compositor-placed
    /// path; the self-placing backends still get it wrong.
    #[test]
    fn a_menu_anchored_to_a_control_opens_below_its_left_edge() {
        let pos = calculate_auto_position_from_rect(
            LogicalPosition::new(100.0, 100.0),
            LogicalSize::new(200.0, 30.0),
            LogicalSize::new(150.0, 200.0),
            work_area(),
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
        let pos = position_relative_to_rect(
            LogicalPosition::new(0.0, 0.0),
            LogicalRect::new(
                LogicalPosition::new(100.0, 1000.0),
                LogicalSize::new(120.0, 30.0),
            ),
            LogicalSize::new(150.0, 200.0),
            work_area(),
            0.0,
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
        let pos = position_submenu_right(
            LogicalPosition::new(0.0, 0.0),
            LogicalRect::new(
                LogicalPosition::new(1700.0, 100.0),
                LogicalSize::new(180.0, 24.0),
            ),
            LogicalSize::new(200.0, 150.0),
            work_area(),
        );
        assert_eq!((pos.x, pos.y), (1500.0, 100.0));
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
        );
        assert_eq!(
            opts.parent_window_id, MENU,
            "a menu opened FROM a menu is placed against that menu; without the parent id its \
             parent-relative offset is resolved against the monitor instead"
        );
    }

    /// The other half, so the two cannot be confused: the ARITHMETIC is
    /// right. Given the parent menu's own origin, a submenu lands at its
    /// item's right edge, aligned with the item's top — (962+160, 451+94).
    /// This is the value the live run should have produced.
    #[test]
    fn a_submenu_opens_at_its_items_right_edge_in_screen_coordinates() {
        let pos = position_submenu_right(
            LogicalPosition::new(962.0, 451.0),
            LogicalRect::new(
                LogicalPosition::new(0.0, 94.0),
                LogicalSize::new(160.0, 22.0),
            ),
            LogicalSize::new(160.0, 54.0),
            LogicalRect::new(
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(1920.0, 1080.0),
            ),
        );
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
