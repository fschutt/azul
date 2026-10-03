//! The table half: the orders in azul's `DataTable`.
//!
//! SKELETON (DATATABLE7 fills it in once the widget lands): the state, the
//! tool row, the table pane and the status bar the window asks for.

use azul::{
    prelude::*,
    shells::ShellEmptyState,
    str::String as AzString,
    widgets::{StatusBar, StatusBarSegment},
};

use crate::{ids, Dashboard};

/// The table's state the app keeps.
#[derive(Debug, Clone, Default)]
pub struct TableState {
    /// The last edit refused, or another notice ("" = none).
    pub notice: String,
}

impl TableState {
    /// A fresh table over `rows` orders: no sort, no filter, the cursor on
    /// the first row.
    pub fn reset(&mut self, _rows: u32) {
        self.notice.clear();
    }
}

/// The tool row over the table (the RecordsShell's tab row).
pub fn tools(_s: &Dashboard, _app: &RefAny) -> Dom {
    Dom::create_div().with_id(ids::TOOLS)
}

/// The table pane: the DataTable, or the empty state while the orders are
/// generated. `charts_height` is the strip over it (its viewport is the rest).
pub fn view(s: &Dashboard, _app: &RefAny, _charts_height: f32) -> Dom {
    let text = if s.source.is_some() {
        "The orders are in."
    } else {
        "Generating the orders..."
    };
    ShellEmptyState::create(AzString::from(text))
        .with_icon(AzString::from("table_chart"))
        .dom()
        .with_id(ids::LOADING)
}

/// The status bar: the row counts.
pub fn status_bar(s: &Dashboard) -> Dom {
    StatusBar::create(vec![StatusBarSegment::create(AzString::from(format!(
        "{} orders",
        s.rows
    )))])
    .dom()
}
