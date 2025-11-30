# Widgets

Azul provides patterns for building common UI widgets. 
This guide shows how to create interactive components.

## Buttons

A button with click counting:

```rust
struct AppState {
    button_clicks: usize,
}

extern "C" fn on_button_click(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    let mut app = data.downcast_mut::<AppState>().unwrap();
    app.button_clicks += 1;
    Update::RefreshDom
}

fn create_button(text: &str, app_data: &RefAny) -> Dom {
    let mut button = Dom::div()
        .with_inline_style("
            padding: 10px 20px;
            background: #4a90e2;
            color: white;
            border-radius: 4px;
            cursor: pointer;
            user-select: none;
        ")
        .with_child(Dom::text(text));
    
    // Add hover effect
    const HOVER_BG: [StyleBackgroundContent; 1] = [StyleBackgroundContent::Color(ColorU {
        r: 60, g: 130, b: 210, a: 255,
    })];
    button.root.add_hover_css_property(CssProperty::BackgroundContent(
        StyleBackgroundContentVec::from_const_slice(&HOVER_BG).into(),
    ));
    
    // Add click callback
    button.root.add_callback(
        EventFilter::Hover(HoverEventFilter::MouseUp),
        app_data.clone(),
        on_button_click as usize,
    );
    
    button
}
```

## Checkboxes

A toggle checkbox:

```rust
struct AppState {
    checkbox_enabled: bool,
}

extern "C" fn on_checkbox_toggle(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    let mut app = data.downcast_mut::<AppState>().unwrap();
    app.checkbox_enabled = !app.checkbox_enabled;
    Update::RefreshDom
}

fn create_checkbox(label: &str, checked: bool, app_data: &RefAny) -> Dom {
    let checkbox_style = if checked {
        "width: 20px; height: 20px; border: 2px solid #4a90e2; border-radius: 4px; \
         background: #4a90e2; display: flex; align-items: center; justify-content: center;"
    } else {
        "width: 20px; height: 20px; border: 2px solid #ccc; border-radius: 4px; \
         background: white;"
    };
    
    let mut checkbox = Dom::div()
        .with_inline_style("display: flex; align-items: center; gap: 10px; cursor: pointer;")
        .with_child(
            Dom::div()
                .with_inline_style(checkbox_style)
                .with_child(if checked { Dom::text("✓") } else { Dom::div() })
        )
        .with_child(Dom::text(label));
    
    checkbox.root.add_callback(
        EventFilter::Hover(HoverEventFilter::MouseUp),
        app_data.clone(),
        on_checkbox_toggle as usize,
    );
    
    checkbox
}
```

## Dropdowns (Native Menus)

Use platform-native menus for dropdowns:

```rust
struct AppState {
    dropdown_selection: usize,
}

extern "C" fn on_option_0(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    let mut app = data.downcast_mut::<AppState>().unwrap();
    app.dropdown_selection = 0;
    Update::RefreshDom
}

extern "C" fn on_option_1(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    let mut app = data.downcast_mut::<AppState>().unwrap();
    app.dropdown_selection = 1;
    Update::RefreshDom
}

extern "C" fn on_dropdown_click(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let hit_node = info.get_hit_node();
    
    let menu = Menu::new(MenuItemVec::from_vec(vec![
        MenuItem::String(
            StringMenuItem::new("Option 1".into())
                .with_callback(data.clone(), on_option_0 as usize),
        ),
        MenuItem::String(
            StringMenuItem::new("Option 2".into())
                .with_callback(data.clone(), on_option_1 as usize),
        ),
    ]));
    
    info.open_menu_for_node(menu, hit_node);
    Update::DoNothing
}

fn create_dropdown(current: &str, app_data: &RefAny) -> Dom {
    let mut dropdown = Dom::div()
        .with_inline_style("
            padding: 10px 15px;
            background: white;
            border: 1px solid #ccc;
            border-radius: 4px;
            cursor: pointer;
            display: flex;
            justify-content: space-between;
            align-items: center;
        ")
        .with_child(Dom::text(current))
        .with_child(Dom::text("▼"));
    
    dropdown.root.add_callback(
        EventFilter::Hover(HoverEventFilter::MouseUp),
        app_data.clone(),
        on_dropdown_click as usize,
    );
    
    dropdown
}
```

## Progress Bars

A visual progress indicator:

```rust
fn create_progress_bar(value: f32) -> Dom {
    let percentage = value.clamp(0.0, 100.0);
    
    Dom::div()
        .with_inline_style("
            width: 100%;
            height: 20px;
            background: #e0e0e0;
            border-radius: 10px;
            overflow: hidden;
        ")
        .with_child(
            Dom::div()
                .with_inline_style(&format!(
                    "width: {}%; height: 100%; background: #4caf50; border-radius: 10px;",
                    percentage
                ))
        )
}
```

## Sliders

An interactive slider with cursor tracking:

