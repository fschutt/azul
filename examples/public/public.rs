//! Example of the new, public API

use azul::prelude::*;

struct Data {
    counter: usize,
}

fn layout(data: RefAny, _info: LayoutInfo) -> Dom {
    let data = data.downcast_ref::<Data>().expect("wrong downcast");
    Dom::label(format!("hello: {}", data.counter).into())
}

fn main() {
    let data = Data {
        counter: 5,
    };
    let app = App::new(RefAny::new(data), AppConfig::new(), layout);
    app.run(WindowCreateOptions::new(Css::native()));
}