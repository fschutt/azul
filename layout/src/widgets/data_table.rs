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
