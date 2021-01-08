#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use azul::prelude::*;
use azul_widgets::table_view::*;
use azul::style::StyledDom;

struct TableDemo {
    // cells: BTreeMap<TableCell, String>,
}

extern "C" fn layout(data: &RefAny, _: LayoutInfo) -> StyledDom {

    let mut table_view_state = TableViewState::default();
    table_view_state.set_cell(TableCell { row: 3, column: 4 }, "Hello World");
    table_view_state.set_selection(Some(TableCellSelection::from(3, 4).to(6, 7)));

    TableView::new()
        .with_state(table_view_state)
        .dom()
}

fn main() {
    let app = App::new(RefAny::new(TableDemo { }), AppConfig::default());
    app.run(WindowCreateOptions::new(layout));
}
