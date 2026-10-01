//! Cell grid widget - the surface of a spreadsheet: a grid of cells under a
//! header row of column letters (A, B, C ...) and beside a header column of
//! row numbers (1, 2, 3 ...), with a cell cursor, ranges, in-cell editing, a
//! fill handle and resizable columns and rows. Excel's grid.
//!
//! GENERIC OVER ITS DATA: the grid holds no cells. It asks the app for the
//! ones it shows through a DATA callback ([`CellGrid::with_data_source`]:
//! the text and the kind of one cell) and a STYLE callback
//! ([`CellGrid::with_style_source`]: fill, ink, borders, bold / italic /
//! underline / strike, alignment, wrap, font size). A spreadsheet backs them
//! with its engine, a contacts or records app with its rows.
//!
//! VIRTUALISED BOTH WAYS, IN WHOLE CELLS: the grid shows the rows from
//! [`CellGridView::top_row`] and the columns from
//! [`CellGridView::left_column`] until its viewport
//! ([`CellGrid::with_viewport`]) is full - only those cells are in the DOM.
//! It scrolls by whole rows and columns, as Excel does: the wheel, the
//! keyboard and the scroll bars move `top_row` / `left_column`, never a
//! pixel offset. So a sheet of 1,048,576 rows needs no 26-million-pixel
//! scroll extent (an `f32` cannot address it to the pixel), and the frozen
//! panes and the headers stay put by construction: the frozen rows and
//! columns are always drawn first, then a freeze line, then the scrolled
//! window.
//!
//! THE APP OWNS THE STATE: the selection, the scroll position, the edit and
//! a drag in progress are the [`CellGridView`] the app hands in; every
//! action reports a [`CellGridEvent`] whose `view` is the NEXT view (the
//! widget computed it: the arrow moved the cursor, the wheel scrolled, the
//! typed character started an edit), plus what the app must do (commit the
//! edit, fill, resize, paste). The app stores `event.view` and rebuilds.
//!
//! KEYBOARD (the grid is ONE Tab stop): arrows move the cell cursor (Shift
//! extends the range, Ctrl / Cmd jumps to the edge of the data, both
//! together extend to it), PageUp / PageDown by a screen, Home to column A
//! (Ctrl+Home to A1, Ctrl+End to the last cell with data), Ctrl+A selects
//! everything, Ctrl+Space a column, Shift+Space a row, Enter / Tab move down
//! / right (Shift backwards), F2 edits the cell, Delete clears it, a
//! character starts an edit that REPLACES the cell (Excel's "enter" mode:
//! an arrow then commits and moves), Escape cancels an edit. Ctrl+C / X / V
//! copy (tab-separated text and an HTML table), cut and paste.
//!
//! POINTER: the grid hit-tests itself (one set of handlers on the grid, none
//! per cell): a press selects a cell (Shift extends, Ctrl adds a range), a
//! drag selects a range, a press on a header selects the column or row, on
//! the corner everything; the edge of a header resizes the column or row
//! (a double-click there auto-fits); the fill handle at the corner of the
//! selection fills; a double-click edits.
//!
//! Key types: [`CellGrid`], [`CellGridView`], [`CellGridEvent`],
//! [`CellGridCell`], [`CellGridCellStyle`].

use alloc::{string::String, vec::Vec};

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec},
    events::FocusEventFilter,
    refany::{OptionRefAny, RefAny},
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option, impl_option_inner, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    impl_vec_partialeq,
    props::basic::color::{ColorU, OptionColorU},
    AzString,
};

use crate::callbacks::{Callback, CallbackInfo};

// ---- the types the app sees ----

/// One cell of the grid: its row and its column, both 0-based (row 0 is the
/// row the header calls "1", column 0 the one it calls "A").
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct CellGridCellRef {
    /// The row, 0-based.
    pub row: u32,
    /// The column, 0-based.
    pub column: u32,
}

impl CellGridCellRef {
    /// The cell at `row`, `column` (0-based).
    #[must_use]
    pub const fn create(row: u32, column: u32) -> Self {
        Self { row, column }
    }
}

