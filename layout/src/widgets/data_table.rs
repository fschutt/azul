//! Data table widget - rows of records under a header of column titles: a
//! sortable, filterable, editable table over as many rows as the app has
//! (500,000 x 25 is the yardstick), with a cell cursor, row selection,
//! resizable columns and a filter row. The table of a records app, a
//! dashboard, a log viewer, a system monitor.
//!
//! GENERIC OVER ITS DATA: the table holds no rows. It asks the app for the
//! cells it shows through a DATA callback ([`DataTable::with_data_source`]:
//! the shown text and the number a Number / Date column sorts by), given the
//! APP'S row index and the column. Only the rows in view are ever asked for
//! and built.
//!
//! VIRTUALISED IN WHOLE ROWS (the scroll window of [`super::cell_grid`]):
//! the table shows the rows from [`DataTableView::top`] until its viewport
//! ([`DataTable::with_viewport`]) is full, and the columns from the frozen
//! ones, then [`DataTableView::left_column`]. The wheel, the keyboard and
//! the table's own scroll bars move `top` / `left_column`, never a pixel
//! offset, so the header (and the filter row under it) stay put by
//! construction and 500,000 rows need no 12-million-pixel scroll extent.
//!
//! SORT AND FILTER, BUILT IN: a click on a header sorts by its column
//! (ascending, descending, unsorted - stable), Shift+click adds the column
//! as one more key; the filter row under the header filters (Text:
//! contains, `=x` equals; Number: `5`, `=5`, `>5`, `<=5`, `5..10`; Date:
//! `2024`, `2024-03`, `2024-03-05`, `>2024-01`, `2024-01..2024-03`). The
//! rows shown are [`DataTableView::order`] - the app's row indices, top to
//! bottom - which the table computes itself, OFF THE UI THREAD for a big
//! table: the keys are read through the data callback in slices on a timer
//! (the callback is the app's, and a managed-language host must be called
//! on the UI thread), then sorted and filtered on an azul `Thread`; the
//! table says "Sorting..." meanwhile and stays usable, and an
//! [`DataTableEventKind::OrderReady`] event brings the order. A table of up
//! to [`DATA_TABLE_SYNC_ROWS`] rows is ordered at once, in the handler.
//!
//! THE APP OWNS THE STATE: the sort, the filters, the order, the selection,
//! the scroll position, an edit and a drag in progress are the
//! [`DataTableView`] the app hands in; every action reports a
//! [`DataTableEvent`] whose `view` is the NEXT view. The app stores
//! `event.view` and rebuilds; the kinds say what else happened (an edit was
//! committed, a column resized ...).
//!
//! EDITING: F2, Enter or a double-click edits the cursor's cell of an
//! editable column (a typed character starts an edit that replaces it);
//! Enter keeps the edit, Tab keeps it and moves right, Escape cancels. The
//! app VALIDATES through [`DataTable::with_on_edit`]: it stores the value
//! and accepts, or refuses with a reason - the edit then stays open and the
//! table reports [`DataTableEventKind::EditRefused`] with the reason.
//!
//! KEYBOARD (the table is ONE Tab stop): the arrows move the cell cursor
//! (Shift+Up / Down extend the row selection, Ctrl / Cmd+Up / Down move the
//! cursor alone), Page Up / Down by a screen, Home / End to the first / last
//! column, Ctrl+Home / Ctrl+End to the first / last row, Ctrl+A selects
//! every row shown, Ctrl+Space toggles the cursor's row, Ctrl+F edits the
//! filter of the cursor's column, Ctrl+C copies the selected rows
//! (tab-separated text and an HTML table).
//!
//! ACCESSIBILITY: the table is a `Grid` (its value says where the cursor
//! is), the header a `Row` of `ColumnHeader`s carrying the sort state
//! (`aria-sort`: [`azul_core::a11y::AccessibilityState::SortedAscending`] /
//! `SortedDescending`), every row a `Row` with its index in the WHOLE table,
//! every cell a `GridCell` named by its column title, its text its value.
//!
//! Key types: [`DataTable`], [`DataTableColumn`], [`DataTableView`],
//! [`DataTableEvent`], [`DataTableCell`], [`DataTableFilter`].

use alloc::{string::String, vec::Vec};

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec},
    events::FocusEventFilter,
    refany::RefAny,
    window::VirtualKeyCode,
};
use azul_css::{
    corety::{OptionU32, U32Vec, U64Vec},
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut, impl_vec_partialeq,
    AzString,
};

use crate::callbacks::CallbackInfo;
use crate::widgets::{
    cell_grid::{CellGridHorizontalAlign, CellGridSize, CellGridSizeVec},
    list_selection::ListSelection,
    themes::decl::{px_height, px_left, px_top, px_width, simple},
};

#[cfg(test)]
#[path = "data_table_tests.rs"]
mod data_table_tests;

// ---- the types the app sees ----

/// What a column holds: how it sorts and how its filter reads.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum DataTableSortKind {
    /// Text: sorted by the shown text (case folded); the filter finds the
    /// typed text in it, `=x` asks for the whole text.
    #[default]
    Text,
    /// A number: sorted by [`DataTableCell::value`]; the filter takes `5`
    /// (what shows as 5), `>5`, `>=5`, `<5`, `<=5` and `5..10`.
    Number,
    /// A date: [`DataTableCell::value`] is the day since 1970-01-01 (a
    /// fraction is the time of day); the filter takes `YYYY`, `YYYY-MM`,
    /// `YYYY-MM-DD`, comparisons and ranges of them.
    Date,
}

/// One column of the table.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DataTableColumn {
    /// The header's title (also what a screen reader names its cells by).
    pub title: AzString,
    /// The width in px (the user's resizing is kept in the view).
    pub width: f32,
    /// The cells' alignment: `General` puts numbers and dates right, text
    /// left.
    pub align: CellGridHorizontalAlign,
    /// How the column sorts and filters.
    pub sort_kind: DataTableSortKind,
    /// A header click sorts by it (default on).
    pub sortable: bool,
    /// It has a filter in the filter row (default on).
    pub filterable: bool,
    /// Its cells can be edited (default off; the app validates every edit).
    pub editable: bool,
}

impl DataTableColumn {
    /// A sortable, filterable, read-only column `width` px wide.
    #[must_use]
    pub const fn create(title: AzString, width: f32, sort_kind: DataTableSortKind) -> Self {
        Self {
            title,
            width,
            align: CellGridHorizontalAlign::General,
            sort_kind,
            sortable: true,
            filterable: true,
            editable: false,
        }
    }

    /// The cells' alignment.
    pub const fn set_align(&mut self, align: CellGridHorizontalAlign) {
        self.align = align;
    }

    /// [`Self::set_align`] for the builder chain.
    #[must_use]
    pub const fn with_align(mut self, align: CellGridHorizontalAlign) -> Self {
        self.set_align(align);
        self
    }

    /// Whether a header click sorts by the column.
    pub const fn set_sortable(&mut self, sortable: bool) {
        self.sortable = sortable;
    }

    /// [`Self::set_sortable`] for the builder chain.
    #[must_use]
    pub const fn with_sortable(mut self, sortable: bool) -> Self {
        self.set_sortable(sortable);
        self
    }

    /// Whether the column has a filter.
    pub const fn set_filterable(&mut self, filterable: bool) {
        self.filterable = filterable;
    }

    /// [`Self::set_filterable`] for the builder chain.
    #[must_use]
    pub const fn with_filterable(mut self, filterable: bool) -> Self {
        self.set_filterable(filterable);
        self
    }

    /// Whether the column's cells can be edited.
    pub const fn set_editable(&mut self, editable: bool) {
        self.editable = editable;
    }

    /// [`Self::set_editable`] for the builder chain.
    #[must_use]
    pub const fn with_editable(mut self, editable: bool) -> Self {
        self.set_editable(editable);
        self
    }
}

impl_option!(
    DataTableColumn,
    OptionDataTableColumn,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    DataTableColumn,
    DataTableColumnVec,
    DataTableColumnVecDestructor,
    DataTableColumnVecDestructorType,
    DataTableColumnVecSlice,
    OptionDataTableColumn
);
impl_vec_clone!(DataTableColumn, DataTableColumnVec, DataTableColumnVecDestructor);
impl_vec_debug!(DataTableColumn, DataTableColumnVec);
impl_vec_mut!(DataTableColumn, DataTableColumnVec);
impl_vec_partialeq!(DataTableColumn, DataTableColumnVec);

/// A cell: the APP'S row index (not where the row is shown) and the column.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DataTableCellRef {
    /// The app's row index, 0-based.
    pub row: u32,
    /// The column, 0-based.
    pub column: u32,
}

impl DataTableCellRef {
    /// Row `row` (the app's index), column `column`.
    #[must_use]
    pub const fn create(row: u32, column: u32) -> Self {
        Self { row, column }
    }
}

impl_option!(
    DataTableCellRef,
    OptionDataTableCellRef,
    [Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// One cell's content - what the DATA callback answers: the text shown
/// (formatted already: "1,250.00", "2024-03-05") and, for a Number or a
/// Date column, the number it sorts and range-filters by (NaN = blank;
/// blanks sort last either way).
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DataTableCell {
    /// The text shown.
    pub text: AzString,
    /// A Number / Date column's sort key; NaN for none.
    pub value: f64,
}

impl DataTableCell {
    /// A cell showing `text` that sorts by `value`.
    #[must_use]
    pub const fn create(text: AzString, value: f64) -> Self {
        Self { text, value }
    }

    /// A text cell (no number).
    #[must_use]
    pub const fn create_text(text: AzString) -> Self {
        Self {
            text,
            value: f64::NAN,
        }
    }

    /// An empty cell.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            text: AzString::from_const_str(""),
            value: f64::NAN,
        }
    }
}

impl Default for DataTableCell {
    fn default() -> Self {
        Self::empty()
    }
}

impl azul_core::host_invoker::HostOut for DataTableCell {
    fn unwritten() -> Self {
        Self::empty()
    }
}

/// Which way a sort key runs.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum DataTableSortDirection {
    /// A to Z, small to big, old to new.
    #[default]
    Ascending,
    /// Z to A, big to small, new to old.
    Descending,
}

/// One sort key: a column and its direction.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DataTableSortKey {
    /// The column.
    pub column: u32,
    /// Which way.
    pub direction: DataTableSortDirection,
}

impl DataTableSortKey {
    /// Sort by `column`, `direction`.
    #[must_use]
    pub const fn create(column: u32, direction: DataTableSortDirection) -> Self {
        Self { column, direction }
    }
}

impl_option!(
    DataTableSortKey,
    OptionDataTableSortKey,
    [Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_vec!(
    DataTableSortKey,
    DataTableSortKeyVec,
    DataTableSortKeyVecDestructor,
    DataTableSortKeyVecDestructorType,
    DataTableSortKeyVecSlice,
    OptionDataTableSortKey
);
impl_vec_clone!(DataTableSortKey, DataTableSortKeyVec, DataTableSortKeyVecDestructor);
impl_vec_debug!(DataTableSortKey, DataTableSortKeyVec);
impl_vec_mut!(DataTableSortKey, DataTableSortKeyVec);
impl_vec_partialeq!(DataTableSortKey, DataTableSortKeyVec);

/// What a filter asks of its column.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum DataTableFilterOp {
    /// The shown text contains the typed text (case folded).
    #[default]
    Contains,
    /// The shown text is the typed text (Text), or the value is what the
    /// typed number / date names (Number, Date: within `min` .. `max`).
    Equals,
    /// The value lies within `min` .. `max`.
    Range,
}

/// One column's filter: what was typed and what it means.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DataTableFilter {
    /// The text typed into the filter row (shown there).
    pub text: AzString,
    /// Equals / Range: the lower bound (`-inf` = none).
    pub min: f64,
    /// Equals / Range: the upper bound (`+inf` = none).
    pub max: f64,
    /// The column.
    pub column: u32,
    /// What the filter asks.
    pub op: DataTableFilterOp,
    /// `min` itself passes.
    pub min_inclusive: bool,
    /// `max` itself passes.
    pub max_inclusive: bool,
}

