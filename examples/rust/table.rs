#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use azul::prelude::*;
use azul::widgets::{
    TableView, TableViewState,
    TableCellIndex, TableCellSelection
};

struct TableDemo {
    // cells: BTreeMap<TableCell, String>,
}

extern "C" fn layout(data: &mut RefAny, _: &mut LayoutCallbackInfo) -> StyledDom {

    let mut table_view_state = TableViewState::default();
    table_view_state.set_cell_content(TableCellIndex { row: 2, column: 2 }, "Hello World");
    table_view_state.set_selection(Some(TableCellSelection::from(3, 4).to(3, 4)));

    TableView::new(table_view_state).dom().style(Css::empty())
}

fn main() {
    let app = App::new(RefAny::new(TableDemo { }), AppConfig::new(LayoutSolver::Default));
    app.run(WindowCreateOptions::new(layout));
}
