//! Wayland menu handling via popup windows
//!
//! This module provides menu popup functionality for Wayland. It returns
//! `WindowCreateOptions` for a generic popup window; xdg_popup integration
//! is pending.
//!
//! Architecture:
//! - Menu data (Menu struct) is passed as RefAny to the layout callback
//! - Events are handled through normal Azul callback system
//! - Rendering goes through the standard menu_renderer / WebRender pipeline

use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo},
    geom::{LogicalRect, LogicalSize},
    menu::Menu,
    refany::RefAny,
};
use azul_css::system::SystemStyle;
use azul_layout::window_state::WindowCreateOptions;

use azul_core::{geom::LogicalPosition, transient::TransientAnchor};

use super::{super::super::common::debug_server::LogCategory, WaylandWindow};
use crate::log_error;

/// Data passed to the menu layout callback
#[derive(Debug, Clone)]
pub(crate) struct MenuLayoutData {
    /// Menu structure to render
    pub menu: Menu,
    /// System style for native look
    pub system_style: SystemStyle,
    /// Trigger rectangle in parent-surface-relative logical coordinates.
    /// Stored here so the future xdg_popup positioner can anchor the popup
    /// to the trigger node on the parent surface — Wayland clients cannot
    /// address absolute screen coordinates, so positioning must happen via
    /// the compositor with this rect.
    pub trigger_rect: LogicalRect,
    /// Which edge of `trigger_rect` the popup opens on. A menu bar item and
    /// a dropdown open BELOW their trigger with the left edges aligned; a
    /// context menu opens at the pointer. This used to be decided at popup
    /// creation with a single answer for every menu - the pointer corner -
    /// so every dropdown hung off its trigger's bottom-RIGHT corner, one
    /// trigger width to the right of where it belongs.
    pub edge: TransientAnchor,
}

/// The edge a menu opens on, from what the opener knew: a real trigger rect
/// means "below the control"; none means "at the pointer" (a context menu,
/// whose trigger rect is the zero-sized cursor point).
#[must_use]
pub fn menu_edge_for(anchor: Option<LogicalRect>) -> TransientAnchor {
    match anchor {
        Some(rect) if rect.size.width > 0.0 || rect.size.height > 0.0 => TransientAnchor::Bottom,
        _ => TransientAnchor::Cursor,
    }
}

/// The popup size as the positioner and the buffer must both see it.
///
/// A menu's measured size is a fraction; the positioner took it truncated
/// (`as i32`) while the buffer and the viewport destination took it
/// rounded up, so the surface was one pixel larger than the box the
/// compositor had positioned and constrained (the "258 logical -> 259 px"
/// in the log). One rounding, up, for everyone.
#[must_use]
pub fn popup_size_px(size: LogicalSize) -> LogicalSize {
    LogicalSize::new(size.width.max(1.0).ceil(), size.height.max(1.0).ceil())
}