impl_option!(
    CellGridCellRef,
    OptionCellGridCellRef,
    [Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// A rectangle of cells, both corners included; `first` is the top-left
/// corner, `last` the bottom-right one (see [`CellGridRange::spanning`]).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct CellGridRange {
    /// The top-left cell.
    pub first: CellGridCellRef,
    /// The bottom-right cell.
    pub last: CellGridCellRef,
}

impl CellGridRange {
    /// The one cell `cell`.
    #[must_use]
    pub const fn create(cell: CellGridCellRef) -> Self {
        Self {
            first: cell,
            last: cell,
        }
    }

    /// The rectangle between two corners given in any order.
    #[must_use]
    pub fn spanning(a: CellGridCellRef, b: CellGridCellRef) -> Self {
        Self {
            first: CellGridCellRef::create(a.row.min(b.row), a.column.min(b.column)),
            last: CellGridCellRef::create(a.row.max(b.row), a.column.max(b.column)),
        }
    }

    /// Whether `cell` lies inside.
    #[must_use]
    pub const fn contains(&self, cell: CellGridCellRef) -> bool {
        cell.row >= self.first.row
            && cell.row <= self.last.row
            && cell.column >= self.first.column
            && cell.column <= self.last.column
    }

    /// The number of rows.
    #[must_use]
    pub const fn row_count(&self) -> u32 {
        self.last.row - self.first.row + 1
    }

    /// The number of columns.
    #[must_use]
    pub const fn column_count(&self) -> u32 {
        self.last.column - self.first.column + 1
    }
}

impl_option!(
    CellGridRange,
    OptionCellGridRange,
    [Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash]
);
impl_vec!(
    CellGridRange,
    CellGridRangeVec,
    CellGridRangeVecDestructor,
    CellGridRangeVecDestructorType,
    CellGridRangeVecSlice,
    OptionCellGridRange
);
impl_vec_clone!(CellGridRange, CellGridRangeVec, CellGridRangeVecDestructor);
impl_vec_debug!(CellGridRange, CellGridRangeVec);
impl_vec_mut!(CellGridRange, CellGridRangeVec);
impl_vec_partialeq!(CellGridRange, CellGridRangeVec);

/// What a pointer drag over the grid is doing.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum CellGridDragKind {
    /// No drag.
    #[default]
    None,
    /// Selecting a range from the pressed cell.
    Select,
    /// Dragging the fill handle.
    Fill,
    /// Dragging the right edge of column `index`'s header.
    ResizeColumn,
    /// Dragging the bottom edge of row `index`'s header.
    ResizeRow,
}

/// A pointer drag in progress. The app keeps it in its [`CellGridView`]
/// between rebuilds, like the rest of the view.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct CellGridDrag {
    /// Resize: the pointer's position along the drag axis when it was
    /// pressed, in window px.
    pub start_px: f32,
    /// Resize: the column's width or the row's height when it was pressed,
    /// in px at 100 % zoom.
    pub start_size: f32,
    /// Resize: the column or row; 0 otherwise.
    pub index: u32,
    /// Select / Fill: the cell the drag started in.
    pub origin: CellGridCellRef,
    /// Select / Fill: the cell under the pointer now.
    pub target: CellGridCellRef,
    /// What the drag does.
    pub kind: CellGridDragKind,
}

/// Whether the active cell is being edited, and how.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum CellGridEditMode {
    /// Not editing: the keys move the cell cursor.
    #[default]
    None,
    /// Typing REPLACED the cell (Excel's "Enter" mode): an arrow key
    /// commits the edit and moves.
    Enter,
    /// F2 or a double-click opened the cell's content (Excel's "Edit"
    /// mode): Left / Right / Home / End move the caret.
    Edit,
}

/// The state of a grid the APP keeps: the selection, the scroll position,
/// the edit and a drag in progress. Every [`CellGridEvent`] carries the next
/// one; store it and rebuild.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct CellGridView {
    /// The selected ranges, the CURRENT one (the one holding `active`)
    /// last. Ctrl + click adds a range.
    pub ranges: CellGridRangeVec,
    /// The edit's text (while `edit_mode` is not `None`).
    pub edit_text: AzString,
    /// The cell cursor.
    pub active: CellGridCellRef,
    /// The corner a Shift-extension grows the current range from.
    pub anchor: CellGridCellRef,
    /// The first SCROLLED row shown below the frozen rows (never a frozen
    /// row: it is at least `frozen_rows`).
    pub top_row: u32,
    /// The first scrolled column shown right of the frozen columns.
    pub left_column: u32,
    /// The caret in `edit_text`, in characters.
    pub edit_cursor: u32,
    /// A pointer drag in progress.
    pub drag: CellGridDrag,
    /// Whether the active cell is being edited.
    pub edit_mode: CellGridEditMode,
}

