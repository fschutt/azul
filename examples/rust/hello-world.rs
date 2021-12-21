use azul::prelude::*;
use azul::str::String as AzString;
use azul::widgets::{Button, Label};

struct DataModel {
    counter: usize,
}

static CSS: AzString = AzString::from_const_str("
    .__azul-native-label { font-size: 50px; }
");

extern "C" fn myLayoutFunc(data: &mut RefAny, _: &mut LayoutCallbackInfo) -> StyledDom {

    let counter = match data.downcast_ref::<DataModel>() {
        Some(d) => format!("{}", d.counter),
        None => return StyledDom::default(),
    };

    let mut label = Label::new(counter.into());
    let mut button = Button::new("Update counter".into())
        .with_on_click(data.clone(), myOnClick);

    Dom::body()
    .with_child(label.dom())
    .with_child(button.dom())
    .style(Css::from_string(CSS.clone()))
}

extern "C" fn myOnClick(data: &mut RefAny, _:  &mut CallbackInfo) -> Update {
    let mut data = match data.downcast_mut::<DataModel>() {
        Some(s) => s,
        None => return Update::DoNothing, // error
    };

    data.counter += 1;

    Update::RefreshDom
}

fn main() {
    let data = DataModel { counter: 0 };
    let app = App::new(RefAny::new(data), AppConfig::new(LayoutSolver::Default));
    let mut window = WindowCreateOptions::new(myLayoutFunc);
    app.run(window);
}