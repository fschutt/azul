//! The process table: azul's `DataTable` over the model's rows.
//!
//! THE APP SORTS AND FILTERS, NOT THE WIDGET: the rows change every second,
//! and an order the widget computed from one reading would be stale at the
//! next. So the table's rows ARE the model's shown rows, in the model's
//! order; a header click hands its sort keys to the model
//! ([`keys_of`] -> `Model::set_sort`) and the view goes back "in app order"
//! ([`in_app_order`]: the header still shows the arrows). The widget's
//! filter row is off - the row under the table has the filter field (name,
//! user, PID).
//!
//! THE SELECTION IS A PROCESS, NOT A ROW: the model keeps the selected PID
//! and every build of the table puts its selection on that process' row
//! ([`follow_selection`]).
//!
//! THE SCROLL POSITION IS A PLACE IN THE PROCESSES, NOT A ROW NUMBER: the
//! table shows its rows from `view.top` (only those rows are built, the
//! scroll bar's thumb is `top` in all of them), but the order under that
//! number changes with every reading - a CPU sort re-sorts, processes start
//! and end. So the table remembers the rows it showed (`Monitor::shown`,
//! process ids): every position it reports (its first row, its cursor, a
//! drag) is a place in THOSE rows, and right before it is built again
//! [`sync`] carries the view over to the model's rows of now. The view
//! stays with the processes it shows ([`anchored_top`]: a process ending or
//! starting above them moves the view with them, as a terminal's scrollback
//! stays on its lines while output arrives); rows re-sorting among
//! themselves leave it at its row; a view at the first row shows the new
//! first rows. A new sort keeps the selected process on its row of the
//! screen ([`revealed_top`]).
//!
//! THE USER'S HAND WINS OVER THE CLOCK: while the user scrolls or drags in
//! the table (and for [`HANDS_OFF_MS`] after), a reading does not redraw it -
//! the rows do not re-sort under the pointer mid-gesture; the next reading
//! after the hand rests brings the table up to date ([`hands_on`]).
//!
//! DENSE, like the old Task Manager: [`ROW_PX`] rows in [`FONT_PX`] text,
//! the CPU in two digits ("07"), the memory in grouped kilobytes
//! ("12,345 K"), the command line as the Description.

use std::collections::HashMap;

use azul::{
    callbacks::{DataTableDataSourceCallbackType, DataTableOnEventCallbackType},
    file::DiskSpace,
    prelude::*,
    vec::U32Vec,
    widgets::{
        CellGridHorizontalAlign, DataTable, DataTableCell, DataTableCellRef, DataTableColumn,
        DataTableDragKind, DataTableEvent, DataTableEventKind, DataTableSortDirection,
        DataTableSortKey, DataTableSortKind, DataTableView, ListSelection,
    },
};

use crate::{
    ids,
    model::{format_cpu_column, format_k, Column, Model, ProcRow, SortKey, COLUMNS},
    ticks::LiveView,
    Monitor,
};

/// A row of the process table, px.
pub const ROW_PX: f32 = 20.0;
/// The table's text, px.
pub const FONT_PX: f32 = 12.0;
/// The table's header row, px (the DataTable's own).
pub const HEADER_PX: f32 = 30.0;
/// The table's scroll bars, px (the DataTable's own).
pub const SCROLLBAR_PX: f32 = 12.0;
/// The least width the table is built at, px.
pub const MIN_WIDTH: f32 = 200.0;
/// The least height the table is built at, px.
pub const MIN_HEIGHT: f32 = 120.0;
/// How long after the user's last scroll / drag in the table a reading
/// leaves the table as it is, ms.
pub const HANDS_OFF_MS: u64 = 600;

/// Whether the user's hand is on the table: a drag in progress, or a scroll
/// / drag `since_ms` ago that is younger than [`HANDS_OFF_MS`] (`None`: no
/// scroll yet).
#[must_use]
pub fn hands_on(view: &DataTableView, since_ms: Option<u64>) -> bool {
    !matches!(view.drag.kind, DataTableDragKind::None)
        || since_ms.is_some_and(|ms| ms < HANDS_OFF_MS)
}

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
            DataTableCell::create(format_cpu_column(f64::from(row.cpu)), f64::from(row.cpu))
        }
        Column::Memory => DataTableCell::create(format_k(row.memory), row.memory as f64),
        Column::Disk => DataTableCell::create(format_rate(row.disk_rate), row.disk_rate),
        Column::Description => DataTableCell::create_text(row.description().to_string()),
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

