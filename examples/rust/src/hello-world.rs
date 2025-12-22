use azul::prelude::*;
use azul::widgets::Button;
use azul::css::StyledDom;

struct DataModel {
    counter: usize,
}

extern "C" 
fn my_layout_func(mut data: RefAny, _: LayoutCallbackInfo) -> StyledDom {
    let counter = match data.downcast_ref::<DataModel>() {
        Some(d) => format!("{}", d.counter),
        None => return StyledDom::default(),
    };

    let mut label = Dom::create_text(counter.as_str());
    label.set_inline_style("font-size: 50px");

    let mut button = Button::create("Update counter");
    button.set_on_click(data.clone(), my_on_click);
    let mut button = button.dom();
    button.set_inline_style("flex-grow: 1");

    Dom::create_body()
        .with_child(label)
        .with_child(button)
        .style(Css::empty())
}

extern "C" 
fn my_on_click(mut data: RefAny, _: CallbackInfo) -> Update {
    let mut data = match data.downcast_mut::<DataModel>() {
        Some(s) => s,
        None => return Update::DoNothing, // error
    };

    data.counter += 1;

    Update::RefreshDom
}

fn main() {
    let data = DataModel { counter: 0 };
    let config = AppConfig::create();
    let app = App::create(RefAny::new(data), config);
    let window = WindowCreateOptions::create(my_layout_func);
    app.run(window);
}
