#![windows_subsystem = "windows"]

extern crate azul;

use azul::{prelude::*, widgets::*};

struct TableDemo {
    table: TableViewOutcome,
}

impl Layout for TableDemo {
    fn layout(&self, _info: WindowInfo<Self>) -> Dom<Self> {
        TableView::new().dom(&self.table, &self)
    }
}

fn main() {
    let mut app = App::new(TableDemo {
        table: TableViewOutcome::default(),
    }, AppConfig::default());

    macro_rules! CSS_PATH { () => (concat!(env!("CARGO_MANIFEST_DIR"), "/examples/table_view.css")) }

    #[cfg(debug_assertions)]
    let css = Css::hot_reload(CSS_PATH!()).unwrap();
    #[cfg(not(debug_assertions))]
    let css = Css::new_from_str(include_str!(CSS_PATH!())).unwrap();

    app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run().unwrap();
}