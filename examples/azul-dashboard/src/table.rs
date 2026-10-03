//! The table half: the orders in azul's `DataTable`.
//!
//! The table holds no rows: its data callback ([`cell`]) formats the one
//! cell it asks for from the [`DataSet`] (a `RefAny` the app keeps in
//! `Dashboard::source`), only for the rows in view - and, in slices on a
//! timer, for the keys of a sort or a filter, which the widget then sorts on
//! an azul Thread. The view (sort, filters, order, selection, scroll, an
//! edit) is the app's: every event's view is stored ([`on_table_event`]).
//! Edits are validated by the data set ([`on_table_edit`] ->
//! `DataSet::edit`): a refused one stays open and the status bar says why.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, DataTableDataSourceCallbackType, DataTableOnEditCallbackType,
        DataTableOnEventCallbackType,
    },
    prelude::*,
    shells::ShellEmptyState,
    str::String as AzString,
    widgets::{
        Button, DataTable, DataTableCell, DataTableCellRef, DataTableColumn, DataTableEdit,
        DataTableEditResult, DataTableEvent, DataTableEventKind, DataTableSortDirection,
        DataTableSortKind, DataTableView, StatusBar, StatusBarSegment,
    },
};

use crate::{
    data::{self, DataSet, Kind},
    ids, Dashboard,
};

/// The table's state the app keeps.
#[derive(Debug, Clone)]
pub struct TableState {
    /// The DataTable's view: store every event's.
    pub view: DataTableView,
    /// The last edit refused, or what was copied ("" = nothing to say).
    pub notice: String,
}

impl Default for TableState {
    fn default() -> Self {
        Self {
            view: DataTableView::create(),
            notice: String::new(),
        }
    }
}

impl TableState {
    /// A fresh table over new rows: no sort, no filter, nothing selected.
    pub fn reset(&mut self, _rows: u32) {
        self.view = DataTableView::create();
        self.notice.clear();
    }
}

/// The px the window's chrome takes over and under the table (the title
/// row, the tool row, the status bar): the table's viewport is the rest.
pub const CHROME_HEIGHT: f32 = 112.0;

/// The table's columns, from the data set's.
#[must_use]
pub fn columns() -> Vec<DataTableColumn> {
    data::COLUMNS
        .iter()
        .map(|c| {
            let kind = match c.kind {
                Kind::Text => DataTableSortKind::Text,
                Kind::Number => DataTableSortKind::Number,
                Kind::Date => DataTableSortKind::Date,
            };
            DataTableColumn::create(AzString::from(c.title), c.width, kind).with_editable(c.editable)
        })
        .collect()
}

