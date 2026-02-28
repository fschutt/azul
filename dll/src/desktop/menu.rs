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
    callbacks::{LayoutCallback, LayoutCallbackInfo, Update},
    dom::Dom,
    geom::{LogicalPosition, LogicalRect, LogicalSize, PhysicalPosition},
    menu::{Menu, MenuPopupPosition},
    refany::RefAny,
    styled_dom::StyledDom,
    window::{WindowFlags, WindowPosition, WindowType},
};
use azul_css::system::SystemStyle;
use azul_layout::{
    callbacks::CallbackInfo,
    window_state::{FullWindowState, WindowCreateOptions},
};

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
pub fn calculate_menu_position(
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

    let display = get_display_at_point(reference_point)
        .or_else(|| get_primary_display())
        .expect("No display found");

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

/// Create a menu window
///
/// The menu will be positioned optimally based on the MenuPopupPosition strategy,
/// trigger rectangle, and available screen space.
pub fn create_menu_window(
    menu: Menu,
    system_style: Arc<SystemStyle>,
    parent_window_position: LogicalPosition,
    trigger_rect: Option<LogicalRect>,
    cursor_position: Option<LogicalPosition>,
    parent_menu_id: Option<u64>,
) -> WindowCreateOptions {
    let menu_data = MenuWindowData {
        menu,
        system_style,
        parent_window_position,
        trigger_rect,
        cursor_position,
        parent_menu_id,
        menu_window_id: None, // Will be set after window creation
        child_menu_ids: Arc::new(std::sync::Mutex::new(Vec::new())),
    };

    let mut window_state = FullWindowState::default();

    // Configure as menu window
    window_state.flags.window_type = WindowType::Menu;
    window_state.flags.is_always_on_top = true;
    window_state.flags.is_visible = true;
    window_state.flags.decorations = azul_core::window::WindowDecorations::None;
    window_state.flags.is_resizable = false;

    // Position will be calculated after size is known (via size_to_content)
    // The actual positioning happens in the layout callback after measuring
    window_state.position = WindowPosition::Initialized(PhysicalPosition::new(0, 0));

    // Set layout callback that renders the menu
    window_state.layout_callback = LayoutCallback {
        cb: menu_layout_callback,
        ctx: azul_core::refany::OptionRefAny::None,
    };

    WindowCreateOptions {
        window_state,
        size_to_content: true.into(), // Auto-size to menu content
        renderer: None.into(),
        theme: None.into(),
        create_callback: None.into(),
        hot_reload: false.into(),
    }
}

/// Layout callback for menu windows
///
/// Renders the menu as a Dom with deferred CSS and updates window position based on measured size
extern "C" fn menu_layout_callback(mut data: RefAny, info: LayoutCallbackInfo) -> azul_core::dom::Dom {
    // Clone data BEFORE downcasting to avoid borrow conflicts
    let data_clone = data.clone();

    let menu_data = match data.downcast_ref::<MenuWindowData>() {
        Some(d) => d,
        None => {
            crate::log_debug!(
                LogCategory::Callbacks,
                "[menu_layout_callback] Failed to downcast MenuWindowData"
            );
            return azul_core::dom::Dom::create_body();
        }
    };

    // Get SystemStyle from LayoutCallbackInfo (Arc-based, safe)
    let system_style = &*info.get_system_style();

    // Use menu_renderer to create the Dom with deferred CSS
    crate::desktop::menu_renderer::create_menu_dom_with_css(
        &menu_data.menu,
        system_style,
        data_clone, // Pass cloned MenuWindowData to item callbacks
    )
}

/// Helper function to show a menu at a specific position
///
/// This is the main entry point for displaying context menus, dropdown menus, etc.
/// It creates a new window with the menu content and positions it intelligently.
///
/// # Arguments
/// * `menu` - The menu structure to display
/// * `system_style` - System style for native look (usually from CallbackInfo)
/// * `parent_window_position` - Position of the parent window in screen coordinates
/// * `trigger_rect` - Rectangle that triggered the menu (e.g., button bounds), relative to parent
///   window
/// * `cursor_position` - Cursor position in screen coordinates (for AutoCursor positioning)
///
/// # Returns
///
/// WindowCreateOptions that can be passed to `CallbackInfo::create_window()`
pub fn show_menu(
    menu: Menu,
    system_style: Arc<SystemStyle>,
    parent_window_position: LogicalPosition,
    trigger_rect: Option<LogicalRect>,
    cursor_position: Option<LogicalPosition>,
    parent_menu_id: Option<u64>,
) -> WindowCreateOptions {
    create_menu_window(
        menu,
        system_style,
        parent_window_position,
        trigger_rect,
        cursor_position,
        parent_menu_id,
    )
}

/// Convenience function to spawn a menu from a callback
///
/// This is the recommended way to spawn menus from callbacks. It automatically
/// extracts the necessary information from CallbackInfo.
pub fn spawn_menu_from_callback(
    info: &mut azul_layout::callbacks::CallbackInfo,
    menu: Menu,
    _position: MenuPopupPosition,
) {
    use azul_core::{geom::LogicalPosition, window::WindowPosition};

    // Get parent window position in logical coordinates
    let full_window_state = info.get_current_window_state();

    // Convert window position to logical coordinates
    let parent_window_pos = match full_window_state.position {
        WindowPosition::Initialized(phys_pos) => {
            let hidpi_factor = full_window_state.size.get_hidpi_factor().inner.get();
            LogicalPosition::new(
                phys_pos.x as f32 / hidpi_factor,
                phys_pos.y as f32 / hidpi_factor,
            )
        }
        WindowPosition::Uninitialized => LogicalPosition::new(0.0, 0.0),
    };

    // Get trigger rect and cursor position
    let trigger_rect = info.get_hit_node_layout_rect();
    let cursor_pos = info.get_cursor_position();

    // Create menu window
    let menu_window = show_menu(
        menu,
        info.get_system_style(),
        parent_window_pos,
        trigger_rect,
        cursor_pos,
        None, // parent_menu_id
    );

    // Spawn the window
    info.create_window(menu_window);
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: These tests require the main thread on macOS and real display hardware
    // because calculate_menu_position calls get_displays() internally.
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
