//! A `table` view as azul's DataTable: the view's columns ([`spec::columns`]), the records' cells
//! ([`rows::grid`]: texts and sort values, formatted once per frame), read-only. The table is
//! VIRTUALISED in whole rows (it builds the rows in view, whatever the register holds) and takes
//! the px the window gives it ([`crate::metrics`]).
//!
//! The SELECTION and the Preview follow the row at the table's TOP EDGE while it scrolls
//! ([`crate::follow`]): scroll through a long list and stop where the Preview shows the record you
//! want; a click, a tap or a key on a row holds the selection until the table is scrolled on. Enter
//! or a double click opens the row's page ([`crate::app::State::row_path`]); on a phone a long
//! press opens the record's action sheet.
//!
//! A phone's table keeps the columns its width holds ([`nav::phone_columns`]: the first two and the
//! last, squeezed to fill the width) and no filter row; the rest of the record is in the Preview.

use azul::{
    callbacks::{DataTableDataSourceCallbackType, DataTableOnEventCallbackType},
    prelude::*,
    str::String as AzString,
    widgets::{
        DataTable, DataTableCell, DataTableCellRef, DataTableColumn, DataTableEvent,
        DataTableEventKind, DataTableSortKind, DataTableView, ListSelection,
    },
};
use azul_appkit::l10n::t;

use super::{frame, states, with_erp, Erp};
use crate::{
    follow::Effect,
    ids,
    metrics::{self, Room},
    nav,
    store::Kind,
    views::{
        rows::{self, Grid},
        spec::{self, ColumnSpec, ColumnType},
        View,
    },
};

/// A phone's row: a touch target (Android's 48 dp).
pub const PHONE_ROW_PX: f32 = 48.0;

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
            DataTableColumn::create(AzString::from(c.title.as_str()), c.width, kind)
                .with_sortable(c.sortable)
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
        icon: AzString::from_const_str(""),
    }
}

/// What a table of the window is made of, beside its cells.
pub struct TableSpec<'a> {
    /// The table's DOM id.
    pub id: AzString,
    /// What a screen reader calls it.
    pub name: &'a str,
    pub columns: &'a [ColumnSpec],
    /// The state the app keeps for it (sort, filters, scroll, selection).
    pub view: DataTableView,
    /// The px it fills.
    pub size: (f32, f32),
    pub phone: bool,
}

/// A read-only DataTable over `grid`; a phone's has rows a finger hits and no filter row.
#[must_use]
pub fn grid_table(
    spec: TableSpec<'_>,
    grid: Grid,
    app: &RefAny,
    on_event: DataTableOnEventCallbackType,
) -> Dom {
    let count = u32::try_from(grid.len()).unwrap_or(u32::MAX);
    let mut table = DataTable::create(data_columns(spec.columns), count)
        .with_id(spec.id)
        .with_accessibility_name(spec.name)
        .with_view(spec.view)
        .with_viewport(
            spec.size.0.max(metrics::MIN_WIDTH),
            spec.size.1.max(metrics::MIN_HEIGHT),
        )
        .with_read_only(true)
        .with_data_source(RefAny::new(grid), cell as DataTableDataSourceCallbackType)
        .with_on_event(app.clone(), on_event);
    if spec.phone {
        table = table
            .with_row_height(PHONE_ROW_PX)
            .with_show_filter_row(false);
    }
    table.dom()
}

/// The columns the page's table shows: all of them, or a phone's (the first two and the last, their
/// widths squeezed to fill `width`).
#[must_use]
pub fn columns_for(
    view: &View,
    labels: &crate::views::Labels,
    phone: bool,
    width: f32,
) -> Vec<ColumnSpec> {
    let all = spec::columns(view, labels);
    if !phone {
        return all;
    }
    let mut kept: Vec<ColumnSpec> = nav::phone_columns(all.len())
        .into_iter()
        .filter_map(|i| all.get(i).cloned())
        .collect();
    // the table's own vertical scroll bar takes a few px
    let widths: Vec<f32> = kept.iter().map(|c| c.width).collect();
    let fitted = nav::fit_widths(&widths, width - 12.0, nav::MIN_COLUMN_PX);
    for (column, w) in kept.iter_mut().zip(fitted) {
        column.width = w;
    }
    kept
}