impl_option!(
    DataTableFilter,
    OptionDataTableFilter,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    DataTableFilter,
    DataTableFilterVec,
    DataTableFilterVecDestructor,
    DataTableFilterVecDestructorType,
    DataTableFilterVecSlice,
    OptionDataTableFilter
);
impl_vec_clone!(DataTableFilter, DataTableFilterVec, DataTableFilterVecDestructor);
impl_vec_debug!(DataTableFilter, DataTableFilterVec);
impl_vec_mut!(DataTableFilter, DataTableFilterVec);
impl_vec_partialeq!(DataTableFilter, DataTableFilterVec);

/// What a pointer drag over the table is doing.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum DataTableDragKind {
    /// No drag.
    #[default]
    None,
    /// Selecting rows from the pressed one.
    Select,
    /// Dragging the right edge of column `column`'s header.
    ResizeColumn,
    /// Dragging the vertical scroll bar's thumb.
    ScrollRows,
    /// Dragging the horizontal scroll bar's thumb.
    ScrollColumns,
}

/// A pointer drag in progress (kept in the view between rebuilds).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct DataTableDrag {
    /// The pointer's position along the drag axis when it was pressed, in
    /// window px.
    pub start_px: f32,
    /// Resize: the column's width when it was pressed; a thumb drag: `top`
    /// / `left_column` when it was pressed.
    pub start_size: f32,
    /// Resize: the width the drag has reached (drawn until the release).
    pub size: f32,
    /// Resize: the column; Select: the shown position the drag started on.
    pub column: u32,
    /// What the drag does.
    pub kind: DataTableDragKind,
}

/// What is being edited.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum DataTableEditTarget {
    /// Nothing: the keys move the cursor.
    #[default]
    None,
    /// The cell `edit_row` x `edit_column`.
    Cell,
    /// The filter of `edit_column`.
    Filter,
}

/// The state of a table the APP keeps. Every [`DataTableEvent`] carries the
/// next one; store it and rebuild.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DataTableView {
    /// The sort keys, the first the primary one; empty = the app's order.
    pub sort: DataTableSortKeyVec,
    /// The filters, at most one per column.
    pub filters: DataTableFilterVec,
    /// The rows shown, top to bottom, as the app's row indices - what the
    /// sort and the filters made (only read when `ordered`).
    pub order: U32Vec,
    /// The selected rows (the app's row indices; the selection's `focus` is
    /// the cursor's row).
    pub selection: ListSelection,
    /// The columns the user resized (px), over the columns' own widths.
    pub widths: CellGridSizeVec,
    /// The edit's text (a cell's or a filter's).
    pub edit_text: AzString,
    /// The first row shown: a POSITION among the rows shown, not an app row.
    pub top: u32,
    /// The first scrolled column right of the frozen ones.
    pub left_column: u32,
    /// The cursor's column.
    pub active_column: u32,
    /// The caret in `edit_text`, in characters.
    pub edit_cursor: u32,
    /// The edited cell's app row (`edit` = Cell).
    pub edit_row: u32,
    /// The edited column (a cell's or a filter's).
    pub edit_column: u32,
    /// Bumped by every change of the sort or the filters.
    pub query_serial: u32,
    /// The `query_serial` the `order` belongs to: while the two differ the
    /// table is sorting (and shows the previous order).
    pub order_serial: u32,
    /// A pointer drag in progress.
    pub drag: DataTableDrag,
    /// What is being edited.
    pub edit: DataTableEditTarget,
    /// `order` holds the rows shown; `false` = every row in the app's order.
    pub ordered: bool,
}

impl DataTableView {
    /// No sort, no filter (every row in the app's order), nothing
    /// selected, scrolled to the top-left, nothing edited.
    #[must_use]
    pub const fn create() -> Self {
        Self {
            sort: DataTableSortKeyVec::from_const_slice(&[]),
            filters: DataTableFilterVec::from_const_slice(&[]),
            order: U32Vec::from_const_slice(&[]),
            selection: ListSelection::create(),
            widths: CellGridSizeVec::from_const_slice(&[]),
            edit_text: AzString::from_const_str(""),
            top: 0,
            left_column: 0,
            active_column: 0,
            edit_cursor: 0,
            edit_row: 0,
            edit_column: 0,
            query_serial: 0,
            order_serial: 0,
            drag: DataTableDrag {
                start_px: 0.0,
                start_size: 0.0,
                size: 0.0,
                column: 0,
                kind: DataTableDragKind::None,
            },
            edit: DataTableEditTarget::None,
            ordered: false,
        }
    }

    /// Whether the rows are being sorted / filtered (the order shown is
    /// still the previous one).
    #[must_use]
    pub const fn is_sorting(&self) -> bool {
        self.query_serial != self.order_serial
    }

    /// Whether a sort key or a filter is set.
    #[must_use]
    pub const fn has_query(&self) -> bool {
        !self.sort.is_empty() || !self.filters.is_empty()
    }

    /// Whether a cell or a filter is being edited.
    #[must_use]
    pub const fn is_editing(&self) -> bool {
        !matches!(self.edit, DataTableEditTarget::None)
    }

    /// How many rows are shown, of the app's `row_count`.
    #[must_use]
    pub fn shown_count(&self, row_count: u32) -> u32 {
        if self.ordered {
            u32::try_from(self.order.len()).unwrap_or(u32::MAX)
        } else {
            row_count
        }
    }

    /// The app's row shown at `position` (0 = the first row shown).
    #[must_use]
    pub fn row_at(&self, position: u32, row_count: u32) -> OptionU32 {
        if self.ordered {
            self.order
                .get(position as usize)
                .copied()
                .filter(|r| *r < row_count)
                .into()
        } else if position < row_count {
            OptionU32::Some(position)
        } else {
            OptionU32::None
        }
    }

    /// Where the app's row `row` is shown (`None`: filtered out).
    #[must_use]
    pub fn position_of(&self, row: u32, row_count: u32) -> OptionU32 {
        if self.ordered {
            self.order
                .as_slice()
                .iter()
                .position(|r| *r == row)
                .and_then(|p| u32::try_from(p).ok())
                .into()
        } else if row < row_count {
            OptionU32::Some(row)
        } else {
            OptionU32::None
        }
    }

    /// The cursor's row (the app's index): the selection's focus.
    #[must_use]
    pub fn cursor_row(&self) -> OptionU32 {
        self.selection
            .focus
            .into_option()
            .and_then(|k| u32::try_from(k).ok())
            .into()
    }

    /// The sort a header click on `column` makes: plain, the column alone
    /// (ascending, then descending, then unsorted when it is already the
    /// only key); `add` (Shift), the column as one more key (ascending,
    /// then descending, then out). The order follows (see the module).
    pub fn click_sort(&mut self, column: u32, add: bool) {
        use DataTableSortDirection::{Ascending, Descending};
        let mut keys = self.sort.as_slice().to_vec();
        let at = keys.iter().position(|k| k.column == column);
        if add {
            match at {
                Some(i) => match keys[i].direction {
                    Ascending => keys[i].direction = Descending,
                    Descending => {
                        keys.remove(i);
                    }
                },
                None => keys.push(DataTableSortKey::create(column, Ascending)),
            }
        } else {
            keys = match (at, keys.len()) {
                (Some(0), 1) => match keys[0].direction {
                    Ascending => alloc::vec![DataTableSortKey::create(column, Descending)],
                    Descending => Vec::new(),
                },
                (Some(0), _) => {
                    let flipped = match keys[0].direction {
                        Ascending => Descending,
                        Descending => Ascending,
                    };
                    alloc::vec![DataTableSortKey::create(column, flipped)]
                }
                _ => alloc::vec![DataTableSortKey::create(column, Ascending)],
            };
        }
        self.set_sort(DataTableSortKeyVec::from_vec(keys));
    }

    /// Sorts by `keys` (the first the primary one; empty = the app's
    /// order). The order follows (see the module).
    pub fn set_sort(&mut self, keys: DataTableSortKeyVec) {
        if keys != self.sort {
            self.sort = keys;
            self.bump_query();
        }
    }

    /// No sort: the app's order (the filters stay).
    pub fn clear_sort(&mut self) {
        self.set_sort(DataTableSortKeyVec::from_const_slice(&[]));
    }

    /// The filter of `column` (a column of kind `kind`) is `text`; an empty
    /// text removes it. The order follows (see the module).
    pub fn set_filter(&mut self, column: u32, kind: DataTableSortKind, text: AzString) {
        let mut filters: Vec<DataTableFilter> = self
            .filters
            .as_slice()
            .iter()
            .filter(|f| f.column != column)
            .cloned()
            .collect();
        if !text.as_str().trim().is_empty() {
            filters.push(DataTableFilter::parse(column, kind, text));
            filters.sort_by_key(|f| f.column);
        }
        let filters = DataTableFilterVec::from_vec(filters);
        if filters != self.filters {
            self.filters = filters;
            self.bump_query();
        }
    }

    /// No filters (the sort stays).
    pub fn clear_filters(&mut self) {
        if !self.filters.is_empty() {
            self.filters = DataTableFilterVec::from_const_slice(&[]);
            self.bump_query();
        }
    }

    /// The text of `column`'s filter ("" = none).
    #[must_use]
    pub fn filter_text(&self, column: u32) -> AzString {
        self.filters
            .as_slice()
            .iter()
            .find(|f| f.column == column)
            .map_or_else(|| AzString::from_const_str(""), |f| f.text.clone())
    }

    /// `column`'s place among the sort keys and its direction.
    #[must_use]
    pub(crate) fn sort_of(&self, column: u32) -> Option<(usize, DataTableSortDirection)> {
        self.sort
            .as_slice()
            .iter()
            .position(|k| k.column == column)
            .map(|i| (i, self.sort.as_slice()[i].direction))
    }

    /// A new query: the order no longer fits it - unless there is no query
    /// left, which needs no work (every row in the app's order).
    fn bump_query(&mut self) {
        self.query_serial = self.query_serial.wrapping_add(1);
        self.top = 0;
        if !self.has_query() {
            self.order = U32Vec::from_const_slice(&[]);
            self.ordered = false;
            self.order_serial = self.query_serial;
        }
    }

    /// The view with `order` (the rows `serial`'s query shows) in place;
    /// the selection keeps only the rows still shown.
    #[must_use]
    pub(crate) fn with_order(mut self, serial: u32, order: Vec<u32>) -> Self {
        if serial != self.query_serial {
            return self; // a newer query is on its way
        }
        let shown = u32::try_from(order.len()).unwrap_or(u32::MAX);
        if !self.selection.keys.is_empty() || self.selection.focus.is_some() {
            let keys: Vec<u64> = order.iter().map(|r| u64::from(*r)).collect();
            self.selection.retain_in(U64Vec::from_vec(keys));
        }
        self.order = U32Vec::from_vec(order);
        self.ordered = true;
        self.order_serial = serial;
        self.top = self.top.min(shown.saturating_sub(1));
        self
    }
}

