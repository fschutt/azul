//! Menu Rendering - Converts Menu structures to StyledDom
//!
//! This module provides functions to render menu structures as styled DOM trees.
//! It uses SystemStyle for native look and feel, and supports:
//! - Regular menu items with labels
//! - Separators
//! - Icons (checkboxes and images)
//! - Keyboard shortcuts
//! - Hover states
//! - Submenus (hierarchical structure)
//! - Callback attachment to clickable items

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, CoreCallbackDataVec, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClassVec},
    menu::{Menu, MenuItem, MenuItemIcon, MenuItemState, OptionMenuItemIcon, StringMenuItem},
    refany::RefAny,
    styled_dom::StyledDom,
};
use azul_css::{
    css::{Css, Stylesheet},
    props::basic::pixel::DEFAULT_FONT_SIZE,
    system::SystemStyle,
    AzString,
};
use azul_layout::callbacks::CallbackInfo;

use crate::desktop::menu::MenuWindowData;
use crate::desktop::shell2::common::debug_server::LogCategory;
use crate::log_debug;

/// Data structure for menu item click callbacks
#[derive(Debug, Clone)]
struct MenuItemCallbackData {
    menu_item: StringMenuItem,
    menu_window_data: RefAny,
    item_index: usize,
}

/// Data structure for submenu hover callbacks
#[derive(Debug, Clone)]
struct SubmenuCallbackData {
    menu_item: StringMenuItem,
    menu_window_data: RefAny,
    item_index: usize,
}

/// Callback invoked when a menu item is clicked
///
/// This:
/// 1. Invokes the menu item's original callback (if present)
/// 2. Closes the menu window
extern "C" fn menu_item_click_callback(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let callback_data = match data.downcast_ref::<MenuItemCallbackData>() {
        Some(d) => d,
        None => {
            log_debug!(
                LogCategory::General,
                "[menu_item_click_callback] Failed to downcast MenuItemCallbackData"
            );
            return Update::DoNothing;
        }
    };

    // Invoke the menu item's callback if present
    if let Some(ref menu_callback) = callback_data.menu_item.callback.as_option() {
        // Convert CoreCallback to actual function pointer using safe wrapper
        let callback = azul_layout::callbacks::Callback::from_core(menu_callback.callback.clone());

        // Invoke with the menu item's data
        let callback_data_refany = menu_callback.refany.clone();
        let result = callback.invoke(callback_data_refany, info.clone());

        log_debug!(
            LogCategory::General,
            "[menu_item_click_callback] Invoked callback for menu item '{}' (index {})",
            callback_data.menu_item.label.as_str(),
            callback_data.item_index
        );

        // Close the menu window
        let mut state = info.get_current_window_state().clone();
        state.flags.close_requested = true;
        info.modify_window_state(state);

        return result;
    }

    // No callback attached, just close the menu
    let mut state = info.get_current_window_state().clone();
    state.flags.close_requested = true;
    info.modify_window_state(state);

    Update::DoNothing
}