```rust
struct AppState {
    slider_value: f32,
}

extern "C" fn on_slider_click(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut app = data.downcast_mut::<AppState>().unwrap();
    let hit_node = info.get_hit_node();
    
    // Get cursor position and element bounds
    if let Some(cursor) = info.get_cursor_position() {
        if let Some(rect) = info.get_hit_node_layout_rect(hit_node) {
            // Calculate percentage based on click position
            let relative_x = cursor.x - rect.x;
            let percentage = (relative_x / rect.width * 100.0).clamp(0.0, 100.0);
            app.slider_value = percentage;
            return Update::RefreshDom;
        }
    }
    
    Update::DoNothing
}

fn create_slider(value: f32, app_data: &RefAny) -> Dom {
    let thumb_position = value.clamp(0.0, 100.0);
    
    let mut slider = Dom::div()
        .with_inline_style("
            width: 100%;
            height: 30px;
            position: relative;
            cursor: pointer;
        ")
        .with_child(
            // Track
            Dom::div()
                .with_inline_style("
                    position: absolute;
                    top: 50%;
                    transform: translateY(-50%);
                    width: 100%;
                    height: 6px;
                    background: #e0e0e0;
                    border-radius: 3px;
                ")
        )
        .with_child(
            // Filled track
            Dom::div()
                .with_inline_style(&format!(
                    "position: absolute; top: 50%; transform: translateY(-50%); \
                     width: {}%; height: 6px; background: #4a90e2; border-radius: 3px;",
                    thumb_position
                ))
        )
        .with_child(
            // Thumb
            Dom::div()
                .with_inline_style(&format!(
                    "position: absolute; top: 50%; left: calc({}% - 10px); \
                     transform: translateY(-50%); width: 20px; height: 20px; \
                     background: #4a90e2; border-radius: 50%; box-shadow: 0 2px 4px rgba(0,0,0,0.2);",
                    thumb_position
                ))
        );
    
    slider.root.add_callback(
        EventFilter::Hover(HoverEventFilter::MouseUp),
        app_data.clone(),
        on_slider_click as usize,
    );
    
    slider
}
```

## Text Inputs

Contenteditable text fields:

```rust
struct AppState {
    text_input: String,
}

extern "C" fn on_text_input(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut app = data.downcast_mut::<AppState>().unwrap();
    
    if let Some(changeset) = info.get_text_changeset() {
        app.text_input = format!("{}{}", changeset.old_text, changeset.inserted_text);
        info.set_text_changeset(changeset.clone());
    }
    
    Update::RefreshDom
}

fn create_text_input(value: &str, app_data: &RefAny) -> Dom {
    let mut input = Dom::div()
        .with_inline_style("
            padding: 10px;
            border: 1px solid #ccc;
            border-radius: 4px;
            min-height: 20px;
            background: white;
        ")
        .with_attribute(AttributeType::Contenteditable, "true")
        .with_child(Dom::text(value));
    
    input.root.add_callback(
        EventFilter::Focus(FocusEventFilter::TextInput),
        app_data.clone(),
        on_text_input as usize,
    );
    
    input
}
```

## Tab Navigation

Tab buttons with the dataset pattern:

```rust
#[derive(Clone)]
struct TabButtonData {
    tab_id: usize,
}

extern "C" fn on_tab_click(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut app = data.downcast_mut::<AppState>().unwrap();
    let hit_node = info.get_hit_node();
    
    if let Some(mut dataset) = info.get_dataset(hit_node) {
        if let Some(tab_data) = dataset.downcast_ref::<TabButtonData>() {
            app.active_tab = tab_data.tab_id;
            return Update::RefreshDom;
        }
    }
    
    Update::DoNothing
}

fn create_tab_button(text: &str, tab_id: usize, is_active: bool) -> Dom {
    let style = if is_active {
        "padding: 10px 20px; background: #4a90e2; color: white; cursor: pointer;"
    } else {
        "padding: 10px 20px; background: #e0e0e0; color: #333; cursor: pointer;"
    };
    
    let mut button = Dom::div()
        .with_inline_style(style)
        .with_dataset(OptionRefAny::Some(RefAny::new(TabButtonData { tab_id })))
        .with_child(Dom::text(text));
    
    if !is_active {
        button.root.add_callback(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            RefAny::new(AppState::default()),
            on_tab_click as usize,
        );
    }
    
    button
}
```

## Dataset Pattern

Store metadata on nodes for generic callbacks:

```rust
// Instead of creating separate callbacks per item:
// on_item_0_click, on_item_1_click, on_item_2_click...

// Use a dataset with a single callback:
#[derive(Clone)]
struct ItemData {
    item_id: usize,
}

extern "C" fn on_item_click(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut app = data.downcast_mut::<AppState>().unwrap();
    let hit_node = info.get_hit_node();
    
    if let Some(mut dataset) = info.get_dataset(hit_node) {
        if let Some(item) = dataset.downcast_ref::<ItemData>() {
            app.selected_item = Some(item.item_id);
            return Update::RefreshDom;
        }
    }
    Update::DoNothing
}

// Apply to items
for (i, item) in items.iter().enumerate() {
    let mut node = Dom::div()
        .with_dataset(OptionRefAny::Some(RefAny::new(ItemData { item_id: i })))
        .with_child(Dom::text(item));
    
    node.root.add_callback(
        EventFilter::Hover(HoverEventFilter::MouseUp),
        app_data.clone(),
        on_item_click as usize,
    );
}
```

[Back to overview](https://azul.rs/guide)