/// The page of a `table` view: the table of its records, or the designed empty state when there
/// are none.
pub fn page(s: &mut Erp, app: &RefAny, view: &View, room: &Room, phone: bool) -> Dom {
    let title = view.title(&s.state.labels, false);
    let Some(kind) = view.kind_of_records() else {
        return states::quiet("table_chart", &t("azerp-table-no-records"), &view.id);
    };
    let (width, height) = metrics::table(room, 0.0);
    let columns = columns_for(view, &s.state.labels, phone, width);
    let grid = rows::grid(kind, &columns, None, &s.state.ctx());
    s.row_ids = grid.ids.clone();
    s.row_paths = grid
        .ids
        .iter()
        .map(|id| s.state.row_path(view, kind, id).unwrap_or_default())
        .collect();
    s.table_kind = Some(kind);
    if grid.is_empty() {
        s.state.clear_selection();
        let actions = spec::actions(&view.actions, &s.state.labels);
        return states::empty_list(s, app, &title, &actions);
    }
    sync_selection(s, kind);
    grid_table(
        TableSpec {
            id: ids::TABLE,
            name: &title,
            columns: &columns,
            view: s.table.clone(),
            size: (width, height),
            phone,
        },
        grid,
        app,
        on_table_event as DataTableOnEventCallbackType,
    )
}

/// The selection is on a row of the table: the record the Preview shows. The first build of a page
/// selects the row at the top edge; a record that left the rows (deleted) lets the top row take
/// over. The table's own selection follows when the app moved it.
pub(crate) fn sync_selection(s: &mut Erp, kind: Kind) {
    let count = u32::try_from(s.row_ids.len()).unwrap_or(u32::MAX);
    let held = s
        .state
        .selected
        .as_ref()
        .filter(|sel| sel.kind == kind)
        .and_then(|sel| s.row_ids.iter().position(|id| *id == sel.id));
    let row = match held {
        Some(row) => row,
        None => {
            let shown = s.table.shown_count(count);
            let top = s.table.top.min(shown.saturating_sub(1));
            let Some(row) = s.table.row_at(top, count).into_option() else {
                return;
            };
            row as usize
        }
    };
    let Some(id) = s.row_ids.get(row).cloned() else {
        return;
    };
    s.state.select(kind, &id);
    let wanted = u64::try_from(row).unwrap_or(u64::MAX);
    if s.table.selection.focus.into_option() != Some(wanted) {
        s.table.selection = ListSelection::create().apply(wanted, false, false);
    }
}

/// The record id of the app's row `row` of the page's table.
fn id_at(s: &Erp, row: u32) -> Option<String> {
    s.row_ids.get(row as usize).cloned()
}

/// Every action in the page table: its view is kept; the selection and the Preview follow the
/// row at the top edge while it scrolls; an activated row opens; a long press asks for the sheet.
extern "C" fn on_table_event(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: DataTableEvent,
) -> Update {
    with_erp(&mut data, &mut info, |s, _info| {
        let phone = frame::phone_now(s);
        apply_event(s, &event, phone);
    })
}

/// What an action of the page table does to the window's state.
pub(crate) fn apply_event(s: &mut Erp, event: &DataTableEvent, phone: bool) {
    s.table = event.view.clone();
    let count = u32::try_from(s.row_ids.len()).unwrap_or(u32::MAX);
    let Some(kind) = s.table_kind else {
        return;
    };
    match event.kind {
        DataTableEventKind::Select => {
            // a click, a tap or a key: the record under the cursor is the Preview's, and it holds
            // until the table is scrolled on
            if let Some(id) = event
                .view
                .cursor_row()
                .into_option()
                .and_then(|row| id_at(s, row))
            {
                s.state.select(kind, &id);
                println!("AZERP_SELECTED {id}");
            }
            s.follow.chose(event.view.top);
        }
        DataTableEventKind::Scroll => {
            let shown = s
                .state
                .selected
                .as_ref()
                .and_then(|sel| s.row_ids.iter().position(|id| *id == sel.id))
                .and_then(|row| u32::try_from(row).ok())
                .and_then(|row| s.table.position_of(row, count).into_option());
            if let Effect::Select(position) = s.follow.scrolled(event.view.top, shown) {
                if let Some(row) = s.table.row_at(position, count).into_option() {
                    if let Some(id) = id_at(s, row) {
                        s.state.select(kind, &id);
                        s.table.selection =
                            ListSelection::create().apply(u64::from(row), false, false);
                        println!("AZERP_SELECTED {id}");
                    }
                }
            }
        }
        DataTableEventKind::Sort | DataTableEventKind::Filter | DataTableEventKind::OrderReady => {
            // other rows are at the top now: the top row takes the Preview at the next build
            s.follow.reset();
            s.state.clear_selection();
        }
        DataTableEventKind::ContextMenu => {
            if event.cell.row != u32::MAX && phone {
                if let Some(id) = id_at(s, event.cell.row) {
                    s.state.select(kind, &id);
                    s.follow.chose(event.view.top);
                    s.sheet_open = true;
                }
            }
        }
        DataTableEventKind::Activate => {
            s.follow.chose(event.view.top);
            let path = s
                .row_paths
                .get(event.cell.row as usize)
                .cloned()
                .unwrap_or_default();
            if !path.is_empty() {
                s.state.open(&path);
                match &s.state.form {
                    Some(draft) => println!("AZERP_FORM {}", draft.view),
                    None => println!("AZERP_PAGE {}", s.state.page),
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "table_test.rs"]
mod table_test;