/// Callback invoked when hovering over a menu item with children
///
/// This spawns a submenu window positioned to the right of the menu item
extern "C" fn submenu_hover_callback(mut data: RefAny, mut info: CallbackInfo) -> Update {
    use alloc::sync::Arc;

    use azul_core::{geom::LogicalPosition, menu::MenuPopupPosition, window::WindowPosition};

    let submenu_data = match data.downcast_ref::<SubmenuCallbackData>() {
        Some(d) => d,
        None => {
            log_debug!(
                LogCategory::General,
                "[submenu_hover_callback] Failed to downcast SubmenuCallbackData"
            );
            return Update::DoNothing;
        }
    };

    // Get the menu item's rectangle (for positioning the submenu)
    let item_rect = match info.get_hit_node_rect() {
        Some(rect) => rect,
        None => {
            log_debug!(
                LogCategory::General,
                "[submenu_hover_callback] Could not get hit node rect"
            );
            return Update::DoNothing;
        }
    };

    // Clone menu_window_data before downcasting to avoid borrow conflicts
    let mut menu_window_data_clone = submenu_data.menu_window_data.clone();

    // Get parent menu window data
    let parent_menu_data = match menu_window_data_clone.downcast_ref::<MenuWindowData>() {
        Some(d) => d,
        None => {
            log_debug!(
                LogCategory::General,
                "[submenu_hover_callback] Failed to downcast parent MenuWindowData"
            );
            return Update::DoNothing;
        }
    };

    // Get system style from CallbackInfo (safe Arc clone)
    let system_style = info.get_system_style();

    // Get parent window position
    let parent_pos = match info.get_current_window_state().position {
        WindowPosition::Initialized(pos) => LogicalPosition::new(pos.x as f32, pos.y as f32),
        _ => LogicalPosition::new(0.0, 0.0),
    };

    // Create submenu using the menu item's children
    let submenu = Menu {
        items: submenu_data.menu_item.children.clone(),
        position: MenuPopupPosition::RightOfHitRect, // Position to the right
        context_mouse_btn: parent_menu_data.menu.context_mouse_btn,
    };

    // Create submenu window (parent_menu_id = this menu's ID)
    let parent_id = parent_menu_data.menu_window_id;
    let submenu_options = crate::desktop::menu::show_menu(
        submenu,
        system_style,
        parent_pos,
        Some(item_rect),
        None, // No cursor position for submenu
        parent_id,
    );

    // Create the submenu window
    // TODO: Track the returned window ID and add to parent_menu_data.child_menu_ids
    info.create_window(submenu_options);

    log_debug!(
        LogCategory::General,
        "[submenu_hover_callback] Spawned submenu for item '{}' (index {})",
        submenu_data.menu_item.label.as_str(),
        submenu_data.item_index
    );

    Update::DoNothing
}

/// Create a styled menu DOM from a Menu structure
///
/// This generates a complete styled menu with:
/// - All menu items rendered with proper styling
/// - Separators between items
/// - Icons (checkboxes, images) next to labels
/// - Keyboard shortcuts aligned to the right
/// - Hover states for interactivity
/// - Disabled/greyed state support
/// - Callbacks attached to clickable items
/// - Sub-menu support with hover detection
///
/// # Arguments
/// * `menu` - Menu structure to render
/// * `system_style` - System style for native look and feel
/// * `menu_window_data` - MenuWindowData RefAny for callback context
///
/// # Returns
/// StyledDom tree for the menu with CSS applied
pub fn create_menu_styled_dom(
    menu: &Menu,
    system_style: &SystemStyle,
    menu_window_data: RefAny,
) -> StyledDom {
    // Create DOM structure with callbacks attached
    let mut dom = create_menu_dom(menu, &menu_window_data);

    // Create stylesheet from SystemStyle
    let stylesheet = system_style.create_menu_stylesheet();

    // Wrap in Css struct
    let css = Css::new(vec![stylesheet]);

    // Apply stylesheet to DOM
    StyledDom::create(&mut dom, css)
}

/// Create a menu DOM with deferred CSS for use in layout callbacks.
///
/// Same as `create_menu_styled_dom` but returns a `Dom` with CSS pushed
/// via `.style()` (deferred cascade). Use this in `LayoutCallbackType` callbacks
/// which now return `Dom` instead of `StyledDom`.
pub fn create_menu_dom_with_css(
    menu: &Menu,
    system_style: &SystemStyle,
    menu_window_data: RefAny,
) -> Dom {
    let mut dom = create_menu_dom(menu, &menu_window_data);

    let stylesheet = system_style.create_menu_stylesheet();
    let css = Css::new(vec![stylesheet]);

    dom.style(css);
    dom
}

/// Create menu DOM structure with callbacks attached (internal helper)
///
/// # Arguments
/// * `menu` - Menu structure to render
/// * `menu_window_data` - MenuWindowData RefAny for callbacks
///
/// # Returns
/// DOM tree for the menu (unstyled but with callbacks)
fn create_menu_dom(menu: &Menu, menu_window_data: &RefAny) -> Dom {
    // Container for all menu items
    let mut container =
        Dom::create_div().with_ids_and_classes(IdOrClassVec::from_vec(vec![IdOrClass::Class(
            "menu-container".into(),
        )]));

    // Render each menu item with its index for identification
    for (idx, item) in menu.items.as_slice().iter().enumerate() {
        let item_dom = create_menu_item_dom(item, idx, menu_window_data);
        container = container.with_child(item_dom);
    }

    container
}

