use azul::prelude::*;
use azul::str::String as AzString;
use azul::widgets::{Button, Label};

struct DataModel {
    counter: usize,
}

extern "C"
fn myLayoutFunc(data: &mut RefAny, _: &mut LayoutCallbackInfo) -> StyledDom {
    Dom::body().with_child(
        Dom::text("Test".into())
        .with_inline_style("font-size: 50px;".into())
    ).style(Css::empty())
}

fn main() {
    let data = DataModel { counter: 0 };
    let app = App::new(RefAny::new(data), AppConfig::new(LayoutSolver::Default));
    let mut window = WindowCreateOptions::new(myLayoutFunc);
    app.run(window);
}