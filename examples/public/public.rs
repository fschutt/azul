//! Example of the new, public API

use azul::{
    app::{App, AppConfig},
    css::Css,
    dom::Dom,
    window::WindowCreateOptions,
    callbacks::{Ref, RefAny, LayoutInfo},
};

struct Data {
    counter: usize,
}

fn layout(_data: RefAny, _info: LayoutInfo) -> Dom {
    // data.downcast::<Data>();
    Dom::div()
}

fn main() {
    let data = Data {
        counter: 5,
    };
    let app = App::new(Ref::new(data).upcast(), AppConfig::new(), layout);
    app.run(WindowCreateOptions::new(Css::native()));
}