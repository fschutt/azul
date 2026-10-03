//! The process table: azul's `DataTable` over the model's rows.
//!
//! THE APP SORTS AND FILTERS, NOT THE WIDGET: the rows change every second,
//! and an order the widget computed from one reading would be stale at the
//! next. So the table's rows ARE the model's shown rows, in the model's
//! order; a header click hands its sort keys to the model
//! ([`keys_of`] -> `Model::set_sort`) and the view goes back "in app order"
//! ([`in_app_order`]: the header still shows the arrows). The widget's
//! filter row is off - the tool row has the filter field (name, user, PID).
//!
//! THE SELECTION IS A PROCESS, NOT A ROW: the model keeps the selected PID
//! and every reading puts the table's selection on that process' new row
//! ([`follow_selection`]).

use azul::{
    callbacks::{DataTableDataSourceCallbackType, DataTableOnEventCallbackType},
    file::DiskSpace,
    prelude::*,
    vec::U32Vec,
    widgets::{
        CellGridHorizontalAlign, DataTable, DataTableCell, DataTableCellRef, DataTableColumn,
        DataTableEvent, DataTableEventKind, DataTableSortDirection, DataTableSortKey,
        DataTableSortKind, DataTableView, ListSelection,
    },
};

use crate::{
    ids,
    model::{format_percent, Column, ProcRow, SortKey, COLUMNS},
    ticks::LiveView,
    Monitor,
};

/// `bytes` as a file manager writes it ("212 MB"): azul's one formatter.
#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    DiskSpace::format_bytes(bytes).as_str().to_string()
}

/// A rate in bytes per second ("4.1 MB/s").
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // rounded, clamped at 0
pub fn format_rate(per_second: f64) -> String {
    let bytes = if per_second.is_finite() && per_second > 0.0 {
        per_second.round() as u64
    } else {
        0
    };
    format!("{}/s", format_bytes(bytes))
}

/// The table's columns (none editable; the filter row is off).
#[must_use]
pub fn columns() -> Vec<DataTableColumn> {
    COLUMNS
        .iter()
        .map(|c| {
            let (kind, align) = if c.is_number() {
                (DataTableSortKind::Number, CellGridHorizontalAlign::Right)
            } else {
                (DataTableSortKind::Text, CellGridHorizontalAlign::Left)
            };
            DataTableColumn::create(c.title(), c.width(), kind)
                .with_align(align)
                .with_filterable(false)
        })
        .collect()
}

/// What the table shows of `row` in `column`: the text and the number a
/// number column sorts by.
#[must_use]
#[allow(clippy::cast_precision_loss)] // memory far below 2^52
pub fn cell_of(row: &ProcRow, column: Column) -> DataTableCell {
    match column {
        Column::Name => DataTableCell::create_text(row.name.clone()),
        Column::User => DataTableCell::create_text(row.user.clone()),
        Column::Status => DataTableCell::create_text(row.status.clone()),
        Column::Pid => DataTableCell::create(row.pid.to_string(), f64::from(row.pid)),
        Column::Cpu => {
            DataTableCell::create(format_percent(f64::from(row.cpu)), f64::from(row.cpu))
        }
        Column::Memory => DataTableCell::create(format_bytes(row.memory), row.memory as f64),
        Column::Disk => DataTableCell::create(format_rate(row.disk_rate), row.disk_rate),
    }
}

/// The model's sort keys the view's header asks for.
#[must_use]
pub fn keys_of(view: &DataTableView) -> Vec<SortKey> {
    view.sort
        .as_slice()
        .iter()
        .filter_map(|k| {
            let column = Column::at(usize::try_from(k.column).ok()?)?;
            let descending = matches!(k.direction, DataTableSortDirection::Descending);
            Some(SortKey::new(column, descending))
        })
        .collect()
}

/// `view` showing every row in the app's order (the model's): the widget
/// computes no order of its own; the sort keys stay for the header's arrows.
#[must_use]
pub fn in_app_order(mut view: DataTableView) -> DataTableView {
    view.order = U32Vec::create();
    view.ordered = false;
    view.order_serial = view.query_serial;
    view
}

