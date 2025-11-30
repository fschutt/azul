# Getting Started with Rust

This guide shows how to create your first Azul application in Rust.

## Quick Start

Add Azul to your `Cargo.toml`:

```toml
[dependencies]
azul = "1.0.0-alpha5"
```

## Hello World

Here's a minimal Azul application:

```rust
use azul_core::{
    callbacks::{LayoutCallbackInfo, Update},
    dom::Dom,
    refany::RefAny,
    styled_dom::StyledDom,
};
use azul_css::{css::Css, parser2::CssApiWrapper};
use azul_dll::desktop::{app::App, resources::AppConfig};
use azul_layout::window_state::WindowCreateOptions;

// Your application state
struct MyApp {
    counter: usize,
}

// Layout function - converts state to UI
extern "C" fn layout(data: &mut RefAny, _info: &mut LayoutCallbackInfo) -> StyledDom {
    let app = data.downcast_ref::<MyApp>().unwrap();
    
    Dom::body()
        .with_child(Dom::text(format!("Counter: {}", app.counter)))
        .style(CssApiWrapper { css: Css::empty() })
}

fn main() {
    let app = App::new(RefAny::new(MyApp { counter: 0 }), AppConfig::new(layout as usize));
    app.run(WindowCreateOptions::default());
}
```

## Adding Callbacks

Make your UI interactive with event callbacks:

```rust
use azul_core::events::{EventFilter, HoverEventFilter};

// Callback function - runs when user clicks
extern "C" fn on_click(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    let mut app = data.downcast_mut::<MyApp>().unwrap();
    app.counter += 1;
    Update::RefreshDom  // Tell Azul to re-render
}

extern "C" fn layout(data: &mut RefAny, _info: &mut LayoutCallbackInfo) -> StyledDom {
    let app = data.downcast_ref::<MyApp>().unwrap();
    
    let button = Dom::div()
        .with_inline_style("padding: 10px 20px; background: #4a90e2; color: white; cursor: pointer;")
        .with_child(Dom::text("Click me!"));
    
    // Attach callback
    button.root.add_callback(
        EventFilter::Hover(HoverEventFilter::MouseUp),
        data.clone(),
        on_click as usize,
    );
    
    Dom::body()
        .with_child(Dom::text(format!("Counter: {}", app.counter)))
        .with_child(button)
        .style(CssApiWrapper { css: Css::empty() })
}
```

## Inline CSS Styling

Style elements with CSS:

```rust
Dom::div()
    .with_inline_style("
        display: flex;
        flex-direction: column;
        gap: 10px;
        padding: 20px;
        background: #f5f5f5;
        border-radius: 8px;
    ")
    .with_child(Dom::text("Styled container"))
```

## Dynamic Content

Build UI based on state:

```rust
struct TodoApp {
    items: Vec<String>,
}

extern "C" fn layout(data: &mut RefAny, _info: &mut LayoutCallbackInfo) -> StyledDom {
    let app = data.downcast_ref::<TodoApp>().unwrap();
    
    let item_list: Vec<Dom> = app.items.iter()
        .map(|item| Dom::div().with_child(Dom::text(item.clone())))
        .collect();
    
    Dom::body()
        .with_children(item_list.into())
        .style(CssApiWrapper { css: Css::empty() })
}
```

## Menu Bar

Add a native menu bar:

```rust
use azul_core::menu::{Menu, MenuItem, MenuItemVec, StringMenuItem};

let menu = Menu::new(MenuItemVec::from_vec(vec![
    MenuItem::String(StringMenuItem::new("File".into()).with_children(
        MenuItemVec::from_vec(vec![
            MenuItem::String(StringMenuItem::new("New".into())),
            MenuItem::String(StringMenuItem::new("Open".into())),
            MenuItem::Separator,
            MenuItem::String(StringMenuItem::new("Quit".into())),
        ]),
    )),
    MenuItem::String(StringMenuItem::new("Edit".into()).with_children(
        MenuItemVec::from_vec(vec![
            MenuItem::String(StringMenuItem::new("Copy".into())),
            MenuItem::String(StringMenuItem::new("Paste".into())),
        ]),
    )),
]));

Dom::body()
    .with_menu_bar(menu)
    .with_child(/* ... */)
```

## Next Steps

- [CSS Styling](css-styling.html) - Learn about CSS properties
- [Widgets](widgets.html) - Buttons, checkboxes, sliders
- [Architecture](architecture.html) - Understanding the framework design

[Back to overview](https://azul.rs/guide)
