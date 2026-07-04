// cargo run --example hello-world

use azul::prelude::*;
use azul::widgets::Button;

struct DataModel {
    counter: usize,
}

extern "C" fn my_layout_func(mut data: RefAny, _: LayoutCallbackInfo) -> Dom {
    let counter = match data.downcast_ref::<DataModel>() {
        Some(d) => format!("{}", d.counter),
        None => return Dom::create_body(),
    };

    // Canonical hello-world shape shared by every language binding:
    // body > div{font-size:32px} > text(counter), then the button.
    let label = Dom::create_div()
        .with_css("font-size: 32px")
        .with_child(Dom::create_text(counter.as_str()));

    let mut button = Button::create("Increase counter");
    button.set_on_click(data.clone(), my_on_click);
    let mut button = button.dom();
    button.set_css("flex-grow: 1");

    Dom::create_body()
        .with_child(label)
        .with_child(button)
}

extern "C" fn my_on_click(mut data: RefAny, _: CallbackInfo) -> Update {
    let mut data = match data.downcast_mut::<DataModel>() {
        Some(s) => s,
        None => return Update::DoNothing, // error
    };

    data.counter += 1;

    Update::RefreshDom
}

fn main() {
    let data = DataModel { counter: 5 };
    let config = AppConfig::create();
    let app = App::create(RefAny::new(data), config);
    let window = WindowCreateOptions::create(my_layout_func);
    app.run(window);
}