/// Create DOM for a single menu item with callbacks
///
/// # Arguments
/// * `item` - Menu item to render
/// * `idx` - Index of this item in the menu (for identification)
/// * `menu_window_data` - MenuWindowData RefAny for callbacks
///
/// # Returns
/// DOM node for this menu item
fn create_menu_item_dom(item: &MenuItem, idx: usize, menu_window_data: &RefAny) -> Dom {
    match item {
        MenuItem::String(string_item) => {
            create_string_menu_item_dom(string_item, idx, menu_window_data)
        }
        MenuItem::Separator => create_separator_dom(),
        MenuItem::BreakLine => {
            // Break lines are only used in horizontal menus (menu bars)
            // For popup menus, we ignore them
            Dom::create_div().with_ids_and_classes(IdOrClassVec::from_vec(vec![IdOrClass::Class(
                "menu-breakline".into(),
            )]))
        }
    }
}

/// Create DOM for a string menu item (label + optional icon + optional shortcut)
///
/// Structure:
/// ```text
/// <div class="menu-item [menu-item-disabled|menu-item-greyed]" id="menu-item-{idx}">
///   <div class="menu-item-icon">[checkbox or image]</div>
///   <div class="menu-item-label">Label Text</div>
///   <div class="menu-item-shortcut">Ctrl+C</div>
///   <div class="menu-item-arrow">▶</div>  <!-- only if has children -->
/// </div>
/// ```
///
/// Callbacks:
/// - MouseDown: Invoke item's callback (if not disabled)
/// - MouseOver: Show submenu if has children
fn create_string_menu_item_dom(
    item: &StringMenuItem,
    idx: usize,
    menu_window_data: &RefAny,
) -> Dom {
    let mut classes = vec![IdOrClass::Class("menu-item".into())];

    let is_disabled = item.menu_item_state == MenuItemState::Disabled
        || item.menu_item_state == MenuItemState::Greyed;
    let has_children = !item.children.as_slice().is_empty();

    // Add state classes
    match item.menu_item_state {
        MenuItemState::Normal => {}
        MenuItemState::Greyed => {
            classes.push(IdOrClass::Class("menu-item-greyed".into()));
        }
        MenuItemState::Disabled => {
            classes.push(IdOrClass::Class("menu-item-disabled".into()));
        }
    }

    // Add submenu class if has children
    if has_children {
        classes.push(IdOrClass::Class("menu-item-has-submenu".into()));
    }

    // Add unique ID for this menu item
    classes.push(IdOrClass::Id(format!("menu-item-{}", idx).into()));

    // Create container with classes
    let mut item_dom = Dom::create_div().with_ids_and_classes(IdOrClassVec::from_vec(classes));

    // Icon section (checkbox, image, or empty space)
    let icon_dom = create_icon_dom(&item.icon);
    item_dom = item_dom.with_child(icon_dom);

    // Label text
    let label_dom =
        Dom::create_text(item.label.clone()).with_ids_and_classes(IdOrClassVec::from_vec(vec![
            IdOrClass::Class("menu-item-label".into()),
        ]));
    item_dom = item_dom.with_child(label_dom);

    // Keyboard shortcut (if present)
    if let Some(ref combo) = item.accelerator.as_option() {
        let shortcut_text = format_accelerator(combo);
        let shortcut_dom =
            Dom::create_text(shortcut_text).with_ids_and_classes(IdOrClassVec::from_vec(vec![
                IdOrClass::Class("menu-item-shortcut".into()),
            ]));
        item_dom = item_dom.with_child(shortcut_dom);
    }

    // Submenu arrow (if has children)
    if has_children {
        let arrow_dom = Dom::create_text("▶").with_ids_and_classes(IdOrClassVec::from_vec(vec![
            IdOrClass::Class("menu-item-arrow".into()),
        ]));
        item_dom = item_dom.with_child(arrow_dom);
    }

    // Attach callbacks if not disabled
    if !is_disabled {
        let mut callbacks = Vec::new();

        // Click callback: Invoke menu item action
        if item.callback.as_option().is_some() {
            // Create callback data containing both the original callback and menu data
            let callback_data = MenuItemCallbackData {
                menu_item: item.clone(),
                menu_window_data: menu_window_data.clone(),
                item_index: idx,
            };

            callbacks.push(CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseDown),
                callback: CoreCallback {
                    cb: menu_item_click_callback as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
                refany: RefAny::new(callback_data),
            });
        }

        // Hover callback: Show submenu if has children
        if has_children {
            let submenu_data = SubmenuCallbackData {
                menu_item: item.clone(),
                menu_window_data: menu_window_data.clone(),
                item_index: idx,
            };

            callbacks.push(CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseOver),
                callback: CoreCallback {
                    cb: submenu_hover_callback as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
                refany: RefAny::new(submenu_data),
            });
        }

        if !callbacks.is_empty() {
            item_dom = item_dom.with_callbacks(CoreCallbackDataVec::from_vec(callbacks));
        }
    }

    item_dom
}

