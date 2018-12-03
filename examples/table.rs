#![windows_subsystem = "windows"]

extern crate azul;

use azul::{prelude::*, widgets::table_view::*};

struct TableDemo {
    table_state: TableViewState,
}

impl Layout for TableDemo {
    fn layout(&self, info: WindowInfo<Self>) -> Dom<Self> {
        TableView::new().dom(&self.table_state, &self, info.window)
    }
}

fn main() {

    let mut table_state = TableViewState::default();
    table_state.work_sheet.set_cell(3, 4, "Hello World");

    let app = App::new(TableDemo {
        table_state,
    }, AppConfig::default());

    app.run(Window::new(WindowCreateOptions::default(), Css::native()).unwrap()).unwrap();
}