impl Default for DataTableView {
    fn default() -> Self {
        Self::create()
    }
}

// ---- filters: what a typed filter means ----

impl DataTableFilter {
    /// What `text`, typed into the filter of `column` (a column of kind
    /// `kind`), asks - see [`DataTableSortKind`]. A Number or Date filter
    /// that reads as neither falls back to "the shown text contains it".
    #[must_use]
    pub fn parse(column: u32, kind: DataTableSortKind, text: AzString) -> Self {
        let typed = String::from(text.as_str().trim());
        let mut filter = Self {
            text,
            min: f64::NEG_INFINITY,
            max: f64::INFINITY,
            column,
            op: DataTableFilterOp::Contains,
            min_inclusive: true,
            max_inclusive: true,
        };
        let bounds = match kind {
            DataTableSortKind::Text => {
                if typed.starts_with('=') {
                    filter.op = DataTableFilterOp::Equals;
                }
                None
            }
            DataTableSortKind::Number => number_bounds(&typed),
            DataTableSortKind::Date => date_bounds(&typed),
        };
        if let Some(b) = bounds {
            filter.op = b.op;
            filter.min = b.min;
            filter.max = b.max;
            filter.min_inclusive = b.min_inclusive;
            filter.max_inclusive = b.max_inclusive;
        }
        filter
    }

    /// Whether nothing was typed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.as_str().trim().is_empty()
    }

    /// Whether `value` lies within the bounds.
    #[must_use]
    pub fn admits(&self, value: f64) -> bool {
        if value.is_nan() {
            return false;
        }
        let above = if self.min_inclusive {
            value >= self.min
        } else {
            value > self.min
        };
        let below = if self.max_inclusive {
            value <= self.max
        } else {
            value < self.max
        };
        above && below
    }

    /// Whether the filter reads the shown text (Contains, or a Text
    /// column's Equals) rather than the value.
    pub(crate) fn reads_text(&self, kind: DataTableSortKind) -> bool {
        self.op == DataTableFilterOp::Contains
            || (kind == DataTableSortKind::Text && self.op == DataTableFilterOp::Equals)
    }

    /// The folded text a text filter looks for (the `=` of Equals off).
    pub(crate) fn needle(&self) -> String {
        let t = self.text.as_str().trim();
        let t = if self.op == DataTableFilterOp::Equals {
            t.strip_prefix('=').unwrap_or(t).trim()
        } else {
            t
        };
        fold(t)
    }
}

/// The bounds a typed number or date filter names.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Bounds {
    min: f64,
    max: f64,
    min_inclusive: bool,
    max_inclusive: bool,
    op: DataTableFilterOp,
}

impl Bounds {
    const fn open(op: DataTableFilterOp) -> Self {
        Self {
            min: f64::NEG_INFINITY,
            max: f64::INFINITY,
            min_inclusive: true,
            max_inclusive: true,
            op,
        }
    }
}

/// Case folded, for comparing texts.
pub(crate) fn fold(text: &str) -> String {
    text.to_lowercase()
}

/// A typed number: `1,234.5`, `-3`, `12 %`. Returns the number and how
/// many decimals were typed.
fn typed_number(text: &str) -> Option<(f64, i32)> {
    let cleaned: String = text
        .trim()
        .trim_end_matches('%')
        .chars()
        .filter(|c| *c != ',' && !c.is_whitespace())
        .collect();
    if cleaned.is_empty() {
        return None;
    }
    let value: f64 = cleaned.parse().ok()?;
    if !value.is_finite() {
        return None;
    }
    let decimals = cleaned
        .split_once('.')
        .map_or(0, |(_, f)| i32::try_from(f.len()).unwrap_or(0));
    Some((value, decimals))
}

/// A Number column's filter: `a..b` (either end may be left out), `>a`,
/// `>=a`, `<a`, `<=a`, `=a` or `a` (what shows as `a`: within half a unit
/// of its last typed decimal).
fn number_bounds(typed: &str) -> Option<Bounds> {
    let t = typed.trim();
    if let Some((lo, hi)) = t.split_once("..") {
        let mut b = Bounds::open(DataTableFilterOp::Range);
        if !lo.trim().is_empty() {
            b.min = typed_number(lo)?.0;
        }
        if !hi.trim().is_empty() {
            b.max = typed_number(hi)?.0;
        }
        return Some(b);
    }
    let mut b = Bounds::open(DataTableFilterOp::Range);
    if let Some(rest) = t.strip_prefix(">=") {
        b.min = typed_number(rest)?.0;
    } else if let Some(rest) = t.strip_prefix("<=") {
        b.max = typed_number(rest)?.0;
    } else if let Some(rest) = t.strip_prefix('>') {
        b.min = typed_number(rest)?.0;
        b.min_inclusive = false;
    } else if let Some(rest) = t.strip_prefix('<') {
        b.max = typed_number(rest)?.0;
        b.max_inclusive = false;
    } else {
        let rest = t.strip_prefix('=').unwrap_or(t);
        let (value, decimals) = typed_number(rest)?;
        let half = 0.5 * 10f64.powi(-decimals);
        b = Bounds {
            min: value - half,
            max: value + half,
            min_inclusive: true,
            max_inclusive: false,
            op: DataTableFilterOp::Equals,
        };
    }
    Some(b)
}

/// The day number (days since 1970-01-01) of a civil date (Howard
/// Hinnant's `days_from_civil`).
pub(crate) fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = i64::from(month);
    let d = i64::from(day);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The days `[start, end)` a typed date names: `YYYY`, `YYYY-MM` or
/// `YYYY-MM-DD` (a real date).
#[allow(clippy::cast_precision_loss)] // day numbers are far below 2^52
fn date_span(text: &str) -> Option<(f64, f64)> {
    let parts: Vec<&str> = text.trim().split('-').map(str::trim).collect();
    let year: u32 = parts.first()?.parse().ok()?;
    if !(1000..=9999).contains(&year) || parts.len() > 3 {
        return None;
    }
    let y = i64::from(year);
    let (start, end) = match parts.len() {
        1 => (days_from_civil(y, 1, 1), days_from_civil(y + 1, 1, 1)),
        2 => {
            let month: u32 = parts[1].parse().ok()?;
            if !(1..=12).contains(&month) {
                return None;
            }
            let next = if month == 12 {
                days_from_civil(y + 1, 1, 1)
            } else {
                days_from_civil(y, month + 1, 1)
            };
            (days_from_civil(y, month, 1), next)
        }
        _ => {
            let month: u32 = parts[1].parse().ok()?;
            let day: u32 = parts[2].parse().ok()?;
            if !(1..=12).contains(&month)
                || day == 0
                || day > crate::widgets::date_picker::days_in_month(year, month)
            {
                return None;
            }
            let d = days_from_civil(y, month, day);
            (d, d + 1)
        }
    };
    Some((start as f64, end as f64))
}

/// A Date column's filter: a period (`2024`, `2024-03`, `2024-03-05`) is
/// every day in it; `a..b` from the start of `a` to the end of `b`; `>a`
/// after the period, `>=a` from its start, `<a` before it, `<=a` to its end.
fn date_bounds(typed: &str) -> Option<Bounds> {
    let t = typed.trim();
    let mut b = Bounds::open(DataTableFilterOp::Range);
    if let Some((lo, hi)) = t.split_once("..") {
        if !lo.trim().is_empty() {
            b.min = date_span(lo)?.0;
        }
        if !hi.trim().is_empty() {
            b.max = date_span(hi)?.1;
            b.max_inclusive = false;
        }
        return Some(b);
    }
    if let Some(rest) = t.strip_prefix(">=") {
        b.min = date_span(rest)?.0;
    } else if let Some(rest) = t.strip_prefix("<=") {
        b.max = date_span(rest)?.1;
        b.max_inclusive = false;
    } else if let Some(rest) = t.strip_prefix('>') {
        b.min = date_span(rest)?.1;
    } else if let Some(rest) = t.strip_prefix('<') {
        b.max = date_span(rest)?.0;
        b.max_inclusive = false;
    } else {
        let (start, end) = date_span(t.strip_prefix('=').unwrap_or(t))?;
        b = Bounds {
            min: start,
            max: end,
            min_inclusive: true,
            max_inclusive: false,
            op: DataTableFilterOp::Equals,
        };
    }
    Some(b)
}

// ---- the order: which rows show, in which order ----

/// A table of at most this many rows is sorted and filtered at once, in the
/// handler; a bigger one off the UI thread (see the module).
pub const DATA_TABLE_SYNC_ROWS: u32 = 20_000;

/// One column a query reads, and what of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlanColumn {
    /// The table's column.
    pub column: u32,
    /// Its folded texts are needed (a text sort or a text filter).
    pub texts: bool,
    /// Its values are needed (a number / date sort or range).
    pub values: bool,
}

/// One sort key of a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlanSort {
    /// The plan column (an index into [`QueryPlan::columns`]).
    pub slot: usize,
    pub descending: bool,
    /// By the folded text (a Text column), else by the value.
    pub by_text: bool,
}

/// One filter of a plan.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlanFilter {
    /// The plan column.
    pub slot: usize,
    /// The filter, as typed and parsed.
    pub filter: DataTableFilter,
    /// It reads the folded text (else the value).
    pub by_text: bool,
    /// The folded text a text filter looks for.
    pub needle: String,
}

/// What a query needs: the columns it reads, its keys, its filters.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct QueryPlan {
    pub columns: Vec<PlanColumn>,
    pub sort: Vec<PlanSort>,
    pub filters: Vec<PlanFilter>,
}

impl QueryPlan {
    /// Whether the query keeps every row in the app's order.
    pub(crate) fn is_empty(&self) -> bool {
        self.sort.is_empty() && self.filters.is_empty()
    }
}

/// The keys of one plan column, row by row (only what the plan needs).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ColumnKeys {
    pub texts: Vec<String>,
    pub values: Vec<f64>,
}

/// The plan of `view`'s query over `columns`: keys and filters on columns
/// that do not exist (or do not sort / filter) are left out.
pub(crate) fn plan_of(view: &DataTableView, columns: &[DataTableColumn]) -> QueryPlan {
    /// The slot of `column`, added (or widened) to read what is asked.
    fn slot_for(plan: &mut QueryPlan, column: u32, texts: bool, values: bool) -> usize {
        if let Some(i) = plan.columns.iter().position(|c| c.column == column) {
            plan.columns[i].texts |= texts;
            plan.columns[i].values |= values;
            return i;
        }
        plan.columns.push(PlanColumn {
            column,
            texts,
            values,
        });
        plan.columns.len() - 1
    }

    let mut plan = QueryPlan::default();
    for key in view.sort.as_slice() {
        let Some(c) = columns.get(key.column as usize) else {
            continue;
        };
        if !c.sortable {
            continue;
        }
        let by_text = c.sort_kind == DataTableSortKind::Text;
        let slot = slot_for(&mut plan, key.column, by_text, !by_text);
        plan.sort.push(PlanSort {
            slot,
            descending: key.direction == DataTableSortDirection::Descending,
            by_text,
        });
    }
    for filter in view.filters.as_slice() {
        let Some(c) = columns.get(filter.column as usize) else {
            continue;
        };
        if !c.filterable || filter.is_empty() {
            continue;
        }
        let by_text = filter.reads_text(c.sort_kind);
        let slot = slot_for(&mut plan, filter.column, by_text, !by_text);
        plan.filters.push(PlanFilter {
            slot,
            filter: filter.clone(),
            by_text,
            needle: filter.needle(),
        });
    }
    plan
}