/// A fresh view whose header shows `keys` (the model's sort).
#[must_use]
pub fn view_for(keys: &[SortKey]) -> DataTableView {
    let mut view = DataTableView::create();
    let header: Vec<DataTableSortKey> = keys
        .iter()
        .map(|k| {
            let direction = if k.descending {
                DataTableSortDirection::Descending
            } else {
                DataTableSortDirection::Ascending
            };
            DataTableSortKey::create(u32::try_from(k.column.index()).unwrap_or(0), direction)
        })
        .collect();
    view.set_sort(header);
    in_app_order(view)
}

/// Puts the view's selection on the selected process' row (`selected`:
/// its position among the `shown` rows; `None`: nothing selected), and
/// keeps the first row shown within the rows.
pub fn follow_selection(view: &mut DataTableView, selected: Option<usize>, shown: usize) {
    let mut selection = ListSelection::create();
    if let Some(position) = selected.filter(|p| *p < shown) {
        selection.click(position as u64);
    }
    view.selection = selection;
    let last = u32::try_from(shown.saturating_sub(1)).unwrap_or(u32::MAX);
    if view.top > last {
        view.top = last;
    }
}

/// The data callback: cell `at` of the model's shown rows.
pub extern "C" fn cell(mut app: RefAny, at: DataTableCellRef) -> DataTableCell {
    let Some(s) = app.downcast_ref::<Monitor>() else {
        return DataTableCell::empty();
    };
    let row = usize::try_from(at.row).unwrap_or(usize::MAX);
    let column = usize::try_from(at.column).ok().and_then(Column::at);
    match (s.model.shown_row(row), column) {
        (Some(r), Some(c)) => cell_of(r, c),
        _ => DataTableCell::empty(),
    }
}

/// The table at `width` x `height` over `rows` rows in `view` (built by the
/// table's live view; the cells come from the app's model).
#[must_use]
pub fn table(app: &RefAny, view: DataTableView, rows: u32, width: f32, height: f32) -> DataTable {
    DataTable::create(columns(), rows)
        .with_id(ids::TABLE)
        .with_accessibility_name("Processes")
        .with_view(view)
        .with_viewport(width.max(200.0), height.max(120.0))
        .with_frozen_columns(1)
        .with_show_filter_row(false)
        .with_read_only(true)
        .with_data_source(app.clone(), cell as DataTableDataSourceCallbackType)
        .with_on_event(app.clone(), on_table_event as DataTableOnEventCallbackType)
}