impl CellGridView {
    /// A view with the cursor on A1, scrolled to the top-left, nothing
    /// edited.
    #[must_use]
    pub fn create() -> Self {
        let a1 = CellGridCellRef::create(0, 0);
        Self {
            ranges: CellGridRangeVec::from_vec(alloc::vec![CellGridRange::create(a1)]),
            edit_text: AzString::from_const_str(""),
            active: a1,
            anchor: a1,
            top_row: 0,
            left_column: 0,
            edit_cursor: 0,
            drag: CellGridDrag::default(),
            edit_mode: CellGridEditMode::None,
        }
    }

    /// The cursor on `cell`, the selection that one cell.
    #[must_use]
    pub fn with_active(mut self, cell: CellGridCellRef) -> Self {
        self.active = cell;
        self.anchor = cell;
        self.ranges = CellGridRangeVec::from_vec(alloc::vec![CellGridRange::create(cell)]);
        self
    }

    /// Scrolled so `top_row` / `left_column` are the first scrolled row
    /// and column.
    #[must_use]
    pub const fn with_scroll(mut self, top_row: u32, left_column: u32) -> Self {
        self.top_row = top_row;
        self.left_column = left_column;
        self
    }

    /// Whether `cell` is in any selected range.
    #[must_use]
    pub fn is_selected(&self, cell: CellGridCellRef) -> bool {
        self.ranges.as_ref().iter().any(|r| r.contains(cell))
    }

    /// The current range (the last one), or the active cell alone.
    #[must_use]
    pub fn current_range(&self) -> CellGridRange {
        self.ranges
            .as_ref()
            .last()
            .copied()
            .unwrap_or_else(|| CellGridRange::create(self.active))
    }

    /// Whether the active cell is being edited.
    #[must_use]
    pub const fn is_editing(&self) -> bool {
        !matches!(self.edit_mode, CellGridEditMode::None)
    }
}

impl Default for CellGridView {
    fn default() -> Self {
        Self::create()
    }
}

/// What a cell holds - decides the "general" alignment (numbers right,
/// text left, booleans and errors centred) and what a data-edge jump
/// (Ctrl + arrow) stops at.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum CellGridCellKind {
    /// Nothing.
    #[default]
    Empty,
    /// Text.
    Text,
    /// A number (also a date, a time, a percentage).
    Number,
    /// TRUE / FALSE.
    Boolean,
    /// An error value (#DIV/0!, #NAME? ...).
    Error,
}

/// A cell's horizontal alignment.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum CellGridHorizontalAlign {
    /// By the cell's kind: numbers right, text left, booleans and errors
    /// centred.
    #[default]
    General,
    /// Left.
    Left,
    /// Centred.
    Center,
    /// Right.
    Right,
}

/// A cell's vertical alignment.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum CellGridVerticalAlign {
    /// At the bottom (the spreadsheet default).
    #[default]
    Bottom,
    /// In the middle.
    Center,
    /// At the top.
    Top,
}

/// How one cell looks - what the STYLE callback answers. The default is a
/// plain cell in the grid's own ink on the grid's paper.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CellGridCellStyle {
    /// The font size in px at 100 % zoom; 0 = the grid's.
    pub font_size: f32,
    /// The horizontal alignment.
    pub align: CellGridHorizontalAlign,
    /// The vertical alignment.
    pub vertical_align: CellGridVerticalAlign,
    /// The background; `None` = the grid's paper (and the text follows the
    /// mode). With a fill and no `ink`, the text is black or white,
    /// whichever reads on the fill.
    pub fill: OptionColorU,
    /// The text colour; `None` = automatic.
    pub ink: OptionColorU,
    /// A thin line along the top edge, in this colour.
    pub border_top: OptionColorU,
    /// ... the right edge.
    pub border_right: OptionColorU,
    /// ... the bottom edge.
    pub border_bottom: OptionColorU,
    /// ... the left edge.
    pub border_left: OptionColorU,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Underlined.
    pub underline: bool,
    /// Struck through.
    pub strike: bool,
    /// Long text wraps inside the cell instead of being cut off.
    pub wrap: bool,
}

