//! Wayland menus: the menu window every backend opens, as an `xdg_popup`.
//!
//! A Wayland menu is the same window-drawn menu as everywhere else
//! (`desktop::menu::show_menu`, its `MenuWindowData` carrying the menu and the
//! rule it is placed by), opened without screen coordinates: the compositor's
//! positioner places it from that rule (`WaylandWindow::open_menu_popup`), and
//! a submenu opens on its menu's surface (`open_menu_on_popup`). This module
//! only adds what is Wayland's own: the popup size the positioner and the
//! buffer must agree on, and the estimate a menu is created at.

use azul_core::{
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    menu::Menu,
};
use azul_css::system::SystemStyle;
use azul_layout::window_state::WindowCreateOptions;

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

/// The create options of a menu opened at `position` for `anchor` (both in the
/// logical px of the surface it opens on), whose items run in the window with
/// the `picks` mailbox.
#[must_use]
pub fn menu_options(
    picks: crate::desktop::menu::MenuPicks,
    menu: &Menu,
    system_style: &alloc::sync::Arc<SystemStyle>,
    position: LogicalPosition,
    anchor: Option<LogicalRect>,
) -> WindowCreateOptions {
    // The one menu model of every backend: the window-drawn menu window,
    // its rule carried in its `MenuWindowData`. Without screen coordinates
    // the compositor places it from that rule (`open_menu_popup`), and the
    // shared submenu code finds the menu it opens from there.
    crate::desktop::menu::show_menu(
        menu.clone(),
        system_style.clone(),
        None,
        anchor,
        Some(position),
        None,
        picks,
    )
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

    /// The rule a Wayland menu opened at `position` for `anchor` is placed by.
    fn placement(
        menu: &Menu,
        position: LogicalPosition,
        anchor: Option<LogicalRect>,
    ) -> azul_layout::placement::FloatingPlacement {
        let opts = menu_options(
            crate::desktop::menu::MenuPicks::default(),
            menu,
            &alloc::sync::Arc::new(defaults::kde_breeze_light()),
            position,
            anchor,
        );
        crate::desktop::menu::menu_placement_of(&opts).expect("a menu window")
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
    /// menu below it with the left edges lined up; the zero-sized cursor point
    /// of a context menu opens it at the pointer.
    #[test]
    fn a_trigger_rect_opens_below_and_a_cursor_point_opens_at_the_pointer() {
        use azul_layout::placement::AxisEnd;

        let menu = menu_of(&[Entry::Item]);
        let swatch = LogicalRect::new(LogicalPosition::new(50.0, 396.0), LogicalSize::new(14.0, 14.0));
        let below = placement(&menu, swatch.origin, Some(swatch));
        assert_eq!(below.anchor, swatch);
        assert_eq!((below.x.anchor, below.x.grow), (AxisEnd::Start, AxisEnd::End));
        assert_eq!((below.y.anchor, below.y.grow), (AxisEnd::End, AxisEnd::End));

        let cursor = LogicalRect::new(LogicalPosition::new(50.0, 396.0), LogicalSize::zero());
        for anchor in [Some(cursor), None] {
            let at = placement(&menu, cursor.origin, anchor);
            assert_eq!(at.anchor, cursor, "at the pointer");
            assert_eq!((at.x.anchor, at.x.grow), (AxisEnd::End, AxisEnd::End));
            assert_eq!((at.y.anchor, at.y.grow), (AxisEnd::End, AxisEnd::End));
            assert!(at.x.flip && at.y.flip, "a context menu flips at the screen edge");
        }
    }

    /// A menu that asks to open beside its item - a submenu - does on Wayland
    /// too. Placed by "a rect means below it", every Wayland submenu opened
    /// under its item, covering the items after it.
    #[test]
    fn a_submenu_opens_beside_its_item() {
        use azul_layout::placement::AxisEnd;

        let mut submenu = menu_of(&[Entry::Item]);
        submenu.position = azul_core::menu::MenuPopupPosition::RightOfHitRect;
        let item = LogicalRect::new(LogicalPosition::new(0.0, 94.0), LogicalSize::new(160.0, 22.0));
        let beside = placement(&submenu, item.origin, Some(item));
        assert_eq!(
            (beside.x.anchor, beside.x.grow),
            (AxisEnd::End, AxisEnd::End),
            "right of its item"
        );
        assert_eq!(
            (beside.y.anchor, beside.y.grow),
            (AxisEnd::Start, AxisEnd::End),
            "its top on the item's top"
        );
    }

    /// A menu opened from inside a popup - a context menu in a dropdown - opens
    /// ON that popup (`opens_on_its_opener`), so it is anchored at its item in
    /// the popup's own coordinates, never at those coordinates read as the
    /// parent's (the parent's corner, for a popup at its top-left).
    #[test]
    fn a_menu_opened_in_a_popup_is_anchored_at_its_item_on_that_popup() {
        let item = LogicalRect::new(LogicalPosition::new(0.0, 40.0), LogicalSize::new(160.0, 22.0));
        let menu = menu_of(&[Entry::Item]);
        assert_eq!(placement(&menu, item.origin, Some(item)).anchor, item);
    }

    /// A Wayland menu is the same menu window every backend opens, carrying the
    /// same menu model: the submenu code finds its menu there, and the menu's
    /// rule anchors it at the item it was opened for. A Wayland-only menu data
    /// type left every Wayland submenu unopened.
    #[test]
    fn a_wayland_menu_carries_the_menu_model_its_submenus_open_from() {
        let item = LogicalRect::new(LogicalPosition::new(0.0, 94.0), LogicalSize::new(160.0, 22.0));
        let opts = menu_options(
            crate::desktop::menu::MenuPicks::default(),
            &menu_of(&[Entry::Item]),
            &alloc::sync::Arc::new(defaults::kde_breeze_light()),
            item.origin,
            Some(item),
        );
        let placement = crate::desktop::menu::menu_placement_of(&opts)
            .expect("the menu window carries the one menu model");
        assert_eq!(placement.anchor, item, "anchored at the item it was opened for");
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
