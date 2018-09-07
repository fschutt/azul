#![windows_subsystem = "windows"]

extern crate azul;

use azul::{prelude::*, widgets::*};

struct TableDemo {
    table: TableViewOutcome,
}

impl Layout for TableDemo {
    fn layout(&self, _info: WindowInfo<Self>) -> Dom<Self> {
        TableView::new().dom()
    }
}

fn main() {
    let mut app = App::new(TableDemo { 
        table: TableViewOutcome::default(), 
    }, AppConfig::default());
    app.create_window(WindowCreateOptions::default(), Css::native()).unwrap();
    app.run().unwrap();
}