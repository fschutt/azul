#![windows_subsystem = "windows"]

extern crate azul;

use azul::{prelude::*, widgets::table_view::*};

struct TableDemo {
    table_state: TableViewState,
}

impl Layout for TableDemo {
    fn layout(&self, _info: WindowInfo<Self>) -> Dom<Self> {
        TableView::new().dom(&self.table_state, &self)
    }
}

fn main() {

    let app = App::new(TableDemo {
        table_state: TableViewState::default(),
    }, AppConfig::default());

    macro_rules! CSS_PATH { () => (concat!(env!("CARGO_MANIFEST_DIR"), "/examples/table.css")) }

    // #[cfg(debug_assertions)]
    // let css = Css::hot_reload(CSS_PATH!()).unwrap();
    // #[cfg(not(debug_assertions))]
    let css = Css::new_from_str(include_str!(CSS_PATH!())).unwrap();

    app.run(Window::new(WindowCreateOptions::default(), css).unwrap()).unwrap();
}