/// Create icon DOM element
fn create_icon_dom(icon: &OptionMenuItemIcon) -> Dom {
    let mut icon_dom =
        Dom::create_div().with_ids_and_classes(IdOrClassVec::from_vec(vec![IdOrClass::Class(
            "menu-item-icon".into(),
        )]));

    if let Some(icon_value) = icon.as_option() {
        match icon_value {
            MenuItemIcon::Checkbox(checked) => {
                // Add checkmark if checked
                if *checked {
                    icon_dom =
                        Dom::create_text("✓").with_ids_and_classes(IdOrClassVec::from_vec(vec![
                            IdOrClass::Class("menu-item-icon".into()),
                            IdOrClass::Class("menu-item-checkbox".into()),
                            IdOrClass::Class("menu-item-checkbox-checked".into()),
                        ]));
                } else {
                    icon_dom = icon_dom.with_ids_and_classes(IdOrClassVec::from_vec(vec![
                        IdOrClass::Class("menu-item-icon".into()),
                        IdOrClass::Class("menu-item-checkbox".into()),
                        IdOrClass::Class("menu-item-checkbox-unchecked".into()),
                    ]));
                }
            }
            MenuItemIcon::Image(_image_ref) => {
                // TODO: Render image icon
                // This requires image rendering support in Azul
                icon_dom = icon_dom.with_ids_and_classes(IdOrClassVec::from_vec(vec![
                    IdOrClass::Class("menu-item-icon".into()),
                    IdOrClass::Class("menu-item-image-icon".into()),
                ]));
            }
        }
    }

    icon_dom
}

/// Create separator DOM element
fn create_separator_dom() -> Dom {
    Dom::create_div().with_ids_and_classes(IdOrClassVec::from_vec(vec![IdOrClass::Class(
        "menu-separator".into(),
    )]))
}

/// Format keyboard accelerator for display
///
/// Converts VirtualKeyCodeCombo to human-readable string like "Ctrl+C"
fn format_accelerator(combo: &azul_core::window::VirtualKeyCodeCombo) -> AzString {
    // For now, just format the keys in the combo
    // TODO: Proper formatting with modifiers
    let key_strs: Vec<String> = combo
        .keys
        .as_slice()
        .iter()
        .map(|k| format!("{:?}", k))
        .collect();

    AzString::from(key_strs.join("+"))
}

