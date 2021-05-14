use azul::prelude::*;

struct DataModel {
    counter: usize,
}

static CSS: AzString = AzString::from_const_str("
    .__azul-native-label { font-size: 50px; }
");

extern "C" fn myLayoutFunc(data: &mut RefAny, _: LayoutCallbackInfo) -> StyledDom {

    let data = match data.downcast_ref::<DataModel>() {
        Some(s) => d,
        None => return StyledDom::default(),
    };

    let label = Label::new(format!("{}", data.counter));
    let button = Button::new("Update counter")
        .with_on_click(data.clone(), myOnClick);

    Dom::body()
    .with_child(label.dom())
    .with_child(button.dom())
    .style(Css::from_string(CSS))
}

extern "C" fn myOnClick(data: &mut RefAny, _: CallbackInfo) -> Update {
    let data = match data.downcast_mut::<DataModel>() {
        Some(s) => s,
        None => return Update::DoNothing, // error
    };

    data.counter += 1;

    Update::RefreshDom
}

fn main() {
    let data = DataModel { counter: 0 };
    let app = App::new(RefAny::new(data), AppConfig::new(LayoutSolver::Default));
    app.run(WindowCreateOptions::new(myLayoutFunc));
}