/// Layout callback for menu popup windows
///
/// This callback uses menu_renderer to create a StyledDom from the Menu structure.
/// It's called by Azul's normal layout system, so rendering happens through the
/// standard WebRender pipeline.
extern "C" fn menu_layout_callback(_data: RefAny, info: LayoutCallbackInfo) -> azul_core::dom::Dom {
    // The menu's `MenuLayoutData` is carried in the layout callback's `ctx`
    // (see `create_menu_popup_options`), NOT in `data`: `data` is the SHARED
    // APP data, common to every window, so downcasting it to MenuLayoutData
    // always failed - every Wayland menu popup opened as a blank surface
    // (KDE session, 2026-08-29). Same fix the unified menu system's
    // `menu_layout_callback` carries; read the per-window payload via
    // `info.get_ctx()`.
    let menu_refany = match info.get_ctx().into_option() {
        Some(r) => r,
        None => {
            log_error!(
                LogCategory::Layout,
                "[Menu Layout] menu window has no ctx (MenuLayoutData)"
            );
            return azul_core::dom::Dom::create_body();
        }
    };
    let mut probe = menu_refany.clone();
    let Some(menu_data) = probe.downcast_ref::<MenuLayoutData>() else {
        log_error!(
            LogCategory::Layout,
            "[Menu Layout] ctx is not MenuLayoutData"
        );
        return azul_core::dom::Dom::create_body();
    };

    // Use menu_renderer to create Dom with deferred CSS
    crate::desktop::menu_renderer::create_menu_dom_with_css(
        &menu_data.menu,
        &menu_data.system_style,
        menu_refany.clone(), // Pass the menu-window RefAny for item callbacks
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
pub fn create_menu_popup_options(
    _parent: &WaylandWindow,
    menu: &Menu,
    system_style: &SystemStyle,
    trigger_rect: LogicalRect,
    edge: TransientAnchor,
    menu_size: LogicalSize,
) -> WindowCreateOptions {
    let menu_data = MenuLayoutData {
        menu: menu.clone(),
        system_style: system_style.clone(),
        trigger_rect,
        edge,
    };

    let menu_data_refany = RefAny::new(menu_data);

    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = menu_size;
    options.window_state.title = "Menu".to_string().into();

    options.window_state.layout_callback = LayoutCallback {
        cb: menu_layout_callback,
        ctx: azul_core::refany::OptionRefAny::Some(menu_data_refany),
    };

    options.window_state.flags.window_type = azul_core::window::WindowType::Menu;
    options.window_state.flags.decorations = azul_core::window::WindowDecorations::None;
    options.window_state.flags.is_always_on_top = true;
    options.window_state.flags.is_resizable = false;

    options
}

/// Default menu item height in logical pixels
const DEFAULT_MENU_ITEM_HEIGHT: f32 = 24.0;
/// Vertical padding above and below menu items in logical pixels
const DEFAULT_MENU_PADDING: f32 = 8.0;
/// Default menu popup width in logical pixels
const DEFAULT_MENU_WIDTH: f32 = 200.0;

/// Calculate menu size from Menu structure
///
/// This estimates the menu size based on the number of items and their content.
/// Used when caller doesn't specify an explicit size.
pub fn calculate_menu_size(menu: &Menu, _system_style: &SystemStyle) -> LogicalSize {
    // TODO: Implement proper size calculation using system_style font metrics

    let item_count = menu.items.len();
    let height = (item_count as f32 * DEFAULT_MENU_ITEM_HEIGHT) + (DEFAULT_MENU_PADDING * 2.0);

    LogicalSize::new(DEFAULT_MENU_WIDTH, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real trigger rect (a menu bar item, a dropdown's control) opens the
    /// menu below it; the zero-sized cursor point of a context menu opens it
    /// at the pointer.
    #[test]
    fn a_trigger_rect_opens_below_and_a_cursor_point_opens_at_the_pointer() {
        let swatch = LogicalRect::new(LogicalPosition::new(50.0, 396.0), LogicalSize::new(14.0, 14.0));
        assert_eq!(menu_edge_for(Some(swatch)), TransientAnchor::Bottom);
        let cursor = LogicalRect::new(LogicalPosition::new(50.0, 396.0), LogicalSize::zero());
        assert_eq!(menu_edge_for(Some(cursor)), TransientAnchor::Cursor);
        assert_eq!(menu_edge_for(None), TransientAnchor::Cursor);
    }

    /// The size the positioner is told and the buffer that is attached must
    /// agree: 258.4 logical is 259 for both, never 258 for one and 259 for
    /// the other.
    #[test]
    fn the_positioner_and_the_buffer_round_the_popup_size_the_same_way() {
        let px = popup_size_px(LogicalSize::new(258.4, 288.0));
        assert_eq!((px.width as i32, px.height as i32), (259, 288));
        assert_eq!(px.width, px.width.ceil(), "already integral");
        let tiny = popup_size_px(LogicalSize::new(0.0, 0.2));
        assert_eq!((tiny.width, tiny.height), (1.0, 1.0), "never a zero-sized popup");
    }

    #[test]
    fn test_calculate_menu_size() {
        use azul_core::menu::{MenuItem, StringMenuItem};

        let menu = Menu {
            items: vec![
                MenuItem::String(StringMenuItem::create("Item 1".to_string().into())),
                MenuItem::String(StringMenuItem::create("Item 2".to_string().into())),
                MenuItem::String(StringMenuItem::create("Item 3".to_string().into())),
            ]
            .into(),
            position: azul_core::menu::MenuPopupPosition::AutoCursor,
            context_mouse_btn: azul_core::window::ContextMenuMouseButton::Left,
        };

        let system_style = SystemStyle::default();
        let size = calculate_menu_size(&menu, &system_style);

        assert!(size.width > 0.0);
        assert!(size.height > 0.0);
        assert_eq!(size.height, 3.0 * 24.0 + 16.0); // 3 items * 24px + padding
    }
}
