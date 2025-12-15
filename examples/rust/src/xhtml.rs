use azul::{
    app::{App, AppConfig, LayoutSolver},
    callbacks::{LayoutCallbackInfo, RefAny},
    css::Css,
    style::StyledDom,
    window::WindowCreateOptions,
};

// Include XHTML at compile time
static XHTML: &str = include_str!("../assets/spreadsheet.xhtml");

struct Data;

extern "C" 
fn layout(_data: RefAny, _info: LayoutCallbackInfo) -> StyledDom {
    StyledDom::from_xml(XHTML.to_string())
}

fn main() {
    let data = RefAny::new(Data);
    let app = App::new(data, AppConfig::new());
    let mut options = WindowCreateOptions::new(layout);
    options.state.title = "XHTML Spreadsheet".into();
    app.run(options);
}