/// "500,000" - a count with thousands separators.
#[must_use]
pub fn grouped(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// What the sort is, for the status bar ("Sales \u{25BC}, Region \u{25B2}").
#[must_use]
pub fn sort_text(view: &DataTableView) -> String {
    let keys: Vec<String> = view
        .sort
        .as_slice()
        .iter()
        .map(|k| {
            let title = data::COLUMNS.get(k.column as usize).map_or("?", |c| c.title);
            let arrow = match k.direction {
                DataTableSortDirection::Ascending => "\u{25B2}",
                DataTableSortDirection::Descending => "\u{25BC}",
            };
            format!("{title} {arrow}")
        })
        .collect();
    if keys.is_empty() {
        "Unsorted".to_string()
    } else {
        format!("Sorted by {}", keys.join(", "))
    }
}

/// The data callback: cell `at` of the orders (the app's row).
extern "C" fn cell(mut source: RefAny, at: DataTableCellRef) -> DataTableCell {
    let Some(set) = source.downcast_ref::<DataSet>() else {
        return DataTableCell::empty();
    };
    let column = at.column as usize;
    DataTableCell {
        text: AzString::from(set.text(at.row, column)),
        value: set.value(at.row, column),
    }
}

/// The edit callback: the data set takes the value or says why not.
extern "C" fn on_table_edit(mut source: RefAny, _info: CallbackInfo, edit: DataTableEdit) -> DataTableEditResult {
    let Some(mut set) = source.downcast_mut::<DataSet>() else {
        return DataTableEditResult::create_refused(AzString::from("The orders are not loaded yet."));
    };
    match set.edit(edit.cell.row, edit.cell.column as usize, edit.text.as_str()) {
        Ok(()) => DataTableEditResult::create_accepted(),
        Err(why) => DataTableEditResult::create_refused(AzString::from(why)),
    }
}

/// The order id of app row `row` (for the scripts).
fn order_id(source: &Option<RefAny>, row: u32) -> String {
    let Some(mut source) = source.clone() else {
        return String::new();
    };
    source
        .downcast_ref::<DataSet>()
        .map(|set| set.text(row, data::c::ORDER))
        .unwrap_or_default()
}

/// The first rows shown, by the column the view is sorted (else filtered)
/// by: `Customer: Anna Abe | Anna Abe | Anna Berger` (for the scripts).
fn first_keys(source: &Option<RefAny>, rows: u32, view: &DataTableView) -> Option<String> {
    let column = view
        .sort
        .as_slice()
        .first()
        .map(|k| k.column)
        .or_else(|| view.filters.as_slice().first().map(|f| f.column))?;
    let title = data::COLUMNS.get(column as usize)?.title;
    let mut source = source.clone()?;
    let set = source.downcast_ref::<DataSet>()?;
    let texts: Vec<String> = (0..5)
        .filter_map(|p| view.row_at(p, rows).into_option())
        .map(|r| set.text(r, column as usize))
        .collect();
    Some(format!("{title}: {}", texts.join(" | ")))
}

/// Every action in the table: the view is stored; the scripts hear what
/// happened.
extern "C" fn on_table_event(mut app: RefAny, _info: CallbackInfo, event: DataTableEvent) -> Update {
    let Some(mut s) = app.downcast_mut::<Dashboard>() else {
        return Update::DoNothing;
    };
    let rows = s.rows;
    let view = &event.view;
    match event.kind {
        DataTableEventKind::Sort | DataTableEventKind::Filter | DataTableEventKind::OrderReady => {
            println!("AZDASH_SORT {}", sort_text(view));
            if !view.is_sorting() {
                if let Some(keys) = first_keys(&s.source, rows, view) {
                    println!("AZDASH_KEYS {keys}");
                }
                println!("AZDASH_SHOWN {} {}", view.shown_count(rows), rows);
            }
        }
        DataTableEventKind::Scroll | DataTableEventKind::Select => {
            if let Some(first) = view.row_at(view.top, rows).into_option() {
                println!("AZDASH_TOP {} {}", view.top, order_id(&s.source, first));
            }
        }
        DataTableEventKind::EditCommit => {
            println!(
                "AZDASH_EDIT {} {} {}",
                order_id(&s.source, event.cell.row),
                event.cell.column,
                event.text.as_str()
            );
            s.table.notice.clear();
        }
        DataTableEventKind::EditRefused => {
            println!("AZDASH_REFUSED {}", event.text.as_str());
            s.table.notice = event.text.as_str().to_string();
        }
        DataTableEventKind::EditCancel => s.table.notice.clear(),
        DataTableEventKind::Copy => {
            let lines = event.text.as_str().lines().count().saturating_sub(1);
            s.table.notice = format!("Copied {} rows", grouped(u32::try_from(lines).unwrap_or(u32::MAX)));
        }
        _ => {}
    }
    s.table.view = event.view;
    Update::RefreshDom
}

/// The table, as the window builds it (also for a button that restarts
/// the order after it changed the query).
fn table(s: &Dashboard, app: &RefAny, source: RefAny, charts_height: f32) -> DataTable {
    let (width, height) = s.window;
    DataTable::create(columns(), s.rows)
        .with_id(ids::TABLE)
        .with_accessibility_name(AzString::from("Orders"))
        .with_view(s.table.view.clone())
        .with_viewport(width.max(200.0), (height - CHROME_HEIGHT - charts_height).max(120.0))
        .with_frozen_columns(1)
        .with_data_source(source.clone(), cell as DataTableDataSourceCallbackType)
        .with_on_edit(source, on_table_edit as DataTableOnEditCallbackType)
        .with_on_event(app.clone(), on_table_event as DataTableOnEventCallbackType)
}

/// The table pane: the DataTable, or the empty state while the orders are
/// generated. `charts_height` is the strip over it.
pub fn view(s: &Dashboard, app: &RefAny, charts_height: f32) -> Dom {
    match s.source.clone() {
        Some(source) => table(s, app, source, charts_height).dom(),
        None => ShellEmptyState::create(AzString::from(format!(
            "Generating {} orders...",
            grouped(s.rows)
        )))
        .with_icon(AzString::from("table_chart"))
        .with_detail(AzString::from("The same orders every time, on a background thread."))
        .dom()
        .with_id(ids::LOADING),
    }
}

/// The tool row over the table: clear the filters, clear the sort.
pub fn tools(_s: &Dashboard, app: &RefAny) -> Dom {
    Dom::create_div()
        .with_id(ids::TOOLS)
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 4px 8px;")
        .with_child(
            Button::create(AzString::from("Clear filters"))
                .with_on_click(app.clone(), on_clear_filters as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::CLEAR_FILTERS)
                .with_css("margin-right: 6px;"),
        )
        .with_child(
            Button::create(AzString::from("Clear sort"))
                .with_on_click(app.clone(), on_clear_sort as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::CLEAR_SORT),
        )
}

/// Changes the query with `change`, then has the table bring its order.
fn requery(app: &mut RefAny, info: &mut CallbackInfo, change: fn(&mut DataTableView)) -> Update {
    let handle = app.clone();
    let Some(mut s) = app.downcast_mut::<Dashboard>() else {
        return Update::DoNothing;
    };
    change(&mut s.table.view);
    if let Some(source) = s.source.clone() {
        table(&s, &handle, source, 0.0).start_query(info);
    }
    println!("AZDASH_SORT {}", sort_text(&s.table.view));
    if !s.table.view.is_sorting() {
        println!("AZDASH_SHOWN {} {}", s.table.view.shown_count(s.rows), s.rows);
    }
    Update::RefreshDom
}

fn clear_filters(view: &mut DataTableView) {
    view.clear_filters();
}

fn clear_sort(view: &mut DataTableView) {
    view.clear_sort();
}

extern "C" fn on_clear_filters(mut app: RefAny, mut info: CallbackInfo) -> Update {
    requery(&mut app, &mut info, clear_filters)
}

extern "C" fn on_clear_sort(mut app: RefAny, mut info: CallbackInfo) -> Update {
    requery(&mut app, &mut info, clear_sort)
}

/// The status bar: the rows, the rows shown, the sort, the last notice.
pub fn status_bar(s: &Dashboard) -> Dom {
    let view = &s.table.view;
    let mut segments = vec![StatusBarSegment::create(AzString::from(format!("{} orders", grouped(s.rows))))];
    if s.source.is_some() {
        let shown = if view.is_sorting() {
            "Sorting...".to_string()
        } else {
            format!("Showing {}", grouped(view.shown_count(s.rows)))
        };
        segments.push(StatusBarSegment::create(AzString::from(shown)));
        segments.push(StatusBarSegment::create(AzString::from(sort_text(view))));
    }
    if !s.table.notice.is_empty() {
        segments.push(StatusBarSegment::create(AzString::from(s.table.notice.clone())));
    }
    StatusBar::create(segments).dom()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_has_the_data_sets_25_columns_with_their_kinds() {
        let cols = columns();
        assert_eq!(cols.len(), 25);
        assert_eq!(cols[data::c::SALES].sort_kind, DataTableSortKind::Number);
        assert_eq!(cols[data::c::ORDER_DATE].sort_kind, DataTableSortKind::Date);
        assert!(cols[data::c::QUANTITY].editable);
        assert!(!cols[data::c::SALES].editable);
    }

    #[test]
    fn counts_have_thousands_separators() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(500_000), "500,000");
        assert_eq!(grouped(1_234_567), "1,234,567");
    }

    #[test]
    fn the_sort_reads_as_its_columns_and_arrows() {
        let mut v = DataTableView::create();
        assert_eq!(sort_text(&v), "Unsorted");
        v.click_sort(data::c::SALES as u32, false);
        v.click_sort(data::c::SALES as u32, false);
        v.click_sort(data::c::REGION as u32, true);
        assert_eq!(sort_text(&v), "Sorted by Sales \u{25BC}, Region \u{25B2}");
    }

    #[test]
    fn a_cell_is_the_data_sets_text_and_value() {
        let source = RefAny::new(DataSet::generate(10));
        let c = cell(source.clone(), DataTableCellRef { row: 2, column: data::c::QUANTITY as u32 });
        let o = data::order(2);
        assert_eq!(c.text.as_str(), o.quantity.to_string());
        assert_eq!(c.value, f64::from(o.quantity));
        let past = cell(source, DataTableCellRef { row: 99, column: 0 });
        assert_eq!(past.text.as_str(), "");
    }
}
