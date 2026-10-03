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