impl CellGridCellStyle {
    /// A plain cell.
    #[must_use]
    pub fn create() -> Self {
        Self::default()
    }
}

impl azul_core::host_invoker::HostOut for CellGridCellStyle {
    fn unwritten() -> Self {
        Self::default()
    }
}

/// One cell's content - what the DATA callback answers: the text the cell
/// shows (formatted already: "1,250.00", "12 %", "#DIV/0!") and its kind.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CellGridCell {
    /// The text shown.
    pub text: AzString,
    /// What the cell holds.
    pub kind: CellGridCellKind,
}

impl CellGridCell {
    /// A cell showing `text` of kind `kind`.
    #[must_use]
    pub const fn create(text: AzString, kind: CellGridCellKind) -> Self {
        Self { text, kind }
    }

    /// An empty cell.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            text: AzString::from_const_str(""),
            kind: CellGridCellKind::Empty,
        }
    }
}

impl azul_core::host_invoker::HostOut for CellGridCell {
    fn unwritten() -> Self {
        Self::empty()
    }
}

/// A column's width or a row's height that differs from the grid's
/// default, in px at 100 % zoom; 0 hides the column or row.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct CellGridSize {
    /// The size in px.
    pub size: f32,
    /// The column or row, 0-based.
    pub index: u32,
}

impl CellGridSize {
    /// Column / row `index` is `size` px.
    #[must_use]
    pub const fn create(index: u32, size: f32) -> Self {
        Self { size, index }
    }
}

impl_option!(
    CellGridSize,
    OptionCellGridSize,
    [Debug, Clone, Copy, PartialEq, PartialOrd]
);
impl_vec!(
    CellGridSize,
    CellGridSizeVec,
    CellGridSizeVecDestructor,
    CellGridSizeVecDestructorType,
    CellGridSizeVecSlice,
    OptionCellGridSize
);
impl_vec_clone!(CellGridSize, CellGridSizeVec, CellGridSizeVecDestructor);
impl_vec_debug!(CellGridSize, CellGridSizeVec);
impl_vec_mut!(CellGridSize, CellGridSizeVec);
impl_vec_partialeq!(CellGridSize, CellGridSizeVec);

/// What happened in the grid. Every event carries the next
/// [`CellGridView`]; the kinds below say what ELSE the app does.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CellGridEventKind {
    /// The selection or the cell cursor changed (a click, a drag, a key):
    /// store the view.
    Select,
    /// The grid scrolled (`top_row` / `left_column` moved): store the view,
    /// and load the cells now in view if the app pages them in.
    Scroll,
    /// An edit started: `view.edit_text` is its text - the typed character
    /// (Enter mode) or EMPTY for F2 / double-click (Edit mode), where the
    /// app puts the cell's own content (its formula) into `edit_text`.
    EditStart,
    /// The edit's text or caret changed.
    EditText,
    /// The edit was committed: `text` is the final text for the cell
    /// `range.first` (the cell edited); the view's cursor has already moved
    /// on.
    EditCommit,
    /// The edit was cancelled (Escape).
    EditCancel,
    /// The fill handle was released: fill from the current selection over
    /// `range` (the source plus the dragged extension).
    Fill,
    /// Column `index` was resized to `size` px (at 100 % zoom).
    ResizeColumn,
    /// Row `index` was resized to `size` px.
    ResizeRow,
    /// The edge of column `index`'s header was double-clicked: fit it to
    /// its content.
    AutoFitColumn,
    /// Ctrl+C: the grid already put the selection on the clipboard (text
    /// and an HTML table) from the data callback.
    Copy,
    /// Ctrl+X: like `Copy`; the app clears `range` when it pastes.
    Cut,
    /// Ctrl+V: paste `text` (the clipboard's plain text, tab-separated) at
    /// `range.first`.
    Paste,
    /// Delete: clear the selected cells (`view.ranges`).
    Delete,
    /// A drag over the grid started, moved or ended without anything else
    /// to do: store the view (its `drag`).
    Drag,
}

