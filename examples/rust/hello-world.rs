use azul::prelude::*;
use azul::str::String as AzString;
use azul::widgets::{Button, Label};

struct DataModel {
    counter: usize,
}

extern "C" fn myLayoutFunc(
    data: &mut RefAny,
    _: &mut LayoutCallbackInfo
) -> StyledDom {

    let counter = match data.downcast_ref::<DataModel>() {
        Some(d) => format!("{}", d.counter),
        None => return StyledDom::default(),
    };

    let mut label = Dom::text(counter.into());
    label.set_inline_style("font-size: 50px");

    let mut button = Button::new("Update counter".into());
    button.set_on_click(data.clone(), myOnClick);
    let mut button = button.dom();
    button.set_inline_style("flex-grow: 1");

    Dom::body()
    .with_child(label)
    .with_child(button)
    .style(Css::empty())
}

extern "C"
fn myOnClick(data: &mut RefAny, _:  &mut CallbackInfo) -> Update {
    let mut data = match data.downcast_mut::<DataModel>() {
        Some(s) => s,
        None => return Update::DoNothing, // error
    };

    data.counter += 1;

    Update::RefreshDom
}

fn main() {
    let data = DataModel { counter: 0 };
    let config = AppConfig::new(LayoutSolver::Default);
    let app = App::new(RefAny::new(data), config);
    let window = WindowCreateOptions::new(myLayoutFunc);
    app.run(window);
}