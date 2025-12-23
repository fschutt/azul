use azul::prelude::*;

// Include XHTML at compile time
static XHTML: &str = include_str!("../assets/spreadsheet.xhtml");

struct AppData {
    dom: StyledDom,
}

extern "C" 
fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> StyledDom {
    match data.downcast_ref::<AppData>() {
        Some(d) => d.dom.clone(),
        None => StyledDom::default(),
    }
}

fn main() {
    let data = RefAny::new(AppData {
        dom: StyledDom::from_xml(XHTML),
    });
    let app = App::create(data, AppConfig::create());
    let mut options = WindowCreateOptions::create(layout);
    options.window_state.title = "XHTML Spreadsheet".into();
    app.run(options);
}