/// How many rows a table `height` px tall shows at once (under its header).
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few dozen rows
pub fn page_rows(height: f32) -> usize {
    let body = (height.max(MIN_HEIGHT) - HEADER_PX).max(ROW_PX);
    ((body / ROW_PX).floor() as usize).max(1)
}

/// How many rows the table of [`table`] - `rows` rows in `view`, built at
/// `width` x `height` - shows at once, as the DataTable lays itself out:
/// under its header, over its horizontal scroll bar when its columns (their
/// widths, the user's resizing over them) are wider than the table.
#[must_use]
pub fn rows_in_view(view: &DataTableView, rows: usize, width: f32, height: f32) -> usize {
    let (width, height) = (width.max(MIN_WIDTH), height.max(MIN_HEIGHT));
    let resizing = matches!(view.drag.kind, DataTableDragKind::ResizeColumn)
        .then_some((view.drag.column, view.drag.size));
    let widths = view.widths.as_slice();
    let columns: f32 = COLUMNS
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let i = u32::try_from(i).unwrap_or(u32::MAX);
            resizing
                .filter(|(column, _)| *column == i)
                .map(|(_, size)| size)
                .or_else(|| widths.iter().rev().find(|w| w.index == i).map(|w| w.size))
                .unwrap_or_else(|| c.width())
                .max(0.0)
        })
        .sum();
    let vbar = rows > page_rows(height);
    let hbar = columns > width - if vbar { SCROLLBAR_PX } else { 0.0 };
    if hbar {
        page_rows(height - SCROLLBAR_PX)
    } else {
        page_rows(height)
    }
}

/// The first row a table of `rows` rows, `page` of them in view, shows for
/// its view's `top`: never past its last screen.
#[must_use]
pub fn shown_top(top: u32, rows: usize, page: usize) -> usize {
    usize::try_from(top)
        .unwrap_or(usize::MAX)
        .min(rows.saturating_sub(page))
}

/// Where the first row shown goes when the rows shown change from `before`
/// to `after` (process ids, top to bottom), the view showing `page` rows
/// from row `top`.
///
/// THE VIEW STAYS WITH WHAT IT SHOWS: every process in view that is still
/// shown votes for the move that keeps it on its row of the screen, and the
/// move MOST of them share wins - processes that started or ended above the
/// view, or sorted past it, move the view along with its rows. When no move
/// has most of them (the rows in view re-sorted among themselves: a CPU
/// sort's busy processes), the view keeps its row, and so it does when none
/// of them is left. A view at the first row stays there: it shows the new
/// first rows, as a terminal at the bottom follows the output.
#[must_use]
#[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
// row numbers far below 2^62
pub fn anchored_top(before: &[u32], top: usize, page: usize, after: &[u32]) -> usize {
    let last = after.len().saturating_sub(1);
    if top == 0 || after.is_empty() {
        return 0;
    }
    let now: HashMap<u32, usize> = after.iter().enumerate().map(|(i, pid)| (*pid, i)).collect();
    // The moves (rows down, negative: up) and their votes, in the order the
    // rows in view first name them: the first row's wins a tie.
    let mut moves: Vec<(i64, usize)> = Vec::new();
    let mut voters = 0_usize;
    for (row, pid) in before.iter().enumerate().skip(top).take(page.max(1)) {
        let Some(&at) = now.get(pid) else {
            continue; // ended, or filtered out
        };
        voters += 1;
        let shift = at as i64 - row as i64;
        match moves.iter_mut().find(|(m, _)| *m == shift) {
            Some((_, votes)) => *votes += 1,
            None => moves.push((shift, 1)),
        }
    }
    let best = moves
        .iter()
        .copied()
        .fold(None::<(i64, usize)>, |best, m| match best {
            Some(b) if b.1 >= m.1 => Some(b),
            _ => Some(m),
        });
    let moved = match best {
        Some((shift, votes)) if votes * 2 > voters => (top as i64 + shift).max(0) as usize,
        _ => top,
    };
    moved.min(last)
}