/// Reads the keys of rows `from..to` through the data callback, appending
/// them to `keys` (one entry per plan column).
pub(crate) fn read_keys(
    source: &OptionDataTableDataSource,
    plan: &QueryPlan,
    keys: &mut Vec<ColumnKeys>,
    from: u32,
    to: u32,
) {
    if keys.len() < plan.columns.len() {
        keys.resize_with(plan.columns.len(), ColumnKeys::default);
    }
    for row in from..to {
        for (slot, pc) in plan.columns.iter().enumerate() {
            let cell = cell_content(source, DataTableCellRef::create(row, pc.column));
            let k = &mut keys[slot];
            if pc.texts {
                k.texts.push(fold(cell.text.as_str()));
            }
            if pc.values {
                k.values.push(cell.value);
            }
        }
    }
}

impl PlanFilter {
    /// Whether row `row` passes (its keys in `keys`).
    fn passes(&self, keys: &[ColumnKeys], row: usize) -> bool {
        let Some(k) = keys.get(self.slot) else {
            return true;
        };
        if self.by_text {
            let text = k.texts.get(row).map_or("", String::as_str);
            match self.filter.op {
                DataTableFilterOp::Equals => text == self.needle,
                DataTableFilterOp::Contains | DataTableFilterOp::Range => text.contains(self.needle.as_str()),
            }
        } else {
            self.filter.admits(k.values.get(row).copied().unwrap_or(f64::NAN))
        }
    }
}

/// Two values, blanks (NaN) last whichever way the key runs.
fn compare_values(a: f64, b: f64, descending: bool) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            let o = a.partial_cmp(&b).unwrap_or(Ordering::Equal);
            if descending {
                o.reverse()
            } else {
                o
            }
        }
    }
}

/// Two folded texts, blanks ("") last whichever way the key runs.
fn compare_texts(a: &str, b: &str, descending: bool) -> core::cmp::Ordering {
    use core::cmp::Ordering;
    match (a.is_empty(), b.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            let o = a.cmp(b);
            if descending {
                o.reverse()
            } else {
                o
            }
        }
    }
}

/// Rows `a` and `b` by the plan's sort keys, in turn.
fn compare_rows(plan: &QueryPlan, keys: &[ColumnKeys], a: usize, b: usize) -> core::cmp::Ordering {
    for s in &plan.sort {
        let Some(k) = keys.get(s.slot) else {
            continue;
        };
        let o = if s.by_text {
            compare_texts(
                k.texts.get(a).map_or("", String::as_str),
                k.texts.get(b).map_or("", String::as_str),
                s.descending,
            )
        } else {
            compare_values(
                k.values.get(a).copied().unwrap_or(f64::NAN),
                k.values.get(b).copied().unwrap_or(f64::NAN),
                s.descending,
            )
        };
        if o != core::cmp::Ordering::Equal {
            return o;
        }
    }
    core::cmp::Ordering::Equal
}

/// The rows `0..row_count` that pass every filter, in the order of the
/// sort keys (stable: rows with equal keys keep the app's order; blanks
/// last in either direction).
pub(crate) fn compute_order(row_count: u32, plan: &QueryPlan, keys: &[ColumnKeys]) -> Vec<u32> {
    let mut rows: Vec<u32> = (0..row_count)
        .filter(|r| plan.filters.iter().all(|f| f.passes(keys, *r as usize)))
        .collect();
    if !plan.sort.is_empty() {
        // `sort_by` is stable: equal keys keep the app's order.
        rows.sort_by(|a, b| compare_rows(plan, keys, *a as usize, *b as usize));
    }
    rows
}

// ---- events ----

/// What happened in the table. Every event carries the next
/// [`DataTableView`]; the kinds below say what ELSE the app does.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DataTableEventKind {
    /// The selection or the cell cursor changed: store the view.
    Select,
    /// The table scrolled (`top` / `left_column` moved): store the view.
    Scroll,
    /// The sort changed (`index` = the clicked column): store the view; the
    /// rows' new order follows with `OrderReady` (at once for a small table:
    /// then this view has it already).
    Sort,
    /// A filter changed (`index` = its column): as `Sort`.
    Filter,
    /// The rows' new order is in the view (`view.order`): store it.
    OrderReady,
    /// An edit started (`cell`; `view.edit_text` holds the cell's text, or
    /// the typed character that replaces it).
    EditStart,
    /// The edit's text or caret changed.
    EditText,
    /// The edit of `cell` was kept: `text` is the new text. The app's
    /// `on_edit` accepted it already (when there is one).
    EditCommit,
    /// The edit was cancelled (Escape).
    EditCancel,
    /// The app's `on_edit` refused `text` for `cell`: the edit stays open;
    /// `text` is the app's reason.
    EditRefused,
    /// Column `index` was resized to `size` px (the view keeps the width).
    ResizeColumn,
    /// Enter or a double-click on a cell that is not editable: open the
    /// record (`cell`).
    Activate,
    /// Ctrl+C: the selected rows are on the clipboard already (`text` is
    /// the tab-separated text).
    Copy,
    /// A drag started, moved or ended without anything else to do: store
    /// the view (its `drag`).
    Drag,
}

/// One action in the table.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct DataTableEvent {
    /// The view after the action: store it.
    pub view: DataTableView,
    /// `EditCommit`: the kept text; `EditRefused`: the reason; `Copy`: the
    /// copied text.
    pub text: AzString,
    /// `EditStart` / `EditCommit` / `EditRefused` / `Activate`: the cell
    /// (the app's row).
    pub cell: DataTableCellRef,
    /// `ResizeColumn`: the new width in px.
    pub size: f32,
    /// `Sort` / `Filter` / `ResizeColumn`: the column.
    pub index: u32,
    /// What happened.
    pub kind: DataTableEventKind,
    /// Shift was held.
    pub shift: bool,
    /// The primary modifier was held: Cmd on macOS, Ctrl elsewhere.
    pub ctrl: bool,
}

impl DataTableEvent {
    /// A `kind` event leaving `view`, nothing else set.
    #[must_use]
    pub const fn create(kind: DataTableEventKind, view: DataTableView) -> Self {
        Self {
            view,
            text: AzString::from_const_str(""),
            cell: DataTableCellRef { row: 0, column: 0 },
            size: 0.0,
            index: 0,
            kind,
            shift: false,
            ctrl: false,
        }
    }
}

/// An edit the app is asked to accept: the typed `text` for `cell`.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataTableEdit {
    /// What the user typed.
    pub text: AzString,
    /// The cell (the app's row).
    pub cell: DataTableCellRef,
}

/// The app's answer to an edit.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataTableEditResult {
    /// Why the edit is refused (shown to the user; empty when accepted).
    pub message: AzString,
    /// The app stored the value.
    pub accepted: bool,
}

impl DataTableEditResult {
    /// The edit is taken.
    #[must_use]
    pub const fn create_accepted() -> Self {
        Self {
            message: AzString::from_const_str(""),
            accepted: true,
        }
    }

    /// The edit is refused, for `message`.
    #[must_use]
    pub const fn create_refused(message: AzString) -> Self {
        Self {
            message,
            accepted: false,
        }
    }
}

impl azul_core::host_invoker::HostOut for DataTableEditResult {
    fn unwritten() -> Self {
        Self::create_refused(AzString::from_const_str(""))
    }
}

// ---- callbacks ----

/// Callback invoked for an action in the table.
pub type DataTableOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, DataTableEvent) -> Update;
impl_widget_callback!(
    DataTableOnEvent,
    OptionDataTableOnEvent,
    DataTableOnEventCallback,
    DataTableOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        DataTableOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: DATA_TABLE_ON_EVENT_INVOKER,
    invoker_ty:     AzDataTableOnEventCallbackInvoker,
    thunk_fn:       az_data_table_on_event_callback_thunk,
    setter_fn:      AzApp_setDataTableOnEventCallbackInvoker,
    from_handle_fn: AzDataTableOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzDataTableOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: DataTableEvent ],
}

/// The EDIT callback: the app validates (and stores) an edit.
pub type DataTableOnEditCallbackType =
    extern "C" fn(RefAny, CallbackInfo, DataTableEdit) -> DataTableEditResult;
impl_widget_callback!(
    DataTableOnEdit,
    OptionDataTableOnEdit,
    DataTableOnEditCallback,
    DataTableOnEditCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        DataTableOnEditCallback,
    info_ty:        CallbackInfo,
    return_ty:      DataTableEditResult,
    default_ret:    DataTableEditResult::create_refused(AzString::from_const_str("The edit could not be checked.")),
    invoker_static: DATA_TABLE_ON_EDIT_INVOKER,
    invoker_ty:     AzDataTableOnEditCallbackInvoker,
    thunk_fn:       az_data_table_on_edit_callback_thunk,
    setter_fn:      AzApp_setDataTableOnEditCallbackInvoker,
    from_handle_fn: AzDataTableOnEditCallback_createFromHostHandle,
    from_handle_byref_fn: AzDataTableOnEditCallback_createFromHostHandleByref,
    extra_args:     [ edit: DataTableEdit ],
}

/// The DATA callback: the content of one cell (the app's row).
pub type DataTableDataSourceCallbackType = extern "C" fn(RefAny, DataTableCellRef) -> DataTableCell;
impl_widget_callback!(
    DataTableDataSource,
    OptionDataTableDataSource,
    DataTableDataSourceCallback,
    DataTableDataSourceCallbackType
);

// Host-invoker plumbing: the cell carries no context, so the thunk reads it
// from the invocation slot.
azul_core::impl_managed_callback! {
    wrapper:        DataTableDataSourceCallback,
    ctx_field:      ctx,
    data:           data: RefAny,
    args:           [cell: DataTableCellRef],
    return_ty:      DataTableCell,
    default_ret:    DataTableCell::empty(),
    invoker_static: DATA_TABLE_DATA_SOURCE_INVOKER,
    invoker_ty:     AzDataTableDataSourceCallbackInvoker,
    thunk_fn:       az_data_table_data_source_callback_thunk,
    setter_fn:      AzApp_setDataTableDataSourceCallbackInvoker,
    from_handle_fn: AzDataTableDataSourceCallback_createFromHostHandle,
    from_handle_byref_fn: AzDataTableDataSourceCallback_createFromHostHandleByref,
}

/// The content of `at`, from the data callback.
pub(crate) fn cell_content(source: &OptionDataTableDataSource, at: DataTableCellRef) -> DataTableCell {
    match source.as_ref() {
        Some(DataTableDataSource { refany, callback }) => callback.invoke(refany.clone(), at),
        None => DataTableCell::empty(),
    }
}

// ---- the widget ----

