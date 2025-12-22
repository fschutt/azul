use azul::prelude::*;

// Include XHTML at compile time
static XHTML: &str = include_str!("../assets/spreadsheet.xhtml");

struct AppData {
    dom: StyledDom,
}

extern "C" 
fn layout(data: RefAny, _info: LayoutCallbackInfo) -> StyledDom {
    match data.downcast_ref::<AppData>() {
        Some(d) => d.dom.clone(),
        None => StyledDom::default(),
    }
}

fn main() {
    let data = RefAny::new(AppData {
        dom: Dom::parse_xhtml(XHTML)
            .unwrap_or_else(|e| {
                eprintln!("Failed to parse XHTML: {}", e);
                StyledDom::default()
            }),
    });
    let app = App::create(data, AppConfig::create());
    let mut options = WindowCreateOptions::create(layout);
    options.set_window_title("XHTML Spreadsheet");
    app.run(options);
}
