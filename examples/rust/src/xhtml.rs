use azul::prelude::*;

// Include XHTML at compile time
static XHTML: &str = include_str!("../assets/spreadsheet.xhtml");

struct AppData;

extern "C" fn layout(_data: RefAny, _info: LayoutCallbackInfo) -> StyledDom {
    // Create fresh from XML each time to avoid clone issues
    StyledDom::from_xml(XHTML)
}

fn main() {
    let data = RefAny::new(AppData);
    let app = App::create(data, AppConfig::create());
    let mut options = WindowCreateOptions::create(layout);
    options.window_state.title = "XHTML Spreadsheet".into();
    app.run(options);
}