/// The data table. See the module documentation.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct DataTable {
    /// The app-owned state: sort, filters, order, selection, scroll, edit.
    pub view: DataTableView,
    /// The columns.
    pub columns: DataTableColumnVec,
    /// The `id` of the table's node (default "data-table"): what an app or
    /// a script focuses it by, and how the table finds itself again when a
    /// sort finishes - two tables in one window need two ids.
    pub id: AzString,
    /// What a screen reader calls the table ("Orders").
    pub accessibility_name: AzString,
    /// Where the cells come from; none = an empty table.
    pub data_source: OptionDataTableDataSource,
    /// Hears every action.
    pub on_event: OptionDataTableOnEvent,
    /// Validates every edit; none = every edit is kept (the app still hears
    /// `EditCommit`).
    pub on_edit: OptionDataTableOnEdit,
    /// The px the table fills (header, filter row and scroll bars
    /// included): how many rows and columns it builds.
    pub viewport_width: f32,
    /// See `viewport_width`.
    pub viewport_height: f32,
    /// A row's height in px.
    pub row_height: f32,
    /// The header's height in px.
    pub header_height: f32,
    /// The cells' font size in px.
    pub font_size: f32,
    /// How many rows the app has.
    pub row_count: u32,
    /// The columns frozen at the left (always shown, never scrolled).
    pub frozen_columns: u32,
    /// The widget theme this table is PINNED to (`with_theme`), or `None`
    /// to follow the app theme.
    pub theme: crate::widgets::themes::OptionUiTheme,
    /// Show the filter row under the header (default on).
    pub show_filter_row: bool,
    /// A table that is looked at, not edited: no edits (sorting, filtering,
    /// selecting, resizing and copying still work).
    pub read_only: bool,
}

impl Default for DataTable {
    fn default() -> Self {
        Self::create(DataTableColumnVec::from_const_slice(&[]), 0)
    }
}

impl DataTable {
    /// A table of `columns` over `row_count` rows: 26 px rows, a filter row,
    /// no data until [`Self::with_data_source`].
    #[must_use]
    pub fn create(columns: DataTableColumnVec, row_count: u32) -> Self {
        Self {
            view: DataTableView::create(),
            columns,
            id: AzString::from_const_str("data-table"),
            accessibility_name: AzString::from_const_str("Table"),
            data_source: None.into(),
            on_event: None.into(),
            on_edit: None.into(),
            viewport_width: 1200.0,
            viewport_height: 800.0,
            row_height: 26.0,
            header_height: 30.0,
            font_size: 13.0,
            row_count,
            frozen_columns: 0,
            theme: None.into(),
            show_filter_row: true,
            read_only: false,
        }
    }

    /// The state the app keeps (store every event's `view`).
    pub fn set_view(&mut self, view: DataTableView) {
        self.view = view;
    }

    /// [`Self::set_view`] for the builder chain.
    #[must_use]
    pub fn with_view(mut self, view: DataTableView) -> Self {
        self.set_view(view);
        self
    }

    /// The node's `id` (unique in the window).
    pub fn set_id(&mut self, id: AzString) {
        self.id = id;
    }

    /// [`Self::set_id`] for the builder chain.
    #[must_use]
    pub fn with_id(mut self, id: AzString) -> Self {
        self.set_id(id);
        self
    }

    /// What a screen reader calls the table.
    pub fn set_accessibility_name(&mut self, name: AzString) {
        self.accessibility_name = name;
    }

    /// [`Self::set_accessibility_name`] for the builder chain.
    #[must_use]
    pub fn with_accessibility_name(mut self, name: AzString) -> Self {
        self.set_accessibility_name(name);
        self
    }

    /// How many rows the app has.
    pub fn set_row_count(&mut self, row_count: u32) {
        self.row_count = row_count;
    }

    /// [`Self::set_row_count`] for the builder chain.
    #[must_use]
    pub fn with_row_count(mut self, row_count: u32) -> Self {
        self.set_row_count(row_count);
        self
    }

    /// Where the cells come from: `callback(data, cell)` for every cell in
    /// view, and for the keys of a sort or a filter.
    pub fn set_data_source<C: Into<DataTableDataSourceCallback>>(&mut self, data: RefAny, callback: C) {
        self.data_source = Some(DataTableDataSource {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// [`Self::set_data_source`] for the builder chain.
    #[must_use]
    pub fn with_data_source<C: Into<DataTableDataSourceCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_data_source(data, callback);
        self
    }

    /// The callback that hears every action.
    pub fn set_on_event<C: Into<DataTableOnEventCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_event = Some(DataTableOnEvent {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<DataTableOnEventCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_on_event(data, callback);
        self
    }

    /// The callback that validates (and stores) every edit.
    pub fn set_on_edit<C: Into<DataTableOnEditCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_edit = Some(DataTableOnEdit {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// [`Self::set_on_edit`] for the builder chain.
    #[must_use]
    pub fn with_on_edit<C: Into<DataTableOnEditCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_on_edit(data, callback);
        self
    }

    /// The px the table fills.
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport_width = width;
        self.viewport_height = height;
    }

    /// [`Self::set_viewport`] for the builder chain.
    #[must_use]
    pub fn with_viewport(mut self, width: f32, height: f32) -> Self {
        self.set_viewport(width, height);
        self
    }

    /// A row's height in px.
    pub fn set_row_height(&mut self, px: f32) {
        self.row_height = px;
    }

    /// [`Self::set_row_height`] for the builder chain.
    #[must_use]
    pub fn with_row_height(mut self, px: f32) -> Self {
        self.set_row_height(px);
        self
    }

    /// The cells' font size in px.
    pub fn set_font_size(&mut self, px: f32) {
        self.font_size = px;
    }

    /// [`Self::set_font_size`] for the builder chain.
    #[must_use]
    pub fn with_font_size(mut self, px: f32) -> Self {
        self.set_font_size(px);
        self
    }

    /// The columns frozen at the left.
    pub fn set_frozen_columns(&mut self, columns: u32) {
        self.frozen_columns = columns;
    }

    /// [`Self::set_frozen_columns`] for the builder chain.
    #[must_use]
    pub fn with_frozen_columns(mut self, columns: u32) -> Self {
        self.set_frozen_columns(columns);
        self
    }

    /// Shows or hides the filter row.
    pub fn set_show_filter_row(&mut self, show: bool) {
        self.show_filter_row = show;
    }

    /// [`Self::set_show_filter_row`] for the builder chain.
    #[must_use]
    pub fn with_show_filter_row(mut self, show: bool) -> Self {
        self.set_show_filter_row(show);
        self
    }

    /// Makes the table read-only (or editable again).
    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    /// [`Self::set_read_only`] for the builder chain.
    #[must_use]
    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.set_read_only(read_only);
        self
    }

    /// Pins the widget theme; unset, the table follows the app theme.
    pub fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = Some(theme).into();
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty table and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    //! A small table with data, for the widget's own tests and the lint
    //! manifest (`widgets::label_convention::every_widget_dom`).
    use super::*;

    /// The names column: case variants and a blank, so the sort shows its
    /// folding and its blanks.
    pub(crate) const NAMES: [&str; 7] = ["Delta", "alpha", "Charlie", "bravo", "Echo", "", "Alpha"];
    /// The cities column.
    pub(crate) const CITIES: [&str; 3] = ["Berlin", "Paris", "Rome"];
    /// 2024-01-01, days since 1970-01-01.
    pub(crate) const NEW_YEAR_2024: i64 = 19_723;

    /// Row `row`'s amount: a quarter step from 0 to 25, blank on every
    /// eleventh row from the fifth.
    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn amount(row: u32) -> f64 {
        if row % 11 == 5 {
            f64::NAN
        } else {
            f64::from((row * 37) % 101) / 4.0
        }
    }

    /// Row `row`'s day (days since 1970-01-01): somewhere in 2024.
    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn day(row: u32) -> f64 {
        (NEW_YEAR_2024 + i64::from((row * 13) % 366)) as f64
    }

    /// Name, Amount, Date, City, Code.
    pub(crate) extern "C" fn cells(_: RefAny, at: DataTableCellRef) -> DataTableCell {
        let row = at.row;
        match at.column {
            0 => DataTableCell::create_text(AzString::from(NAMES[(row % 7) as usize])),
            1 => {
                let v = amount(row);
                if v.is_nan() {
                    DataTableCell::empty()
                } else {
                    DataTableCell::create(AzString::from(alloc::format!("{v:.2}")), v)
                }
            }
            2 => DataTableCell::create(AzString::from(alloc::format!("{}", day(row))), day(row)),
            3 => DataTableCell::create_text(AzString::from(CITIES[(row % 3) as usize])),
            4 => DataTableCell::create_text(AzString::from(alloc::format!("C{row:04}"))),
            _ => DataTableCell::empty(),
        }
    }

    /// The five columns (Amount and Date editable).
    pub(crate) fn columns() -> DataTableColumnVec {
        DataTableColumnVec::from_vec(alloc::vec![
            DataTableColumn::create(AzString::from_const_str("Name"), 120.0, DataTableSortKind::Text),
            DataTableColumn::create(AzString::from_const_str("Amount"), 90.0, DataTableSortKind::Number)
                .with_editable(true),
            DataTableColumn::create(AzString::from_const_str("Date"), 100.0, DataTableSortKind::Date)
                .with_editable(true),
            DataTableColumn::create(AzString::from_const_str("City"), 100.0, DataTableSortKind::Text),
            DataTableColumn::create(AzString::from_const_str("Code"), 80.0, DataTableSortKind::Text),
        ])
    }

    /// A 1000-row table, 400 x 300 px, the columns above.
    pub(crate) fn small() -> DataTable {
        DataTable::create(columns(), 1000)
            .with_viewport(400.0, 300.0)
            .with_data_source(RefAny::new(()), cells as DataTableDataSourceCallbackType)
            .with_accessibility_name(AzString::from_const_str("Orders"))
    }
}

// ---- geometry: which rows and columns are in view, and where ----

use crate::widgets::cell_grid::{axis_bands, band_at, size_at, Band, FREEZE_LINE_PX, RESIZE_GRIP_PX};

/// The vertical scroll bar's width and the horizontal one's height, px.
pub(crate) const SCROLLBAR_PX: f32 = 12.0;
/// The shortest thumb a scroll bar draws, px.
pub(crate) const MIN_THUMB_PX: f32 = 24.0;
/// A column's width when nothing says otherwise, px.
pub(crate) const DEFAULT_COLUMN_PX: f32 = 100.0;
/// The narrowest a drag leaves a column, px.
pub(crate) const MIN_COLUMN_PX: f32 = 24.0;
/// A cell's left and right padding, px.
pub(crate) const CELL_PADDING_PX: f32 = 6.0;

/// One scroll bar: its track (px from the table's top-left) and its thumb
/// along it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScrollBar {
    /// x, y, width, height.
    pub track: (f32, f32, f32, f32),
    /// The thumb's start, px from the track's start.
    pub thumb_start: f32,
    /// The thumb's length, px.
    pub thumb_len: f32,
}

impl ScrollBar {
    /// Whether `(x, y)` lies on the track.
    pub(crate) fn contains(&self, x: f32, y: f32) -> bool {
        let (tx, ty, tw, th) = self.track;
        x >= tx && x < tx + tw && y >= ty && y < ty + th
    }
}

/// Where everything the table shows sits.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Geometry {
    /// The frozen columns, then the scrolled columns in view; `index` is
    /// the column, `start` its x.
    pub columns: Vec<Band>,
    /// How many of `columns` are frozen (the freeze line follows them).
    pub frozen_columns: usize,
    /// The rows in view; `index` is the POSITION among the rows shown,
    /// `start` its y.
    pub rows: Vec<Band>,
    /// The header row's height.
    pub header_height: f32,
    /// The filter row's height (0 without it).
    pub filter_height: f32,
    /// Where the rows start (under the header and the filter row).
    pub body_top: f32,
    /// Where the rows end (over the horizontal scroll bar, if any).
    pub body_bottom: f32,
    /// The width the columns get (left of the vertical scroll bar, if any).
    pub body_width: f32,
    /// The frozen columns' width, the freeze line included.
    pub frozen_width: f32,
    /// The rows that fit wholly (Page Up / Down move by that many).
    pub page_rows: u32,
    /// The scrolled columns that fit wholly.
    pub page_columns: u32,
    /// How many rows are shown (all of them, not just the ones in view).
    pub shown: u32,
    /// The first row in view (`view.top`, kept in range).
    pub top: u32,
    /// The last `top` there is (the last page).
    pub max_top: u32,
    /// The first scrolled column (`view.left_column`, kept in range).
    pub left: u32,
    /// The last `left` there is.
    pub max_left: u32,
    /// The vertical scroll bar, when not every row fits.
    pub vbar: Option<ScrollBar>,
    /// The horizontal scroll bar, when not every column fits.
    pub hbar: Option<ScrollBar>,
}

