//! Kitchen Sink Example - Comprehensive showcase of Azul features
//!
//! This example demonstrates the Azul API with interactive widgets:
//!
//! ## Features Demonstrated
//!
//! ### Tab 0: Text & Fonts
//! - Text rendering with various styles (fonts, sizes, colors, decorations)
//! - Contenteditable text inputs with TextInput event filter
//! - Text changeset system for tracking input changes
//! - Color showcase with various backgrounds
//! - Scrolling containers
//!
//! ### Tab 1: Interactive Widgets
//! - **Button**: Click counter with hover/active styles
//! - **Checkbox**: Toggle state with visual checkmark (✓)
//! - **Dropdown**: Native platform menu integration
//! - **Progress Bar**: Visual percentage display
//!
//! ### Tab 2: Scrolling Demo
//! - Large scrollable list (50 items)
//! - Overflow: auto handling
//!
//! ### Tab 3: Grid Layout Examples
//! - CSS Grid demonstrations
//! - Various layout patterns
//!
//! ### Tab 4: Slider Demo
//! - Interactive slider with click-to-set value
//! - Uses `info.get_cursor_position()` and `info.get_hit_node_layout_rect()`
//! - Visual thumb position calculated with CSS `calc()`
//! - Real-time value display
//!
//! ### Tab 5: Code Editor
//! - Split-pane layout: Preview (left) | Editor (right)
//! - **Editor**: Contenteditable with monospace font and dark theme
//! - **Preview**: Placeholder for DOM rendering from XHTML
//! - **Toolbar**: Save to File and Refresh Preview buttons
//! - Demonstrates text editing with changeset system
//! - Future: iframe-based virtual scrolling for infinite line support
//!
//! ## Architecture Patterns
//!
//! This example showcases the **dataset pattern** for event handling:
//! - Tab buttons use `.with_dataset(RefAny::new(TabButtonData { tab_id }))`
//! - Single `on_tab_click` callback reads metadata via `info.get_dataset(node)`
//! - Scales better than separate callbacks per button
//!
//! Dropdown demonstrates native menu integration:
//! - `info.open_menu_for_node()` opens platform-native menus
//! - Menu items have callbacks attached via `StringMenuItem.with_callback()`
//! - Menu selection updates app state and triggers re-render
//!
//! Slider demonstrates cursor position tracking:
//! - `info.get_cursor_position()` provides mouse coordinates
//! - `info.get_hit_node_layout_rect()` gives element dimensions
//! - Calculate percentage: `(cursor.x - rect.x) / rect.width * 100`
//!
//! ## Run Instructions
//!
//! ```bash
//! cargo run --bin kitchen_sink --features desktop
//! ```

use std::fs;

use azul_core::{
    callbacks::{
        IFrameCallbackInfo, IFrameCallbackReturn, IFrameCallbackType, LayoutCallbackInfo,
        LayoutCallbackType, Update,
    },
    dom::{AttributeType, Dom, DomId},
    events::{EventFilter, FocusEventFilter, HoverEventFilter, WindowEventFilter},
    geom::{LogicalPosition, LogicalSize},
    menu::{Menu, MenuItem, MenuItemVec, StringMenuItem},
    refany::{OptionRefAny, RefAny},
    styled_dom::{OptionStyledDom, StyledDom},
    window::{WindowFrame, WindowSize},
};
use azul_css::{
    css::Css,
    parser2::CssApiWrapper,
    props::{
        basic::color::ColorU,
        property::CssProperty,
        style::background::{StyleBackgroundContent, StyleBackgroundContentVec},
    },
};
use azul_dll::desktop::{
    app::App, dialogs::save_file_dialog, resources::AppConfig as DllAppConfig,
};
use azul_layout::{callbacks::CallbackInfo, window_state::WindowCreateOptions, xml::DomXmlExt};

// Application state
struct KitchenSinkApp {
    /// Currently active tab (stored but not yet interactive)
    active_tab: usize,
    /// Text input field 1
    text_input_1: String,
    /// Text input field 2
    text_input_2: String,
    /// Multi-line text area
    text_area: String,
    /// Button click counter
    button_counter: usize,
    /// Checkbox state
    checkbox_enabled: bool,
    /// Progress bar value (0-100)
    progress_value: f32,
    /// Selected dropdown option
    dropdown_selection: usize,
    /// Slider value (0-100)
    slider_value: f32,
    /// Code editor content
    code_content: String,
    /// Code editor scroll offset (line number)
    code_scroll_offset: usize,
    /// Code editor font size (configurable via zoom)
    code_font_size: f32,
    /// Code editor line height (calculated from font size)
    code_line_height: f32,
}

impl Default for KitchenSinkApp {
    fn default() -> Self {
        let font_size = 14.0;
        Self {
            active_tab: 0,
            text_input_1: "Type here...".to_string(),
            text_input_2: "Another input...".to_string(),
            text_area: "Multi-line text area.\nYou can type multiple lines here.".to_string(),
            button_counter: 0,
            checkbox_enabled: false,
            progress_value: 35.0,
            dropdown_selection: 0,
            slider_value: 50.0,
            code_content: Self::generate_sample_code(),
            code_scroll_offset: 0,
            code_font_size: font_size,
            code_line_height: font_size * 1.5,
        }
    }
}