/// The first row shown after a new sort put the selected process at
/// `position`: `row` is the row of the screen it was on before the sort
/// (`None`: it was not in view), `page` the rows in view. It stays on its
/// row (as close as the first row allows); a process that was not in view
/// comes into it - half a screen down when it is past the first screen.
#[must_use]
pub fn revealed_top(position: usize, row: Option<usize>, page: usize) -> usize {
    match row {
        Some(row) => position.saturating_sub(row),
        None if position < page => 0,
        None => position - page / 2,
    }
}

/// What a new sort keeps in view: the selected process, and the row of the
/// screen it was on before the sort (`None`: it was not in view).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortAnchor {
    pub pid: u32,
    pub row: Option<usize>,
}

/// The [`SortAnchor`] of `selected` in `view` as the table shows it: its rows
/// `shown` (process ids, top to bottom), `page` of them in view. `None`
/// without a selection.
#[must_use]
pub fn sort_anchor(
    view: &DataTableView,
    shown: &[u32],
    page: usize,
    selected: Option<u32>,
) -> Option<SortAnchor> {
    let pid = selected?;
    let top = usize::try_from(view.top).unwrap_or(0);
    let row = shown
        .iter()
        .position(|p| *p == pid)
        .and_then(|at| at.checked_sub(top))
        .filter(|row| *row < page);
    Some(SortAnchor { pid, row })
}

/// Carries `view` - its positions are places in the rows the table showed,
/// `shown` (process ids, top to bottom) - over to `model`'s rows of now,
/// right before the table is built again with `page` rows in view: the first
/// row follows the processes in view ([`anchored_top`]; after a new sort,
/// `anchor`, the selected process: [`revealed_top`]); a drag in progress
/// goes on from where it is; the selection follows its process. `shown`
/// becomes the model's rows.
///
/// `page` only says which rows were in view: the table keeps its first row
/// within its last screen itself, so a `top` past it is left to the table
/// (the app's count of rows in view can never hide its last row).
#[allow(clippy::cast_precision_loss)] // row numbers far below 2^24
pub fn sync(
    view: &mut DataTableView,
    shown: &mut Vec<u32>,
    model: &Model,
    page: usize,
    anchor: Option<SortAnchor>,
) {
    let rows = model.shown_pids();
    let top = usize::try_from(view.top).unwrap_or(0);
    // The first row the table showed.
    let seen = shown_top(view.top, shown.len(), page);
    let changed = *shown != rows;
    let placed = anchor.and_then(|a| {
        rows.iter()
            .position(|p| *p == a.pid)
            .map(|at| revealed_top(at, a.row, page))
    });
    let next = match placed {
        Some(at) => at,
        None if changed => anchored_top(shown, seen, page, &rows),
        None => top,
    };
    if changed {
        match view.drag.kind {
            // A thumb drag measures from the row it started on: that row
            // moved as the view did.
            DataTableDragKind::ScrollRows => {
                view.drag.start_size += next as f32 - seen as f32;
            }
            // A selection drag goes on from its first row's process.
            DataTableDragKind::Select => {
                let from = usize::try_from(view.drag.column)
                    .ok()
                    .and_then(|i| shown.get(i))
                    .and_then(|pid| rows.iter().position(|p| p == pid));
                if let Some(at) = from {
                    view.drag.column = u32::try_from(at).unwrap_or(0);
                }
            }
            _ => {}
        }
        *shown = rows;
    }
    view.top = u32::try_from(next).unwrap_or(0);
    follow_selection(view, model.selected_position(), model.shown_count());
}

/// The table as it is built, for the scripts: `AZMON_VIEW <top> <pid>
/// <selected> <name>` - the first row shown and its process, the selected
/// process' row of the screen (`-`: none selected, `out`: not in view).
pub fn print_view(s: &Monitor, page: usize) {
    let top = shown_top(s.table.top, s.model.shown_count(), page);
    let (pid, name) = s
        .model
        .shown_row(top)
        .map_or((0, ""), |r| (r.pid, r.name.as_str()));
    let selected = match s.model.selected_position() {
        None => "-".to_string(),
        Some(at) if at >= top && at < top + page => (at - top).to_string(),
        Some(_) => "out".to_string(),
    };
    println!("AZMON_VIEW {top} {pid} {selected} {name}");
}

