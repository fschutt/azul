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
    geom::{LogicalPosition, LogicalRect, LogicalSize, PhysicalPosition},
    menu::{Menu, MenuPopupPosition},
    refany::RefAny,
    window::{WindowPosition, WindowType},
};
use azul_css::system::SystemStyle;
use azul_layout::window_state::{FullWindowState, WindowCreateOptions};

use crate::desktop::display::{get_display_at_point, get_primary_display};
use crate::desktop::shell2::common::debug_server::LogCategory;
use crate::log_debug;

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
                -menu_size.height, // above
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
    // Default: right-bottom
    let mut pos = LogicalPosition::new(
        trigger_abs.x + trigger_size.width,
        trigger_abs.y + trigger_size.height,
    );

    // Check right edge overflow
    if pos.x + menu_size.width > work_area.origin.x + work_area.size.width {
        // Try left-bottom instead
        pos.x = trigger_abs.x - menu_size.width;
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

    let pos = LogicalPosition::new(
        trigger_abs.x,
        trigger_abs.y + trigger_rect.size.height + y_offset,
    );

    clamp_to_work_area(pos, menu_size, work_area)
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
    let item_count = menu.items.as_slice().len().max(1);
    let estimated_size = LogicalSize::new(220.0, item_count as f32 * 28.0 + 8.0);
    let abs_cursor = cursor_position.map(|c| {
        LogicalPosition::new(parent_window_position.x + c.x, parent_window_position.y + c.y)
    });
    let menu_pos = calculate_menu_position(
        if trigger_rect.is_some() {
            MenuPopupPosition::AutoHitRect
        } else {
            MenuPopupPosition::AutoCursor
        },
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

    let mut window_state = FullWindowState::default();

    window_state.flags.window_type = WindowType::Menu;
    window_state.flags.is_always_on_top = true;
    window_state.flags.is_visible = true;
    window_state.flags.decorations = azul_core::window::WindowDecorations::None;
    window_state.flags.is_resizable = false;
    window_state.title = "Menu".into();
    window_state.window_id = "azul-menu".into();
    window_state.position = WindowPosition::Initialized(PhysicalPosition::new(
        menu_pos.x as i32,
        menu_pos.y as i32,
    ));

    window_state.layout_callback = LayoutCallback {
        cb: menu_layout_callback,
        // Carry the per-window MenuWindowData to menu_layout_callback via the callback
        // ctx (read with info.get_ctx()); the callback's `data` arg is the shared app
        // data, so the menu data cannot travel that way.
        ctx: azul_core::refany::OptionRefAny::Some(RefAny::new(menu_data)),
    };

    // A menu is a borderless, WM-unmanaged popup: declare override-redirect + a
    // WM_CLASS on the window options. The X11 backend honors x11_override_redirect
    // (creates the window frameless so the WM doesn't draw a titlebar); the
    // compositor reads the WM_CLASS + _NET_WM_WINDOW_TYPE=POPUP_MENU to classify it.
    {
        use azul_core::window::{AzStringPair, StringPairVec};
        let lin = &mut window_state.platform_specific_options.linux_options;
        lin.x11_override_redirect = true;
        lin.x11_wm_classes = StringPairVec::from_vec(vec![AzStringPair {
            key: "azul-menu".into(),
            value: "Azul".into(),
        }]);
    }

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
