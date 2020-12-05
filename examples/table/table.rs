#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use azul::prelude::*;
use azul_widgets::table_view::*;

struct TableDemo {
    table_view_state: RefAny,
}

extern "C" fn layout(data: RefAny, _info: LayoutInfo) -> Dom {
    let data = data.borrow::<TableDemo>().expect("wrong downcast");
    let table_view_state = data.table_view_state.borrow::<TableViewState>().expect("wrong downcast table_view");
    TableView::new(table_view_state.clone()).dom()
}

fn main() {
    let mut table_view_state = TableViewState::default();
    table_view_state.set_cell(3, 4, "Hello World");
    let data = TableDemo { table_view_state: RefAny::new(table_view_state) };
    let app = App::new(RefAny::new(data), AppConfig::default(), layout);
    app.run(WindowCreateOptions::new(Css::native()));
}
