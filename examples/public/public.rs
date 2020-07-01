//! Example of the new, public API

use azul::prelude::*;

struct Data {
    counter: usize,
}

extern "C" fn layout(data: RefAny, _info: LayoutInfo) -> Dom {
    let data = data.borrow::<Data>().expect("wrong downcast");
    let dom = Dom::body()
        .with_child(Dom::label(format!("hello: {}", data.counter).into()));

    println!("dom (client side): {}", dom.get_html_string());
    println!("size of dom (client side): {}", std::mem::size_of::<Dom>());

    dom
}


fn main() {
    let data = Data {
        counter: 5,
    };
    let app = App::new(RefAny::new(data), AppConfig::default(), layout);
    app.run(WindowCreateOptions::new(Css::native()));
}