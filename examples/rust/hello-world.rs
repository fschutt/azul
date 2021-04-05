#![windows_subsystem = "windows"]

use azul::{
    callbacks::{RefAny, CallbackInfo, LayoutCallbackInfo},
    styled_dom::StyledDom,
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

    let counter = data.downcast_ref::<DataModel>() {
        Some(s) => s.counter,
        None => return StyledDom::body(),
    };

    let label = Label::new(format!("{}", counter));
    let button = Button::with_label("Update counter")
        .on_click(increment_counter);

    StyledDom::body()
    .append(label.dom())
    .append(button.dom())
}

extern "C" fn increment_counter(data: &mut RefAny, _: CallbackInfo) -> UpdateScreen {
    match data.downcast_mut::<DataModel>() {
        Some(s) => { s.counter += 1; UpdateScreen::Redraw },
        None => UpdateScreen::DoNothing, // error
    }
}

fn main() {
    let app = App::new(DataModel { counter: 0 }, AppConfig::default()).unwrap();
    app.run(WindowCreateOptions::new(layout));
}