/// One action in the grid.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct CellGridEvent {
    /// The view after the action: store it.
    pub view: CellGridView,
    /// `EditCommit`: the committed text; `Paste`: the clipboard text.
    pub text: AzString,
    /// `EditCommit` / `Paste`: the target cell in `first`; `Fill` / `Cut`:
    /// the range.
    pub range: CellGridRange,
    /// `ResizeColumn` / `ResizeRow`: the new size in px.
    pub size: f32,
    /// `ResizeColumn` / `ResizeRow` / `AutoFitColumn`: the column or row.
    pub index: u32,
    /// What happened.
    pub kind: CellGridEventKind,
    /// Shift was held.
    pub shift: bool,
    /// Ctrl (or Cmd) was held.
    pub ctrl: bool,
}

impl CellGridEvent {
    /// A `kind` event leaving `view`, nothing else set.
    #[must_use]
    pub fn create(kind: CellGridEventKind, view: CellGridView) -> Self {
        Self {
            view,
            text: AzString::from_const_str(""),
            range: CellGridRange::default(),
            size: 0.0,
            index: 0,
            kind,
            shift: false,
            ctrl: false,
        }
    }
}

// ---- callbacks ----

/// Callback invoked for an action in the grid.
pub type CellGridOnEventCallbackType =
    extern "C" fn(RefAny, CallbackInfo, CellGridEvent) -> Update;
impl_widget_callback!(
    CellGridOnEvent,
    OptionCellGridOnEvent,
    CellGridOnEventCallback,
    CellGridOnEventCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        CellGridOnEventCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: CELL_GRID_ON_EVENT_INVOKER,
    invoker_ty:     AzCellGridOnEventCallbackInvoker,
    thunk_fn:       az_cell_grid_on_event_callback_thunk,
    setter_fn:      AzApp_setCellGridOnEventCallbackInvoker,
    from_handle_fn: AzCellGridOnEventCallback_createFromHostHandle,
    from_handle_byref_fn: AzCellGridOnEventCallback_createFromHostHandleByref,
    extra_args:     [ event: CellGridEvent ],
}

/// The DATA callback: the content of one cell.
pub type CellGridDataSourceCallbackType = extern "C" fn(RefAny, CellGridCellRef) -> CellGridCell;
impl_widget_callback!(
    CellGridDataSource,
    OptionCellGridDataSource,
    CellGridDataSourceCallback,
    CellGridDataSourceCallbackType
);

// Host-invoker plumbing: the cell carries no context, so the thunk reads it
// from the invocation slot.
azul_core::impl_managed_callback! {
    wrapper:        CellGridDataSourceCallback,
    ctx_field:      ctx,
    data:           data: RefAny,
    args:           [cell: CellGridCellRef],
    return_ty:      CellGridCell,
    default_ret:    CellGridCell::empty(),
    invoker_static: CELL_GRID_DATA_SOURCE_INVOKER,
    invoker_ty:     AzCellGridDataSourceCallbackInvoker,
    thunk_fn:       az_cell_grid_data_source_callback_thunk,
    setter_fn:      AzApp_setCellGridDataSourceCallbackInvoker,
    from_handle_fn: AzCellGridDataSourceCallback_createFromHostHandle,
    from_handle_byref_fn: AzCellGridDataSourceCallback_createFromHostHandleByref,
}

/// The STYLE callback: how one cell looks.
pub type CellGridStyleSourceCallbackType =
    extern "C" fn(RefAny, CellGridCellRef) -> CellGridCellStyle;
impl_widget_callback!(
    CellGridStyleSource,
    OptionCellGridStyleSource,
    CellGridStyleSourceCallback,
    CellGridStyleSourceCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        CellGridStyleSourceCallback,
    ctx_field:      ctx,
    data:           data: RefAny,
    args:           [cell: CellGridCellRef],
    return_ty:      CellGridCellStyle,
    default_ret:    CellGridCellStyle::default(),
    invoker_static: CELL_GRID_STYLE_SOURCE_INVOKER,
    invoker_ty:     AzCellGridStyleSourceCallbackInvoker,
    thunk_fn:       az_cell_grid_style_source_callback_thunk,
    setter_fn:      AzApp_setCellGridStyleSourceCallbackInvoker,
    from_handle_fn: AzCellGridStyleSourceCallback_createFromHostHandle,
    from_handle_byref_fn: AzCellGridStyleSourceCallback_createFromHostHandleByref,
}

// ---- the widget ----

