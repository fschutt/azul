#![windows_subsystem = "windows"]

use azul::{
    css::Css,
    dom::Dom,
    callbacks::{RefAny, UpdateScreen, CallbackInfo, LayoutCallbackInfo},
    style::StyledDom,
    app::{App, AppConfig},
    window::WindowCreateOptions,
};
use azul_widgets::{
    label::Label,
    button::Button
};

struct DataModel {
    counter: usize,
}

extern "C" fn layout(data: &mut RefAny, _info: LayoutCallbackInfo) -> StyledDom {

    let counter = match data.downcast_ref::<DataModel>() {
        Some(s) => s.counter,
        None => return Dom::body().style(Css::empty()),
    };

    Dom::body()
    .with_children(vec![
        Label::new(format!("{}", counter)).dom(),
        Button::text("Update counter")
            .on_click(data.clone(), increment_counter).dom(),
    ].into())
    .style(Css::empty())
}

extern "C" fn increment_counter(data: &mut RefAny, _: CallbackInfo) -> UpdateScreen {
    match data.downcast_mut::<DataModel>() {
        Some(mut s) => { s.counter += 1; UpdateScreen::RegenerateStyledDomForCurrentWindow },
        None => UpdateScreen::DoNothing, // error
    }
}

fn main() {
    let app = App::new(RefAny::new(DataModel { counter: 0 }), AppConfig::default());
    app.run(WindowCreateOptions::new(layout));
}