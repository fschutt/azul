#![windows_subsystem = "windows"]

use azul::prelude::*;

#[derive(Debug)]
struct Data { }

extern "C" fn layout(data: &mut RefAny, _info: LayoutCallbackInfo) -> StyledDom {
    StyledDom::from_file("./ui.xml".into())
}

fn main() {
    let data = RefAny::new(Data { });
    let app = App::new(data, AppConfig::new(LayoutSolver::Default));
    let mut window = WindowCreateOptions::new(layout);
    // window.hot_reload = true;
    window.state.flags.frame = WindowFrame::Maximized;
    app.run(window);
}
