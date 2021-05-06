#![windows_subsystem = "windows"]

use azul::{
    app::{App, AppConfig},
    window::WindowCreateOptions,
    style::StyledDom,
    css::Css,
    dom::Dom,
    callbacks::{RefAny, LayoutCallbackInfo},
};
use azul_widgets::label::Label;

#[derive(Debug)]
struct Data { }

extern "C" fn layout(_: &mut RefAny, _info: LayoutCallbackInfo) -> StyledDom {
    let mut styled_dom = Dom::div()
    .style(Css::from_string("div {
        margin: 0px;
        padding: 0px;
        background: green;
        flex-grow: 1;
    }".into()));

    let child1 = Label::new("Child IP: ").dom()
    .style(Css::from_string("* {
        flex-grow: 1;
        align-items: center;
        color: white;
    }
    ".into()));
    // child1.restyle(Css::from_string(...));

    let child2 = Dom::div()
    .style(Css::from_string("div {
        flex-grow: 3;
        background: red;
    }".into()));

    styled_dom.append_child(child1);
    styled_dom.append_child(child2);

    styled_dom
}

fn main() {
    let data = RefAny::new(Data { });
    let app = App::new(data, AppConfig::default());
    app.run(WindowCreateOptions::new(layout));
}