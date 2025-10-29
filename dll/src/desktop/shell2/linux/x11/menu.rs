//! X11 menu handling - Proper StyledDom rendering using Azul's window system
//!
//! This module implements menu popups as proper Azul windows with StyledDom rendering.
//! Each menu popup is a real window with its own layout callback that uses menu_renderer.
//!
//! Architecture:
//! - Menu popups are created using WindowCreateOptions with MarshaledLayoutCallback
//! - The layout callback uses menu_renderer::create_menu_styled_dom() for rendering
//! - Menu data (Menu struct) is passed as RefAny to the layout callback
//! - Events are handled through normal Azul callback system
//! - This works identically across all platforms (X11, Wayland, macOS, Windows)

use azul_core::{
    callbacks::{
        LayoutCallback, LayoutCallbackInfo, MarshaledLayoutCallback, MarshaledLayoutCallbackInner,
    },
    geom::LogicalSize,
    menu::Menu,
    refany::RefAny,
    styled_dom::StyledDom,
    window::WindowPosition,
};
use azul_css::system::SystemStyle;
use azul_layout::window_state::{WindowCreateOptions, WindowState};

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
extern "C" fn menu_layout_callback(
    data: &mut RefAny,
    _system_style: &mut RefAny,
    _info: &mut LayoutCallbackInfo,
) -> StyledDom {
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

/// Create a menu popup window using Azul's window system
///
/// This creates a proper Azul window with StyledDom rendering, not a raw X11 window.
/// The menu is rendered through the normal layout/rendering pipeline.
///
/// # Arguments
/// * `menu` - Menu structure to display
/// * `system_style` - System style for native look
/// * `x` - X position for menu (screen coordinates)
/// * `y` - Y position for menu (screen coordinates)
///
/// # Returns
/// * `WindowCreateOptions` - Window options that can be passed to info.create_window()
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
///     
///     let menu_options = create_menu_window_options(
///         &menu,
///         &system_style,
///         100, // x
///         100, // y
///     );
///     
///     info.create_window(menu_options);
///     Update::DoNothing
/// }
/// ```
pub fn create_menu_window_options(
    menu: &Menu,
    system_style: &SystemStyle,
    x: i32,
    y: i32,
) -> WindowCreateOptions {
    // Calculate menu size based on items
    // TODO: Use actual font metrics for accurate sizing
    let item_height = 25;
    let menu_width = 200;
    let menu_height = (menu.items.len() * item_height) as u32;

    // Create menu layout data
    let menu_data = MenuLayoutData {
        menu: menu.clone(),
        system_style: system_style.clone(),
    };

    // Create window state with menu layout callback
    let mut window_state = WindowState::default();

    // Set window position
    window_state.position = WindowPosition::Initialized(azul_core::geom::PhysicalPosition { x, y });

    // Set window size
    window_state.size.dimensions = LogicalSize::new(menu_width as f32, menu_height as f32);

    // Configure window flags for popup behavior
    window_state.flags.decorations = azul_core::window::WindowDecorations::None;
    window_state.flags.is_always_on_top = true;
    window_state.flags.is_resizable = false;

    // Set layout callback with marshaled data
    window_state.layout_callback = LayoutCallback::Marshaled(MarshaledLayoutCallback {
        marshal_data: RefAny::new(menu_data),
        cb: MarshaledLayoutCallbackInner {
            cb: menu_layout_callback,
        },
    });

    // Create window options
    WindowCreateOptions {
        state: window_state,
        size_to_content: false,
        renderer: None.into(),
        theme: None.into(),
        create_callback: None.into(),
        hot_reload: false,
    }
}
