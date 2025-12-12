//! XHTML file loading and rendering example

#![windows_subsystem = "windows"]

use azul::{
    app::{App, AppConfig, LayoutSolver},
    callbacks::{LayoutCallbackInfo, RefAny},
    css::Css,
    style::StyledDom,
    window::WindowCreateOptions,
};

// Load XHTML file at compile time
static XHTML: &str = include_str!("../assets/spreadsheet.xhtml");

struct Data;

extern "C" fn layout(_data: &mut RefAny, _info: &mut LayoutCallbackInfo) -> StyledDom {
    StyledDom::from_xml(XHTML.to_string())
}

fn main() {
    let data = RefAny::new(Data);
    let app = App::new(data, AppConfig::new(LayoutSolver::Default));
    let mut options = WindowCreateOptions::new(layout);
    options.state.title = "XHTML Spreadsheet".into();
    app.run(options);
}