/// The data callback: cell `at` of the rows the table shows
/// (`Monitor::shown`), from the model by process id. A build carries the
/// rows over first ([`sync`]); between builds the table's positions stay
/// places in the rows it shows, so what it asks for then - a copy, a column
/// fitted to its text - is what it shows, also after a reading re-sorted
/// the model.
pub extern "C" fn cell(mut app: RefAny, at: DataTableCellRef) -> DataTableCell {
    let Some(s) = app.downcast_ref::<Monitor>() else {
        return DataTableCell::empty();
    };
    let row = usize::try_from(at.row)
        .ok()
        .and_then(|r| s.shown.get(r))
        .and_then(|pid| s.model.row_of(*pid));
    let column = usize::try_from(at.column).ok().and_then(Column::at);
    match (row, column) {
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
        .with_viewport(width.max(MIN_WIDTH), height.max(MIN_HEIGHT))
        .with_row_height(ROW_PX)
        .with_font_size(FONT_PX)
        .with_frozen_columns(1)
        .with_show_filter_row(false)
        .with_read_only(true)
        .with_data_source(app.clone(), cell as DataTableDataSourceCallbackType)
        .with_on_event(app.clone(), on_table_event as DataTableOnEventCallbackType)
}

/// Every action in the table: a sort goes to the model, a selection becomes
/// the selected process; the view is stored as it is - its positions are
/// places in the rows the table SHOWS (`Monitor::shown`), which the next
/// build carries over to the model's rows ([`sync`]). A sort or a selection
/// rebuilds the page (the End button, the status bar); a scroll, a resize or
/// a drag re-renders the table's live view alone.
extern "C" fn on_table_event(
    mut app: RefAny,
    mut info: CallbackInfo,
    event: DataTableEvent,
) -> Update {
    let kind = event.kind;
    {
        let Some(mut guard) = app.downcast_mut::<Monitor>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let mut view = event.view;
        match kind {
            DataTableEventKind::Sort => {
                // Where the selected process is on the screen BEFORE the new
                // order (`s.table` is still the view the table showed).
                s.sort_anchor = sort_anchor(&s.table, &s.shown, s.table_page, s.model.selected());
                s.model.set_sort(keys_of(&view));
                view = in_app_order(view);
                println!("AZMON_SORT {}", crate::sort_text(s.model.sort()));
            }
            DataTableEventKind::Filter | DataTableEventKind::OrderReady => {
                view = in_app_order(view)
            }
            DataTableEventKind::Select => {
                // The process of the row the table showed there: the model
                // may have read the system since the table was built.
                let pid = view
                    .cursor_row()
                    .into_option()
                    .and_then(|r| usize::try_from(r).ok())
                    .and_then(|r| s.shown.get(r).copied());
                s.model.select(pid);
            }
            _ => {}
        }
        s.table = view;
        if matches!(kind, DataTableEventKind::Select) {
            crate::print_selected(s);
        }
        if matches!(kind, DataTableEventKind::Scroll | DataTableEventKind::Drag) {
            // The hand is on the table: the next readings leave it as it is.
            let now = s.now_ms();
            s.table_touched_ms = Some(now);
        }
        if matches!(kind, DataTableEventKind::Scroll) {
            println!("AZMON_SCROLL {}", s.table.top);
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
    use crate::model::{ProcSample, Snapshot, DEFAULT_SORT};

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
        assert_eq!(cols[Column::Name.index()].title.as_str(), "Image Name");
        assert_eq!(
            cols[Column::Description.index()].title.as_str(),
            "Description"
        );
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
        // The old Task Manager's spelling: two-digit CPU, grouped kilobytes.
        let cpu = cell_of(&r, Column::Cpu);
        assert_eq!(cpu.text.as_str(), "26");
        assert!((cpu.value - 25.6).abs() < 0.001);
        let pid = cell_of(&r, Column::Pid);
        assert_eq!(pid.text.as_str(), "5102");
        assert_eq!(pid.value, 5102.0);
        assert_eq!(cell_of(&r, Column::Memory).text.as_str(), "962,560 K");
        assert_eq!(cell_of(&r, Column::Disk).text.as_str(), "4.0 MB/s");
        assert_eq!(cell_of(&r, Column::Status).text.as_str(), "Running");
        // No command line: the description is the name.
        assert_eq!(cell_of(&r, Column::Description).text.as_str(), "cargo");
    }

    #[test]
    fn a_reading_leaves_the_table_alone_while_the_hand_is_on_it() {
        let mut v = DataTableView::create();
        assert!(!hands_on(&v, None), "never touched");
        assert!(hands_on(&v, Some(0)), "just scrolled");
        assert!(hands_on(&v, Some(HANDS_OFF_MS - 1)));
        assert!(!hands_on(&v, Some(HANDS_OFF_MS)), "the hand rested");
        v.drag.kind = DataTableDragKind::ScrollRows;
        assert!(hands_on(&v, None), "a thumb drag in progress");
        assert!(hands_on(&v, Some(10 * HANDS_OFF_MS)));
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

    // ---- the scroll position is a place in the processes, not a row number ----

    /// Processes `from..to`, as their ids.
    fn ids(from: u32, to: u32) -> Vec<u32> {
        (from..to).collect()
    }

    #[test]
    fn a_table_shows_as_many_rows_as_fit_under_its_header() {
        assert_eq!(page_rows(HEADER_PX + 12.0 * ROW_PX), 12);
        assert_eq!(page_rows(HEADER_PX + 12.0 * ROW_PX + 19.0), 12);
        // Never less than the least height the table is built at.
        assert_eq!(page_rows(0.0), 4);
    }

    #[test]
    fn a_process_ending_above_the_view_leaves_the_same_process_on_top() {
        // 40 processes, the view from row 10 (process 110), 12 rows of it.
        let before = ids(100, 140);
        let ended: Vec<u32> = before.iter().copied().filter(|p| *p != 103).collect();
        let top = anchored_top(&before, 10, 12, &ended);
        assert_eq!(ended[top], 110, "row {top} shows {}", ended[top]);
        // Two new processes sort in above the view: it moves two rows down.
        let mut started = before.clone();
        started.insert(0, 90);
        started.insert(5, 91);
        let top = anchored_top(&before, 10, 12, &started);
        assert_eq!(started[top], 110, "row {top} shows {}", started[top]);
    }

    #[test]
    fn the_rows_in_view_re_sorting_among_themselves_leave_the_view_at_its_row() {
        // A CPU sort: the rows in view swap places every reading; no move is
        // shared by most of them, so the view keeps its row.
        let before = ids(0, 40);
        let mut after = before.clone();
        after[10..22].reverse();
        assert_eq!(anchored_top(&before, 10, 12, &after), 10);
    }

    #[test]
    fn one_process_leaving_the_view_does_not_take_the_view_along() {
        // The process on top gets busy and sorts to the first row: the other
        // eleven in view stay where they are, so the view stays.
        let before = ids(0, 40);
        let mut after = before.clone();
        let busy = after.remove(10);
        after.insert(0, busy);
        assert_eq!(anchored_top(&before, 10, 12, &after), 10);
    }

    #[test]
    fn a_view_at_the_first_row_shows_the_new_first_rows() {
        let before = ids(0, 40);
        let mut after = before.clone();
        after.insert(0, 99);
        assert_eq!(anchored_top(&before, 0, 12, &after), 0);
    }

    #[test]
    fn when_every_process_in_view_is_gone_the_view_keeps_its_row() {
        let before = ids(0, 40);
        let after: Vec<u32> = before
            .iter()
            .copied()
            .filter(|p| !(10..22).contains(p))
            .collect();
        assert_eq!(anchored_top(&before, 10, 12, &after), 10);
        // ...within the rows there are now.
        assert_eq!(anchored_top(&before, 10, 12, &ids(0, 5)), 4);
    }

    #[test]
    fn a_new_sort_keeps_the_selected_process_on_its_row_of_the_screen() {
        // On row 7 of the screen before the sort, at row 50 of the table
        // after it: the view starts 7 rows above it.
        assert_eq!(revealed_top(50, Some(7), 20), 43);
        // Near the first row, as close as the table allows.
        assert_eq!(revealed_top(3, Some(7), 20), 0);
        // Not in view before the sort: in view after it (half a screen down
        // when it is past the first screen).
        assert_eq!(revealed_top(50, None, 20), 40);
        assert_eq!(revealed_top(12, None, 20), 0);
    }

    // ---- a build carries the view over to the rows of now ----

    /// A reading of processes `pids` (named `p<pid>`, idle).
    fn machine(pids: &[u32]) -> Snapshot {
        Snapshot {
            elapsed_ms: 1000,
            processes: pids
                .iter()
                .map(|pid| ProcSample {
                    pid: *pid,
                    name: format!("p{pid}"),
                    ..ProcSample::default()
                })
                .collect(),
            ..Snapshot::default()
        }
    }

    /// Processes 100..140 by PID, built once; the view from process 110
    /// (row 10), process 115 selected (row 5 of the screen), 12 rows in view.
    fn scrolled() -> (Model, DataTableView, Vec<u32>) {
        let mut m = Model::new();
        m.set_sort(Vec::new());
        m.apply(machine(&ids(100, 140)));
        let mut view = DataTableView::create();
        let mut shown = Vec::new();
        sync(&mut view, &mut shown, &m, 12, None);
        view.top = 10;
        m.select(Some(115));
        sync(&mut view, &mut shown, &m, 12, None);
        (m, view, shown)
    }

    #[test]
    fn a_reading_that_ends_a_process_above_the_view_keeps_the_view_on_its_processes() {
        let (mut m, mut view, mut shown) = scrolled();
        assert_eq!(view.top, 10);
        assert_eq!(view.cursor_row().into_option(), Some(15));
        // Process 103 ends, 150 starts (after the last row).
        let next: Vec<u32> = ids(100, 140)
            .into_iter()
            .filter(|p| *p != 103)
            .chain([150])
            .collect();
        m.apply(machine(&next));
        sync(&mut view, &mut shown, &m, 12, None);
        assert_eq!(m.shown_row(view.top as usize).map(|r| r.pid), Some(110));
        // The selection is on its process' new row, the table's rows are the model's.
        assert_eq!(view.cursor_row().into_option(), Some(14));
        assert_eq!(shown, m.shown_pids());
        // Nothing changed: the view stays as it is.
        sync(&mut view, &mut shown, &m, 12, None);
        assert_eq!(view.top, 9);
    }

    #[test]
    fn a_thumb_drag_in_progress_goes_on_from_the_row_it_started_on() {
        let (mut m, mut view, mut shown) = scrolled();
        view.drag.kind = DataTableDragKind::ScrollRows;
        view.drag.start_size = 10.0;
        m.apply(machine(&ids(101, 140)));
        sync(&mut view, &mut shown, &m, 12, None);
        assert_eq!(view.top, 9);
        assert!((view.drag.start_size - 9.0).abs() < 0.001);
    }

    #[test]
    fn a_new_sort_shows_the_selected_process_where_it_was_on_the_screen() {
        let (mut m, mut view, mut shown) = scrolled();
        let anchor = sort_anchor(&view, &shown, 12, m.selected());
        assert_eq!(
            anchor,
            Some(SortAnchor {
                pid: 115,
                row: Some(5)
            })
        );
        // By name, descending: p139 first, p115 on row 24. The table's own
        // sort click put its first row at 0.
        m.set_sort(vec![SortKey::new(Column::Name, true)]);
        view.top = 0;
        sync(&mut view, &mut shown, &m, 12, anchor);
        assert_eq!(view.top, 19);
        assert_eq!(view.cursor_row().into_option(), Some(24));
        assert_eq!(m.shown_row(24).map(|r| r.pid), Some(115));
    }

    #[test]
    fn a_table_wider_than_its_columns_shows_a_row_more_than_a_narrow_one() {
        let v = DataTableView::create();
        let height = HEADER_PX + 12.0 * ROW_PX + 5.0;
        // 950 px of columns: no horizontal scroll bar at 1200 px.
        assert_eq!(rows_in_view(&v, 100, 1200.0, height), 12);
        // At 800 px it takes 12 px of the rows' height.
        assert_eq!(rows_in_view(&v, 100, 800.0, height), 11);
    }
}
