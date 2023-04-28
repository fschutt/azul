#![windows_subsystem = "windows"]

use azul::{
    app::{App, AppConfig, LayoutSolver},
    css::Css,
    style::StyledDom,
    dom::Dom,
    menu::{Menu, MenuItem, StringMenuItem},
    callbacks::{RefAny, CallbackInfo, Update, LayoutCallbackInfo},
    window::{WindowCreateOptions, WindowFrame},
};

static DEFAULT_CONTENTS: &str = "<html>
    <head>
        <style>
            p {
                font-size: 29px;
            }
        </style>
    </head>
    <body>
        <p>Start editing the text in the editor</p>
        <p>Then use Export > [language] to save the UI to a .cpp / .rs file.</p>
    </body>
</html>";

struct Data {
    text_editor_contents: String,
    console: Option<DebugConsole>
}

// What to show in the debugging console
enum DebugConsole {
    Layout,
    DisplayList,
    ScrollClips,
    CssCascade,
}

extern "C" fn layout(data: &mut RefAny, _info: &mut LayoutCallbackInfo) -> StyledDom {

    let xml_string = match data.downcast_ref::<Data>() {
        Some(s) => s.text_editor_contents.clone(),
        None => return StyledDom::default(),
    };

    let editor = xml_string.lines().enumerate().map(|(line_idx, l)| {
        Dom::div()
        .with_children(vec![
            Dom::text(l.to_string())
        ])
    }).collect::<Dom>()
    .with_inline_style("
        display:flex;
        flex-direction:column;
        flex-grow:1;
        min-width: 800px;
        font-family: monospace;
        font-size: 12px;
        background:repeating-linear-gradient(red 0%, blue 50%, red 51%, blue 100%);
        background-size: 10px 10px;
    ")
    .style(Css::empty());

    let rendered_preview = Dom::body()
    .with_inline_style("padding:10px;")
    .style(Css::empty())
    .with_child(
        Dom::div()
        .with_inline_style("
            min-width:800px;
            border:1px solid grey;
            display:flex;
            flex-direction:column;
            flex-grow:1;
        ")
        .style(Css::empty())
        .with_child(StyledDom::from_xml(xml_string.clone()))
    );

    Dom::body()
    .with_menu_bar(Menu::new(vec![
        MenuItem::String(StringMenuItem::new("File").with_children(vec![
            MenuItem::String(StringMenuItem::new("Load...").with_callback(data.clone(), load_xml_file)),
            MenuItem::String(StringMenuItem::new("Save...").with_callback(data.clone(), save_xml_file)),
        ])),
        MenuItem::String(StringMenuItem::new("Export").with_children(vec![
            MenuItem::String(StringMenuItem::new("Rust (.rs)").with_callback(data.clone(), export_rust)),
            MenuItem::String(StringMenuItem::new("C (.c)").with_callback(data.clone(), export_c)),
            MenuItem::String(StringMenuItem::new("C++ (.cpp)").with_callback(data.clone(), export_cpp)),
            MenuItem::String(StringMenuItem::new("Python (.py)").with_callback(data.clone(), export_py)),
            MenuItem::String(StringMenuItem::new("HTML (.html)").with_callback(data.clone(), export_html)),
        ])),
        MenuItem::String(StringMenuItem::new("Debug").with_children(vec![
            MenuItem::String(
                StringMenuItem::new("Layout")
                .with_callback(data.clone(), enable_debug_layout)
            ),
            MenuItem::String(
                StringMenuItem::new("Display list")
                .with_callback(data.clone(), enable_debug_layout)
            ),
            MenuItem::String(
                StringMenuItem::new("Scroll clips")
                .with_callback(data.clone(), enable_debug_layout)
            ),
            MenuItem::String(
                StringMenuItem::new("Cascade")
                .with_callback(data.clone(), enable_debug_layout)
            ),
        ]))
    ]))
    .with_inline_style("display:flex;flex-direction:row;flex-grow:1;")
    .style(Css::empty())
    .with_child(editor)
    .with_child(rendered_preview)
}

extern "C"
fn load_xml_file(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    Update::RefreshDom
}

extern "C"
fn save_xml_file(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    Update::RefreshDom
}

extern "C"
fn export_rust(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    Update::RefreshDom
}

extern "C"
fn export_c(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    Update::RefreshDom
}

extern "C"
fn export_cpp(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    Update::RefreshDom
}

extern "C"
fn export_py(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    Update::RefreshDom
}

extern "C"
fn export_html(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    Update::RefreshDom
}

extern "C"
fn enable_debug_layout(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    Update::RefreshDom
}

extern "C"
fn enable_debug_display_list(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    Update::RefreshDom
}

extern "C"
fn enable_debug_scroll_clips(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    Update::RefreshDom
}

extern "C"
fn enable_debug_css_cascade(data: &mut RefAny, _info: &mut CallbackInfo) -> Update {
    Update::RefreshDom
}

fn main() {
    let data = RefAny::new(Data {
        text_editor_contents: DEFAULT_CONTENTS.to_string(),
        console: None,
    });
    let app = App::new(data, AppConfig::new(LayoutSolver::Default));
    let mut window = WindowCreateOptions::new(layout);
    window.state.flags.frame = WindowFrame::Maximized;
    app.run(window);
}
