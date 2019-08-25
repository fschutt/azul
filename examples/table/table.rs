#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

extern crate azul;

use azul::{prelude::*, widgets::table_view::*};

struct TableDemo {
    table_view_state: Ref<TableViewState>,
}

impl Layout for TableDemo {
    fn layout(&self, _: LayoutInfo) -> Dom<Self> {
        TableView::new(self.table_view_state.clone()).dom()
    }
}

fn main() {

    let mut table_view_state = TableViewState::default();
    table_view_state.set_cell(3, 4, "Hello World");

    let app_data = TableDemo { table_view_state: Ref::new(table_view_state) };
    let app = App::new(app_data, AppConfig::default()).unwrap();
    app.run(WindowCreateOptions::new(css::native()));
}
