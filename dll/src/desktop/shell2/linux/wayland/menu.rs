//! Wayland menu handling using xdg_popup protocol
//!
//! This module provides menu popup functionality for Wayland using the xdg_popup
//! protocol. Unlike X11 which uses override_redirect windows, Wayland uses
//! xdg_popup for proper compositor-managed menu positioning and stacking.
//!
//! Architecture:
//! - Menu popups are created using WaylandPopup with xdg_popup protocol
//! - The compositor manages positioning, stacking, and automatic dismissal
//! - Menu data (Menu struct) is passed as RefAny to the layout callback
//! - Events are handled through normal Azul callback system
//! - This provides native-quality menus on Wayland

use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    menu::Menu,
    refany::RefAny,
    styled_dom::StyledDom,
};
use azul_css::system::SystemStyle;
use azul_layout::window_state::WindowCreateOptions;

use super::WaylandWindow;

/// Data passed to the menu layout callback
#[derive(Debug, Clone)]
struct MenuLayoutData {
    /// Menu structure to render
    menu: Menu,
    /// System style for native look
    system_style: SystemStyle,
}

/// Layout callback for menu popup windows
///
/// This callback uses menu_renderer to create a StyledDom from the Menu structure.
/// It's called by Azul's normal layout system, so rendering happens through the
/// standard WebRender pipeline.
extern "C" fn menu_layout_callback(data: &mut RefAny, _info: &mut LayoutCallbackInfo) -> StyledDom {
    // Clone data early to avoid borrow issues
    let data_clone = data.clone();

    // Extract menu data from RefAny
    let menu_data = match data.downcast_ref::<MenuLayoutData>() {
        Some(d) => d,
        None => {
            eprintln!("[Menu Layout] Failed to downcast menu data");
            return StyledDom::default();
        }
    };

    // Use menu_renderer to create styled DOM
    crate::desktop::menu_renderer::create_menu_styled_dom(
        &menu_data.menu,
        &menu_data.system_style,
        data_clone, // Pass cloned RefAny for menu window data
    )
}

/// Create a menu popup window using Wayland's xdg_popup protocol
///
/// This creates a proper Wayland popup with compositor-managed positioning.
/// The menu is rendered through the normal layout/rendering pipeline.
///
/// # Arguments
/// * `parent` - Parent WaylandWindow
/// * `menu` - Menu structure to display
/// * `system_style` - System style for native look
/// * `trigger_rect` - Rectangle where menu was triggered (logical coords, relative to parent)
/// * `menu_size` - Size of menu window (logical coords)
///
/// # Returns
/// * `WindowCreateOptions` - Window options for creating the popup
///
/// # Example
/// ```rust,ignore
/// use azul_core::menu::Menu;
/// use azul_css::system::SystemStyle;
///
/// // In a callback:
/// extern "C" fn on_right_click(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
///     let menu = Menu::new(vec![/* ... */]);
///     let system_style = SystemStyle::default();
///     let trigger_rect = info.get_hit_node_rect()?;
///     
///     let menu_options = create_menu_popup_options(
///         parent_window,
///         &menu,
///         &system_style,
///         trigger_rect,
///         LogicalSize::new(200.0, 300.0),
///     );
///     
///     // Create popup using WaylandPopup::new()
///     Update::DoNothing
/// }
/// ```
pub fn create_menu_popup_options(
    parent: &WaylandWindow,
    menu: &Menu,
    system_style: &SystemStyle,
    trigger_rect: LogicalRect,
    menu_size: LogicalSize,
) -> WindowCreateOptions {
    // Create menu data for layout callback
    let menu_data = MenuLayoutData {
        menu: menu.clone(),
        system_style: system_style.clone(),
    };

    let menu_data_refany = RefAny::new(menu_data);

    // Create window options
    let mut options = WindowCreateOptions::default();
    options.state.size.dimensions = menu_size;
    options.state.title = "Menu".to_string().into();

    // Set layout callback - RefAny contains menu data, callback knows how to use it
    options.state.layout_callback = LayoutCallback {
        cb: menu_layout_callback,
        callable: azul_core::refany::OptionRefAny::None,
    };

    // Store menu data in app_data (will be passed to callback)
    // Note: The app needs to ensure this RefAny is passed when creating the window

    // Set window flags for popup behavior
    options.state.flags.decorations = azul_core::window::WindowDecorations::None;
    options.state.flags.is_always_on_top = true;
    options.state.flags.is_resizable = false;

    options
}

/// Calculate menu size from Menu structure
///
/// This estimates the menu size based on the number of items and their content.
/// Used when caller doesn't specify an explicit size.
pub fn calculate_menu_size(menu: &Menu, system_style: &SystemStyle) -> LogicalSize {
    // TODO: Implement proper size calculation based on menu items
    // For now, use reasonable defaults

    let item_count = menu.items.len();
    let item_height = 24.0; // Default item height in pixels
    let padding = 8.0;

    let width = 200.0; // Default width
    let height = (item_count as f32 * item_height) + (padding * 2.0);

    LogicalSize::new(width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_menu_size() {
        use azul_core::menu::MenuItem;

        let menu = Menu {
            items: vec![
                MenuItem::String("Item 1".into()),
                MenuItem::String("Item 2".into()),
                MenuItem::String("Item 3".into()),
            ],
            position: azul_core::menu::MenuPopupPosition::AutoCursor,
            context_mouse_btn: azul_core::events::MouseButton::Left,
        };

        let system_style = SystemStyle::default();
        let size = calculate_menu_size(&menu, &system_style);

        assert!(size.width > 0.0);
        assert!(size.height > 0.0);
        assert_eq!(size.height, 3.0 * 24.0 + 16.0); // 3 items * 24px + padding
    }
}
