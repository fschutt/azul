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
        // Set by the spawner (show_window_based_context_menu) which knows the
        // parent window's id; 0 here = filled in later / no parent.
        parent_window_id: 0,
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