/// The spreadsheet grid. See the module documentation.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct CellGrid {
    /// The app-owned state: selection, scroll position, edit, drag.
    pub view: CellGridView,
    /// The `id` attribute of the grid's node (default "cell-grid"): what an
    /// app or a script focuses it by.
    pub id: AzString,
    /// What a screen reader calls the grid ("Sheet1").
    pub accessibility_name: AzString,
    /// Columns whose width differs from `default_column_width`.
    pub column_widths: CellGridSizeVec,
    /// Rows whose height differs from `default_row_height`.
    pub row_heights: CellGridSizeVec,
    /// Where the cells' content comes from; none = an empty grid.
    pub data_source: OptionCellGridDataSource,
    /// Where the cells' looks come from; none = every cell plain.
    pub style_source: OptionCellGridStyleSource,
    /// Hears every action.
    pub on_event: OptionCellGridOnEvent,
    /// The px the grid fills (headers included): how many rows and columns
    /// it renders. Hand in the space the grid gets; a little too much only
    /// renders a few cells that are clipped.
    pub viewport_width: f32,
    /// See `viewport_width`.
    pub viewport_height: f32,
    /// A column's width in px when `column_widths` names no other.
    pub default_column_width: f32,
    /// A row's height in px when `row_heights` names no other.
    pub default_row_height: f32,
    /// The width of the row-number column, in px.
    pub header_width: f32,
    /// The height of the column-letter row, in px.
    pub header_height: f32,
    /// The scale of everything (1.0 = 100 %).
    pub zoom: f32,
    /// The cells' font size in px at 100 % zoom.
    pub font_size: f32,
    /// The grid's rows (Excel: 1,048,576).
    pub row_count: u32,
    /// The grid's columns (Excel: 16,384).
    pub column_count: u32,
    /// The rows that hold data (the data's extent): Ctrl + arrow, Ctrl+End
    /// and a copy of whole columns look no further.
    pub content_rows: u32,
    /// The columns that hold data.
    pub content_columns: u32,
    /// The rows frozen at the top (always shown above the scrolled ones).
    pub frozen_rows: u32,
    /// The columns frozen at the left.
    pub frozen_columns: u32,
    /// The widget theme this grid is PINNED to (`with_theme`), or `None` to
    /// follow the app theme.
    pub theme: crate::widgets::themes::OptionUiTheme,
    /// Show the column letters and the row numbers.
    pub show_headers: bool,
    /// Draw the grid lines between the cells.
    pub show_grid_lines: bool,
    /// Show the fill handle at the selection's corner.
    pub fill_handle: bool,
    /// A grid that is looked at, not edited: no edits, no fill, no paste,
    /// no delete (selection, copy and resizing still work).
    pub read_only: bool,
}

/// Excel's sheet size: 1,048,576 rows.
pub const CELL_GRID_MAX_ROWS: u32 = 1_048_576;
/// Excel's sheet size: 16,384 columns (A .. XFD).
pub const CELL_GRID_MAX_COLUMNS: u32 = 16_384;

impl Default for CellGrid {
    fn default() -> Self {
        Self::create(CELL_GRID_MAX_ROWS, CELL_GRID_MAX_COLUMNS)
    }
}

impl CellGrid {
    /// A grid of `row_count` x `column_count` cells with Excel's metrics
    /// (64 px columns, 20 px rows), the cursor on A1, no data.
    #[must_use]
    pub fn create(row_count: u32, column_count: u32) -> Self {
        Self {
            view: CellGridView::create(),
            id: AzString::from_const_str("cell-grid"),
            accessibility_name: AzString::from_const_str("Grid"),
            column_widths: CellGridSizeVec::from_const_slice(&[]),
            row_heights: CellGridSizeVec::from_const_slice(&[]),
            data_source: None.into(),
            style_source: None.into(),
            on_event: None.into(),
            viewport_width: 1200.0,
            viewport_height: 800.0,
            default_column_width: 64.0,
            default_row_height: 20.0,
            header_width: 40.0,
            header_height: 20.0,
            zoom: 1.0,
            font_size: 13.0,
            row_count,
            column_count,
            content_rows: 0,
            content_columns: 0,
            frozen_rows: 0,
            frozen_columns: 0,
            theme: crate::widgets::themes::OptionUiTheme::None,
            show_headers: true,
            show_grid_lines: true,
            fill_handle: true,
            read_only: false,
        }
    }

