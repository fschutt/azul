#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use azul::{
    app::{App, AppConfig},
    window::WindowCreateOptions,
    style::StyledDom,
    callbacks::{RefAny, LayoutCallbackInfo},
};
use azul_widgets::{
    button::Button,
    label::Label,
    table_view::TableView,
    // text_input::TextInput,
    // progress::ProgressBar,
    // frame::Frame,
    // tab::Tab,
};

#[derive(Default)]
struct WidgetShowcase {
}

extern "C" fn layout(data: &mut RefAny, _: LayoutCallbackInfo) -> StyledDom {
    StyledDom::from_file("./widgets.xml".into())
}

fn main() {
    let data = RefAny::new(WidgetShowcase::default());
    let app = App::new(data, AppConfig::default());
    let mut options = WindowCreateOptions::new(layout);
    options.hot_reload = true;
    options.state.flags.is_maximized = true;
    app.run(options);
}
