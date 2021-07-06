#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use azul::prelude::*;

#[derive(Default)]
struct WidgetShowcase {
}

extern "C" fn layout(data: &mut RefAny, _: LayoutCallbackInfo) -> StyledDom {
    StyledDom::from_file("./widgets.xml".into())
}

fn main() {
    let data = RefAny::new(WidgetShowcase::default());
    let app = App::new(data, AppConfig::new(LayoutSolver::Default));
    let mut options = WindowCreateOptions::new(layout);
    options.hot_reload = true;
    options.state.flags.frame = WindowFrame::Maximized;
    app.run(options);
}