/// Extension trait for SystemStyle to create menu stylesheets
impl SystemStyleMenuExt for SystemStyle {
    fn create_menu_stylesheet(&self) -> Stylesheet {
        use azul_css::{parser2::new_from_str, props::basic::ColorU};

        let mut css = String::new();

        // Get colors from system style
        let bg_color = self
            .colors
            .window_background
            .as_option()
            .copied()
            .unwrap_or(ColorU::new_rgb(240, 240, 240));
        let text_color = self
            .colors
            .text
            .as_option()
            .copied()
            .unwrap_or(ColorU::new_rgb(0, 0, 0));
        let hover_color = self
            .colors
            .selection_background
            .as_option()
            .copied()
            .unwrap_or(ColorU::new_rgb(0, 120, 215));
        let hover_text_color = self
            .colors
            .selection_text
            .as_option()
            .copied()
            .unwrap_or(ColorU::new_rgb(255, 255, 255));
        let disabled_color = self
            .colors
            .text
            .as_option()
            .copied()
            .unwrap_or(ColorU::new_rgb(128, 128, 128)); // Fallback for disabled
        let separator_color = self
            .colors
            .background
            .as_option()
            .copied()
            .unwrap_or(ColorU::new_rgb(200, 200, 200)); // Fallback for separator

        // Get font settings
        let font_size = self.fonts.ui_font_size.as_option().copied().unwrap_or(14.0);
        let font_family = self
            .fonts
            .ui_font
            .as_option()
            .map(|f| f.as_str().to_string())
            .unwrap_or_else(|| "sans-serif".to_string());

        // Get metrics
        let corner_radius = self
            .metrics
            .corner_radius
            .map(|px| px.to_pixels_internal(1.0, DEFAULT_FONT_SIZE))
            .unwrap_or(4.0);
        let padding = 8.0; // Fixed padding value

        // Menu container
        css.push_str(&format!(
            ".menu-container {{\nbackground: rgb({}, {}, {});\nborder: 1px solid rgb(180, 180, \
             180);\nborder-radius: {}px;\nbox-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);\npadding: \
             4px 0;\nmin-width: 160px;\n}}\n",
            bg_color.r, bg_color.g, bg_color.b, corner_radius
        ));

        // Menu item
        css.push_str(&format!(
            ".menu-item {{\ndisplay: flex;\nflex-direction: row;\nalign-items: center;\npadding: \
             {}px {}px;\ncolor: rgb({}, {}, {});\nfont-family: {};\nfont-size: {}px;\ncursor: \
             pointer;\nuser-select: none;\n}}\n",
            padding / 2.0,
            padding,
            text_color.r,
            text_color.g,
            text_color.b,
            font_family,
            font_size
        ));

        // Menu item hover state
        css.push_str(&format!(
            ".menu-item:hover {{\nbackground: rgb({}, {}, {});\ncolor: rgb({}, {}, {});\n}}\n",
            hover_color.r,
            hover_color.g,
            hover_color.b,
            hover_text_color.r,
            hover_text_color.g,
            hover_text_color.b
        ));

        // Disabled menu item
        css.push_str(&format!(
            ".menu-item-disabled, .menu-item-greyed {{\ncolor: rgb({}, {}, {});\ncursor: \
             default;\n}}\n",
            disabled_color.r, disabled_color.g, disabled_color.b
        ));

        // No hover for disabled items
        css.push_str(
            ".menu-item-disabled:hover, .menu-item-greyed:hover {\nbackground: \
             transparent;\ncolor: inherit;\n}\n",
        );

        // Menu item icon
        css.push_str(&format!(
            ".menu-item-icon {{\nwidth: 20px;\nheight: 20px;\nmargin-right: {}px;\ntext-align: \
             center;\nflex-shrink: 0;\n}}\n",
            padding / 2.0
        ));

        // Checkbox styling
        css.push_str(".menu-item-checkbox-checked {\nfont-weight: bold;\n}\n");

        // Menu item label
        css.push_str(".menu-item-label {\nflex-grow: 1;\nwhite-space: nowrap;\n}\n");

        // Menu item shortcut
        css.push_str(&format!(
            ".menu-item-shortcut {{\nmargin-left: {}px;\nopacity: 0.6;\nfont-size: \
             {}px;\nwhite-space: nowrap;\n}}\n",
            padding * 2.0,
            font_size * 0.9
        ));

        // Submenu arrow
        css.push_str(&format!(
            ".menu-item-arrow {{\nmargin-left: {}px;\nopacity: 0.6;\n}}\n",
            padding / 2.0
        ));

        // Separator
        css.push_str(&format!(
            ".menu-separator {{\nheight: 1px;\nbackground: rgb({}, {}, {});\nmargin: 4px \
             8px;\n}}\n",
            separator_color.r, separator_color.g, separator_color.b
        ));

        // Parse CSS and extract first stylesheet
        let (mut parsed_css, _errors) = new_from_str(&css);
        parsed_css.stylesheets.into_library_owned_vec().remove(0)
    }
}

/// Extension trait to add menu stylesheet creation to SystemStyle
pub trait SystemStyleMenuExt {
    /// Create a stylesheet for menu rendering
    fn create_menu_stylesheet(&self) -> Stylesheet;
}
