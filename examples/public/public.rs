//! Example of the new, public API

use azul::prelude::*;
use azul::style::StyledDom;

struct Data {
    counter: usize,
}

extern "C" fn layout(data: &RefAny, _info: LayoutInfo) -> StyledDom {
    let data = data.borrow::<Data>().expect("wrong downcast");
    Dom::body()
    .with_child(Dom::label(format!("hello: {}", data.counter).into()))
    .style(Css::empty())
}

fn main() {
    let data = Data {
        counter: 5,
    };
    let app = App::new(RefAny::new(data), AppConfig::default());
    app.run(WindowCreateOptions::new(layout));
}