    /// The column letters of column `index` (0-based): "A" .. "Z", "AA",
    /// .. "XFD" - what the header shows and an A1 reference spells.
    #[must_use]
    pub fn column_label(index: u32) -> AzString {
        AzString::from(column_letters(index))
    }

    /// The A1 name of a cell ("B7").
    #[must_use]
    pub fn cell_label(cell: CellGridCellRef) -> AzString {
        AzString::from(alloc::format!(
            "{}{}",
            column_letters(cell.column),
            u64::from(cell.row) + 1
        ))
    }

    /// The cell an A1 reference names ("B7", "$B$7", "b7"), if it is one.
    #[must_use]
    pub fn parse_cell_label(label: AzString) -> OptionCellGridCellRef {
        parse_a1(label.as_str()).into()
    }

    /// Sets the view (selection, scroll, edit, drag).
    pub fn set_view(&mut self, view: CellGridView) {
        self.view = view;
    }

    /// [`Self::set_view`] for the builder chain.
    #[must_use]
    pub fn with_view(mut self, view: CellGridView) -> Self {
        self.set_view(view);
        self
    }

    /// Sets the grid node's `id` attribute.
    pub fn set_id(&mut self, id: AzString) {
        self.id = id;
    }

    /// [`Self::set_id`] for the builder chain.
    #[must_use]
    pub fn with_id(mut self, id: AzString) -> Self {
        self.set_id(id);
        self
    }

    /// Sets what a screen reader calls the grid.
    pub fn set_accessibility_name(&mut self, name: AzString) {
        self.accessibility_name = name;
    }

    /// [`Self::set_accessibility_name`] for the builder chain.
    #[must_use]
    pub fn with_accessibility_name(mut self, name: AzString) -> Self {
        self.set_accessibility_name(name);
        self
    }

    /// Sets the columns whose width differs from the default.
    pub fn set_column_widths(&mut self, widths: CellGridSizeVec) {
        self.column_widths = widths;
    }

    /// [`Self::set_column_widths`] for the builder chain.
    #[must_use]
    pub fn with_column_widths(mut self, widths: CellGridSizeVec) -> Self {
        self.set_column_widths(widths);
        self
    }

    /// Sets the rows whose height differs from the default.
    pub fn set_row_heights(&mut self, heights: CellGridSizeVec) {
        self.row_heights = heights;
    }

    /// [`Self::set_row_heights`] for the builder chain.
    #[must_use]
    pub fn with_row_heights(mut self, heights: CellGridSizeVec) -> Self {
        self.set_row_heights(heights);
        self
    }

    /// Sets where the cells' content comes from.
    pub fn set_data_source<C: Into<CellGridDataSourceCallback>>(&mut self, data: RefAny, cb: C) {
        self.data_source = Some(CellGridDataSource {
            refany: data,
            callback: cb.into(),
        })
        .into();
    }

