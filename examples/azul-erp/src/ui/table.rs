//! A `table` view as azul's DataTable: the view's columns ([`spec::columns`]),
//! the records' cells ([`rows::grid`]: texts and sort values, formatted
//! once per frame), read-only; Enter or a double-click on a row opens the
//! view's row action ([`crate::app::State::row_path`]).

use azul::{
    callbacks::{DataTableDataSourceCallbackType, DataTableOnEventCallbackType},
    prelude::*,
    str::String as AzString,
    widgets::{
        DataTable, DataTableCell, DataTableCellRef, DataTableColumn, DataTableEvent, DataTableEventKind,
        DataTableSortKind, DataTableView,
    },
};

use super::{column, empty, page_header, with_erp, Erp, CHROME_HEIGHT};
use crate::{
    ids,
    views::{
        rows::{self, Grid},
        spec::{self, ColumnSpec, ColumnType},
        Params, View,
    },
};

/// The DataTable columns of the view's columns.
#[must_use]
pub fn data_columns(columns: &[ColumnSpec]) -> Vec<DataTableColumn> {
    columns
        .iter()
        .map(|c| {
            let kind = match c.kind {
                ColumnType::Currency | ColumnType::Integer => DataTableSortKind::Number,
                ColumnType::Date => DataTableSortKind::Date,
                ColumnType::Text | ColumnType::Status => DataTableSortKind::Text,
            };
            DataTableColumn::create(AzString::from(c.title.as_str()), c.width, kind).with_sortable(c.sortable)
        })
        .collect()
}

/// The data callback: cell `at` of the grid.
pub extern "C" fn cell(mut source: RefAny, at: DataTableCellRef) -> DataTableCell {
    let Some(grid) = source.downcast_ref::<Grid>() else {
        return DataTableCell::empty();
    };
    let (row, col) = (at.row as usize, at.column as usize);
    let text = grid
        .text
        .get(row)
        .and_then(|r| r.get(col))
        .cloned()
        .unwrap_or_default();
    let value = grid
        .sort
        .get(row)
        .and_then(|r| r.get(col))
        .copied()
        .unwrap_or(f64::NAN);
    DataTableCell {
        text: AzString::from(text),
        value,
    }
}

/// A read-only DataTable over `grid`, `height` px high.
#[must_use]
pub fn grid_table(
    id: AzString,
    name: &str,
    columns: &[ColumnSpec],
    grid: Grid,
    view: DataTableView,
    size: (f32, f32),
    app: &RefAny,
    on_event: DataTableOnEventCallbackType,
) -> Dom {
    let count = u32::try_from(grid.len()).unwrap_or(u32::MAX);
    DataTable::create(data_columns(columns), count)
        .with_id(id)
        .with_accessibility_name(name)
        .with_view(view)
        .with_viewport(size.0.max(200.0), size.1.max(120.0))
        .with_read_only(true)
        .with_data_source(RefAny::new(grid), cell as DataTableDataSourceCallbackType)
        .with_on_event(app.clone(), on_event)
        .dom()
}

/// The page of a `table` view: its title and tools over the table.
pub fn page(s: &mut Erp, app: &RefAny, view: &View) -> Dom {
    let title = view.title(&s.state.labels, false);
    let header = page_header(s, app, view, &title, &Params::new());
    let Some(kind) = view.kind_of_records() else {
        return column(vec![header, empty("This table reads no records.", &view.id)]);
    };
    let columns = spec::columns(view, &s.state.labels);
    let grid = rows::grid(kind, &columns, None, &s.state.ctx());
    s.row_paths = grid
        .ids
        .iter()
        .map(|id| s.state.row_path(view, kind, id).unwrap_or_default())
        .collect();
    if grid.is_empty() {
        let detail = match view.actions.first() {
            Some(_) => "Add the first one with the button above.",
            None => "Nothing is recorded yet.",
        };
        return column(vec![header, empty(&format!("{title}: none yet"), detail)]);
    }
    let size = (s.window.0, s.window.1 - CHROME_HEIGHT);
    let table = grid_table(
        ids::TABLE,
        &title,
        &columns,
        grid,
        s.table.clone(),
        size,
        app,
        on_table_event as DataTableOnEventCallbackType,
    );
    column(vec![header, table])
}

/// Every action in the page table: its view is kept; an activated row opens.
extern "C" fn on_table_event(mut data: RefAny, mut info: CallbackInfo, event: DataTableEvent) -> Update {
    with_erp(&mut data, &mut info, |s, _info| {
        s.table = event.view.clone();
        if matches!(event.kind, DataTableEventKind::Activate) {
            let path = s.row_paths.get(event.cell.row as usize).cloned().unwrap_or_default();
            if !path.is_empty() {
                s.state.open(&path);
                match &s.state.form {
                    Some(draft) => println!("AZERP_FORM {}", draft.view),
                    None => println!("AZERP_PAGE {}", s.state.page),
                }
            }
        }
    })
}
