//! Client-Side Decorations (CSD) - Custom Window Titlebar
//!
//! This module provides automatic titlebar generation for frameless windows.
//! When `WindowFlags::has_decorations` is enabled, a custom titlebar with
//! window controls (close, minimize, maximize) is automatically injected
//! into the user's DOM.
//!
//! All button/drag logic lives in [`azul_layout::widgets::titlebar::Titlebar`].
//! This module is just the integration layer that:
//!   1. Decides *whether* to inject a titlebar (`should_inject_csd`)
//!   2. Creates a `Titlebar` from the live `SystemStyle`
//!   3. Styles it with the CSD stylesheet (`SystemStyle::create_csd_stylesheet`)
//!   4. Optionally injects a menu bar below the titlebar

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClassVec},
    menu::{Menu, MenuItem},
    refany::RefAny,
    styled_dom::StyledDom,
    window::{WindowDecorations, WindowFrame},
};
use azul_css::{css::Css, system::SystemStyle};
use azul_layout::callbacks::CallbackInfo;
use azul_layout::widgets::titlebar::Titlebar;

use crate::desktop::menu_renderer::SystemStyleMenuExt;
use crate::desktop::shell2::common::debug_server::LogCategory;
use crate::log_debug;

// ── Menu bar callback (not part of Titlebar) ───────────────────────────

/// Callback for menu bar items - shows dropdown menu below the item
extern "C" fn csd_menubar_item_callback(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    use azul_core::geom::LogicalPosition;

    let menu = match data.downcast_ref::<Menu>() {
        Some(m) => m.clone(),
        None => {
            log_debug!(
                LogCategory::General,
                "[CSD Menu] Failed to downcast menu data"
            );
            return Update::DoNothing;
        }
    };

    log_debug!(
        LogCategory::General,
        "[CSD Menu] Menu bar item clicked, creating popup menu"
    );

    let system_style = info.get_system_style();

    let window_state = info.get_current_window_state();
    let parent_pos = match window_state.position {
        azul_core::window::WindowPosition::Initialized(pos) => {
            LogicalPosition::new(pos.x as f32, pos.y as f32)
        }
        _ => LogicalPosition::new(0.0, 0.0),
    };

    let trigger_rect = match info.get_hit_node_rect() {
        Some(rect) => rect,
        None => {
            log_debug!(
                LogCategory::General,
                "[CSD Menu] No hit node rect available"
            );
            return Update::DoNothing;
        }
    };

    let menu_options = crate::desktop::menu::show_menu(
        menu,
        system_style.clone(),
        parent_pos,
        Some(trigger_rect),
        None,
        None,
    );

    info.create_window(menu_options);

    Update::DoNothing
}

// ── Titlebar creation (delegates to Titlebar) ────────────────────────

/// Create a CSD titlebar `StyledDom` with window controls using `SystemStyle`.
///
/// Builds a [`Titlebar`] in full-CSD mode (`dom_with_buttons`),
/// then styles it with the CSD stylesheet from `SystemStyle`.
pub fn create_titlebar_styled_dom(
    title: &str,
    system_style: &SystemStyle,
) -> StyledDom {
    let tm = &system_style.metrics.titlebar;

    let titlebar = Titlebar::from_system_style_csd(
        title.into(),
        system_style,
    );

    let mut dom = titlebar.dom_with_buttons(&tm.buttons, tm.button_side);

    let stylesheet = system_style.create_csd_stylesheet();
    let css = azul_css::css::Css::new(vec![stylesheet]);

    StyledDom::create(&mut dom, css)
}

/// Create a CSD menu bar `StyledDom` with menu items and callbacks.
fn create_menubar_styled_dom(menu: &Menu, system_style: &SystemStyle) -> StyledDom {
    let mut menu_items = Vec::new();

    for item in menu.items.as_slice().iter() {
        if let MenuItem::String(string_item) = item {
            let item_classes =
                IdOrClassVec::from_vec(vec![IdOrClass::Class("csd-menubar-item".into())]);

            let submenu = Menu::create(string_item.children.clone());

            let dom_item = Dom::create_div()
                .with_ids_and_classes(item_classes)
                .with_child(Dom::create_text(string_item.label.as_str()))
                .with_callbacks(
                    vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseDown),
                        callback: CoreCallback {
                            cb: csd_menubar_item_callback as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: RefAny::new(submenu),
                    }]
                    .into(),
                );

            menu_items.push(dom_item);
        }
    }

    let menubar_classes = IdOrClassVec::from_vec(vec![IdOrClass::Class("csd-menubar".into())]);
    let mut dom = Dom::create_div()
        .with_ids_and_classes(menubar_classes)
        .with_children(DomVec::from_vec(menu_items));

    let stylesheet = system_style.create_menu_stylesheet();
    let css = azul_css::css::Css::new(vec![stylesheet]);

    StyledDom::create(&mut dom, css)
}

// ── Public helpers ───────────────────────────────────────────────────────

/// Check if CSD should be injected for a window.
///
/// CSD is injected when:
/// 1. `has_decorations` flag is true, AND
/// 2. `decorations` is set to `None` (frameless window)
#[inline]
pub fn should_inject_csd(has_decorations: bool, decorations: WindowDecorations) -> bool {
    has_decorations && decorations == WindowDecorations::None
}

/// Inject CSD titlebar and/or menu into user's DOM.
///
/// Creates a container `StyledDom` and appends:
/// 1. Titlebar with close/min/max buttons (via [`Titlebar::dom_with_buttons`])
/// 2. Menu bar (if the root node carries a `Menu`)
/// 3. User's content DOM
pub fn wrap_user_dom_with_decorations(
    user_dom: StyledDom,
    window_title: &str,
    should_inject_titlebar: bool,
    system_style: &SystemStyle,
) -> StyledDom {
    // Extract menu bar from user DOM's root node if present
    let menu_bar = user_dom
        .node_data
        .as_container()
        .get(azul_core::dom::NodeId::ZERO)
        .and_then(|root_node| root_node.get_menu_bar())
        .map(|boxed_menu| (**boxed_menu).clone());

    // If no decorations needed and no menu bar, return user's DOM unmodified
    if !should_inject_titlebar && menu_bar.is_none() {
        return user_dom;
    }

    // Use an Html root so we don't get double <body> nesting.
    let mut container_dom = Dom::create_html();
    let mut container_styled = StyledDom::create(&mut container_dom, azul_css::css::Css::empty());

    // Inject titlebar if needed
    if should_inject_titlebar {
        let titlebar_styled = create_titlebar_styled_dom(
            window_title,
            system_style,
        );
        container_styled.append_child(titlebar_styled);
    }

    // Inject menu bar if present
    if let Some(menu) = menu_bar {
        let menubar_styled = create_menubar_styled_dom(&menu, system_style);
        container_styled.append_child(menubar_styled);
    }

    // Append user's content
    container_styled.append_child(user_dom);

    container_styled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_inject_csd() {
        assert!(should_inject_csd(true, WindowDecorations::None));
        assert!(!should_inject_csd(false, WindowDecorations::None));
        assert!(!should_inject_csd(true, WindowDecorations::Normal));
        assert!(!should_inject_csd(true, WindowDecorations::NoTitle));
        assert!(!should_inject_csd(true, WindowDecorations::NoControls));
    }
}