    /// [`Self::set_data_source`] for the builder chain.
    #[must_use]
    pub fn with_data_source<C: Into<CellGridDataSourceCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_data_source(data, cb);
        self
    }

    /// Sets where the cells' looks come from.
    pub fn set_style_source<C: Into<CellGridStyleSourceCallback>>(&mut self, data: RefAny, cb: C) {
        self.style_source = Some(CellGridStyleSource {
            refany: data,
            callback: cb.into(),
        })
        .into();
    }

    /// [`Self::set_style_source`] for the builder chain.
    #[must_use]
    pub fn with_style_source<C: Into<CellGridStyleSourceCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_style_source(data, cb);
        self
    }

    /// Sets the callback that hears every action.
    pub fn set_on_event<C: Into<CellGridOnEventCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_event = Some(CellGridOnEvent {
            refany: data,
            callback: cb.into(),
        })
        .into();
    }

    /// [`Self::set_on_event`] for the builder chain.
    #[must_use]
    pub fn with_on_event<C: Into<CellGridOnEventCallback>>(mut self, data: RefAny, cb: C) -> Self {
        self.set_on_event(data, cb);
        self
    }

    /// Sets the px the grid fills.
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

    /// Sets the default column width and row height, in px.
    pub fn set_default_sizes(&mut self, column_width: f32, row_height: f32) {
        self.default_column_width = column_width;
        self.default_row_height = row_height;
    }

    /// [`Self::set_default_sizes`] for the builder chain.
    #[must_use]
    pub fn with_default_sizes(mut self, column_width: f32, row_height: f32) -> Self {
        self.set_default_sizes(column_width, row_height);
        self
    }

    /// Sets the row-number column's width and the column-letter row's
    /// height, in px.
    pub fn set_header_sizes(&mut self, width: f32, height: f32) {
        self.header_width = width;
        self.header_height = height;
    }

    /// [`Self::set_header_sizes`] for the builder chain.
    #[must_use]
    pub fn with_header_sizes(mut self, width: f32, height: f32) -> Self {
        self.set_header_sizes(width, height);
        self
    }

    /// Sets the scale (1.0 = 100 %).
    pub fn set_zoom(&mut self, zoom: f32) {
        self.zoom = zoom;
    }

    /// [`Self::set_zoom`] for the builder chain.
    #[must_use]
    pub fn with_zoom(mut self, zoom: f32) -> Self {
        self.set_zoom(zoom);
        self
    }

    /// Sets the cells' font size in px.
    pub fn set_font_size(&mut self, px: f32) {
        self.font_size = px;
    }

    /// [`Self::set_font_size`] for the builder chain.
    #[must_use]
    pub fn with_font_size(mut self, px: f32) -> Self {
        self.set_font_size(px);
        self
    }

    /// Sets the rows and columns that hold data.
    pub fn set_content_extent(&mut self, rows: u32, columns: u32) {
        self.content_rows = rows;
        self.content_columns = columns;
    }

    /// [`Self::set_content_extent`] for the builder chain.
    #[must_use]
    pub fn with_content_extent(mut self, rows: u32, columns: u32) -> Self {
        self.set_content_extent(rows, columns);
        self
    }

    /// Freezes the first `rows` rows and `columns` columns.
    pub fn set_frozen(&mut self, rows: u32, columns: u32) {
        self.frozen_rows = rows;
        self.frozen_columns = columns;
    }

    /// [`Self::set_frozen`] for the builder chain.
    #[must_use]
    pub fn with_frozen(mut self, rows: u32, columns: u32) -> Self {
        self.set_frozen(rows, columns);
        self
    }

    /// Pins the grid to `theme`.
    pub fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = Some(theme).into();
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Shows or hides the column letters and row numbers.
    pub fn set_show_headers(&mut self, show: bool) {
        self.show_headers = show;
    }

    /// [`Self::set_show_headers`] for the builder chain.
    #[must_use]
    pub fn with_show_headers(mut self, show: bool) -> Self {
        self.set_show_headers(show);
        self
    }

    /// Shows or hides the grid lines.
    pub fn set_show_grid_lines(&mut self, show: bool) {
        self.show_grid_lines = show;
    }

    /// [`Self::set_show_grid_lines`] for the builder chain.
    #[must_use]
    pub fn with_show_grid_lines(mut self, show: bool) -> Self {
        self.set_show_grid_lines(show);
        self
    }

    /// Shows or hides the fill handle.
    pub fn set_fill_handle(&mut self, show: bool) {
        self.fill_handle = show;
    }

    /// [`Self::set_fill_handle`] for the builder chain.
    #[must_use]
    pub fn with_fill_handle(mut self, show: bool) -> Self {
        self.set_fill_handle(show);
        self
    }

    /// Makes the grid read-only (or editable again).
    pub fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    /// [`Self::set_read_only`] for the builder chain.
    #[must_use]
    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.set_read_only(read_only);
        self
    }

    /// Replaces `self` with a default grid and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    /// The grid's DOM. The data and style callbacks are asked ONCE for the
    /// cells in view; the look comes from the theme module
    /// (`themes::flat::cell_grid` / `themes::flora::cell_grid`), `None`
    /// carrying both looks, each in its `@theme(<name>)` block.
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::UiTheme;
        let theme = self.theme.into_option();
        let resolved = resolve(self);
        match theme {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::cell_grid(resolved),
            Some(UiTheme::Flat) => crate::widgets::themes::flat::cell_grid(resolved),
            None => crate::widgets::themes::theme_blocks::follow_app_theme(
                resolved,
                crate::widgets::themes::flat::cell_grid,
                crate::widgets::themes::flora::cell_grid,
            ),
        }
    }
}

impl From<CellGrid> for Dom {
    fn from(g: CellGrid) -> Self {
        g.dom()
    }
}
