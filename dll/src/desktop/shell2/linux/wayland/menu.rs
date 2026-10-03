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

    // Measured before the popup is created (see `measure_popup_content`);
    // `menu_size` is only the estimate the measurement falls back to.
    options.size_to_content = true;
    options.window_state.flags.window_type = azul_core::window::WindowType::Menu;
    options.window_state.flags.decorations = azul_core::window::WindowDecorations::None;
    options.window_state.flags.is_always_on_top = true;
    options.window_state.flags.is_resizable = false;

    options
}

/// The size the menu popup surface is created at.
///
/// Every number comes from the live `SystemStyle` (see
/// `menu_renderer::MenuMetrics`, which the menu STYLESHEET is built from too,
/// so the surface and its contents cannot disagree). It used to be a flat
/// 200x(items*24)+16 that ignored its `system_style` argument outright — a
/// menu the same size on every desktop, at every font size, and never the size
/// the stylesheet said.
#[must_use]
pub fn calculate_menu_size(menu: &Menu, system_style: &SystemStyle) -> LogicalSize {
    crate::desktop::menu_renderer::MenuMetrics::from_system_style(system_style)
        .estimate_menu_size(menu)
}

#[cfg(test)]
mod tests {
    use azul_core::menu::{MenuItem, StringMenuItem};
    use azul_css::system::defaults;

    use super::*;

    enum Entry {
        Item,
        Separator,
    }

    fn menu_of(entries: &[Entry]) -> Menu {
        Menu {
            items: entries
                .iter()
                .map(|e| match e {
                    Entry::Item => {
                        MenuItem::String(StringMenuItem::create("Item".to_string().into()))
                    }
                    Entry::Separator => MenuItem::Separator,
                })
                .collect::<Vec<_>>()
                .into(),
            position: azul_core::menu::MenuPopupPosition::AutoCursor,
            context_mouse_btn: azul_core::window::ContextMenuMouseButton::Left,
        }
    }

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

        // KDE Breeze: menuFont "Noto Sans,10" (POINTS -> 13.33px), control
        // padding 4px (menus take half), border 1px.
        //   nominal line box (1.2em)   = 16px
        //   checkmark gutter (1.4em)   = 19px   <- the taller of the two
        //   item height                = 19 + 2*2 = 23px
        //   frame                      = 2*1 border + 2*2 padding = 6px
        let size = calculate_menu_size(&menu, &defaults::kde_breeze_light());

        assert_eq!(
            size.height,
            3.0 * 23.0 + 6.0,
            "three items in the desktop's own menu font and padding"
        );
    }

    /// A menu is exactly as tall as the SYSTEM says: the desktop's menu font
    /// size, the desktop's control padding, the desktop's border width - not
    /// a 24px item and an 8px pad azul made up. A separator is not an item
    /// and must not be counted as one.
    #[test]
    fn a_menu_is_as_tall_as_the_desktop_says() {
        let menu = menu_of(&[Entry::Item, Entry::Item, Entry::Separator, Entry::Item]);
        let size = calculate_menu_size(&menu, &defaults::kde_breeze_light());

        // 3 items * 23 + separator (1px rule + 2*2 margin = 5) + 6 frame.
        assert_eq!(size.height, 80.0, "3 items + 1 separator at Breeze's 10pt");
        // The estimate is the MINIMUM the stylesheet promises, so the layout
        // pass - not the estimate - decides the real width. 12em at 13.33px.
        assert_eq!(size.width, 160.0, "the menu's declared minimum width");
    }

    /// The same menu on a desktop with a bigger menu font is a bigger menu.
    /// A size that ignores `system_style` cannot be.
    #[test]
    fn the_menu_estimate_follows_the_desktops_font() {
        let menu = menu_of(&[Entry::Item, Entry::Item, Entry::Separator, Entry::Item]);
        let mut big = defaults::kde_breeze_light();
        big.fonts.menu_font_size = azul_css::corety::OptionF32::Some(12.0);
        let size = calculate_menu_size(&menu, &big);

        // 12pt -> 16px: line box 19, gutter 22, item 26. 3*26 + 5 + 6.
        assert_eq!(size.height, 89.0);
        assert_eq!(size.width, 192.0, "12em at 16px");
        assert_ne!(
            size,
            calculate_menu_size(&menu, &defaults::kde_breeze_light()),
            "the desktop's font size must reach the menu's size"
        );
    }
}
