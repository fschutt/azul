//! Icon System Demo for Azul GUI Framework
//!
//! This example demonstrates:
//! - Creating icons with `Dom::create_icon()`
//! - Using icons in XML with `<icon name="...">` or `<icon>name</icon>`
//! - CSS styling of icons
//! - The automatic icon resolution system
//!
//! Run with: cargo run -p azul-examples --bin icons

use azul::css::StyledDom;
use azul::prelude::*;

const CSS: &str = r#"
body {
    font-family: sans-serif;
    padding: 20px;
}

.container {
    display: flex;
    flex-direction: column;
    gap: 20px;
}

.row {
    display: flex;
    gap: 16px;
    align-items: center;
}

/* Icons can be styled with the 'icon' selector */
icon {
    font-size: 24px;
    color: #333;
}

/* Different color variants using CSS classes */
.primary icon {
    color: #1976d2;
}

.success icon {
    color: #388e3c;
}

.warning icon {
    color: #f57c00;
}

.error icon {
    color: #d32f2f;
}

/* Size variants */
.small icon {
    font-size: 16px;
}

.large icon {
    font-size: 48px;
}

.note {
    margin-top: 20px;
    color: #666;
    font-size: 12px;
}
"#;

struct IconDemo;

extern "C" fn icon_demo_layout(_data: RefAny, _info: LayoutCallbackInfo) -> StyledDom {
    let css = Css::from_string(CSS).unwrap_or_else(|_| Css::empty());
    
    // Create icons using the Rust API
    let home_icon = Dom::create_icon("home");
    let settings_icon = Dom::create_icon("settings");
    let search_icon = Dom::create_icon("search");
    let menu_icon = Dom::create_icon("menu");
    
    // Action icons
    let add_icon = Dom::create_icon("add");
    let done_icon = Dom::create_icon("done");
    let edit_icon = Dom::create_icon("edit");
    let delete_icon = Dom::create_icon("delete");
    
    // Build rows
    let mut nav_row = Dom::create_div();
    nav_row.set_ids_and_classes(IdOrClassVec::from_vec(vec![
        IdOrClass::Class(AzString::from("row"))
    ]));
    nav_row = nav_row
        .with_child(home_icon)
        .with_child(settings_icon)
        .with_child(search_icon)
        .with_child(menu_icon);
    
    let mut action_row = Dom::create_div();
    action_row.set_ids_and_classes(IdOrClassVec::from_vec(vec![
        IdOrClass::Class(AzString::from("row"))
    ]));
    action_row = action_row
        .with_child(add_icon)
        .with_child(done_icon)
        .with_child(edit_icon)
        .with_child(delete_icon);
    
    let mut note = Dom::create_text("Note: Icons show '?' when no icon provider is registered.");
    note.set_ids_and_classes(IdOrClassVec::from_vec(vec![
        IdOrClass::Class(AzString::from("note"))
    ]));
    
    let mut container = Dom::create_div();
    container.set_ids_and_classes(IdOrClassVec::from_vec(vec![
        IdOrClass::Class(AzString::from("container"))
    ]));
    container = container
        .with_child(Dom::create_text("Navigation Icons"))
        .with_child(nav_row)
        .with_child(Dom::create_text("Action Icons"))
        .with_child(action_row)
        .with_child(note);
    
    Dom::create_body()
        .with_child(container)
        .style(css)
}

fn main() {
    println!("Azul Icon System Demo");
    println!("=====================\n");
    println!("This demo shows how to use icons in Azul:");
    println!("  - Dom::create_icon(\"name\") - Create icon nodes");
    println!("  - <icon name=\"...\"> - XML syntax");
    println!("  - CSS 'icon' selector for styling\n");
    println!("Icon resolution order:");
    println!("  1. Registered individual images");
    println!("  2. Icon packs (in priority order)");  
    println!("  3. Placeholder if not found\n");
    
    let data = RefAny::new(IconDemo);
    let app = App::new(data, AppConfig::default());
    let mut window = WindowCreateOptions::new(icon_demo_layout);
    window.window_state.title = AzString::from("Icon System Demo");
    window.window_state.size.dimensions.width = 500.0;
    window.window_state.size.dimensions.height = 400.0;
    app.run(window);
}
