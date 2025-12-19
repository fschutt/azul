//! Menu Rendering Demo
//!
//! This example demonstrates how to use the menu rendering system
//! to create styled menus from Menu structures.
//!
//! The menu_renderer module converts Menu structures into StyledDom
//! trees that can be rendered as part of the application UI.

#![allow(unused)]

use azul_core::{
    callbacks::Update,
    dom::Dom,
    menu::{Menu, MenuItem, MenuItemIcon, MenuItemState, MenuItemVec, StringMenuItem},
    refany::RefAny,
    styled_dom::StyledDom,
    window::VirtualKeyCode,
};
use azul_css::{system::SystemStyle, AzString};
use azul_dll::desktop::menu_renderer::{create_menu_styled_dom, SystemStyleMenuExt};

/// Example: Create a simple context menu
fn create_simple_context_menu() -> Menu {
    let items = vec![
        MenuItem::String(StringMenuItem::create(AzString::from("Copy"))),
        MenuItem::String(StringMenuItem::create(AzString::from("Paste"))),
        MenuItem::Separator,
        MenuItem::String(StringMenuItem::create(AzString::from("Delete"))),
    ];

    Menu::create(MenuItemVec::from_vec(items))
}

/// Example: Create a menu with icons and shortcuts
fn create_menu_with_features() -> Menu {
    let mut copy_item = StringMenuItem::create(AzString::from("Copy"));
    // TODO: Add keyboard shortcut when VirtualKeyCodeCombo is properly set up
    // copy_item.accelerator = Some(VirtualKeyCodeCombo { ... }).into();

    let mut paste_item = StringMenuItem::create(AzString::from("Paste"));
    // paste_item.accelerator = ...;

    let mut checkbox_item = StringMenuItem::create(AzString::from("Enable Feature"));
    checkbox_item.icon = Some(MenuItemIcon::Checkbox(true)).into();

    let mut disabled_item = StringMenuItem::create(AzString::from("Disabled Item"));
    disabled_item.menu_item_state = MenuItemState::Greyed;

    let items = vec![
        MenuItem::String(copy_item),
        MenuItem::String(paste_item),
        MenuItem::Separator,
        MenuItem::String(checkbox_item),
        MenuItem::String(disabled_item),
    ];

    Menu::create(MenuItemVec::from_vec(items))
}

/// Example: Create a hierarchical menu with submenus
fn create_hierarchical_menu() -> Menu {
    // File submenu
    let file_items = vec![
        MenuItem::String(StringMenuItem::create(AzString::from("New"))),
        MenuItem::String(StringMenuItem::create(AzString::from("Open"))),
        MenuItem::String(StringMenuItem::create(AzString::from("Save"))),
        MenuItem::Separator,
        MenuItem::String(StringMenuItem::create(AzString::from("Exit"))),
    ];

    let mut file_menu = StringMenuItem::create(AzString::from("File"));
    file_menu.children = MenuItemVec::from_vec(file_items);

    // Edit submenu
    let edit_items = vec![
        MenuItem::String(StringMenuItem::create(AzString::from("Undo"))),
        MenuItem::String(StringMenuItem::create(AzString::from("Redo"))),
        MenuItem::Separator,
        MenuItem::String(StringMenuItem::create(AzString::from("Cut"))),
        MenuItem::String(StringMenuItem::create(AzString::from("Copy"))),
        MenuItem::String(StringMenuItem::create(AzString::from("Paste"))),
    ];

    let mut edit_menu = StringMenuItem::create(AzString::from("Edit"));
    edit_menu.children = MenuItemVec::from_vec(edit_items);

    // Top-level menu bar
    let items = vec![MenuItem::String(file_menu), MenuItem::String(edit_menu)];

    Menu::create(MenuItemVec::from_vec(items))
}

/// Example: Render a menu using SystemStyle
fn render_menu_example() {
    // Get system style (detects platform colors, fonts, metrics)
    let system_style = SystemStyle::new();

    // Create a menu
    let menu = create_menu_with_features();

    // Create dummy MenuWindowData for the example
    use std::sync::Arc;

    use azul_core::geom::{LogicalPosition, LogicalRect};
    use azul_dll::desktop::menu::MenuWindowData;

    let menu_window_data = MenuWindowData {
        menu: menu.clone(),
        system_style: Arc::new(system_style.clone()),
        parent_window_position: LogicalPosition::new(0.0, 0.0),
        trigger_rect: None,
        cursor_position: None,
        parent_menu_id: None,
        menu_window_id: None,
        child_menu_ids: Arc::new(std::sync::Mutex::new(Vec::new())),
    };
    let menu_window_data_refany = RefAny::new(menu_window_data);

    // Convert menu to StyledDom with native styling
    let styled_dom = create_menu_styled_dom(&menu, &system_style, menu_window_data_refany);

    println!("Menu rendered successfully!");
    println!("System theme: {:?}", system_style.theme);
    println!("System platform: {:?}", system_style.platform);

    // The styled_dom can now be:
    // 1. Used in a layout callback as part of the UI
    // 2. Converted to display lists for rendering
    // 3. Used with append_child() to add to existing DOM
}

/// Example: Use menu in layout callback
///
/// ```rust,ignore
/// fn my_layout(_info: LayoutCallbackInfo) -> StyledDom {
///     let system_style = &_info.system_style;
///     let menu = create_simple_context_menu();
///     
///     // Create menu DOM
///     let menu_dom = create_menu_styled_dom(&menu, system_style);
///     
///     // Wrap in container
///     let mut container = Dom::new_div();
///     container = container.with_child(menu_dom.into_dom());
///     
///     container.style(/* ... */)
/// }
/// ```

/// Example: CSS customization via SystemStyle
fn demonstrate_css_generation() {
    let system_style = SystemStyle::new();

    // The menu renderer uses SystemStyle to generate CSS automatically:
    // - Colors from system_style.colors (text, background, hover, selection, etc.)
    // - Fonts from system_style.fonts (ui_font, ui_font_size)
    // - Metrics from system_style.metrics (corner_radius, border_width)

    let stylesheet = system_style.create_menu_stylesheet();
    println!(
        "Generated menu stylesheet with {} rules",
        stylesheet.rules.len()
    );
}

fn main() {
    println!("Menu Rendering Demo\n");

    println!("1. Simple context menu:");
    let simple = create_simple_context_menu();
    println!("   - {} items", simple.items.len());

    println!("\n2. Menu with features:");
    let featured = create_menu_with_features();
    println!("   - {} items", featured.items.len());
    println!("   - Icons: checkbox");
    println!("   - States: disabled/greyed");
    println!("   - Shortcuts: (TODO)");

    println!("\n3. Hierarchical menu:");
    let hierarchical = create_hierarchical_menu();
    println!("   - {} top-level items", hierarchical.items.len());

    println!("\n4. Rendering with SystemStyle:");
    render_menu_example();

    println!("\n5. CSS generation:");
    demonstrate_css_generation();

    println!("\nDemo Complete!");
    println!("\nFor integration:");
    println!("1. Use create_menu_styled_dom() in layout callbacks");
    println!("2. SystemStyle provides native look and feel");
    println!("3. Menu items support icons, shortcuts, hover states");
    println!("4. Hierarchical menus with submenus supported");
}
