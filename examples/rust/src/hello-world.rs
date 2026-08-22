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

    let label = Dom::create_div()
        .with_css("font-size: 32px")
        // A counter display is a LABEL, not prose: a <span> carries no UA
        // paragraph margin (a <p> here grew the line by two font-sizes).
        .with_child(Dom::create_span_with_text(counter.as_str()));

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
        None => return Update::DoNothing,
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
