#![windows_subsystem = "windows"]

use azul::{
    app::{App, AppConfig},
    window::WindowCreateOptions,
    style::StyledDom,
    callbacks::{RefAny, LayoutCallbackInfo},
};

#[derive(Debug)]
struct Data { }

extern "C" fn layout(data: &mut RefAny, _info: LayoutCallbackInfo) -> StyledDom {
    StyledDom::from_file("./ui.xml".into())
}

fn main() {
    let data = RefAny::new(Data { });
    let app = App::new(data, AppConfig::default());
    let mut window = WindowCreateOptions::new(layout);
    window.hot_reload = true;
    app.run(window);
}
