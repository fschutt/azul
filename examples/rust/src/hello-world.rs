use azul::{prelude::*, widgets::Button};

struct DataModel {
    counter: usize,
}

extern "C" 
fn myLayoutFunc(mut data: RefAny, _: LayoutCallbackInfo) -> StyledDom {
    let counter = match data.downcast_ref::<DataModel>() {
        Some(d) => format!("{}", d.counter),
        None => return StyledDom::default(),
    };

    let mut label = Dom::text(counter);
    label.set_inline_style("font-size: 50px");

    let mut button = Button::new("Update counter");
    button.set_on_click(data.clone(), myOnClick);
    let mut button = button.dom();
    button.set_inline_style("flex-grow: 1");

    Dom::new_body()
        .with_child(label)
        .with_child(button)
        .style(Css::empty())
}

extern "C" 
fn myOnClick(mut data: RefAny, _: CallbackInfo) -> Update {
    let mut data = match data.downcast_mut::<DataModel>() {
        Some(s) => s,
        None => return Update::DoNothing, // error
    };

    data.counter += 1;

    Update::RefreshDom
}

fn main() {
    let data = DataModel { counter: 0 };
    let config = AppConfig::new();
    let app = App::new(RefAny::new(data), config);
    let window = WindowCreateOptions::new(myLayoutFunc);
    app.run(window);
}
