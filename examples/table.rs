#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

extern crate azul;

use azul::{prelude::*, widgets::table_view::*};

struct TableDemo {
    table_state: TableViewState,
}

impl Layout for TableDemo {
    fn layout(&self, info: LayoutInfo<Self>) -> Dom<Self> {
        TableView::new().dom(&self.table_state, &self, info.window)
    }
}

fn main() {

    let mut table_state = TableViewState::default();
    table_state.work_sheet.set_cell(3, 4, "Hello World");

    let mut app = App::new(TableDemo { table_state }, AppConfig::default()).unwrap();
    let window = app.create_window(WindowCreateOptions::default(), css::native()).unwrap();
    app.run(window).unwrap();
}