/// Every column's width (px): its own, the user's resizing over it, a
/// resize drag in progress over that.
pub(crate) fn column_sizes(t: &DataTable) -> Vec<CellGridSize> {
    let mut sizes: Vec<CellGridSize> = t
        .columns
        .as_slice()
        .iter()
        .enumerate()
        .map(|(i, c)| CellGridSize::create(u32::try_from(i).unwrap_or(u32::MAX), c.width))
        .collect();
    sizes.extend(t.view.widths.as_slice().iter().copied());
    if t.view.drag.kind == DataTableDragKind::ResizeColumn {
        sizes.push(CellGridSize::create(t.view.drag.column, t.view.drag.size));
    }
    sizes
}

/// A scroll bar's thumb over a track of `len` px: `page` of `total`
/// items shown from `at` (of `max` + 1 starting places).
#[allow(clippy::cast_precision_loss)] // positions are far below 2^24 per px
fn thumb(len: f32, page: f32, total: f32, at: u32, max: u32) -> (f32, f32) {
    let size = if total > 0.0 {
        (len * (page / total).min(1.0)).max(MIN_THUMB_PX.min(len))
    } else {
        len
    };
    let start = if max == 0 {
        0.0
    } else {
        (len - size) * (at.min(max) as f32 / max as f32)
    };
    (start, size)
}

/// The geometry of `t` as it is built now.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
pub(crate) fn geometry(t: &DataTable) -> Geometry {
    let width = t.viewport_width.max(0.0);
    let height = t.viewport_height.max(0.0);
    let header_height = t.header_height.max(0.0);
    let row_h = t.row_height.max(1.0);
    let filter_height = if t.show_filter_row { row_h } else { 0.0 };
    let body_top = header_height + filter_height;
    let shown = t.view.shown_count(t.row_count);
    let sizes = column_sizes(t);
    let ncols = u32::try_from(t.columns.len()).unwrap_or(u32::MAX);
    let total_width: f32 = (0..ncols).map(|i| size_at(&sizes, i, DEFAULT_COLUMN_PX)).sum();

    // The bars take room from each other: decide the vertical one, then the
    // horizontal one, then the vertical one again in what is left.
    let fits = |bottom: f32| ((bottom - body_top).max(0.0) / row_h).floor() as u32;
    let mut has_vbar = shown > fits(height);
    let has_hbar = total_width > width - if has_vbar { SCROLLBAR_PX } else { 0.0 };
    if has_hbar && !has_vbar {
        has_vbar = shown > fits(height - SCROLLBAR_PX);
    }
    let body_width = (width - if has_vbar { SCROLLBAR_PX } else { 0.0 }).max(0.0);
    let body_bottom = (height - if has_hbar { SCROLLBAR_PX } else { 0.0 }).max(body_top);

    // Rows: whole rows from `top`, the last page never scrolled past.
    let page_rows = fits(body_bottom).max(1);
    let max_top = shown.saturating_sub(page_rows);
    let top = t.view.top.min(max_top);
    let (rows, _, _) = axis_bands(shown, 0, top, &[], row_h, 1.0, body_top, body_bottom);

    // Columns: the frozen ones, then whole columns from `left`; the last
    // `left` is the first column of the widest tail that still fits.
    let frozen = t.frozen_columns.min(ncols);
    let frozen_width: f32 = (0..frozen).map(|i| size_at(&sizes, i, DEFAULT_COLUMN_PX)).sum::<f32>()
        + if frozen > 0 { FREEZE_LINE_PX } else { 0.0 };
    let room = (body_width - frozen_width).max(0.0);
    let mut first_of_tail = ncols;
    let mut tail = 0.0;
    while first_of_tail > frozen {
        let w = size_at(&sizes, first_of_tail - 1, DEFAULT_COLUMN_PX);
        if tail + w > room {
            break;
        }
        tail += w;
        first_of_tail -= 1;
    }
    let max_left = first_of_tail.max(frozen).min(ncols.saturating_sub(1).max(frozen));
    let left = t.view.left_column.max(frozen).min(max_left);
    let (columns, frozen_columns, page_columns) =
        axis_bands(ncols, frozen, left, &sizes, DEFAULT_COLUMN_PX, 1.0, 0.0, body_width);

    let vbar = has_vbar.then(|| {
        let len = body_bottom - body_top;
        let (thumb_start, thumb_len) = thumb(len, page_rows as f32, shown as f32, top, max_top);
        ScrollBar {
            track: (body_width, body_top, SCROLLBAR_PX, len),
            thumb_start,
            thumb_len,
        }
    });
    let hbar = has_hbar.then(|| {
        let len = (body_width - frozen_width).max(0.0);
        let scrolled = (total_width - frozen_width).max(1.0);
        let (thumb_start, thumb_len) = thumb(len, room, scrolled, left - frozen, max_left - frozen);
        ScrollBar {
            track: (frozen_width, body_bottom, len, SCROLLBAR_PX),
            thumb_start,
            thumb_len,
        }
    });

    Geometry {
        columns,
        frozen_columns,
        rows,
        header_height,
        filter_height,
        body_top,
        body_bottom,
        body_width,
        frozen_width,
        page_rows,
        page_columns,
        shown,
        top,
        max_top,
        left,
        max_left,
        vbar,
        hbar,
    }
}

/// What a point of the table is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    /// A column's header: sort by it.
    Header(u32),
    /// The right edge of a column's header: resize it.
    HeaderEdge(u32),
    /// A column's filter.
    Filter(u32),
    /// A cell: its POSITION among the rows shown, its column.
    Cell(u32, u32),
    /// The vertical track, before (`false`) or after (`true`) the thumb.
    RowsTrack(bool),
    /// The vertical thumb.
    RowsThumb,
    /// The horizontal track, before or after the thumb.
    ColumnsTrack(bool),
    /// The horizontal thumb.
    ColumnsThumb,
    /// Nothing (past the last row or column, the corner of the bars).
    Nothing,
}

/// What is at `(x, y)` - px relative to the table's top-left corner.
pub(crate) fn hit_test(geo: &Geometry, x: f32, y: f32) -> Hit {
    if let Some(v) = geo.vbar.filter(|v| v.contains(x, y)) {
        let along = y - v.track.1;
        return if along < v.thumb_start {
            Hit::RowsTrack(false)
        } else if along > v.thumb_start + v.thumb_len {
            Hit::RowsTrack(true)
        } else {
            Hit::RowsThumb
        };
    }
    if let Some(h) = geo.hbar.filter(|h| h.contains(x, y)) {
        let along = x - h.track.0;
        return if along < h.thumb_start {
            Hit::ColumnsTrack(false)
        } else if along > h.thumb_start + h.thumb_len {
            Hit::ColumnsTrack(true)
        } else {
            Hit::ColumnsThumb
        };
    }
    if x < 0.0 || y < 0.0 || x >= geo.body_width || y >= geo.body_bottom {
        return Hit::Nothing;
    }
    if y < geo.header_height {
        return match band_at(&geo.columns, x) {
            Some(b) if b.end() - x <= RESIZE_GRIP_PX && x <= b.end() => Hit::HeaderEdge(b.index),
            // The grip reaches a little into the NEXT column too.
            Some(b) if x - b.start <= RESIZE_GRIP_PX / 2.0 => {
                match geo.columns.iter().rev().find(|c| c.end() <= b.start + 0.5) {
                    Some(prev) => Hit::HeaderEdge(prev.index),
                    None => Hit::Header(b.index),
                }
            }
            Some(b) => Hit::Header(b.index),
            None => Hit::Nothing,
        };
    }
    if y < geo.body_top {
        return band_at(&geo.columns, x).map_or(Hit::Nothing, |b| Hit::Filter(b.index));
    }
    match (band_at(&geo.rows, y), band_at(&geo.columns, x)) {
        (Some(r), Some(c)) => Hit::Cell(r.index, c.index),
        _ => Hit::Nothing,
    }
}

/// The rectangle `(x, y, w, h)` of the cell at `position` x `column` in
/// view; `None` when it is not.
pub(crate) fn cell_rect(geo: &Geometry, position: u32, column: u32) -> Option<(f32, f32, f32, f32)> {
    let r = geo.rows.iter().find(|b| b.index == position)?;
    let c = geo.columns.iter().find(|b| b.index == column)?;
    Some((c.start, r.start, c.size, r.size))
}

/// The rectangle of `column`'s filter in view.
pub(crate) fn filter_rect(geo: &Geometry, column: u32) -> Option<(f32, f32, f32, f32)> {
    let c = geo.columns.iter().find(|b| b.index == column)?;
    Some((c.start, geo.header_height, c.size, geo.filter_height))
}

// ---- the build: the rows in view, the header, the filter row, the overlays ----

use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{length::FloatValue, StyleFontSize},
        layout::{
            LayoutAlignItems, LayoutBoxSizing, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutFlexShrink, LayoutJustifyContent, LayoutMinHeight, LayoutMinWidth, LayoutOverflow,
            LayoutPaddingLeft, LayoutPaddingRight, LayoutPosition,
        },
        property::CssProperty,
        style::{StyleCursor, StyleUserSelect, StyleWhiteSpace},
    },
};