/// Every action in the table: a sort goes to the model, a selection becomes
/// the selected process; the view is stored. A sort or a selection rebuilds
/// the page (the End button, the status bar); a scroll, a resize or a drag
/// re-renders the table's live view alone.
extern "C" fn on_table_event(
    mut app: RefAny,
    mut info: CallbackInfo,
    event: DataTableEvent,
) -> Update {
    let kind = event.kind;
    {
        let Some(mut s) = app.downcast_mut::<Monitor>() else {
            return Update::DoNothing;
        };
        let mut view = event.view;
        match kind {
            DataTableEventKind::Sort => {
                s.model.set_sort(keys_of(&view));
                view = in_app_order(view);
                println!("AZMON_SORT {}", crate::sort_text(s.model.sort()));
            }
            DataTableEventKind::Filter | DataTableEventKind::OrderReady => {
                view = in_app_order(view)
            }
            DataTableEventKind::Select => {
                let cursor = view.cursor_row().into_option();
                s.model
                    .select_position(cursor.and_then(|r| usize::try_from(r).ok()));
            }
            _ => {}
        }
        let selected = s.model.selected_position();
        let shown = s.model.shown_count();
        follow_selection(&mut view, selected, shown);
        s.table = view;
        if matches!(kind, DataTableEventKind::Select) {
            crate::print_selected(&s);
        }
    }
    match kind {
        DataTableEventKind::Sort | DataTableEventKind::Select => Update::RefreshDom,
        _ => {
            crate::rerender(&mut info, &[LiveView::Table]);
            Update::DoNothing
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ProcSample, DEFAULT_SORT};

    fn row() -> ProcRow {
        ProcRow::of(
            &ProcSample {
                pid: 5102,
                name: "cargo".to_string(),
                user: "user".to_string(),
                status: "Running".to_string(),
                cpu: 25.6,
                memory: 940 << 20,
                disk_read: 3 << 20,
                disk_written: 1 << 20,
                ..ProcSample::default()
            },
            1000,
            1,
        )
    }

    #[test]
    fn the_table_has_the_models_columns_none_editable() {
        let cols = columns();
        assert_eq!(cols.len(), COLUMNS.len());
        assert_eq!(cols[Column::Name.index()].title.as_str(), "Name");
        assert_eq!(
            cols[Column::Cpu.index()].sort_kind,
            DataTableSortKind::Number
        );
        assert_eq!(
            cols[Column::User.index()].sort_kind,
            DataTableSortKind::Text
        );
        assert!(cols.iter().all(|c| !c.editable));
    }

    #[test]
    fn a_cell_shows_the_formatted_value_and_sorts_by_the_number() {
        let r = row();
        assert_eq!(cell_of(&r, Column::Name).text.as_str(), "cargo");
        let cpu = cell_of(&r, Column::Cpu);
        assert_eq!(cpu.text.as_str(), "25.6 %");
        assert!((cpu.value - 25.6).abs() < 0.001);
        let pid = cell_of(&r, Column::Pid);
        assert_eq!(pid.text.as_str(), "5102");
        assert_eq!(pid.value, 5102.0);
        assert_eq!(cell_of(&r, Column::Memory).text.as_str(), "940 MB");
        assert_eq!(cell_of(&r, Column::Disk).text.as_str(), "4.0 MB/s");
        assert_eq!(cell_of(&r, Column::Status).text.as_str(), "Running");
    }

    #[test]
    fn rates_read_as_bytes_per_second() {
        assert_eq!(format_rate(0.0), "0 B/s");
        assert_eq!(format_rate(1536.0), "1.5 KB/s");
        assert_eq!(format_rate(-5.0), "0 B/s");
    }

    #[test]
    fn a_header_click_becomes_the_models_sort_keys() {
        let mut v = DataTableView::create();
        v.click_sort(Column::Cpu.index() as u32, false);
        assert_eq!(keys_of(&v), vec![SortKey::new(Column::Cpu, false)]);
        v.click_sort(Column::Cpu.index() as u32, false);
        assert_eq!(keys_of(&v), vec![SortKey::new(Column::Cpu, true)]);
        v.click_sort(Column::Name.index() as u32, true);
        assert_eq!(
            keys_of(&v),
            vec![
                SortKey::new(Column::Cpu, true),
                SortKey::new(Column::Name, false)
            ]
        );
    }

    #[test]
    fn the_view_shows_the_rows_in_the_models_order_with_the_arrows() {
        let mut v = DataTableView::create();
        v.click_sort(Column::Memory.index() as u32, false);
        let v = in_app_order(v);
        assert!(!v.is_sorting());
        assert!(!v.ordered);
        assert_eq!(v.sort.as_slice().len(), 1);
        assert_eq!(v.shown_count(5), 5);
        assert_eq!(v.row_at(2, 5).into_option(), Some(2));
    }

    #[test]
    fn a_fresh_view_shows_the_default_sort() {
        let v = view_for(&DEFAULT_SORT);
        let keys = v.sort.as_slice();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].column, Column::Cpu.index() as u32);
        assert_eq!(keys[0].direction, DataTableSortDirection::Descending);
        assert!(!v.is_sorting());
        assert_eq!(keys_of(&v), DEFAULT_SORT.to_vec());
    }

    #[test]
    fn the_selection_follows_the_process_to_its_new_row() {
        let mut v = DataTableView::create();
        follow_selection(&mut v, Some(3), 10);
        assert!(v.selection.contains(3));
        assert_eq!(v.selection.focus.into_option(), Some(3));
        assert_eq!(v.cursor_row().into_option(), Some(3));
        follow_selection(&mut v, Some(7), 10);
        assert!(!v.selection.contains(3));
        assert!(v.selection.contains(7));
        follow_selection(&mut v, None, 10);
        assert!(v.selection.is_empty());
    }

    #[test]
    fn the_first_row_shown_stays_within_fewer_rows() {
        let mut v = DataTableView::create();
        v.top = 40;
        follow_selection(&mut v, None, 12);
        assert_eq!(v.top, 11);
        follow_selection(&mut v, None, 0);
        assert_eq!(v.top, 0);
    }
}
