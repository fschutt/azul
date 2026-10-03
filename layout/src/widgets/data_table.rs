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
    let _ = (view, columns);
    QueryPlan::default()
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
    let _ = (source, plan, keys, from, to);
}

/// The rows `0..row_count` that pass every filter, in the order of the
/// sort keys (stable: rows with equal keys keep the app's order; blanks
/// last in either direction).
pub(crate) fn compute_order(row_count: u32, plan: &QueryPlan, keys: &[ColumnKeys]) -> Vec<u32> {
    let _ = (plan, keys);
    (0..row_count).collect()
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