/// The table's class; the table node also carries the app's `id`.
pub(crate) const TABLE_CLASS_NAME: &str = "__azul-native-data-table";
/// A row (the header row and the filter row too).
pub(crate) const ROW_CLASS_NAME: &str = "__azul-native-data-table-row";
/// A column's header.
pub(crate) const HEADER_CLASS_NAME: &str = "__azul-native-data-table-header";
/// A column's filter.
pub(crate) const FILTER_CLASS_NAME: &str = "__azul-native-data-table-filter";
/// A cell.
pub(crate) const CELL_CLASS_NAME: &str = "__azul-native-data-table-cell";
/// The line after the frozen columns.
pub(crate) const FREEZE_CLASS_NAME: &str = "__azul-native-data-table-freeze";
/// The outline of the cursor's cell.
pub(crate) const CURSOR_CLASS_NAME: &str = "__azul-native-data-table-cursor";
/// The in-cell (or in-filter) editor.
pub(crate) const EDITOR_CLASS_NAME: &str = "__azul-native-data-table-editor";
/// The editor's caret.
pub(crate) const CARET_CLASS_NAME: &str = "__azul-native-data-table-caret";
/// A scroll bar's track.
pub(crate) const TRACK_CLASS_NAME: &str = "__azul-native-data-table-track";
/// A scroll bar's thumb.
pub(crate) const THUMB_CLASS_NAME: &str = "__azul-native-data-table-thumb";
/// "Sorting ..." / "No rows ..." over the rows.
pub(crate) const NOTICE_CLASS_NAME: &str = "__azul-native-data-table-notice";

static TABLE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(TABLE_CLASS_NAME))];
static ROW_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(ROW_CLASS_NAME))];
static HEADER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(HEADER_CLASS_NAME))];
static FILTER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(FILTER_CLASS_NAME))];
static CELL_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(CELL_CLASS_NAME))];
static FREEZE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(FREEZE_CLASS_NAME))];
static CURSOR_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(CURSOR_CLASS_NAME))];
static EDITOR_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(EDITOR_CLASS_NAME))];
static CARET_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(CARET_CLASS_NAME))];
static TRACK_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(TRACK_CLASS_NAME))];
static THUMB_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(THUMB_CLASS_NAME))];
static NOTICE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(NOTICE_CLASS_NAME))];

/// What a theme decides about a table: the SKIN of each part, laid over
/// the part's base (the structure, the same in every theme) by [`build`].
pub(crate) struct DataTableLook {
    /// The table: the face, the ink, the paper.
    pub table: Vec<CssPropertyWithConditions>,
    /// A column header: the strip, the strong ink, the hairlines.
    pub header: Vec<CssPropertyWithConditions>,
    /// Added to the header of a column the rows are sorted by.
    pub header_sorted: Vec<CssPropertyWithConditions>,
    /// A filter: a field's face, the hairlines.
    pub filter: Vec<CssPropertyWithConditions>,
    /// Added to an empty filter (its "Filter" placeholder's ink).
    pub filter_empty: Vec<CssPropertyWithConditions>,
    /// A cell's right hairline.
    pub cell: Vec<CssPropertyWithConditions>,
    /// A row's bottom hairline.
    pub row: Vec<CssPropertyWithConditions>,
    /// Added to every other row (the zebra band).
    pub row_alternate: Vec<CssPropertyWithConditions>,
    /// Added to a selected row.
    pub row_selected: Vec<CssPropertyWithConditions>,
    /// The outline of the cursor's cell.
    pub cursor: Vec<CssPropertyWithConditions>,
    /// The editor.
    pub editor: Vec<CssPropertyWithConditions>,
    /// The editor's caret.
    pub caret: Vec<CssPropertyWithConditions>,
    /// The line after the frozen columns.
    pub freeze_line: Vec<CssPropertyWithConditions>,
    /// A scroll bar's track.
    pub track: Vec<CssPropertyWithConditions>,
    /// A scroll bar's thumb.
    pub thumb: Vec<CssPropertyWithConditions>,
    /// "Sorting ..." / "No rows ...".
    pub notice: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the table, if it has one.
    pub marker: Option<&'static str>,
}

/// The table: a column of rows that takes its pane, clips what does not
/// fit, is the containing block of the overlays, and is ONE focus stop
/// whose text a drag never selects.
pub(crate) static DATA_TABLE_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    simple(CssProperty::const_position(LayoutPosition::Relative)),
    simple(CssProperty::const_cursor(StyleCursor::Default)),
    simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// A row: the cells side by side, never shrinking, its hairline inside its
/// height.
pub(crate) static DATA_TABLE_ROW_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
];

/// A cell (a header, a filter): its width, its content centred on the
/// row's line and set by its alignment, clipped.
pub(crate) static DATA_TABLE_CELL_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(6))),
    simple(CssProperty::const_padding_right(LayoutPaddingRight::const_px(6))),
];

/// A header: a cell that is clicked.
pub(crate) static DATA_TABLE_HEADER_BASE: &[CssPropertyWithConditions] =
    &[simple(CssProperty::const_cursor(StyleCursor::Pointer))];

/// A filter: a cell that is typed into.
pub(crate) static DATA_TABLE_FILTER_BASE: &[CssPropertyWithConditions] =
    &[simple(CssProperty::const_cursor(StyleCursor::Text))];

/// The freeze line between the frozen and the scrolled columns.
pub(crate) static DATA_TABLE_FREEZE_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// An overlay (the cursor, a track, a notice): placed by px.
pub(crate) static DATA_TABLE_OVERLAY_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_position(LayoutPosition::Absolute)),
    simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
];

/// The editor: the text and the caret on one line.
pub(crate) static DATA_TABLE_EDITOR_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_position(LayoutPosition::Absolute)),
    simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    simple(CssProperty::const_cursor(StyleCursor::Text)),
];

/// A cell's text: one line.
pub(crate) static DATA_TABLE_TEXT_BASE: &[CssPropertyWithConditions] = &[simple(
    CssProperty::WhiteSpace(CssPropertyValue::Exact(StyleWhiteSpace::Pre)),
)];

/// One row in view: the app's row (none past the end of a stale order)
/// and the cells of the columns in view.
#[derive(Debug, Clone)]
pub(crate) struct ResolvedRow {
    pub row: Option<u32>,
    pub cells: Vec<DataTableCell>,
}

/// The table with its window laid out and the cells in view asked for
/// ONCE (both looks are built from it when the table follows the app
/// theme).
#[derive(Debug, Clone)]
pub(crate) struct DataTableResolved {
    /// The table, its view's `top` / `left_column` kept in range.
    pub table: DataTable,
    /// Where the rows and columns in view sit.
    pub geo: Geometry,
    /// `rows[i]` for `geo.rows[i]`.
    pub rows: Vec<ResolvedRow>,
}

/// Lays the table out and asks the data callback for the cells in view.
pub(crate) fn resolve(mut table: DataTable) -> DataTableResolved {
    let geo = geometry(&table);
    table.view.top = geo.top;
    table.view.left_column = geo.left;
    let rows = geo
        .rows
        .iter()
        .map(|b| {
            let row = table.view.row_at(b.index, table.row_count).into_option();
            let cells = geo
                .columns
                .iter()
                .map(|c| match row {
                    Some(r) => cell_content(&table.data_source, DataTableCellRef::create(r, c.index)),
                    None => DataTableCell::empty(),
                })
                .collect();
            ResolvedRow { row, cells }
        })
        .collect();
    DataTableResolved { table, geo, rows }
}

impl DataTable {
    /// The table's DOM. The data callback is asked ONCE for the cells in
    /// view; the look comes from the theme module
    /// (`themes::flat::data_table` / `themes::flora::data_table`), `None`
    /// carrying both looks.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::UiTheme;
        let theme = self.theme.into_option();
        let resolved = resolve(self);
        match theme {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::data_table(resolved),
            Some(UiTheme::Flat) => crate::widgets::themes::flat::data_table(resolved),
            None => crate::widgets::themes::theme_blocks::follow_app_theme(
                resolved,
                crate::widgets::themes::flat::data_table,
                crate::widgets::themes::flora::data_table,
            ),
        }
    }
}

impl From<DataTable> for Dom {
    fn from(t: DataTable) -> Self {
        t.dom()
    }
}

/// How a cell of `column` lines up its content.
fn justify(column: Option<&DataTableColumn>) -> LayoutJustifyContent {
    let Some(c) = column else {
        return LayoutJustifyContent::Start;
    };
    match c.align {
        CellGridHorizontalAlign::Left => LayoutJustifyContent::Start,
        CellGridHorizontalAlign::Center => LayoutJustifyContent::Center,
        CellGridHorizontalAlign::Right => LayoutJustifyContent::End,
        CellGridHorizontalAlign::General => match c.sort_kind {
            DataTableSortKind::Text => LayoutJustifyContent::Start,
            DataTableSortKind::Number | DataTableSortKind::Date => LayoutJustifyContent::End,
        },
    }
}

/// `left` / `top` / `width` / `height` of an overlay.
fn place(x: f32, y: f32, w: f32, h: f32) -> [CssPropertyWithConditions; 4] {
    [px_left(x), px_top(y), px_width(w.max(0.0)), px_height(h.max(0.0))]
}

/// A line of text the table wrote (one line, never selected).
fn text_line(text: AzString) -> Dom {
    crate::widgets::widget_p_with_text(text)
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(DATA_TABLE_TEXT_BASE))
}