impl KitchenSinkApp {
    /// Generate a large sample HTML file to demonstrate virtual scrolling
    fn generate_sample_code() -> String {
        let mut code = String::from(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Azul Code Editor Demo</title>
    <style>
        body { font-family: Arial, sans-serif; margin: 20px; }
        .container { max-width: 800px; margin: 0 auto; }
        h1 { color: #4a90e2; }
        p { line-height: 1.6; }
    </style>
</head>
<body>
    <div class="container">
        <h1>Welcome to Azul Code Editor</h1>
        <p>This editor demonstrates virtual scrolling for large files.</p>
        <p>Only visible lines are rendered, allowing infinite file sizes.</p>
"#,
        );

        // Generate many lines to test virtual scrolling (simulate a large file)
        for i in 1..=100 {
            code.push_str(&format!(
                r#"        <div class="item-{}">
            <h2>Section {}</h2>
            <p>This is paragraph {} with some sample content to demonstrate scrolling.</p>
            <p>Line height is 21px (14px * 1.5), and we render 2x window height.</p>
        </div>
"#,
                i, i, i
            ));
        }

        code.push_str(
            r#"    </div>
</body>
</html>"#,
        );

        code
    }
}

// Main layout

// Dataset for tab buttons - stores which tab this button represents
// Dataset for tab buttons - stores which tab this button represents
#[derive(Debug, Clone)]
struct TabButtonData {
    tab_id: usize,
}

// Generic tab click callback - reads tab_id from node's dataset
extern "C" fn on_tab_click(_data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut app_data = _data.downcast_mut::<KitchenSinkApp>().unwrap();

    // Get the tab_id from the clicked node's dataset
    let hit_node = info.get_hit_node();
    if let Some(mut dataset) = info.get_dataset(hit_node) {
        if let Some(tab_data) = dataset.downcast_ref::<TabButtonData>() {
            app_data.active_tab = tab_data.tab_id;
            return Update::RefreshDom;
        }
    }

    Update::DoNothing
}

// Button counter callback
extern "C" fn on_button_click(_data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    let mut app_data = _data.downcast_mut::<KitchenSinkApp>().unwrap();
    app_data.button_counter += 1;
    Update::RefreshDom
}

// Checkbox toggle callback
extern "C" fn on_checkbox_toggle(_data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    let mut app_data = _data.downcast_mut::<KitchenSinkApp>().unwrap();
    app_data.checkbox_enabled = !app_data.checkbox_enabled;
    Update::RefreshDom
}

// Dropdown menu option callbacks - directly set the selection
extern "C" fn on_dropdown_option_0(_data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    let mut app_data = _data.downcast_mut::<KitchenSinkApp>().unwrap();
    app_data.dropdown_selection = 0;
    Update::RefreshDom
}

extern "C" fn on_dropdown_option_1(_data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    let mut app_data = _data.downcast_mut::<KitchenSinkApp>().unwrap();
    app_data.dropdown_selection = 1;
    Update::RefreshDom
}

extern "C" fn on_dropdown_option_2(_data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    let mut app_data = _data.downcast_mut::<KitchenSinkApp>().unwrap();
    app_data.dropdown_selection = 2;
    Update::RefreshDom
}

// Dropdown button callback - opens native menu
extern "C" fn on_dropdown_button_click(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let hit_node = info.get_hit_node();

    // Create menu with 3 options - pass app_data to each menu item callback
    let menu = Menu::new(MenuItemVec::from_vec(vec![
        MenuItem::String(
            StringMenuItem::new("Option 1".into())
                .with_callback(data.clone(), on_dropdown_option_0 as usize),
        ),
        MenuItem::String(
            StringMenuItem::new("Option 2".into())
                .with_callback(data.clone(), on_dropdown_option_1 as usize),
        ),
        MenuItem::String(
            StringMenuItem::new("Option 3".into())
                .with_callback(data.clone(), on_dropdown_option_2 as usize),
        ),
    ]));

    // Open menu at the clicked node position
    info.open_menu_for_node(menu, hit_node);

    Update::DoNothing
}

// Code export menu callbacks

// Export code to Rust
extern "C" fn on_export_rust(_data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    let app_data = _data.downcast_ref::<KitchenSinkApp>().unwrap();

    // Get Rust code from editor content
    let rust_code = compile_to_rust(&app_data.code_content);

    // Open save dialog
    if let Some(path) = save_file_dialog("Export Rust Code", Some("output.rs")) {
        if let Err(e) = fs::write(path.as_str(), rust_code) {
            eprintln!("[EXPORT] Failed to write Rust file: {}", e);
        } else {
            eprintln!("[EXPORT] Rust code exported successfully");
        }
    }

    Update::DoNothing
}

// Export code to C
extern "C" fn on_export_c(_data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    let app_data = _data.downcast_ref::<KitchenSinkApp>().unwrap();

    let c_code = compile_to_c(&app_data.code_content);

    if let Some(path) = save_file_dialog("Export C Code", Some("output.c")) {
        if let Err(e) = fs::write(path.as_str(), c_code) {
            eprintln!("[EXPORT] Failed to write C file: {}", e);
        } else {
            eprintln!("[EXPORT] C code exported successfully");
        }
    }

    Update::DoNothing
}

// Export code to C++
extern "C" fn on_export_cpp(_data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    let app_data = _data.downcast_ref::<KitchenSinkApp>().unwrap();

    let cpp_code = compile_to_cpp(&app_data.code_content);

    if let Some(path) = save_file_dialog("Export C++ Code", Some("output.cpp")) {
        if let Err(e) = fs::write(path.as_str(), cpp_code) {
            eprintln!("[EXPORT] Failed to write C++ file: {}", e);
        } else {
            eprintln!("[EXPORT] C++ code exported successfully");
        }
    }

    Update::DoNothing
}

// Export code to Python
extern "C" fn on_export_python(_data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    let app_data = _data.downcast_ref::<KitchenSinkApp>().unwrap();

    let python_code = compile_to_python(&app_data.code_content);

    if let Some(path) = save_file_dialog("Export Python Code", Some("output.py")) {
        if let Err(e) = fs::write(path.as_str(), python_code) {
            eprintln!("[EXPORT] Failed to write Python file: {}", e);
        } else {
            eprintln!("[EXPORT] Python code exported successfully");
        }
    }

    Update::DoNothing
}

// Code compilation functions

fn compile_to_rust(xml_content: &str) -> String {
    #[cfg(feature = "xml")]
    {
        use azul_core::xml::{str_to_rust_code, XmlComponentMap};
        use azul_layout::xml::parse_xml_string;

        // Parse XML string
        let parsed = match parse_xml_string(xml_content) {
            Ok(parsed) => parsed,
            Err(e) => {
                return format!(
                    "// Error parsing XML:\n// {}\n\nfn main() {{\nprintln!(\"XML Parse \
                     Error\");\n}}",
                    e
                );
            }
        };

        // Compile to Rust
        let mut component_map = XmlComponentMap::default();
        match str_to_rust_code(parsed.as_ref(), "", &mut component_map) {
            Ok(rust_code) => rust_code,
            Err(e) => {
                format!(
                    "// Error compiling XML to Rust:\n// {}\n\nfn main() \
                     {{\nprintln!(\"Compilation Error\");\n}}",
                    e
                )
            }
        }
    }

    #[cfg(not(feature = "xml"))]
    {
        format!(
            "// XML compilation requires 'xml' feature\n// Source XML:\n/*\n{}\n*/\n\nfn main() \
             {{\nprintln!(\"XML feature not enabled\");\n}}\n",
            xml_content
        )
    }
}

fn compile_to_c(xml_content: &str) -> String {
    // TODO: Implement XML -> C compilation
    format!(
        "/* Auto-generated C code from XML */\n/* Source XML:\n{}\n*/\n\n/* TODO: Implement XML \
         to C compilation */\n#include <stdio.h>\n\nint main() {{\nprintf(\"Generated from \
         XML\\n\");\nreturn 0;\n}}\n",
        xml_content
    )
}

fn compile_to_cpp(xml_content: &str) -> String {
    // TODO: Implement XML -> C++ compilation
    format!(
        "// Auto-generated C++ code from XML\n// Source XML:\n/*\n{}\n*/\n\n// TODO: Implement \
         XML to C++ compilation\n#include <iostream>\n\nint main() {{\nstd::cout << \"Generated \
         from XML\" << std::endl;\nreturn 0;\n}}\n",
        xml_content
    )
}

fn compile_to_python(xml_content: &str) -> String {
    // TODO: Implement XML -> Python compilation
    format!(
        "# Auto-generated Python code from XML\n# Source XML:\n'''\n{}\n'''\n\n# TODO: Implement \
         XML to Python compilation\ndef main():\nprint('Generated from XML')\n\nif __name__ == \
         '__main__':\nmain()\n",
        xml_content
    )
}

// Text input callbacks
extern "C" fn on_text_input_1(_data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut app_data = _data.downcast_mut::<KitchenSinkApp>().unwrap();

    // Get the text changeset from the layout engine
    if let Some(changeset) = info.get_text_changeset() {
        // Apply the changes to our state
        // The changeset has inserted_text and old_text, we need to compute the new text
        app_data.text_input_1 = format!("{}{}", changeset.old_text, changeset.inserted_text);

        // Set the changeset back (this updates cursor position, etc.)
        info.set_text_changeset(changeset.clone());
    }

    Update::RefreshDom
}

extern "C" fn on_text_input_2(_data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut app_data = _data.downcast_mut::<KitchenSinkApp>().unwrap();

    if let Some(changeset) = info.get_text_changeset() {
        app_data.text_input_2 = format!("{}{}", changeset.old_text, changeset.inserted_text);
        info.set_text_changeset(changeset.clone());
    }

    Update::RefreshDom
}

extern "C" fn on_text_area(_data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut app_data = _data.downcast_mut::<KitchenSinkApp>().unwrap();

    if let Some(changeset) = info.get_text_changeset() {
        app_data.text_area = format!("{}{}", changeset.old_text, changeset.inserted_text);
        info.set_text_changeset(changeset.clone());
    }

    Update::RefreshDom
}

extern "C" fn main_layout(_data: &mut RefAny, _info: &mut LayoutCallbackInfo) -> StyledDom {
    eprintln!("[KITCHEN_SINK] main_layout called");

    let data_clone = _data.clone();
    eprintln!("[KITCHEN_SINK] data cloned");

    let app_data = match _data.downcast_ref::<KitchenSinkApp>() {
        Some(data) => {
            eprintln!(
                "[KITCHEN_SINK] Got app_data, active_tab={}",
                data.active_tab
            );
            data
        }
        None => {
            eprintln!("[KITCHEN_SINK] ERROR: Failed to downcast to KitchenSinkApp!");
            panic!("Failed to downcast data");
        }
    };

    eprintln!("[KITCHEN_SINK] Creating menu...");
    // Menu bar - changes based on active tab
    let menu = if app_data.active_tab == 5 {
        // Code Editor tab - show Compile and Debug menus
        Menu::new(MenuItemVec::from_vec(vec![
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
            MenuItem::String(StringMenuItem::new("View".into()).with_children(
                MenuItemVec::from_vec(vec![
                    MenuItem::String(StringMenuItem::new("Zoom In".into())),
                    MenuItem::String(StringMenuItem::new("Zoom Out".into())),
                ]),
            )),
            MenuItem::String(StringMenuItem::new("Compile".into()).with_children(
                MenuItemVec::from_vec(vec![
                    MenuItem::String(
                        StringMenuItem::new("Export to Rust...".into())
                            .with_callback(data_clone.clone(), on_export_rust as usize),
                    ),
                    MenuItem::String(
                        StringMenuItem::new("Export to C...".into())
                            .with_callback(data_clone.clone(), on_export_c as usize),
                    ),
                    MenuItem::String(
                        StringMenuItem::new("Export to C++...".into())
                            .with_callback(data_clone.clone(), on_export_cpp as usize),
                    ),
                    MenuItem::String(
                        StringMenuItem::new("Export to Python...".into())
                            .with_callback(data_clone.clone(), on_export_python as usize),
                    ),
                ]),
            )),
            MenuItem::String(StringMenuItem::new("Debug".into()).with_children(
                MenuItemVec::from_vec(vec![
                    MenuItem::String(StringMenuItem::new("Run".into())),
                    MenuItem::String(StringMenuItem::new("Step Over".into())),
                    MenuItem::String(StringMenuItem::new("Step Into".into())),
                ]),
            )),
        ]))
    } else {
        // Other tabs - standard menu without Compile/Debug
        Menu::new(MenuItemVec::from_vec(vec![
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
            MenuItem::String(StringMenuItem::new("View".into()).with_children(
                MenuItemVec::from_vec(vec![
                    MenuItem::String(StringMenuItem::new("Zoom In".into())),
                    MenuItem::String(StringMenuItem::new("Zoom Out".into())),
                ]),
            )),
        ]))
    };
    eprintln!("[KITCHEN_SINK] Menu created");

    // Tab bar
    eprintln!("[KITCHEN_SINK] Creating tab bar...");
    let tab_bar = Dom::div()
        .with_inline_style(
            "display: flex; gap: 5px; padding: 10px; background: #f5f5f5; border-bottom: 2px \
             solid #ccc;",
        )
        .with_children(
            vec![
                create_tab_button("Text & Fonts", 0, app_data.active_tab == 0),
                create_tab_button("Widgets", 1, app_data.active_tab == 1),
                create_tab_button("Scrolling", 2, app_data.active_tab == 2),
                create_tab_button("Grid Layout", 3, app_data.active_tab == 3),
                create_tab_button("Slider Demo", 4, app_data.active_tab == 4),
                create_tab_button("Code Editor", 5, app_data.active_tab == 5),
            ]
            .into(),
        );
    eprintln!("[KITCHEN_SINK] Tab bar created");

    // Main content area with 4-quadrant grid layout
    eprintln!(
        "[KITCHEN_SINK] Creating content area for tab {}...",
        app_data.active_tab
    );
    let content = Dom::div()
        .with_inline_style(
            "display: grid; grid-template-columns: 1fr 1fr; grid-template-rows: 1fr 1fr; gap: \
             20px; padding: 20px; height: calc(100% - 60px);",
        )
        .with_children(
            match app_data.active_tab {
                0 => vec![
                    // Tab 0: Text & Fonts showcase
                    create_text_showcase(),
                    create_color_showcase(),
                    create_contenteditable_demo(&*app_data),
                    create_scrolling_demo(),
                ],
                1 => vec![
                    // Tab 1: Interactive widgets
                    create_button_demo(&*app_data),
                    create_checkbox_demo(&*app_data),
                    create_dropdown_demo(&*app_data),
                    create_progress_demo(&*app_data),
                ],
                2 => vec![
                    // Tab 2: Scrolling demo (full width)
                    Dom::div()
                        .with_inline_style("grid-column: 1 / -1; grid-row: 1 / -1;")
                        .with_child(create_scrolling_demo()),
                ],
                3 => vec![
                    // Tab 3: Grid layout examples
                    create_text_showcase(),
                    create_color_showcase(),
                    create_contenteditable_demo(&*app_data),
                    create_scrolling_demo(),
                ],
                4 => vec![
                    // Tab 4: Slider demo (full width)
                    Dom::div()
                        .with_inline_style("grid-column: 1 / -1; grid-row: 1 / -1;")
                        .with_child(create_slider_demo(&*app_data)),
                ],
                5 => vec![
                    // Tab 5: Code Editor (full width)
                    Dom::div()
                        .with_inline_style("grid-column: 1 / -1; grid-row: 1 / -1;")
                        .with_child(create_code_editor(&*app_data, data_clone.clone())),
                ],
                _ => vec![Dom::div().with_child(Dom::text("Unknown tab"))],
            }
            .into(),
        );

    eprintln!("[KITCHEN_SINK] Creating body DOM...");
    let styled = Dom::body()
        .with_inline_style(
            "margin: 0; padding: 0; box-sizing: border-box; font-family: -apple-system, \
             BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif; \
             font-size: 16px; line-height: 1.5; height: 100vh; display: flex; flex-direction: \
             column;",
        )
        .with_menu_bar(menu)
        .with_children(vec![tab_bar, content].into())
        .style(CssApiWrapper { css: Css::empty() });

    eprintln!("[KITCHEN_SINK] main_layout returning StyledDom");
    styled
}

fn create_tab_button(text: &str, tab_id: usize, is_active: bool) -> Dom {
    const HOVER_BG: [StyleBackgroundContent; 1] = [StyleBackgroundContent::Color(ColorU {
        r: 200,
        g: 200,
        b: 200,
        a: 255,
    })];
    const ACTIVE_BG: [StyleBackgroundContent; 1] = [StyleBackgroundContent::Color(ColorU {
        r: 180,
        g: 180,
        b: 180,
        a: 255,
    })];

    let base_style = if is_active {
        "padding: 10px 20px; background: #4a90e2; color: white; border-radius: 4px 4px 0 0; \
         cursor: pointer; user-select: none; font-weight: bold; box-sizing: border-box;"
    } else {
        "padding: 10px 20px; background: #e0e0e0; color: #333; border-radius: 4px 4px 0 0; cursor: \
         pointer; user-select: none; box-sizing: border-box;"
    };

    let mut button = Dom::div()
        .with_inline_style(base_style)
        .with_dataset(OptionRefAny::Some(RefAny::new(TabButtonData { tab_id })))
        .with_child(Dom::text(text));

    // Add hover effect (lighter background)
    if !is_active {
        button
            .root
            .add_hover_css_property(CssProperty::BackgroundContent(
                StyleBackgroundContentVec::from_const_slice(&HOVER_BG).into(),
            ));
    }

    // Add active/pressed effect (darker)
    if !is_active {
        button
            .root
            .add_active_css_property(CssProperty::BackgroundContent(
                StyleBackgroundContentVec::from_const_slice(&ACTIVE_BG).into(),
            ));
    }

    // Add click callback - only for inactive tabs
    if !is_active {
        button.root.add_callback(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            RefAny::new(KitchenSinkApp::default()),
            on_tab_click as usize,
        );
    }

    button
}

// Text & font showcase

fn create_text_showcase() -> Dom {
    Dom::div()
        .with_inline_style(
            "border: 1px solid #ccc; padding: 15px; border-radius: 8px; background: #f9f9f9; \
             overflow: auto; box-sizing: border-box; height: 100%;",
        )
        .with_children(
            vec![
                Dom::div()
                    .with_inline_style(
                        "font-size: 20px; font-weight: bold; margin-bottom: 15px; color: #333;",
                    )
                    .with_child(Dom::text("Text & Font Showcase")),
                Dom::div()
                    .with_inline_style("margin: 8px 0;")
                    .with_child(Dom::text("Default text (16px, regular weight)")),
                Dom::div()
                    .with_inline_style("font-size: 24px; margin: 8px 0;")
                    .with_child(Dom::text("Large Text (24px)")),
                Dom::div()
                    .with_inline_style("font-size: 14px; margin: 8px 0;")
                    .with_child(Dom::text("Small Text (14px)")),
                Dom::div()
                    .with_inline_style("font-weight: bold; margin: 8px 0;")
                    .with_child(Dom::text("Bold Text")),
                Dom::div()
                    .with_inline_style("font-style: italic; margin: 8px 0;")
                    .with_child(Dom::text("Italic Text")),
                Dom::div()
                    .with_inline_style("text-decoration: underline; margin: 8px 0;")
                    .with_child(Dom::text("Underlined Text")),
                Dom::div()
                    .with_inline_style("text-decoration: line-through; margin: 8px 0;")
                    .with_child(Dom::text("Strikethrough Text")),
                Dom::div()
                    .with_inline_style("color: #4a90e2; margin: 8px 0;")
                    .with_child(Dom::text("Blue Colored Text")),
                Dom::div()
                    .with_inline_style("color: #4caf50; margin: 8px 0;")
                    .with_child(Dom::text("Green Colored Text")),
                Dom::div()
                    .with_inline_style("color: #f44336; margin: 8px 0;")
                    .with_child(Dom::text("Red Colored Text")),
                Dom::div()
                    .with_inline_style(
                        "font-family: monospace; background: #f0f0f0; padding: 8px; \
                         border-radius: 4px; margin: 8px 0;",
                    )
                    .with_child(Dom::text("Monospace Font (for code)")),
            ]
            .into(),
        )
}
    
// Color & shape showcase

fn create_color_showcase() -> Dom {
    Dom::div()
        .with_inline_style(
            "border: 1px solid #ccc; padding: 15px; border-radius: 8px; background: #f9f9f9; \
             overflow: auto; box-sizing: border-box; height: 100%;",
        )
        .with_children(
            vec![
                Dom::div()
                    .with_inline_style(
                        "font-size: 20px; font-weight: bold; margin-bottom: 15px; color: #333;",
                    )
                    .with_child(Dom::text("Colors & Shapes")),
                // Color grid
                Dom::div()
                    .with_inline_style(
                        "display: grid; grid-template-columns: repeat(4, 1fr); gap: 10px; \
                         margin-bottom: 20px;",
                    )
                    .with_children(
                        vec![
                            create_color_box("#FF0000", "Red"),
                            create_color_box("#00FF00", "Green"),
                            create_color_box("#0000FF", "Blue"),
                            create_color_box("#FFFF00", "Yellow"),
                            create_color_box("#FF00FF", "Magenta"),
                            create_color_box("#00FFFF", "Cyan"),
                            create_color_box("#FFA500", "Orange"),
                            create_color_box("#800080", "Purple"),
                        ]
                        .into(),
                    ),
                // Shapes with borders
                Dom::div()
                    .with_inline_style("font-size: 16px; font-weight: bold; margin: 15px 0 10px 0;")
                    .with_child(Dom::text("Shapes & Borders")),
                Dom::div()
                    .with_inline_style("display: flex; gap: 15px; flex-wrap: wrap;")
                    .with_children(
                        vec![
                            Dom::div().with_inline_style(
                                "width: 80px; height: 80px; background: #4a90e2; border: 3px \
                                 solid #2171b5; border-radius: 8px;",
                            ),
                            Dom::div().with_inline_style(
                                "width: 80px; height: 80px; background: #4caf50; border-radius: \
                                 50%;",
                            ),
                            Dom::div().with_inline_style(
                                "width: 0; height: 0; border-left: 40px solid transparent; \
                                 border-right: 40px solid transparent; border-bottom: 80px solid \
                                 #ff9800;",
                            ),
                        ]
                        .into(),
                    ),
                // Gradient boxes
                Dom::div()
                    .with_inline_style("font-size: 16px; font-weight: bold; margin: 15px 0 10px 0;")
                    .with_child(Dom::text("Gradients")),
                Dom::div().with_inline_style(
                    "width: 100%; height: 60px; background: linear-gradient(to right, #4a90e2, \
                     #2196f3); border-radius: 8px; margin-bottom: 10px;",
                ),
                Dom::div().with_inline_style(
                    "width: 100%; height: 60px; background: linear-gradient(to bottom, #ff9800, \
                     #f44336); border-radius: 8px;",
                ),
            ]
            .into(),
        )
}

fn create_color_box(color: &str, label: &str) -> Dom {
    Dom::div()
        .with_inline_style("display: flex; flex-direction: column; align-items: center; gap: 5px;")
        .with_children(
            vec![
                Dom::div().with_inline_style(&format!(
                    "width: 60px; height: 60px; background: {}; border-radius: 8px; border: 2px \
                     solid #333;",
                    color
                )),
                Dom::div()
                    .with_inline_style("font-size: 12px; color: #666;")
                    .with_child(Dom::text(label)),
            ]
            .into(),
        )
}

// Contenteditable demo

fn create_contenteditable_demo(app_data: &KitchenSinkApp) -> Dom {
    Dom::div()
        .with_inline_style(
            "border: 1px solid #ccc; padding: 15px; border-radius: 8px; background: #f9f9f9; \
             overflow: auto; box-sizing: border-box; height: 100%;",
        )
        .with_children(
            vec![
                Dom::div()
                    .with_inline_style(
                        "font-size: 20px; font-weight: bold; margin-bottom: 15px; color: #333;",
                    )
                    .with_child(Dom::text("Text Input (Interactive)")),
                Dom::div()
                    .with_inline_style("margin-bottom: 15px; color: #666; font-size: 14px;")
                    .with_child(Dom::text(
                        "Click in the boxes below to edit text - now with working callbacks!",
                    )),
                // Single line input 1
                Dom::div()
                    .with_inline_style("margin-bottom: 15px;")
                    .with_children(
                        vec![
                            Dom::div()
                                .with_inline_style(
                                    "font-size: 14px; font-weight: bold; margin-bottom: 5px;",
                                )
                                .with_child(Dom::text("Single Line Input 1:")),
                            {
                                let mut input = Dom::div()
                                    .with_attribute(AttributeType::ContentEditable(true))
                                    .with_attribute(AttributeType::TabIndex(0.into()))
                                    .with_attribute(AttributeType::AriaLabel(
                                        "Single line text input".into(),
                                    ))
                                    .with_inline_style(
                                        "border: 2px solid #4a90e2; padding: 8px; border-radius: \
                                         4px; background: white; min-height: 30px; box-sizing: \
                                         border-box; outline: none;",
                                    )
                                    .with_child(Dom::text(app_data.text_input_1.as_str()));

                                // Add text input callback
                                input.root.add_callback(
                                    EventFilter::Focus(FocusEventFilter::TextInput),
                                    RefAny::new(KitchenSinkApp::default()), /* Will be replaced
                                                                             * with actual data
                                                                             * at runtime */
                                    on_text_input_1 as usize,
                                );

                                input
                            },
                        ]
                        .into(),
                    ),
                // Single line input 2
                Dom::div()
                    .with_inline_style("margin-bottom: 15px;")
                    .with_children(
                        vec![
                            Dom::div()
                                .with_inline_style(
                                    "font-size: 14px; font-weight: bold; margin-bottom: 5px;",
                                )
                                .with_child(Dom::text("Single Line Input 2:")),
                            {
                                let mut input = Dom::div()
                                    .with_attribute(AttributeType::ContentEditable(true))
                                    .with_attribute(AttributeType::TabIndex(1.into()))
                                    .with_attribute(AttributeType::AriaLabel(
                                        "Another text input".into(),
                                    ))
                                    .with_inline_style(
                                        "border: 2px solid #9c27b0; padding: 8px; border-radius: \
                                         4px; background: white; min-height: 30px; box-sizing: \
                                         border-box; outline: none;",
                                    )
                                    .with_child(Dom::text(app_data.text_input_2.as_str()));

                                input.root.add_callback(
                                    EventFilter::Focus(FocusEventFilter::TextInput),
                                    RefAny::new(KitchenSinkApp::default()),
                                    on_text_input_2 as usize,
                                );

                                input
                            },
                        ]
                        .into(),
                    ),
                // Multi-line text area
                Dom::div().with_children(
                    vec![
                        Dom::div()
                            .with_inline_style(
                                "font-size: 14px; font-weight: bold; margin-bottom: 5px;",
                            )
                            .with_child(Dom::text("Multi-line Text Area:")),
                        {
                            let mut textarea = Dom::div()
                                .with_attribute(AttributeType::ContentEditable(true))
                                .with_attribute(AttributeType::TabIndex(2.into()))
                                .with_attribute(AttributeType::AriaLabel(
                                    "Multi-line text area".into(),
                                ))
                                .with_inline_style(
                                    "border: 2px solid #4caf50; padding: 8px; border-radius: 4px; \
                                     background: white; min-height: 100px; white-space: pre-wrap; \
                                     box-sizing: border-box; outline: none;",
                                )
                                .with_child(Dom::text(app_data.text_area.as_str()));

                            textarea.root.add_callback(
                                EventFilter::Focus(FocusEventFilter::TextInput),
                                RefAny::new(KitchenSinkApp::default()),
                                on_text_area as usize,
                            );

                            textarea
                        },
                    ]
                    .into(),
                ),
            ]
            .into(),
        )
}

// Widget demos (Tab 1)

fn create_button_demo(app_data: &KitchenSinkApp) -> Dom {
    Dom::div()
        .with_inline_style(
            "border: 1px solid #ccc; padding: 15px; border-radius: 8px; background: #f9f9f9; \
             box-sizing: border-box;",
        )
        .with_children(
            vec![
                Dom::div()
                    .with_inline_style(
                        "font-size: 20px; font-weight: bold; margin-bottom: 15px; color: #333;",
                    )
                    .with_child(Dom::text("Button Demo")),
                Dom::div()
                    .with_inline_style("margin-bottom: 10px; color: #666; font-size: 14px;")
                    .with_child(Dom::text(
                        format!("Button clicked {} times", app_data.button_counter).as_str(),
                    )),
                {
                    const HOVER_BG: [StyleBackgroundContent; 1] =
                        [StyleBackgroundContent::Color(ColorU {
                            r: 66,
                            g: 135,
                            b: 245,
                            a: 255,
                        })];
                    const ACTIVE_BG: [StyleBackgroundContent; 1] =
                        [StyleBackgroundContent::Color(ColorU {
                            r: 50,
                            g: 100,
                            b: 200,
                            a: 255,
                        })];

                    let mut button = Dom::div()
                        .with_inline_style(
                            "padding: 10px 20px; background: #4a90e2; color: white; \
                             border-radius: 4px; cursor: pointer; user-select: none; display: \
                             inline-block; font-weight: bold;",
                        )
                        .with_child(Dom::text("Click Me!"));

                    button
                        .root
                        .add_hover_css_property(CssProperty::BackgroundContent(
                            StyleBackgroundContentVec::from_const_slice(&HOVER_BG).into(),
                        ));
                    button
                        .root
                        .add_active_css_property(CssProperty::BackgroundContent(
                            StyleBackgroundContentVec::from_const_slice(&ACTIVE_BG).into(),
                        ));
                    button.root.add_callback(
                        EventFilter::Hover(HoverEventFilter::MouseUp),
                        RefAny::new(KitchenSinkApp::default()),
                        on_button_click as usize,
                    );

                    button
                },
            ]
            .into(),
        )
}

fn create_checkbox_demo(app_data: &KitchenSinkApp) -> Dom {
    Dom::div()
        .with_inline_style(
            "border: 1px solid #ccc; padding: 15px; border-radius: 8px; background: #f9f9f9; \
             box-sizing: border-box;",
        )
        .with_children(
            vec![
                Dom::div()
                    .with_inline_style(
                        "font-size: 20px; font-weight: bold; margin-bottom: 15px; color: #333;",
                    )
                    .with_child(Dom::text("Checkbox Demo")),
                {
                    const HOVER_BG: [StyleBackgroundContent; 1] =
                        [StyleBackgroundContent::Color(ColorU {
                            r: 240,
                            g: 240,
                            b: 240,
                            a: 255,
                        })];
                    const ACTIVE_BG: [StyleBackgroundContent; 1] =
                        [StyleBackgroundContent::Color(ColorU {
                            r: 220,
                            g: 220,
                            b: 220,
                            a: 255,
                        })];

                    let checkmark = if app_data.checkbox_enabled { "✓" } else { "" };
                    let bg_color = if app_data.checkbox_enabled {
                        "#4a90e2"
                    } else {
                        "white"
                    };

                    let mut checkbox = Dom::div()
                        .with_inline_style(
                            "display: flex; align-items: center; gap: 10px; cursor: pointer; \
                             user-select: none;",
                        )
                        .with_children(
                            vec![
                                {
                                    let mut box_div = Dom::div()
                                        .with_inline_style(
                                            format!(
                                                "width: 20px; height: 20px; border: 2px solid \
                                                 #4a90e2; border-radius: 3px; background: {}; \
                                                 display: flex; align-items: center; \
                                                 justify-content: center; color: white; \
                                                 font-weight: bold; font-size: 16px;",
                                                bg_color
                                            )
                                            .as_str(),
                                        )
                                        .with_child(Dom::text(checkmark));

                                    box_div.root.add_hover_css_property(
                                        CssProperty::BackgroundContent(
                                            StyleBackgroundContentVec::from_const_slice(&HOVER_BG)
                                                .into(),
                                        ),
                                    );
                                    box_div.root.add_active_css_property(
                                        CssProperty::BackgroundContent(
                                            StyleBackgroundContentVec::from_const_slice(&ACTIVE_BG)
                                                .into(),
                                        ),
                                    );

                                    box_div
                                },
                                Dom::div()
                                    .with_inline_style("font-size: 14px;")
                                    .with_child(Dom::text("Enable feature")),
                            ]
                            .into(),
                        );

                    checkbox.root.add_callback(
                        EventFilter::Hover(HoverEventFilter::MouseUp),
                        RefAny::new(KitchenSinkApp::default()),
                        on_checkbox_toggle as usize,
                    );

                    checkbox
                },
            ]
            .into(),
        )
}

fn create_dropdown_demo(app_data: &KitchenSinkApp) -> Dom {
    const HOVER_BG: [StyleBackgroundContent; 1] = [StyleBackgroundContent::Color(ColorU {
        r: 240,
        g: 240,
        b: 255,
        a: 255,
    })];
    const ACTIVE_BG: [StyleBackgroundContent; 1] = [StyleBackgroundContent::Color(ColorU {
        r: 220,
        g: 220,
        b: 240,
        a: 255,
    })];

    let options = ["Option 1", "Option 2", "Option 3"];
    let selected = options
        .get(app_data.dropdown_selection)
        .unwrap_or(&"Select...");

    Dom::div()
        .with_inline_style(
            "border: 1px solid #ccc; padding: 15px; border-radius: 8px; background: #f9f9f9; \
             box-sizing: border-box;",
        )
        .with_children(
            vec![
                Dom::div()
                    .with_inline_style(
                        "font-size: 20px; font-weight: bold; margin-bottom: 15px; color: #333;",
                    )
                    .with_child(Dom::text("Dropdown Demo")),
                Dom::div()
                    .with_inline_style("margin-bottom: 10px; color: #666; font-size: 14px;")
                    .with_child(Dom::text(format!("Selected: {}", selected).as_str())),
                {
                    let mut button = Dom::div()
                        .with_inline_style(
                            "padding: 8px 12px; background: white; border: 2px solid #4a90e2; \
                             border-radius: 4px; cursor: pointer; user-select: none; display: \
                             inline-block;",
                        )
                        .with_child(Dom::text("Open Dropdown ▼"));

                    button
                        .root
                        .add_hover_css_property(CssProperty::BackgroundContent(
                            StyleBackgroundContentVec::from_const_slice(&HOVER_BG).into(),
                        ));
                    button
                        .root
                        .add_active_css_property(CssProperty::BackgroundContent(
                            StyleBackgroundContentVec::from_const_slice(&ACTIVE_BG).into(),
                        ));
                    button.root.add_callback(
                        EventFilter::Hover(HoverEventFilter::MouseUp),
                        RefAny::new(KitchenSinkApp::default()),
                        on_dropdown_button_click as usize,
                    );

                    button
                },
            ]
            .into(),
        )
}

fn create_progress_demo(app_data: &KitchenSinkApp) -> Dom {
    let progress_width = format!("{}%", app_data.progress_value);

    Dom::div()
        .with_inline_style(
            "border: 1px solid #ccc; padding: 15px; border-radius: 8px; background: #f9f9f9; \
             box-sizing: border-box;",
        )
        .with_children(
            vec![
                Dom::div()
                    .with_inline_style(
                        "font-size: 20px; font-weight: bold; margin-bottom: 15px; color: #333;",
                    )
                    .with_child(Dom::text("Progress Bar Demo")),
                Dom::div()
                    .with_inline_style("margin-bottom: 10px; color: #666; font-size: 14px;")
                    .with_child(Dom::text(
                        format!("Progress: {:.0}%", app_data.progress_value).as_str(),
                    )),
                // Progress bar track
                Dom::div()
                    .with_inline_style(
                        "width: 100%; height: 30px; background: #e0e0e0; border-radius: 15px; \
                         overflow: hidden; position: relative;",
                    )
                    .with_child(
                        // Progress bar fill
                        Dom::div().with_inline_style(
                            format!(
                                "width: {}; height: 100%; background: #4a90e2; transition: width \
                                 0.3s ease;",
                                progress_width
                            )
                            .as_str(),
                        ),
                    ),
            ]
            .into(),
        )
}

// Scrolling demo

fn create_scrolling_demo() -> Dom {
    // Generate 50 items to demonstrate scrolling
    let items: Vec<Dom> = (0..50)
        .map(|i| {
            Dom::div()
                .with_inline_style(
                    "padding: 10px; border-bottom: 1px solid #e0e0e0; font-size: 14px;",
                )
                .with_child(Dom::text(
                    format!("List Item #{} - This is a scrollable item", i + 1).as_str(),
                ))
        })
        .collect();

    Dom::div()
        .with_inline_style(
            "border: 1px solid #ccc; padding: 15px; border-radius: 8px; background: #f9f9f9; \
             box-sizing: border-box; display: flex; flex-direction: column; height: 100%;",
        )
        .with_children(
            vec![
                Dom::div()
                    .with_inline_style(
                        "font-size: 20px; font-weight: bold; margin-bottom: 15px; color: #333; \
                         flex-shrink: 0;",
                    )
                    .with_child(Dom::text("Scrolling Demo")),
                Dom::div()
                    .with_inline_style(
                        "margin-bottom: 10px; color: #666; font-size: 14px; flex-shrink: 0;",
                    )
                    .with_child(Dom::text(
                        "This container has overflow:auto and shows 50 items",
                    )),
                Dom::div()
                    .with_inline_style(
                        "flex: 1; overflow: auto; border: 2px solid #ccc; border-radius: 4px; \
                         background: white; min-height: 0;",
                    )
                    .with_children(items.into()),
            ]
            .into(),
        )
}

// Slider demo

// Slider drag callback
extern "C" fn on_slider_track_click(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut app_data = data.downcast_mut::<KitchenSinkApp>().unwrap();

    // Get cursor position and track rect
    if let Some(cursor_pos) = info.get_cursor_position() {
        if let Some(track_rect) = info.get_hit_node_layout_rect() {
            // Calculate percentage based on cursor X position within track
            let relative_x = cursor_pos.x - track_rect.origin.x;
            let percentage = (relative_x / track_rect.size.width * 100.0)
                .max(0.0)
                .min(100.0);
            app_data.slider_value = percentage;
            return Update::RefreshDom;
        }
    }

    Update::DoNothing
}

fn create_slider_demo(app_data: &KitchenSinkApp) -> Dom {
    let slider_percentage = app_data.slider_value;
    let thumb_position = format!("calc({}% - 10px)", slider_percentage); // 10px = half thumb width

    Dom::div()
        .with_inline_style(
            "border: 1px solid #ccc; padding: 30px; border-radius: 8px; background: #f9f9f9; \
             box-sizing: border-box; height: 100%; display: flex; flex-direction: column; \
             justify-content: center; align-items: center;",
        )
        .with_children(
            vec![
                Dom::div()
                    .with_inline_style(
                        "font-size: 24px; font-weight: bold; margin-bottom: 20px; color: #333;",
                    )
                    .with_child(Dom::text("Slider Demo")),
                Dom::div()
                    .with_inline_style("font-size: 18px; margin-bottom: 30px; color: #666;")
                    .with_child(Dom::text(
                        format!("Value: {:.1}", slider_percentage).as_str(),
                    )),
                // Slider container
                Dom::div()
                    .with_inline_style("width: 80%; max-width: 600px; position: relative;")
                    .with_children(
                        vec![
                            // Track
                            {
                                let mut track = Dom::div().with_inline_style(
                                    "width: 100%; height: 8px; background: #e0e0e0; \
                                     border-radius: 4px; position: relative; cursor: pointer;",
                                );

                                // Add click handler to track
                                track.root.add_callback(
                                    EventFilter::Hover(HoverEventFilter::MouseDown),
                                    RefAny::new(KitchenSinkApp::default()),
                                    on_slider_track_click as usize,
                                );

                                track
                            },
                            // Filled portion
                            Dom::div().with_inline_style(
                                format!(
                                    "position: absolute; top: 0; left: 0; width: {}%; height: \
                                     8px; background: #4a90e2; border-radius: 4px; \
                                     pointer-events: none;",
                                    slider_percentage
                                )
                                .as_str(),
                            ),
                            // Thumb
                            Dom::div().with_inline_style(
                                format!(
                                    "position: absolute; top: -6px; left: {}; width: 20px; \
                                     height: 20px; background: white; border: 3px solid #4a90e2; \
                                     border-radius: 50%; cursor: pointer; box-shadow: 0 2px 4px \
                                     rgba(0,0,0,0.2);",
                                    thumb_position
                                )
                                .as_str(),
                            ),
                        ]
                        .into(),
                    ),
                // Instructions
                Dom::div()
                    .with_inline_style("margin-top: 30px; color: #888; font-size: 14px;")
                    .with_child(Dom::text(
                        "Click anywhere on the slider track to set the value",
                    )),
            ]
            .into(),
        )
}

// Code editor

// Dataset for tracking line numbers in code editor
#[derive(Debug, Clone)]
struct CodeLineData {
    line_number: usize,
}

// Code editor callbacks
extern "C" fn on_code_scroll(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut app_data = data.downcast_mut::<KitchenSinkApp>().unwrap();

    // Get scroll position of the editor node
    let hit_node = info.get_hit_node();

    // Get DomId from the layout info - we need to access the current DOM
    // For now, use DomId(0) as the main DOM
    use azul_core::dom::DomId;
    let dom_id = DomId::ROOT_ID;
    let node_id = hit_node.node.inner.into();

    if let Some(scroll_state) = info.get_scroll_state(dom_id, node_id) {
        // Calculate which line is at the top based on scroll offset
        let scroll_y = scroll_state.current_offset.y;
        let top_line = (scroll_y / app_data.code_line_height).floor() as usize;

        // Only update if scroll position changed significantly
        if app_data.code_scroll_offset != top_line {
            app_data.code_scroll_offset = top_line;
            return Update::RefreshDom;
        }
    }

    Update::DoNothing
}

extern "C" fn on_code_zoom_in(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    let mut app_data = data.downcast_mut::<KitchenSinkApp>().unwrap();

    // Increase font size by 2px, max 32px
    if app_data.code_font_size < 32.0 {
        app_data.code_font_size += 2.0;
        app_data.code_line_height = app_data.code_font_size * 1.5;
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

extern "C" fn on_code_zoom_out(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    let mut app_data = data.downcast_mut::<KitchenSinkApp>().unwrap();

    // Decrease font size by 2px, min 8px
    if app_data.code_font_size > 8.0 {
        app_data.code_font_size -= 2.0;
        app_data.code_line_height = app_data.code_font_size * 1.5;
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

extern "C" fn on_code_text_input(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let mut app_data = data.downcast_mut::<KitchenSinkApp>().unwrap();

    if let Some(changeset) = info.get_text_changeset() {
        app_data.code_content = format!("{}{}", changeset.old_text, changeset.inserted_text);
        info.set_text_changeset(changeset.clone());

        // Trigger preview IFrame update
        if let Some(iframe_node_id) =
            info.get_node_id_by_id_attribute(DomId::ROOT_ID, "preview-iframe")
        {
            info.trigger_iframe_rerender(DomId::ROOT_ID, iframe_node_id);
        }
    }

    Update::RefreshDom
}

// Helper function: Render only visible lines for virtual scrolling
// Returns DOM nodes for lines in the visible range (scroll_offset ± window_height)
fn render_visible_code_lines(
    code: &str,
    scroll_offset: usize,
    window_height_estimate: usize,
    font_size: f32,
    line_height: f32,
) -> Vec<Dom> {
    let lines: Vec<&str> = code.lines().collect();
    let total_lines = lines.len();

    if total_lines == 0 {
        return vec![Dom::div().with_child(Dom::text(""))];
    }

    // Calculate visible range: render 2x window height for smooth scrolling
    let lines_buffer = window_height_estimate * 2;
    let start_line = scroll_offset.saturating_sub(lines_buffer / 2);
    let end_line = (scroll_offset + lines_buffer).min(total_lines);

    // Render each visible line with line number
    lines[start_line..end_line]
        .iter()
        .enumerate()
        .map(|(idx, line_text)| {
            let actual_line_num = start_line + idx + 1; // 1-indexed

            Dom::div()
                .with_dataset(OptionRefAny::Some(RefAny::new(CodeLineData {
                    line_number: actual_line_num,
                })))
                .with_inline_style(&format!(
                    "display: flex; font-size: {}px; line-height: {}px;",
                    font_size, line_height
                ))
                .with_children(
                    vec![
                        // Line number gutter
                        Dom::div()
                            .with_inline_style(
                                "min-width: 50px; padding-right: 10px; color: #5c6370; \
                                 text-align: right; user-select: none; flex-shrink: 0;",
                            )
                            .with_child(Dom::text(format!("{}", actual_line_num).as_str())),
                        // Line content
                        Dom::div()
                            .with_inline_style("flex: 1; white-space: pre;")
                            .with_child(Dom::text(*line_text)),
                    ]
                    .into(),
                )
        })
        .collect()
}

fn create_code_editor(app_data: &KitchenSinkApp, data_refany: RefAny) -> Dom {
    // Split layout: Preview (left) | Editor (right)
    Dom::div()
        .with_inline_style(
            "display: flex; flex-direction: column; height: 100%; box-sizing: border-box;",
        )
        .with_children(
            vec![
                // Top: Split panes (preview + editor)
                Dom::div()
                    .with_inline_style(
                        "flex: 1; display: grid; grid-template-columns: 1fr 1fr; gap: 20px; \
                         padding: 10px; min-height: 0; overflow: hidden;",
                    )
                    .with_children(
                        vec![
                            // Left: Preview pane (IFrame)
                            Dom::div()
                                .with_inline_style(
                                    "border: 2px solid #4a90e2; border-radius: 8px; background: \
                                     white; overflow: auto; padding: 10px; box-sizing: \
                                     border-box; display: flex; flex-direction: column;",
                                )
                                .with_children(
                                    vec![
                                        Dom::div()
                                            .with_inline_style(
                                                "font-size: 14px; font-weight: bold; color: \
                                                 #4a90e2; margin-bottom: 10px; border-bottom: 1px \
                                                 solid #e0e0e0; padding-bottom: 5px;",
                                            )
                                            .with_child(Dom::text("Preview")),
                                        // IFrame for rendering parsed XHTML
                                        Dom::iframe(
                                            data_refany.clone(),
                                            preview_iframe_callback as IFrameCallbackType,
                                        )
                                        .with_inline_style(
                                            "flex: 1; overflow: auto; min-height: 0;",
                                        )
                                        .with_attributes(
                                            vec![AttributeType::Id("preview-iframe".into())].into(),
                                        ),
                                    ]
                                    .into(),
                                ),
                            // Right: Code editor pane with iframe scrolling
                            Dom::div()
                                .with_inline_style(
                                    "border: 2px solid #4caf50; border-radius: 8px; background: \
                                     #282c34; overflow: hidden; display: flex; flex-direction: \
                                     column; box-sizing: border-box;",
                                )
                                .with_children(
                                    vec![
                                        Dom::div()
                                            .with_inline_style(
                                                "font-size: 14px; font-weight: bold; color: \
                                                 #4caf50; padding: 10px; background: #21252b; \
                                                 border-bottom: 1px solid #181a1f; display: flex; \
                                                 justify-content: space-between;",
                                            )
                                            .with_children(
                                                vec![
                                                    Dom::div().with_child(Dom::text("Editor")),
                                                    Dom::div()
                                                        .with_inline_style(
                                                            "color: #5c6370; font-size: 12px; \
                                                             font-weight: normal;",
                                                        )
                                                        .with_child(Dom::text(
                                                            format!(
                                                                "Scroll offset: line {}",
                                                                app_data.code_scroll_offset
                                                            )
                                                            .as_str(),
                                                        )),
                                                ]
                                                .into(),
                                            ),
                                        // Editor content with iframe virtual scrolling
                                        {
                                            // Use virtual scrolling for large files
                                            const USE_VIRTUAL_SCROLLING: bool = true;

                                            let mut editor = if USE_VIRTUAL_SCROLLING {
                                                // Render only visible lines (iframe approach)
                                                let visible_lines = render_visible_code_lines(
                                                    &app_data.code_content,
                                                    app_data.code_scroll_offset,
                                                    40, // Estimate: 40 lines visible at once
                                                    app_data.code_font_size,
                                                    app_data.code_line_height,
                                                );

                                                Dom::div()
                                                    .with_inline_style(
                                                        "flex: 1; font-family: 'Monaco', 'Menlo', \
                                                         'Courier New', monospace; color: \
                                                         #abb2bf; overflow: auto; outline: none; \
                                                         box-sizing: border-box; padding: 10px;",
                                                    )
                                                    .with_children(visible_lines.into())
                                            } else {
                                                // Original: render full content as contenteditable
                                                Dom::div()
                                                    .with_attribute(AttributeType::ContentEditable(
                                                        true,
                                                    ))
                                                    .with_attribute(AttributeType::TabIndex(
                                                        0.into(),
                                                    ))
                                                    .with_inline_style(&format!(
                                                        "flex: 1; padding: 10px; font-family: \
                                                         'Monaco', 'Menlo', 'Courier New', \
                                                         monospace; font-size: {}px; line-height: \
                                                         {}px; color: #abb2bf; overflow: auto; \
                                                         white-space: pre; outline: none; \
                                                         box-sizing: border-box;",
                                                        app_data.code_font_size,
                                                        app_data.code_line_height
                                                    ))
                                                    .with_child(Dom::text(
                                                        app_data.code_content.as_str(),
                                                    ))
                                            };

                                            // Add scroll callback for virtual scrolling
                                            editor.root.add_callback(
                                                EventFilter::Window(WindowEventFilter::Scroll),
                                                RefAny::new(KitchenSinkApp::default()),
                                                on_code_scroll as usize,
                                            );

                                            // Add text input callback (for contenteditable mode)
                                            if !USE_VIRTUAL_SCROLLING {
                                                editor.root.add_callback(
                                                    EventFilter::Focus(FocusEventFilter::TextInput),
                                                    RefAny::new(KitchenSinkApp::default()),
                                                    on_code_text_input as usize,
                                                );
                                            }

                                            editor
                                        },
                                    ]
                                    .into(),
                                ),
                        ]
                        .into(),
                    ),
                // Bottom: Toolbar with Zoom buttons
                Dom::div()
                    .with_inline_style(
                        "padding: 10px; background: #f5f5f5; border-top: 2px solid #ccc; display: \
                         flex; gap: 10px; justify-content: center; flex-shrink: 0;",
                    )
                    .with_children(
                        vec![
                            // Zoom out button
                            {
                                let mut zoom_out_btn = Dom::div()
                                    .with_inline_style(
                                        "padding: 10px 20px; background: #ff9800; color: white; \
                                         border-radius: 4px; cursor: pointer; user-select: none; \
                                         font-weight: bold;",
                                    )
                                    .with_child(Dom::text("Zoom Out -"));

                                zoom_out_btn.root.add_callback(
                                    EventFilter::Hover(HoverEventFilter::MouseUp),
                                    RefAny::new(KitchenSinkApp::default()),
                                    on_code_zoom_out as usize,
                                );

                                zoom_out_btn
                            },
                            // Font size display
                            Dom::div()
                                .with_inline_style(
                                    "padding: 10px 20px; background: #e0e0e0; color: #333; \
                                     border-radius: 4px; user-select: none; font-weight: bold;",
                                )
                                .with_child(Dom::text(
                                    format!("{}px", app_data.code_font_size as u32).as_str(),
                                )),
                            // Zoom in button
                            {
                                let mut zoom_in_btn = Dom::div()
                                    .with_inline_style(
                                        "padding: 10px 20px; background: #4caf50; color: white; \
                                         border-radius: 4px; cursor: pointer; user-select: none; \
                                         font-weight: bold;",
                                    )
                                    .with_child(Dom::text("Zoom In +"));

                                zoom_in_btn.root.add_callback(
                                    EventFilter::Hover(HoverEventFilter::MouseUp),
                                    RefAny::new(KitchenSinkApp::default()),
                                    on_code_zoom_in as usize,
                                );

                                zoom_in_btn
                            },
                        ]
                        .into(),
                    ),
            ]
            .into(),
        )
}
    
// IFrame callback for preview pane

/// IFrame callback that renders the XHTML preview
///
/// This callback is invoked by the layout engine to populate the preview pane
/// with the parsed XHTML content from the code editor.
extern "C" fn preview_iframe_callback(
    data: &mut RefAny,
    info: &mut IFrameCallbackInfo,
) -> IFrameCallbackReturn {
    let app_data = match data.downcast_ref::<KitchenSinkApp>() {
        Some(d) => d,
        None => {
            // Return empty DOM if data type is wrong
            let mut empty_dom = Dom::body();
            empty_dom.add_child(Dom::text("Error: Invalid data type"));
            let css = CssApiWrapper::empty();
            let styled_dom = StyledDom::new(&mut empty_dom, css);
            return IFrameCallbackReturn {
                dom: OptionStyledDom::Some(styled_dom),
                scroll_size: info.bounds.get_logical_size(),
                scroll_offset: LogicalPosition::zero(),
                virtual_scroll_size: info.bounds.get_logical_size(),
                virtual_scroll_offset: LogicalPosition::zero(),
            };
        }
    };

    // Parse the XHTML code
    let styled_dom = if app_data.code_content.is_empty() {
        let mut preview_dom = Dom::body();
        preview_dom.add_child(
            Dom::div()
                .with_inline_style(
                    "padding: 10px; background: #f9f9f9; border-radius: 4px; color: #666; \
                     font-style: italic;",
                )
                .with_child(Dom::text("Enter XHTML code in the editor to see preview")),
        );
        let css = CssApiWrapper::empty();
        StyledDom::new(&mut preview_dom, css)
    } else {
        // Parse XML directly to StyledDom using DomXmlExt trait
        Dom::from_xml_string(&app_data.code_content)
    };

    IFrameCallbackReturn {
        dom: OptionStyledDom::Some(styled_dom),
        scroll_size: info.bounds.get_logical_size(),
        scroll_offset: LogicalPosition::zero(),
        virtual_scroll_size: info.bounds.get_logical_size(),
        virtual_scroll_offset: LogicalPosition::zero(),
    }
}

// main entry point
#[cfg(feature = "desktop")]
fn main() {
    eprintln!("[KITCHEN_SINK] Starting application...");

    let initial_data = KitchenSinkApp::default();
    eprintln!("[KITCHEN_SINK] Created initial data");

    let config = DllAppConfig::new();
    eprintln!("[KITCHEN_SINK] Created config");

    let app = App::new(RefAny::new(initial_data), config);
    eprintln!("[KITCHEN_SINK] Created app");

    let mut window = WindowCreateOptions::new(main_layout as LayoutCallbackType);
    window.state.title = "Azul Kitchen Sink - Layout & Rendering Demo".into();
    window.state.size = WindowSize {
        dimensions: LogicalSize::new(1200.0, 800.0),
        ..Default::default()
    };
    window.state.flags.frame = WindowFrame::Normal;
    eprintln!("[KITCHEN_SINK] Created window options");

    eprintln!("[KITCHEN_SINK] Calling app.run()...");
    app.run(window);
    eprintln!("[KITCHEN_SINK] app.run() returned (should not happen - app.run is blocking)");
}

#[cfg(not(feature = "desktop"))]
fn main() {
    eprintln!("This example requires the 'desktop' feature to be enabled.");
    eprintln!("Run with: cargo run --bin kitchen_sink --features desktop");
    std::process::exit(1);
}