/// "12,345" - a count with thousands separators.
pub(crate) fn grouped(n: u32) -> String {
    let digits = alloc::format!("{n}");
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// The header's label: the title, and for a sorted column its arrow (and
/// its place among several keys).
fn header_label(title: &str, sort: Option<(usize, DataTableSortDirection)>, keys: usize) -> String {
    match sort {
        None => String::from(title),
        Some((i, direction)) => {
            let arrow = match direction {
                DataTableSortDirection::Ascending => '\u{25B2}',
                DataTableSortDirection::Descending => '\u{25BC}',
            };
            if keys > 1 {
                alloc::format!("{title} {arrow}{}", i + 1)
            } else {
                alloc::format!("{title} {arrow}")
            }
        }
    }
}

/// What a screen reader hears the table say where the cursor is.
fn cursor_value(t: &DataTable, shown: u32) -> String {
    let column = t
        .columns
        .get(t.view.active_column as usize)
        .map_or("", |c| c.title.as_str());
    let position = t
        .view
        .cursor_row()
        .into_option()
        .and_then(|r| t.view.position_of(r, t.row_count).into_option());
    match position {
        Some(p) => alloc::format!("Row {} of {}, {column}", grouped(p + 1), grouped(shown)),
        None => alloc::format!("{} rows", grouped(shown)),
    }
}

/// The table's DOM in `look`: table [header row, filter row?, rows ..,
/// cursor?, editor?, notice?, scroll bars?].
#[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
pub(crate) fn build(resolved: DataTableResolved, look: &DataTableLook) -> Dom {
    use azul_core::a11y::{AccessibilityInfo, AccessibilityRole, AccessibilityState, AccessibilityStateVec};

    let part = |base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]| {
        super::themes::decl::on_base(base, skin)
    };
    let DataTableResolved { table, geo, rows } = resolved;
    let view = &table.view;
    let columns = table.columns.as_slice();
    let keys = view.sort.len();
    let any_frozen = geo.frozen_columns > 0;
    let cursor = view.cursor_row().into_option();

    let freeze = |height: f32| -> Dom {
        let mut p = part(DATA_TABLE_FREEZE_BASE, &look.freeze_line);
        p.push(px_width(FREEZE_LINE_PX));
        p.push(px_height(height));
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(FREEZE_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(p))
    };
    let row_node = |height: f32, skin: Vec<CssPropertyWithConditions>, a11y: AccessibilityInfo, cells: Vec<Dom>| {
        let mut p = part(DATA_TABLE_ROW_BASE, &skin);
        p.push(px_height(height));
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(ROW_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(p))
            .with_accessibility_info(a11y)
            .with_children(DomVec::from_vec(cells))
    };

    let mut children: Vec<Dom> = Vec::with_capacity(geo.rows.len() + 8);

    // The header row: the titles, the sort state.
    let mut header_cells: Vec<Dom> = Vec::with_capacity(geo.columns.len() + 1);
    for (i, c) in geo.columns.iter().enumerate() {
        if i == geo.frozen_columns && any_frozen {
            header_cells.push(freeze(geo.header_height));
        }
        let column = columns.get(c.index as usize);
        let title = column.map_or("", |x| x.title.as_str());
        let sort = view.sort_of(c.index);
        let mut p = part(DATA_TABLE_CELL_BASE, DATA_TABLE_HEADER_BASE);
        p.extend(look.header.iter().cloned());
        if sort.is_some() {
            p.extend(look.header_sorted.iter().cloned());
        }
        p.push(px_width(c.size));
        p.push(simple(CssProperty::const_justify_content(justify(column))));
        let states = match sort {
            Some((_, DataTableSortDirection::Ascending)) => alloc::vec![AccessibilityState::SortedAscending],
            Some((_, DataTableSortDirection::Descending)) => alloc::vec![AccessibilityState::SortedDescending],
            None => Vec::new(),
        };
        header_cells.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(HEADER_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(p))
                .with_accessibility_info(AccessibilityInfo {
                    column_index: azul_css::corety::OptionUsize::Some(c.index as usize + 1),
                    states: AccessibilityStateVec::from_vec(states),
                    ..AccessibilityInfo::named(AzString::from(title), AccessibilityRole::ColumnHeader)
                })
                .with_child(text_line(AzString::from(header_label(title, sort, keys)))),
        );
    }
    if geo.frozen_columns == geo.columns.len() && any_frozen {
        header_cells.push(freeze(geo.header_height));
    }
    children.push(row_node(
        geo.header_height,
        Vec::new(),
        AccessibilityInfo {
            role: AccessibilityRole::Row,
            ..Default::default()
        },
        header_cells,
    ));

    // The filter row: what each column is filtered by, or a placeholder.
    if table.show_filter_row {
        let mut filter_cells: Vec<Dom> = Vec::with_capacity(geo.columns.len() + 1);
        for (i, c) in geo.columns.iter().enumerate() {
            if i == geo.frozen_columns && any_frozen {
                filter_cells.push(freeze(geo.filter_height));
            }
            let column = columns.get(c.index as usize);
            let title = column.map_or("", |x| x.title.as_str());
            let filterable = column.is_some_and(|x| x.filterable);
            let editing_here = view.edit == DataTableEditTarget::Filter && view.edit_column == c.index;
            let text = view.filter_text(c.index);
            let empty = text.as_str().is_empty();
            let mut p = part(DATA_TABLE_CELL_BASE, DATA_TABLE_FILTER_BASE);
            p.extend(look.filter.iter().cloned());
            if empty {
                p.extend(look.filter_empty.iter().cloned());
            }
            p.push(px_width(c.size));
            let shown_text = if editing_here || !filterable {
                AzString::from_const_str("")
            } else if empty {
                AzString::from_const_str("Filter")
            } else {
                text.clone()
            };
            let value = if empty { None } else { Some(text) };
            let mut node = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(FILTER_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(p))
                .with_accessibility_info(AccessibilityInfo {
                    column_index: azul_css::corety::OptionUsize::Some(c.index as usize + 1),
                    accessibility_value: value.into(),
                    ..AccessibilityInfo::named(
                        AzString::from(alloc::format!("Filter {title}")),
                        AccessibilityRole::Text,
                    )
                });
            if !shown_text.as_str().is_empty() {
                node = node.with_child(text_line(shown_text));
            }
            filter_cells.push(node);
        }
        if geo.frozen_columns == geo.columns.len() && any_frozen {
            filter_cells.push(freeze(geo.filter_height));
        }
        children.push(row_node(
            geo.filter_height,
            Vec::new(),
            AccessibilityInfo {
                role: AccessibilityRole::Row,
                ..Default::default()
            },
            filter_cells,
        ));
    }

    // The rows in view.
    let mut cursor_rect = None;
    for (band, resolved_row) in geo.rows.iter().zip(rows) {
        let ResolvedRow { row, cells } = resolved_row;
        let selected = row.is_some_and(|r| view.selection.contains(u64::from(r)));
        let mut skin = look.row.clone();
        if band.index % 2 == 1 {
            skin.extend(look.row_alternate.iter().cloned());
        }
        if selected {
            skin.extend(look.row_selected.iter().cloned());
        }
        let mut row_cells: Vec<Dom> = Vec::with_capacity(geo.columns.len() + 1);
        for (i, (c, cell)) in geo.columns.iter().zip(cells).enumerate() {
            if i == geo.frozen_columns && any_frozen {
                row_cells.push(freeze(band.size));
            }
            let column = columns.get(c.index as usize);
            let title = column.map_or("", |x| x.title.as_str());
            let editing_here = view.edit == DataTableEditTarget::Cell
                && row == Some(view.edit_row)
                && view.edit_column == c.index;
            if row.is_some() && row == cursor && c.index == view.active_column {
                cursor_rect = Some((c.start, band.start, c.size, band.size));
            }
            let mut p = part(DATA_TABLE_CELL_BASE, &look.cell);
            p.push(px_width(c.size));
            p.push(simple(CssProperty::const_justify_content(justify(column))));
            let text = if editing_here {
                AzString::from_const_str("")
            } else {
                cell.text
            };
            let value = if text.as_str().is_empty() {
                None
            } else {
                Some(text.clone())
            };
            let mut node = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(CELL_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(p))
                .with_accessibility_info(AccessibilityInfo {
                    row_index: azul_css::corety::OptionUsize::Some(band.index as usize + 1),
                    column_index: azul_css::corety::OptionUsize::Some(c.index as usize + 1),
                    accessibility_value: value.into(),
                    ..AccessibilityInfo::named(AzString::from(title), AccessibilityRole::GridCell)
                });
            if !text.as_str().is_empty() {
                node = node.with_child(text_line(text));
            }
            row_cells.push(node);
        }
        if geo.frozen_columns == geo.columns.len() && any_frozen {
            row_cells.push(freeze(band.size));
        }
        let states = if selected {
            alloc::vec![AccessibilityState::Selected]
        } else {
            Vec::new()
        };
        children.push(row_node(
            band.size,
            skin,
            AccessibilityInfo {
                role: AccessibilityRole::Row,
                row_index: azul_css::corety::OptionUsize::Some(band.index as usize + 1),
                states: AccessibilityStateVec::from_vec(states),
                ..Default::default()
            },
            row_cells,
        ));
    }

    // The overlays: the cursor's outline, the editor, the notice, the bars.
    if let (Some((x, y, w, h)), false) = (cursor_rect, view.is_editing()) {
        let mut p = part(DATA_TABLE_OVERLAY_BASE, &look.cursor);
        p.extend(place(x, y, w, h));
        children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(CURSOR_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(p)),
        );
    }
    let editor_rect = match view.edit {
        DataTableEditTarget::None => None,
        DataTableEditTarget::Filter => filter_rect(&geo, view.edit_column),
        DataTableEditTarget::Cell => view
            .position_of(view.edit_row, table.row_count)
            .into_option()
            .and_then(|pos| cell_rect(&geo, pos, view.edit_column)),
    };
    if let Some((x, y, w, h)) = editor_rect {
        let chars: Vec<char> = view.edit_text.as_str().chars().collect();
        let caret = (view.edit_cursor as usize).min(chars.len());
        let before: String = chars[..caret].iter().collect();
        let after: String = chars[caret..].iter().collect();
        let mut p = part(DATA_TABLE_EDITOR_BASE, &look.editor);
        p.extend(place(x, y, w, h));
        let mut caret_props = look.caret.clone();
        caret_props.push(px_width(1.0));
        caret_props.push(px_height((table.font_size + 2.0).max(8.0)));
        children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(EDITOR_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(p))
                .with_children(DomVec::from_vec(alloc::vec![
                    text_line(AzString::from(before)),
                    Dom::create_div()
                        .with_ids_and_classes(IdOrClassVec::from_const_slice(CARET_CLASS))
                        .with_css_props(CssPropertyWithConditionsVec::from_vec(caret_props)),
                    text_line(AzString::from(after)),
                ])),
        );
    }
    let notice = if view.is_sorting() {
        Some(alloc::format!(
            "Sorting {} rows...",
            grouped(table.row_count)
        ))
    } else if geo.shown == 0 {
        Some(String::from(if view.filters.is_empty() {
            "No rows"
        } else {
            "No rows match the filters"
        }))
    } else {
        None
    };
    if let Some(text) = notice {
        let mut p = part(DATA_TABLE_OVERLAY_BASE, &look.notice);
        p.push(px_left(8.0));
        p.push(px_top(geo.body_top + 6.0));
        children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(NOTICE_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(p))
                .with_accessibility_info(AccessibilityInfo {
                    is_live_region: true,
                    ..AccessibilityInfo::named(AzString::from(text.clone()), AccessibilityRole::StaticText)
                })
                .with_child(text_line(AzString::from(text))),
        );
    }
    let bar = |b: &ScrollBar, vertical: bool, name: &'static str| -> Dom {
        let (x, y, w, h) = b.track;
        let mut track = part(DATA_TABLE_OVERLAY_BASE, &look.track);
        track.extend(place(x, y, w, h));
        let mut thumb = part(DATA_TABLE_OVERLAY_BASE, &look.thumb);
        if vertical {
            thumb.extend(place(2.0, b.thumb_start, (w - 4.0).max(1.0), b.thumb_len));
        } else {
            thumb.extend(place(b.thumb_start, 2.0, b.thumb_len, (h - 4.0).max(1.0)));
        }
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(TRACK_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(track))
            .with_accessibility_info(AccessibilityInfo::named(
                AzString::from_const_str(name),
                AccessibilityRole::ScrollBar,
            ))
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(THUMB_CLASS))
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(thumb)),
            )
    };
    if let Some(v) = &geo.vbar {
        children.push(bar(v, true, "Rows"));
    }
    if let Some(h) = &geo.hbar {
        children.push(bar(h, false, "Columns"));
    }

    // The table: one focus stop; its value says where the cursor is.
    let mut classes: Vec<IdOrClass> = TABLE_CLASS.to_vec();
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    let mut table_props = part(DATA_TABLE_BASE, &look.table);
    table_props.push(simple(CssProperty::const_font_size(StyleFontSize::px(table.font_size))));
    let mut states = alloc::vec![AccessibilityState::Multiselectable];
    if view.is_sorting() {
        states.push(AccessibilityState::Busy);
    }
    let a11y = AccessibilityInfo {
        accessibility_value: Some(AzString::from(cursor_value(&table, geo.shown))).into(),
        states: AccessibilityStateVec::from_vec(states),
        ..AccessibilityInfo::named(table.accessibility_name.clone(), AccessibilityRole::Grid)
    };
    let id = table.id.clone();
    let shared = RefAny::new(TableShared { table, geo });
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_id(id.clone())
        .with_marker(azul_css::OptionString::Some(id))
        .with_dataset(azul_core::refany::OptionRefAny::Some(shared.clone()))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(table_props))
        .with_tab_index(azul_core::dom::TabIndex::Auto)
        .with_accessibility_info(a11y)
        .with_callbacks(table_callbacks(&shared).into())
        .with_children(DomVec::from_vec(children))
}
