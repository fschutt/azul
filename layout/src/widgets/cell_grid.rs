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
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec},
    events::FocusEventFilter,
    refany::RefAny,
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    impl_vec_partialeq,
    props::basic::color::{ColorU, OptionColorU},
    AzString,
};

use crate::callbacks::{Callback, CallbackInfo};
use crate::widgets::themes::decl::{
    px_height, px_left, px_min_width, px_top, px_width, simple,
};

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
    /// Pointing at a range while a formula is typed (point mode): the
    /// reference at the caret follows the pointer from `origin`.
    Point,
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
    /// Resize: the size the drag has reached so far (the grid draws the
    /// column or row at it until the release reports it).
    pub size: f32,
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
    /// The primary modifier was held: Cmd on macOS, Ctrl elsewhere.
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

// ---- A1 names ----

/// "A" .. "Z", "AA" .. "XFD" for a 0-based column.
#[allow(clippy::cast_possible_truncation)] // a remainder of 26 fits a u8
pub(crate) fn column_letters(index: u32) -> String {
    let mut n = u64::from(index) + 1;
    let mut out = Vec::new();
    while n > 0 {
        let rem = ((n - 1) % 26) as u8;
        out.push(b'A' + rem);
        n = (n - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// The cell an A1 reference names: letters then digits, `$` signs allowed,
/// case ignored. `None` for anything else (an empty string, "A0", "1A").
#[allow(clippy::cast_possible_truncation)] // both bounded by u32::MAX above
pub(crate) fn parse_a1(label: &str) -> Option<CellGridCellRef> {
    let s = label.trim();
    let mut chars = s.chars().peekable();
    if chars.peek() == Some(&'$') {
        chars.next();
    }
    let mut column: u64 = 0;
    let mut letters = 0;
    while let Some(c) = chars.peek().copied() {
        if c.is_ascii_alphabetic() {
            column = column * 26 + u64::from(c.to_ascii_uppercase() as u8 - b'A' + 1);
            letters += 1;
            chars.next();
            if letters > 3 {
                return None;
            }
        } else {
            break;
        }
    }
    if letters == 0 {
        return None;
    }
    if chars.peek() == Some(&'$') {
        chars.next();
    }
    let digits: String = chars.collect();
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let row: u64 = digits.parse().ok()?;
    if row == 0 || row > u64::from(u32::MAX) || column > u64::from(u32::MAX) {
        return None;
    }
    Some(CellGridCellRef::create((row - 1) as u32, (column - 1) as u32))
}

// ---- geometry: which rows and columns are in view, and where ----

/// The px of the line drawn after the frozen rows / columns.
pub(crate) const FREEZE_LINE_PX: f32 = 2.0;
/// How close to a header's edge the pointer must be to resize, in px.
pub(crate) const RESIZE_GRIP_PX: f32 = 4.0;
/// The fill handle's side, in px.
pub(crate) const FILL_HANDLE_PX: f32 = 7.0;
/// The smallest size a drag resizes a column or row to, in px at 100 %.
pub(crate) const MIN_RESIZE_PX: f32 = 4.0;

/// One row or column in view: which, where it starts (px from the grid's
/// top / left edge, zoom applied) and how big it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Band {
    pub index: u32,
    pub start: f32,
    pub size: f32,
}

impl Band {
    pub(crate) fn end(&self) -> f32 {
        self.start + self.size
    }
}

/// Where everything the grid shows sits.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Geometry {
    /// The frozen rows, then the scrolled rows in view (hidden ones left out).
    pub rows: Vec<Band>,
    /// The frozen columns, then the scrolled columns in view.
    pub columns: Vec<Band>,
    /// How many of `rows` are frozen (the freeze line follows them).
    pub frozen_rows: usize,
    /// How many of `columns` are frozen.
    pub frozen_columns: usize,
    /// The header column's width (0 without headers), zoom applied.
    pub header_width: f32,
    /// The header row's height (0 without headers), zoom applied.
    pub header_height: f32,
    /// The scrolled rows that fit WHOLLY (a PageDown moves by that many).
    pub page_rows: u32,
    /// The scrolled columns that fit wholly.
    pub page_columns: u32,
}

/// The size of column / row `index`: its override, else the default; px at
/// 100 %.
pub(crate) fn size_at(overrides: &[CellGridSize], index: u32, default: f32) -> f32 {
    overrides
        .iter()
        .rev()
        .find(|s| s.index == index)
        .map_or(default, |s| s.size)
        .max(0.0)
}

/// The bands of one axis: the frozen ones `0..frozen`, the freeze line,
/// then the scrolled ones from `first` until `extent` px are covered (the
/// band straddling the edge included). Returns the bands, how many are
/// frozen and how many scrolled bands fit wholly.
#[allow(clippy::too_many_arguments)]
fn axis_bands(
    count: u32,
    frozen: u32,
    first: u32,
    overrides: &[CellGridSize],
    default: f32,
    zoom: f32,
    origin: f32,
    extent: f32,
) -> (Vec<Band>, usize, u32) {
    let mut bands = Vec::new();
    let mut at = origin;
    let frozen = frozen.min(count);
    for index in 0..frozen {
        let size = size_at(overrides, index, default) * zoom;
        if size <= 0.0 {
            continue;
        }
        bands.push(Band {
            index,
            start: at,
            size,
        });
        at += size;
    }
    let frozen_bands = bands.len();
    if frozen > 0 {
        at += FREEZE_LINE_PX;
    }
    let mut whole = 0u32;
    let mut index = first.max(frozen);
    // A guard against a sheet of hidden rows: never walk more than this
    // many hidden bands in a row.
    let mut hidden_run = 0u32;
    while index < count && at < extent {
        let size = size_at(overrides, index, default) * zoom;
        if size <= 0.0 {
            hidden_run += 1;
            if hidden_run > 100_000 {
                break;
            }
            index += 1;
            continue;
        }
        hidden_run = 0;
        bands.push(Band {
            index,
            start: at,
            size,
        });
        at += size;
        if at <= extent {
            whole += 1;
        }
        index += 1;
    }
    (bands, frozen_bands, whole.max(1))
}

/// The geometry of `grid` as it is built now.
pub(crate) fn geometry(grid: &CellGrid) -> Geometry {
    let zoom = if grid.zoom.is_finite() && grid.zoom > 0.0 {
        grid.zoom
    } else {
        1.0
    };
    let (header_width, header_height) = if grid.show_headers {
        (grid.header_width * zoom, grid.header_height * zoom)
    } else {
        (0.0, 0.0)
    };
    let (columns, frozen_columns, page_columns) = axis_bands(
        grid.column_count,
        grid.frozen_columns,
        grid.view.left_column,
        grid.column_widths.as_ref(),
        grid.default_column_width,
        zoom,
        header_width,
        grid.viewport_width.max(0.0),
    );
    let (rows, frozen_rows, page_rows) = axis_bands(
        grid.row_count,
        grid.frozen_rows,
        grid.view.top_row,
        grid.row_heights.as_ref(),
        grid.default_row_height,
        zoom,
        header_height,
        grid.viewport_height.max(0.0),
    );
    Geometry {
        rows,
        columns,
        frozen_rows,
        frozen_columns,
        header_width,
        header_height,
        page_rows,
        page_columns,
    }
}

/// What a point of the grid is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    /// The top-left corner: select everything.
    Corner,
    /// A column header.
    ColumnHeader(u32),
    /// The right edge of a column header: resize it.
    ColumnEdge(u32),
    /// A row header.
    RowHeader(u32),
    /// The bottom edge of a row header.
    RowEdge(u32),
    /// A cell.
    Cell(CellGridCellRef),
    /// The fill handle.
    FillHandle,
    /// Outside every band (below the last row, right of the last column).
    Nothing,
}

/// The band of `bands` at `p`, else the nearest one before it (a point on
/// the freeze line belongs to the frozen band before it).
fn band_at(bands: &[Band], p: f32) -> Option<Band> {
    let mut found = None;
    for b in bands {
        if p >= b.start {
            found = Some(*b);
        } else {
            break;
        }
    }
    found.filter(|b| p < b.end() + FREEZE_LINE_PX)
}

/// What is at `(x, y)` - px relative to the grid's top-left corner.
pub(crate) fn hit_test(geo: &Geometry, fill_handle: Option<(f32, f32)>, x: f32, y: f32) -> Hit {
    if let Some((hx, hy)) = fill_handle {
        let half = FILL_HANDLE_PX / 2.0 + 1.0;
        if (x - hx).abs() <= half && (y - hy).abs() <= half {
            return Hit::FillHandle;
        }
    }
    if y < geo.header_height {
        if x < geo.header_width {
            return Hit::Corner;
        }
        return match band_at(&geo.columns, x) {
            Some(b) if b.end() - x <= RESIZE_GRIP_PX && x <= b.end() => Hit::ColumnEdge(b.index),
            // The grip reaches a little into the NEXT column too.
            Some(b) if x - b.start <= RESIZE_GRIP_PX / 2.0 => {
                match geo.columns.iter().rev().find(|c| c.end() <= b.start + 0.5) {
                    Some(prev) => Hit::ColumnEdge(prev.index),
                    None => Hit::ColumnHeader(b.index),
                }
            }
            Some(b) => Hit::ColumnHeader(b.index),
            None => Hit::Nothing,
        };
    }
    if x < geo.header_width {
        return match band_at(&geo.rows, y) {
            Some(b) if b.end() - y <= RESIZE_GRIP_PX && y <= b.end() => Hit::RowEdge(b.index),
            Some(b) => Hit::RowHeader(b.index),
            None => Hit::Nothing,
        };
    }
    match (band_at(&geo.rows, y), band_at(&geo.columns, x)) {
        (Some(r), Some(c)) => Hit::Cell(CellGridCellRef::create(r.index, c.index)),
        _ => Hit::Nothing,
    }
}

/// The cell under `(x, y)`, the nearest cell in view when the point lies
/// past the last row or column (a drag that left the grid keeps a target).
pub(crate) fn nearest_cell(geo: &Geometry, x: f32, y: f32) -> Option<CellGridCellRef> {
    let pick = |bands: &[Band], p: f32| -> Option<u32> {
        let first = bands.first()?;
        if p < first.start {
            return Some(first.index);
        }
        Some(
            bands
                .iter()
                .rev()
                .find(|b| p >= b.start)
                .map_or(first.index, |b| b.index),
        )
    };
    Some(CellGridCellRef::create(
        pick(&geo.rows, y)?,
        pick(&geo.columns, x)?,
    ))
}

/// The rectangle of `range` in view: `(x, y, width, height)`, clipped to
/// the bands shown; `None` when no part of it is in view.
pub(crate) fn range_rect(geo: &Geometry, range: &CellGridRange) -> Option<(f32, f32, f32, f32)> {
    let span = |bands: &[Band], lo: u32, hi: u32| -> Option<(f32, f32)> {
        let inside: Vec<&Band> = bands
            .iter()
            .filter(|b| b.index >= lo && b.index <= hi)
            .collect();
        let first = inside.first()?;
        let last = inside.last()?;
        Some((first.start, last.end()))
    };
    let (x0, x1) = span(&geo.columns, range.first.column, range.last.column)?;
    let (y0, y1) = span(&geo.rows, range.first.row, range.last.row)?;
    Some((x0, y0, x1 - x0, y1 - y0))
}

// ---- navigation: keys and clicks to the next view ----

/// A direction of the cell cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dir {
    Up,
    Down,
    Left,
    Right,
}

/// What navigation needs to know about the grid besides the view.
pub(crate) struct Bounds<'a> {
    pub row_count: u32,
    pub column_count: u32,
    pub content_rows: u32,
    pub content_columns: u32,
    pub frozen_rows: u32,
    pub frozen_columns: u32,
    pub column_widths: &'a [CellGridSize],
    pub row_heights: &'a [CellGridSize],
    /// Rows / columns a page holds.
    pub page_rows: u32,
    pub page_columns: u32,
}

impl Bounds<'_> {
    fn hidden(&self, dir: Dir, index: u32) -> bool {
        match dir {
            Dir::Up | Dir::Down => size_at(self.row_heights, index, 1.0) <= 0.0,
            Dir::Left | Dir::Right => size_at(self.column_widths, index, 1.0) <= 0.0,
        }
    }

    /// One visible step from `cell` in `dir`; `cell` itself at the edge.
    pub(crate) fn step(&self, cell: CellGridCellRef, dir: Dir) -> CellGridCellRef {
        let mut c = cell;
        loop {
            let next = match dir {
                Dir::Up if c.row > 0 => CellGridCellRef::create(c.row - 1, c.column),
                Dir::Down if c.row + 1 < self.row_count => CellGridCellRef::create(c.row + 1, c.column),
                Dir::Left if c.column > 0 => CellGridCellRef::create(c.row, c.column - 1),
                Dir::Right if c.column + 1 < self.column_count => {
                    CellGridCellRef::create(c.row, c.column + 1)
                }
                _ => return cell,
            };
            let index = match dir {
                Dir::Up | Dir::Down => next.row,
                Dir::Left | Dir::Right => next.column,
            };
            if !self.hidden(dir, index) {
                return next;
            }
            c = next;
        }
    }

    /// `n` visible steps (at most to the edge).
    pub(crate) fn steps(&self, cell: CellGridCellRef, dir: Dir, n: u32) -> CellGridCellRef {
        let mut c = cell;
        for _ in 0..n {
            let next = self.step(c, dir);
            if next == c {
                break;
            }
            c = next;
        }
        c
    }

    /// The last row / column index in `dir`.
    fn sheet_edge(&self, cell: CellGridCellRef, dir: Dir) -> CellGridCellRef {
        match dir {
            Dir::Up => CellGridCellRef::create(0, cell.column),
            Dir::Down => CellGridCellRef::create(self.row_count.saturating_sub(1), cell.column),
            Dir::Left => CellGridCellRef::create(cell.row, 0),
            Dir::Right => CellGridCellRef::create(cell.row, self.column_count.saturating_sub(1)),
        }
    }

    /// Whether `cell` lies past the data in `dir` (nothing more to find).
    fn past_content(&self, cell: CellGridCellRef, dir: Dir) -> bool {
        match dir {
            Dir::Down => cell.row + 1 >= self.content_rows,
            Dir::Right => cell.column + 1 >= self.content_columns,
            Dir::Up => cell.row == 0,
            Dir::Left => cell.column == 0,
        }
    }

    /// Ctrl + arrow (Excel): from a cell with data whose neighbour has data,
    /// to the last cell of that run; otherwise to the next cell with data,
    /// or the sheet's edge when there is none.
    pub(crate) fn data_edge(
        &self,
        cell: CellGridCellRef,
        dir: Dir,
        has_data: &mut dyn FnMut(CellGridCellRef) -> bool,
    ) -> CellGridCellRef {
        let next = self.step(cell, dir);
        if next == cell {
            return cell;
        }
        if has_data(cell) && has_data(next) {
            let mut c = next;
            loop {
                let n = self.step(c, dir);
                if n == c || !has_data(n) {
                    return c;
                }
                c = n;
            }
        }
        let mut c = next;
        loop {
            if has_data(c) {
                return c;
            }
            if self.past_content(c, dir) {
                return self.sheet_edge(c, dir);
            }
            let n = self.step(c, dir);
            if n == c {
                return c;
            }
            c = n;
        }
    }
}

/// The corner of the current range opposite the anchor: the end a Shift
/// key or a drag moves.
pub(crate) fn moving_end(view: &CellGridView) -> CellGridCellRef {
    let r = view.current_range();
    let a = view.anchor;
    CellGridCellRef::create(
        if r.first.row == a.row { r.last.row } else { r.first.row },
        if r.first.column == a.column {
            r.last.column
        } else {
            r.first.column
        },
    )
}

/// The view with the cursor on `cell`: alone (`extend` and `add` off), the
/// current range grown from the anchor to `cell` (`extend`), or a new range
/// beside the others (`add`, Ctrl + click).
pub(crate) fn select(view: &CellGridView, cell: CellGridCellRef, extend: bool, add: bool) -> CellGridView {
    let mut next = view.clone();
    let mut ranges: Vec<CellGridRange> = view.ranges.as_ref().to_vec();
    if extend {
        let range = CellGridRange::spanning(view.anchor, cell);
        match ranges.last_mut() {
            Some(last) => *last = range,
            None => ranges.push(range),
        }
        next.active = view.anchor;
    } else if add {
        ranges.push(CellGridRange::create(cell));
        next.active = cell;
        next.anchor = cell;
    } else {
        ranges = alloc::vec![CellGridRange::create(cell)];
        next.active = cell;
        next.anchor = cell;
    }
    next.ranges = CellGridRangeVec::from_vec(ranges);
    next
}

/// Scrolls the view so `cell` is in view: the scrolled window moves the
/// least that shows it; a frozen row / column is always in view.
pub(crate) fn reveal(view: &mut CellGridView, b: &Bounds<'_>, cell: CellGridCellRef) {
    if cell.row >= b.frozen_rows {
        if cell.row < view.top_row {
            view.top_row = cell.row;
        } else if cell.row >= view.top_row.saturating_add(b.page_rows) {
            view.top_row = cell.row + 1 - b.page_rows.max(1);
        }
    }
    if cell.column >= b.frozen_columns {
        if cell.column < view.left_column {
            view.left_column = cell.column;
        } else if cell.column >= view.left_column.saturating_add(b.page_columns) {
            view.left_column = cell.column + 1 - b.page_columns.max(1);
        }
    }
    view.top_row = view.top_row.max(b.frozen_rows);
    view.left_column = view.left_column.max(b.frozen_columns);
}

/// The view after scrolling `rows` / `columns` whole bands (negative: up /
/// left), clamped to the sheet; the selection stays.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to 0..count
pub(crate) fn scroll_by(view: &CellGridView, b: &Bounds<'_>, rows: i64, columns: i64) -> CellGridView {
    let mut next = view.clone();
    let clamp = |at: u32, by: i64, lo: u32, count: u32| -> u32 {
        let hi = i64::from(count.saturating_sub(1)).max(i64::from(lo));
        (i64::from(at) + by).clamp(i64::from(lo), hi) as u32
    };
    next.top_row = clamp(view.top_row.max(b.frozen_rows), rows, b.frozen_rows, b.row_count);
    next.left_column = clamp(
        view.left_column.max(b.frozen_columns),
        columns,
        b.frozen_columns,
        b.column_count,
    );
    next
}

/// A navigation key's answer: the next view, and whether the cursor moved
/// (Select) or only the window did (Scroll).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Navigated {
    pub view: CellGridView,
    pub kind: CellGridEventKind,
}

/// The keys the grid navigates with (no edit in progress). `shift` / `ctrl`
/// as held; `None` for a key the grid leaves alone.
pub(crate) fn navigate(
    view: &CellGridView,
    b: &Bounds<'_>,
    key: VirtualKeyCode,
    shift: bool,
    ctrl: bool,
    has_data: &mut dyn FnMut(CellGridCellRef) -> bool,
) -> Option<Navigated> {
    use VirtualKeyCode as K;
    let from = if shift { moving_end(view) } else { view.active };
    let dir = match key {
        K::Up => Some(Dir::Up),
        K::Down => Some(Dir::Down),
        K::Left => Some(Dir::Left),
        K::Right => Some(Dir::Right),
        _ => None,
    };
    let target = if let Some(dir) = dir {
        if ctrl {
            b.data_edge(from, dir, has_data)
        } else {
            b.step(from, dir)
        }
    } else {
        match key {
            K::PageDown => {
                let to = b.steps(from, Dir::Down, b.page_rows.max(1));
                let mut next = select(view, to, shift, false);
                next = scroll_by(&next, b, i64::from(b.page_rows.max(1)), 0);
                reveal(&mut next, b, to);
                return Some(Navigated {
                    view: next,
                    kind: CellGridEventKind::Select,
                });
            }
            K::PageUp => {
                let to = b.steps(from, Dir::Up, b.page_rows.max(1));
                let mut next = select(view, to, shift, false);
                next = scroll_by(&next, b, -i64::from(b.page_rows.max(1)), 0);
                reveal(&mut next, b, to);
                return Some(Navigated {
                    view: next,
                    kind: CellGridEventKind::Select,
                });
            }
            K::Home if ctrl => CellGridCellRef::create(0, 0),
            K::Home => CellGridCellRef::create(from.row, 0),
            K::End if ctrl => CellGridCellRef::create(
                b.content_rows.saturating_sub(1),
                b.content_columns.saturating_sub(1),
            ),
            K::Space if ctrl && shift => {
                return Some(select_all(view, b));
            }
            K::Space if ctrl => {
                let mut next = view.clone();
                let range = CellGridRange::spanning(
                    CellGridCellRef::create(0, view.active.column),
                    CellGridCellRef::create(b.row_count.saturating_sub(1), view.active.column),
                );
                next.ranges = CellGridRangeVec::from_vec(alloc::vec![range]);
                next.anchor = next.active;
                return Some(Navigated {
                    view: next,
                    kind: CellGridEventKind::Select,
                });
            }
            K::Space if shift => {
                let mut next = view.clone();
                let range = CellGridRange::spanning(
                    CellGridCellRef::create(view.active.row, 0),
                    CellGridCellRef::create(view.active.row, b.column_count.saturating_sub(1)),
                );
                next.ranges = CellGridRangeVec::from_vec(alloc::vec![range]);
                next.anchor = next.active;
                return Some(Navigated {
                    view: next,
                    kind: CellGridEventKind::Select,
                });
            }
            K::A if ctrl => return Some(select_all(view, b)),
            K::Return | K::NumpadEnter => {
                let to = b.step(view.active, if shift { Dir::Up } else { Dir::Down });
                let mut next = select(view, to, false, false);
                reveal(&mut next, b, to);
                return Some(Navigated {
                    view: next,
                    kind: CellGridEventKind::Select,
                });
            }
            K::Tab => {
                let to = b.step(view.active, if shift { Dir::Left } else { Dir::Right });
                let mut next = select(view, to, false, false);
                reveal(&mut next, b, to);
                return Some(Navigated {
                    view: next,
                    kind: CellGridEventKind::Select,
                });
            }
            _ => return None,
        }
    };
    let mut next = select(view, target, shift, false);
    reveal(&mut next, b, target);
    Some(Navigated {
        view: next,
        kind: CellGridEventKind::Select,
    })
}

/// Everything selected (Ctrl+A), the cursor kept.
fn select_all(view: &CellGridView, b: &Bounds<'_>) -> Navigated {
    let mut next = view.clone();
    next.ranges = CellGridRangeVec::from_vec(alloc::vec![CellGridRange::spanning(
        CellGridCellRef::create(0, 0),
        CellGridCellRef::create(b.row_count.saturating_sub(1), b.column_count.saturating_sub(1)),
    )]);
    next.anchor = next.active;
    Navigated {
        view: next,
        kind: CellGridEventKind::Select,
    }
}

// ---- the wheel ----

/// The wheel travel (px) that scrolls one row.
pub(crate) const WHEEL_PX_PER_ROW: f32 = 20.0;
/// The most rows / columns one wheel event scrolls (a momentum burst must
/// not fly through the sheet).
pub(crate) const WHEEL_MAX_STEPS: i64 = 12;

/// Adds `delta` px to the running `travel` and returns the whole steps it
/// unlocks (at most `WHEEL_MAX_STEPS` either way), keeping the remainder.
pub(crate) fn wheel_steps(travel: &mut f32, delta: f32, px_per_step: f32) -> i64 {
    if !delta.is_finite() || !px_per_step.is_finite() || px_per_step <= 0.0 {
        return 0;
    }
    *travel += delta;
    #[allow(clippy::cast_possible_truncation)]
    let steps = ((*travel / px_per_step) as i64).clamp(-WHEEL_MAX_STEPS, WHEEL_MAX_STEPS);
    #[allow(clippy::cast_precision_loss)]
    let used = steps as f32 * px_per_step;
    *travel -= used;
    steps
}

#[cfg(feature = "std")]
thread_local! {
    // The wheel travel not yet turned into whole rows / columns. Thread-local
    // like the time picker's: one pointer scrolls at a time, and the grid's
    // view is an FFI struct the app rebuilds on every step.
    static WHEEL_TRAVEL: core::cell::Cell<(f32, f32)> = const { core::cell::Cell::new((0.0, 0.0)) };
}

/// [`wheel_steps`] on the thread's running travel: (rows, columns).
fn take_wheel(dx: f32, dy: f32, px_per_row: f32, px_per_column: f32) -> (i64, i64) {
    #[cfg(feature = "std")]
    {
        WHEEL_TRAVEL.with(|cell| {
            let (mut tx, mut ty) = cell.get();
            let rows = wheel_steps(&mut ty, dy, px_per_row);
            let columns = wheel_steps(&mut tx, dx, px_per_column);
            cell.set((tx, ty));
            (rows, columns)
        })
    }
    #[cfg(not(feature = "std"))]
    {
        let (mut tx, mut ty) = (0.0, 0.0);
        (
            wheel_steps(&mut ty, dy, px_per_row),
            wheel_steps(&mut tx, dx, px_per_column),
        )
    }
}

// ---- the clipboard flavours ----

/// One field of tab-separated text: quoted (`"` doubled) when it holds a
/// tab, a line break or starts with a quote, so a spreadsheet reads it back
/// as one cell.
fn tsv_field(field: &str) -> String {
    if field.contains(['\t', '\n', '\r']) || field.starts_with('"') {
        alloc::format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        String::from(field)
    }
}

/// Rows of cells as tab-separated text (what Excel puts on the clipboard).
pub(crate) fn cells_to_tsv(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let fields: Vec<String> = row.iter().map(|f| tsv_field(f)).collect();
        out.push_str(&fields.join("\t"));
    }
    out
}

/// Rows of cells as an HTML table (the rich flavour a word processor or a
/// mail pastes as a table).
pub(crate) fn cells_to_html(rows: &[Vec<String>]) -> String {
    let mut out = String::from("<table>");
    for row in rows {
        out.push_str("<tr>");
        for cell in row {
            out.push_str("<td>");
            out.push_str(&azul_core::xml::html::encode_text(cell));
            out.push_str("</td>");
        }
        out.push_str("</tr>");
    }
    out.push_str("</table>");
    out
}

// ---- the build: the cells in view, their looks, the overlays ----

use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{length::FloatValue, StyleFontSize, StyleFontStyle},
        layout::{
            LayoutAlignItems, LayoutBoxSizing, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutFlexShrink, LayoutHeight, LayoutJustifyContent, LayoutLeft, LayoutMinHeight,
            LayoutMinWidth, LayoutOverflow, LayoutPaddingLeft, LayoutPaddingRight, LayoutPosition,
            LayoutTop, LayoutWidth,
        },
        property::CssProperty,
        style::{
            BorderStyle, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth,
            LayoutBorderTopWidth, StyleBorderBottomColor, StyleBorderBottomStyle,
            StyleBorderLeftColor, StyleBorderLeftStyle, StyleBorderRightColor,
            StyleBorderRightStyle, StyleBorderTopColor, StyleBorderTopStyle, StyleCursor,
            StyleTextColor, StyleTextDecoration, StyleUserSelect, StyleWhiteSpace,
        },
    },
};

/// The grid's class; the grid node also carries the app's `id`.
pub(crate) const GRID_CLASS_NAME: &str = "__azul-native-cell-grid";
/// A row of cells (the header row too).
pub(crate) const ROW_CLASS_NAME: &str = "__azul-native-cell-grid-row";
/// A cell.
pub(crate) const CELL_CLASS_NAME: &str = "__azul-native-cell-grid-cell";
/// A column letter or a row number.
pub(crate) const HEADER_CLASS_NAME: &str = "__azul-native-cell-grid-header";
/// The top-left corner.
pub(crate) const CORNER_CLASS_NAME: &str = "__azul-native-cell-grid-corner";
/// The line after the frozen rows / columns.
pub(crate) const FREEZE_CLASS_NAME: &str = "__azul-native-cell-grid-freeze";
/// The outline around the current range.
pub(crate) const OUTLINE_CLASS_NAME: &str = "__azul-native-cell-grid-outline";
/// The fill handle.
pub(crate) const FILL_HANDLE_CLASS_NAME: &str = "__azul-native-cell-grid-fill-handle";
/// The outline of the range a fill drag reaches.
pub(crate) const FILL_PREVIEW_CLASS_NAME: &str = "__azul-native-cell-grid-fill-preview";
/// The in-cell editor.
pub(crate) const EDITOR_CLASS_NAME: &str = "__azul-native-cell-grid-editor";
/// The editor's caret.
pub(crate) const CARET_CLASS_NAME: &str = "__azul-native-cell-grid-caret";

static GRID_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(GRID_CLASS_NAME))];
static ROW_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(ROW_CLASS_NAME))];
static CELL_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(CELL_CLASS_NAME))];
static HEADER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(HEADER_CLASS_NAME))];
static CORNER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(CORNER_CLASS_NAME))];
static FREEZE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(FREEZE_CLASS_NAME))];
static OUTLINE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(OUTLINE_CLASS_NAME))];
static FILL_HANDLE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str(FILL_HANDLE_CLASS_NAME))];
static FILL_PREVIEW_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str(FILL_PREVIEW_CLASS_NAME))];
static EDITOR_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(EDITOR_CLASS_NAME))];
static CARET_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(CARET_CLASS_NAME))];

/// What a theme decides about a grid: the SKIN of each part, laid over the
/// part's base (the structure, the same in every theme) by [`build`].
pub(crate) struct CellGridLook {
    /// The grid: the face, the ink, the paper.
    pub grid: Vec<CssPropertyWithConditions>,
    /// A column letter / row number: the strip, the soft ink, hairlines.
    pub header: Vec<CssPropertyWithConditions>,
    /// Added to the headers of the selection's columns and rows.
    pub header_active: Vec<CssPropertyWithConditions>,
    /// Added to the header of a WHOLLY selected column or row.
    pub header_selected: Vec<CssPropertyWithConditions>,
    /// The top-left corner.
    pub corner: Vec<CssPropertyWithConditions>,
    /// A cell's right grid line (width, style and colour).
    pub grid_line_right: Vec<CssPropertyWithConditions>,
    /// A cell's bottom grid line.
    pub grid_line_bottom: Vec<CssPropertyWithConditions>,
    /// The right edge without grid lines (same width, the paper's colour,
    /// so hiding the lines moves nothing).
    pub no_line_right: Vec<CssPropertyWithConditions>,
    /// The bottom edge without grid lines.
    pub no_line_bottom: Vec<CssPropertyWithConditions>,
    /// Added to a selected cell (the active one stays clear).
    pub selected: Vec<CssPropertyWithConditions>,
    /// The freeze line.
    pub freeze_line: Vec<CssPropertyWithConditions>,
    /// The outline around the current range.
    pub outline: Vec<CssPropertyWithConditions>,
    /// The fill handle.
    pub fill_handle: Vec<CssPropertyWithConditions>,
    /// The outline of the range a fill drag reaches.
    pub fill_preview: Vec<CssPropertyWithConditions>,
    /// The in-cell editor.
    pub editor: Vec<CssPropertyWithConditions>,
    /// The editor's caret.
    pub caret: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the grid, if it has one.
    pub marker: Option<&'static str>,
}

/// The grid: a column of rows that takes its pane, clips what does not fit,
/// is the containing block of the overlays, and is ONE focus stop whose
/// text a drag never selects.
pub(crate) static CELL_GRID_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    simple(CssProperty::const_min_height(LayoutMinHeight::const_px(0))),
    simple(CssProperty::const_min_width(LayoutMinWidth::const_px(0))),
    simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    simple(CssProperty::const_position(LayoutPosition::Relative)),
    simple(CssProperty::const_cursor(StyleCursor::Cell)),
    simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// A row: the header, then the cells side by side, never shrinking.
pub(crate) static CELL_GRID_ROW_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// A cell: its width, its content set by its alignment, clipped, the grid
/// lines inside its box.
pub(crate) static CELL_GRID_CELL_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(3))),
    simple(CssProperty::const_padding_right(LayoutPaddingRight::const_px(3))),
];

/// A header (a column letter, a row number, the corner): its label centred.
pub(crate) static CELL_GRID_HEADER_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_justify_content(LayoutJustifyContent::Center)),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    simple(CssProperty::const_cursor(StyleCursor::Default)),
];

/// The freeze line between the frozen and the scrolled part.
pub(crate) static CELL_GRID_FREEZE_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// An overlay (outline, fill handle, editor): placed by px over the cells.
pub(crate) static CELL_GRID_OVERLAY_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_position(LayoutPosition::Absolute)),
    simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
];

/// The editor: the text and the caret on one line, never wrapped.
pub(crate) static CELL_GRID_EDITOR_BASE: &[CssPropertyWithConditions] = &[
    simple(CssProperty::const_position(LayoutPosition::Absolute)),
    simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    simple(CssProperty::const_display(LayoutDisplay::Flex)),
    simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    simple(CssProperty::const_cursor(StyleCursor::Text)),
];

/// A cell's text: one line unless the cell wraps.
pub(crate) static CELL_GRID_TEXT_BASE: &[CssPropertyWithConditions] = &[simple(
    CssProperty::WhiteSpace(CssPropertyValue::Exact(StyleWhiteSpace::Pre)),
)];

/// The grid with its window laid out and the cells in view asked for ONCE
/// (both looks are built from it when the grid follows the app theme).
#[derive(Debug, Clone)]
pub(crate) struct CellGridResolved {
    /// The grid, a resize in progress applied to its sizes.
    pub grid: CellGrid,
    /// Where the rows and columns in view sit.
    pub geo: Geometry,
    /// `cells[r][c]` for `geo.rows[r]` x `geo.columns[c]`.
    pub cells: Vec<Vec<(CellGridCell, CellGridCellStyle)>>,
}

/// The content of `at`, from the data callback.
fn cell_content(source: &OptionCellGridDataSource, at: CellGridCellRef) -> CellGridCell {
    match source.as_ref() {
        Some(CellGridDataSource { refany, callback }) => callback.invoke(refany.clone(), at),
        None => CellGridCell::empty(),
    }
}

/// The look of `at`, from the style callback.
fn cell_style(source: &OptionCellGridStyleSource, at: CellGridCellRef) -> CellGridCellStyle {
    match source.as_ref() {
        Some(CellGridStyleSource { refany, callback }) => callback.invoke(refany.clone(), at),
        None => CellGridCellStyle::default(),
    }
}

/// A column / row being resized is drawn at the dragged size.
fn apply_resize_preview(grid: &mut CellGrid) {
    let drag = grid.view.drag;
    match drag.kind {
        CellGridDragKind::ResizeColumn => {
            let mut v = grid.column_widths.as_ref().to_vec();
            v.push(CellGridSize::create(drag.index, drag.size));
            grid.column_widths = CellGridSizeVec::from_vec(v);
        }
        CellGridDragKind::ResizeRow => {
            let mut v = grid.row_heights.as_ref().to_vec();
            v.push(CellGridSize::create(drag.index, drag.size));
            grid.row_heights = CellGridSizeVec::from_vec(v);
        }
        _ => {}
    }
}

/// Lays the grid out and asks the callbacks for the cells in view.
pub(crate) fn resolve(mut grid: CellGrid) -> CellGridResolved {
    apply_resize_preview(&mut grid);
    grid.view.top_row = grid.view.top_row.max(grid.frozen_rows);
    grid.view.left_column = grid.view.left_column.max(grid.frozen_columns);
    let geo = geometry(&grid);
    let cells = geo
        .rows
        .iter()
        .map(|r| {
            geo.columns
                .iter()
                .map(|c| {
                    let at = CellGridCellRef::create(r.index, c.index);
                    (
                        cell_content(&grid.data_source, at),
                        cell_style(&grid.style_source, at),
                    )
                })
                .collect()
        })
        .collect();
    CellGridResolved { grid, geo, cells }
}

/// How many columns each cell of row `ri` (an index into `geo.rows`) is
/// drawn across: 1 for a cell of its own, n > 1 for a text that spills over
/// the n - 1 empty cells after it (Excel), 0 for a cell drawn under such a
/// spill.
///
/// Only a left-aligned (or General) text that does not wrap spills; it runs
/// on while the next cell is empty, unselected and has no fill or border of
/// its own, never across the freeze line. Its width is estimated the way an
/// auto-fit is ([`SPILL_EM`] of the font size per character, plus the
/// cell's padding): the grid is built before its text is measured.
pub(crate) fn spill_spans(resolved: &CellGridResolved, ri: usize) -> Vec<u32> {
    let Some(row) = resolved.cells.get(ri) else {
        return Vec::new();
    };
    let grid = &resolved.grid;
    let geo = &resolved.geo;
    let zoom = if grid.zoom.is_finite() && grid.zoom > 0.0 {
        grid.zoom
    } else {
        1.0
    };
    let row_index = geo.rows.get(ri).map_or(0, |b| b.index);
    let mut spans = alloc::vec![1u32; row.len()];
    let mut ci = 0;
    while ci < row.len() {
        let (content, style) = &row[ci];
        let spills = content.kind == CellGridCellKind::Text
            && !style.wrap
            && matches!(
                style.align,
                CellGridHorizontalAlign::General | CellGridHorizontalAlign::Left
            );
        if !spills {
            ci += 1;
            continue;
        }
        let font = if style.font_size > 0.0 && style.font_size.is_finite() {
            style.font_size
        } else {
            grid.font_size
        };
        #[allow(clippy::cast_precision_loss)]
        let chars = content.text.as_str().chars().count() as f32;
        let needed = chars * font * zoom * SPILL_EM + 2.0 * CELL_PADDING_PX;
        let mut reach = geo.columns.get(ci).map_or(0.0, |b| b.size);
        let mut end = ci + 1;
        while reach < needed && end < row.len() {
            if geo.frozen_columns > 0 && end == geo.frozen_columns {
                break; // never across the freeze line
            }
            let (next, next_style) = &row[end];
            let at = CellGridCellRef::create(row_index, geo.columns[end].index);
            if !next.text.as_str().is_empty() || grid.view.is_selected(at) || has_own_look(next_style) {
                break;
            }
            reach += geo.columns[end].size;
            end += 1;
        }
        if end > ci + 1 {
            #[allow(clippy::cast_possible_truncation)]
            {
                spans[ci] = (end - ci) as u32;
            }
            for covered in &mut spans[ci + 1..end] {
                *covered = 0;
            }
        }
        ci = end;
    }
    spans
}

/// The px per character of font size a spilled text is reckoned at (an
/// average glyph; the auto-fit of a column uses the same).
pub(crate) const SPILL_EM: f32 = 0.6;
/// A cell's left + right padding is twice this (`CELL_GRID_CELL_BASE`).
pub(crate) const CELL_PADDING_PX: f32 = 3.0;

/// Whether a cell draws something of its own besides text (a fill, a border):
/// a spilled text stops before it.
fn has_own_look(style: &CellGridCellStyle) -> bool {
    style.fill.as_ref().is_some()
        || style.border_top.as_ref().is_some()
        || style.border_right.as_ref().is_some()
        || style.border_bottom.as_ref().is_some()
        || style.border_left.as_ref().is_some()
}

/// The range a fill drag from `source` to `target` covers: the source
/// stretched down / up or right / left (whichever way the pointer went
/// further), never both.
pub(crate) fn fill_range(source: CellGridRange, target: CellGridCellRef) -> CellGridRange {
    let below = target.row.saturating_sub(source.last.row);
    let above = source.first.row.saturating_sub(target.row);
    let right = target.column.saturating_sub(source.last.column);
    let left = source.first.column.saturating_sub(target.column);
    let vertical = below.max(above);
    let horizontal = right.max(left);
    let mut r = source;
    if vertical == 0 && horizontal == 0 {
        return r;
    }
    if vertical >= horizontal {
        if below > 0 {
            r.last.row = target.row;
        } else {
            r.first.row = target.row;
        }
    } else if right > 0 {
        r.last.column = target.column;
    } else {
        r.first.column = target.column;
    }
    r
}

/// The automatic ink on a filled cell: black or white, whichever contrasts
/// more with the fill (`ColorU::best_contrast_text`, the WCAG rule).
pub(crate) fn auto_ink(fill: ColorU) -> ColorU {
    fill.best_contrast_text()
}

/// `left` / `top` / `width` / `height` of an overlay.
fn place(x: f32, y: f32, w: f32, h: f32) -> [CssPropertyWithConditions; 4] {
    [
        px_left(x),
        px_top(y),
        px_width(w.max(0.0)),
        px_height(h.max(0.0)),
    ]
}

/// A thin border edge in a cell's own colour.
fn edge(which: Dir, color: ColorU) -> [CssPropertyWithConditions; 3] {
    match which {
        Dir::Up => [
            simple(CssProperty::const_border_top_width(LayoutBorderTopWidth::const_px(1))),
            simple(CssProperty::const_border_top_style(StyleBorderTopStyle {
                inner: BorderStyle::Solid,
            })),
            simple(CssProperty::const_border_top_color(StyleBorderTopColor { inner: color })),
        ],
        Dir::Right => [
            simple(CssProperty::const_border_right_width(
                LayoutBorderRightWidth::const_px(1),
            )),
            simple(CssProperty::const_border_right_style(StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            })),
            simple(CssProperty::const_border_right_color(StyleBorderRightColor {
                inner: color,
            })),
        ],
        Dir::Down => [
            simple(CssProperty::const_border_bottom_width(
                LayoutBorderBottomWidth::const_px(1),
            )),
            simple(CssProperty::const_border_bottom_style(StyleBorderBottomStyle {
                inner: BorderStyle::Solid,
            })),
            simple(CssProperty::const_border_bottom_color(StyleBorderBottomColor {
                inner: color,
            })),
        ],
        Dir::Left => [
            simple(CssProperty::const_border_left_width(LayoutBorderLeftWidth::const_px(1))),
            simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            })),
            simple(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: color })),
        ],
    }
}

/// The declarations a cell's own style adds: font, fill, ink, alignment,
/// custom borders (in place of the grid line on that edge).
fn cell_style_props(
    style: &CellGridCellStyle,
    kind: CellGridCellKind,
    zoom: f32,
    look: &CellGridLook,
    grid_lines: bool,
    selected: bool,
) -> Vec<CssPropertyWithConditions> {
    let mut v = Vec::new();
    v.extend(match style.border_right.into_option() {
        Some(c) => edge(Dir::Right, c).to_vec(),
        None if grid_lines => look.grid_line_right.clone(),
        None => look.no_line_right.clone(),
    });
    v.extend(match style.border_bottom.into_option() {
        Some(c) => edge(Dir::Down, c).to_vec(),
        None if grid_lines => look.grid_line_bottom.clone(),
        None => look.no_line_bottom.clone(),
    });
    if let Some(c) = style.border_top.into_option() {
        v.extend(edge(Dir::Up, c));
    }
    if let Some(c) = style.border_left.into_option() {
        v.extend(edge(Dir::Left, c));
    }
    let justify = match style.align {
        CellGridHorizontalAlign::Left => LayoutJustifyContent::Start,
        CellGridHorizontalAlign::Center => LayoutJustifyContent::Center,
        CellGridHorizontalAlign::Right => LayoutJustifyContent::End,
        CellGridHorizontalAlign::General => match kind {
            CellGridCellKind::Number => LayoutJustifyContent::End,
            CellGridCellKind::Boolean | CellGridCellKind::Error => LayoutJustifyContent::Center,
            CellGridCellKind::Empty | CellGridCellKind::Text => LayoutJustifyContent::Start,
        },
    };
    v.push(simple(CssProperty::const_justify_content(justify)));
    v.push(simple(CssProperty::const_align_items(match style.vertical_align {
        CellGridVerticalAlign::Bottom => LayoutAlignItems::End,
        CellGridVerticalAlign::Center => LayoutAlignItems::Center,
        CellGridVerticalAlign::Top => LayoutAlignItems::Start,
    })));
    if style.font_size > 0.0 && style.font_size.is_finite() {
        v.push(simple(CssProperty::const_font_size(StyleFontSize::px(
            style.font_size * zoom,
        ))));
    }
    if style.bold {
        v.push(super::themes::decl::bold());
    }
    if style.italic {
        v.push(simple(CssProperty::font_style(StyleFontStyle::Italic)));
    }
    match style.fill.into_option() {
        Some(fill) => {
            v.push(simple(super::themes::decl::fill(fill)));
            let ink = style.ink.into_option().unwrap_or_else(|| auto_ink(fill));
            v.push(simple(CssProperty::const_text_color(StyleTextColor { inner: ink })));
        }
        None => {
            if selected {
                v.extend(look.selected.iter().cloned());
            }
            if let Some(ink) = style.ink.into_option() {
                v.push(simple(CssProperty::const_text_color(StyleTextColor { inner: ink })));
            }
        }
    }
    v
}

/// A cell's text carrier: decorations, one line unless it wraps.
fn cell_text(text: AzString, style: &CellGridCellStyle) -> Dom {
    let mut props: Vec<CssPropertyWithConditions> = Vec::new();
    if style.wrap {
        props.push(simple(CssProperty::WhiteSpace(CssPropertyValue::Exact(
            StyleWhiteSpace::PreWrap,
        ))));
    } else {
        props.extend_from_slice(CELL_GRID_TEXT_BASE);
    }
    if style.underline {
        props.push(simple(CssProperty::text_decoration(StyleTextDecoration::Underline)));
    } else if style.strike {
        props.push(simple(CssProperty::text_decoration(StyleTextDecoration::LineThrough)));
    }
    crate::widgets::widget_p_with_text(text).with_css_props(CssPropertyWithConditionsVec::from_vec(props))
}

/// Whether column `c` is wholly selected (a range spans every row).
fn column_wholly_selected(view: &CellGridView, row_count: u32, c: u32) -> bool {
    view.ranges.as_ref().iter().any(|r| {
        r.first.row == 0 && r.last.row + 1 >= row_count && r.first.column <= c && c <= r.last.column
    })
}

/// Whether row `r` is wholly selected.
fn row_wholly_selected(view: &CellGridView, column_count: u32, r: u32) -> bool {
    view.ranges.as_ref().iter().any(|g| {
        g.first.column == 0
            && g.last.column + 1 >= column_count
            && g.first.row <= r
            && r <= g.last.row
    })
}

/// The grid's DOM in `look`: grid [header row?, (freeze line), rows ..,
/// outline, fill preview?, fill handle?, editor?].
#[allow(clippy::too_many_lines)]
pub(crate) fn build(resolved: CellGridResolved, look: &CellGridLook) -> Dom {
    use azul_core::a11y::{AccessibilityInfo, AccessibilityRole, AccessibilityState, AccessibilityStateVec};

    let part = |base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]| {
        super::themes::decl::on_base(base, skin)
    };
    // Asked before the cells are taken apart: which texts spill over which
    // empty cells, row by row.
    let spans: Vec<Vec<u32>> = (0..resolved.cells.len())
        .map(|ri| spill_spans(&resolved, ri))
        .collect();
    let CellGridResolved { grid, geo, cells } = resolved;
    let zoom = if grid.zoom.is_finite() && grid.zoom > 0.0 {
        grid.zoom
    } else {
        1.0
    };
    let view = &grid.view;
    let current = view.current_range();
    let any_frozen_rows = grid.frozen_rows > 0;
    let any_frozen_columns = grid.frozen_columns > 0;
    let row_label_width = geo.header_width;

    let freeze_v = |height: f32| -> Dom {
        let mut p = part(CELL_GRID_FREEZE_BASE, &look.freeze_line);
        p.push(px_width(FREEZE_LINE_PX));
        p.push(px_height(height));
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(FREEZE_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(p))
    };

    let mut children: Vec<Dom> = Vec::with_capacity(geo.rows.len() + 6);

    // The header row: the corner, then the column letters.
    if grid.show_headers {
        let mut cells_row: Vec<Dom> = Vec::with_capacity(geo.columns.len() + 2);
        let mut corner = part(CELL_GRID_HEADER_BASE, &look.corner);
        corner.push(px_width(row_label_width));
        cells_row.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(CORNER_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(corner)),
        );
        for (i, c) in geo.columns.iter().enumerate() {
            if i == geo.frozen_columns && any_frozen_columns {
                cells_row.push(freeze_v(geo.header_height));
            }
            let mut p = part(CELL_GRID_HEADER_BASE, &look.header);
            let in_selection = view
                .ranges
                .as_ref()
                .iter()
                .any(|r| r.first.column <= c.index && c.index <= r.last.column);
            if column_wholly_selected(view, grid.row_count, c.index) {
                p.extend(look.header_selected.iter().cloned());
            } else if in_selection {
                p.extend(look.header_active.iter().cloned());
            }
            p.push(px_width(c.size));
            let label = CellGrid::column_label(c.index);
            cells_row.push(
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(HEADER_CLASS))
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(p))
                    .with_accessibility_info(AccessibilityInfo {
                        column_index: azul_css::corety::OptionUsize::Some(c.index as usize + 1),
                        ..AccessibilityInfo::named(label.clone(), AccessibilityRole::ColumnHeader)
                    })
                    .with_child(crate::widgets::widget_p_with_text(label)),
            );
        }
        if geo.frozen_columns == geo.columns.len() && any_frozen_columns {
            cells_row.push(freeze_v(geo.header_height));
        }
        let mut row = CELL_GRID_ROW_BASE.to_vec();
        row.push(px_height(geo.header_height));
        children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(ROW_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(row))
                .with_accessibility_info(AccessibilityInfo {
                    role: AccessibilityRole::Row,
                    ..Default::default()
                })
                .with_children(DomVec::from_vec(cells_row)),
        );
    }

    // The rows: the frozen ones, the freeze line, the scrolled ones.
    let freeze_h = || -> Dom {
        let mut p = part(CELL_GRID_FREEZE_BASE, &look.freeze_line);
        p.push(px_height(FREEZE_LINE_PX));
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(FREEZE_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_vec(p))
    };
    for (ri, (r, row_cells)) in geo.rows.iter().zip(cells).enumerate() {
        if ri == geo.frozen_rows && any_frozen_rows {
            children.push(freeze_h());
        }
        let mut row_children: Vec<Dom> = Vec::with_capacity(geo.columns.len() + 2);
        if grid.show_headers {
            let mut p = part(CELL_GRID_HEADER_BASE, &look.header);
            let in_selection = view
                .ranges
                .as_ref()
                .iter()
                .any(|g| g.first.row <= r.index && r.index <= g.last.row);
            if row_wholly_selected(view, grid.column_count, r.index) {
                p.extend(look.header_selected.iter().cloned());
            } else if in_selection {
                p.extend(look.header_active.iter().cloned());
            }
            p.push(px_width(row_label_width));
            let label = AzString::from(alloc::format!("{}", u64::from(r.index) + 1));
            row_children.push(
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(HEADER_CLASS))
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(p))
                    .with_accessibility_info(AccessibilityInfo {
                        row_index: azul_css::corety::OptionUsize::Some(r.index as usize + 1),
                        ..AccessibilityInfo::named(label.clone(), AccessibilityRole::RowHeader)
                    })
                    .with_child(crate::widgets::widget_p_with_text(label)),
            );
        }
        let row_spans = spans.get(ri).map_or(&[][..], Vec::as_slice);
        for (ci, (c, (content, style))) in geo.columns.iter().zip(row_cells).enumerate() {
            if ci == geo.frozen_columns && any_frozen_columns {
                row_children.push(freeze_v(r.size));
            }
            // A text spilling over the empty cells after it is one cell as
            // wide as all of them; the cells under it are not built.
            let span = row_spans.get(ci).copied().unwrap_or(1) as usize;
            if span == 0 {
                continue;
            }
            let width: f32 = geo.columns[ci..(ci + span).min(geo.columns.len())]
                .iter()
                .map(|b| b.size)
                .sum();
            let at = CellGridCellRef::create(r.index, c.index);
            let selected = view.is_selected(at);
            let shaded = selected && at != view.active;
            let mut p = part(CELL_GRID_CELL_BASE, &[]);
            p.push(px_width(width));
            p.extend(cell_style_props(
                &style,
                content.kind,
                zoom,
                look,
                grid.show_grid_lines,
                shaded,
            ));
            let mut states = Vec::new();
            if selected {
                states.push(AccessibilityState::Selected);
            }
            let editing_here = view.is_editing() && at == view.active;
            let text = if editing_here {
                // The editor overlay shows the edit; the cell under it is blank.
                AzString::from_const_str("")
            } else {
                content.text
            };
            // Named by its place ("B2", what a screen reader says first),
            // its text as its value.
            let value = if text.as_str().is_empty() {
                None
            } else {
                Some(text.clone())
            };
            row_children.push(
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(CELL_CLASS))
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(p))
                    .with_accessibility_info(AccessibilityInfo {
                        row_index: azul_css::corety::OptionUsize::Some(r.index as usize + 1),
                        column_index: azul_css::corety::OptionUsize::Some(c.index as usize + 1),
                        states: AccessibilityStateVec::from_vec(states),
                        accessibility_value: value.into(),
                        ..AccessibilityInfo::named(CellGrid::cell_label(at), AccessibilityRole::GridCell)
                    })
                    .with_child(cell_text(text, &style)),
            );
        }
        if geo.frozen_columns == geo.columns.len() && any_frozen_columns {
            row_children.push(freeze_v(r.size));
        }
        let mut row = CELL_GRID_ROW_BASE.to_vec();
        row.push(px_height(r.size));
        children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(ROW_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(row))
                .with_accessibility_info(AccessibilityInfo {
                    role: AccessibilityRole::Row,
                    row_index: azul_css::corety::OptionUsize::Some(r.index as usize + 1),
                    ..Default::default()
                })
                .with_children(DomVec::from_vec(row_children)),
        );
    }
    if geo.frozen_rows == geo.rows.len() && any_frozen_rows {
        children.push(freeze_h());
    }

    // The overlays: the current range's outline and its fill handle, the
    // range a fill drag reaches, the editor.
    let outline_rect = range_rect(&geo, &current);
    if let Some((x, y, w, h)) = outline_rect {
        let mut p = part(CELL_GRID_OVERLAY_BASE, &look.outline);
        p.extend(place(x - 1.0, y - 1.0, w + 1.0, h + 1.0));
        children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(OUTLINE_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(p)),
        );
    }
    if view.drag.kind == CellGridDragKind::Fill {
        let reach = fill_range(current, view.drag.target);
        if reach != current {
            if let Some((x, y, w, h)) = range_rect(&geo, &reach) {
                let mut p = part(CELL_GRID_OVERLAY_BASE, &look.fill_preview);
                p.extend(place(x - 1.0, y - 1.0, w + 1.0, h + 1.0));
                children.push(
                    Dom::create_div()
                        .with_ids_and_classes(IdOrClassVec::from_const_slice(FILL_PREVIEW_CLASS))
                        .with_css_props(CssPropertyWithConditionsVec::from_vec(p)),
                );
            }
        }
    }
    if let Some((hx, hy)) = fill_handle_at(&grid, &geo) {
        let mut p = part(CELL_GRID_OVERLAY_BASE, &look.fill_handle);
        let half = FILL_HANDLE_PX / 2.0;
        p.extend(place(hx - half, hy - half, FILL_HANDLE_PX, FILL_HANDLE_PX));
        children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(FILL_HANDLE_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(p)),
        );
    }
    if view.is_editing() {
        if let Some((x, y, w, h)) = range_rect(&geo, &CellGridRange::create(view.active)) {
            let chars: Vec<char> = view.edit_text.as_str().chars().collect();
            let caret = (view.edit_cursor as usize).min(chars.len());
            let before: String = chars[..caret].iter().collect();
            let after: String = chars[caret..].iter().collect();
            let mut p = part(CELL_GRID_EDITOR_BASE, &look.editor);
            p.push(simple(CssProperty::const_left(LayoutLeft::px(x - 1.0))));
            p.push(simple(CssProperty::const_top(LayoutTop::px(y - 1.0))));
            p.push(px_min_width(w + 1.0));
            p.push(px_height(h + 1.0));
            let mut caret_props = look.caret.clone();
            caret_props.push(px_width(1.0));
            caret_props.push(px_height((grid.font_size * zoom).max(8.0)));
            let text_props = || CssPropertyWithConditionsVec::from_const_slice(CELL_GRID_TEXT_BASE);
            children.push(
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(EDITOR_CLASS))
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(p))
                    .with_children(DomVec::from_vec(alloc::vec![
                        crate::widgets::widget_p_with_text(AzString::from(before))
                            .with_css_props(text_props()),
                        Dom::create_div()
                            .with_ids_and_classes(IdOrClassVec::from_const_slice(CARET_CLASS))
                            .with_css_props(CssPropertyWithConditionsVec::from_vec(caret_props)),
                        crate::widgets::widget_p_with_text(AzString::from(after))
                            .with_css_props(text_props()),
                    ])),
            );
        }
    }

    // The grid: one focus stop; its value names the cell cursor's cell so a
    // screen reader reads where it went.
    let mut classes: Vec<IdOrClass> = GRID_CLASS.to_vec();
    if let Some(marker) = look.marker {
        classes.push(Class(AzString::from_const_str(marker)));
    }
    let mut grid_props = part(CELL_GRID_BASE, &look.grid);
    grid_props.push(simple(CssProperty::const_font_size(StyleFontSize::px(
        grid.font_size * zoom,
    ))));
    let active_value = AzString::from(alloc::format!(
        "{}",
        CellGrid::cell_label(view.active).as_str()
    ));
    let a11y = AccessibilityInfo {
        accessibility_value: Some(active_value).into(),
        ..AccessibilityInfo::named(grid.accessibility_name.clone(), AccessibilityRole::Grid)
    };
    let id = grid.id.clone();
    let shared = RefAny::new(GridShared { grid, geo });
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_id(id)
        .with_css_props(CssPropertyWithConditionsVec::from_vec(grid_props))
        .with_tab_index(azul_core::dom::TabIndex::Auto)
        .with_accessibility_info(a11y)
        .with_callbacks(grid_callbacks(&shared).into())
        .with_children(DomVec::from_vec(children))
}

/// The fill handle's centre, when the grid shows one: at the bottom-right
/// corner of the current range, if that corner is in view and nothing is
/// being edited.
pub(crate) fn fill_handle_at(grid: &CellGrid, geo: &Geometry) -> Option<(f32, f32)> {
    if !grid.fill_handle || grid.read_only || grid.view.is_editing() {
        return None;
    }
    let current = grid.view.current_range();
    let corner = range_rect(geo, &CellGridRange::create(current.last))?;
    Some((corner.0 + corner.2, corner.1 + corner.3))
}

// ---- the handlers: one set on the grid node, the grid hit-tests itself ----

/// What every handler of one grid build shares: the grid (its view, sizes
/// and callbacks) and where its rows and columns sit. A drag in progress
/// updates the view here too, so the next move compares against it before
/// the app's rebuild arrives.
#[derive(Debug)]
pub(crate) struct GridShared {
    pub grid: CellGrid,
    pub geo: Geometry,
}

/// The grid node's handlers.
pub(crate) fn grid_callbacks(shared: &RefAny) -> Vec<CoreCallbackData> {
    alloc::vec![
        CoreCallbackData::create(
            EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
            shared.clone(),
            on_grid_key as usize
        ),
        CoreCallbackData::create(
            EventFilter::Focus(FocusEventFilter::TextInput),
            shared.clone(),
            on_grid_text as usize
        ),
        CoreCallbackData::create(EventFilter::Focus(FocusEventFilter::Copy), shared.clone(), on_grid_copy as usize),
        CoreCallbackData::create(EventFilter::Focus(FocusEventFilter::Cut), shared.clone(), on_grid_cut as usize),
        CoreCallbackData::create(EventFilter::Focus(FocusEventFilter::Paste), shared.clone(), on_grid_paste as usize),
        CoreCallbackData::create(
            EventFilter::Hover(HoverEventFilter::LeftMouseDown),
            shared.clone(),
            on_grid_mouse_down as usize
        ),
        CoreCallbackData::create(
            EventFilter::Hover(HoverEventFilter::MouseMove),
            shared.clone(),
            on_grid_mouse_move as usize
        ),
        CoreCallbackData::create(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            shared.clone(),
            on_grid_mouse_up as usize
        ),
        CoreCallbackData::create(
            EventFilter::Hover(HoverEventFilter::DoubleClick),
            shared.clone(),
            on_grid_double_click as usize
        ),
        CoreCallbackData::create(
            EventFilter::Hover(HoverEventFilter::Scroll),
            shared.clone(),
            on_grid_wheel as usize
        ),
    ]
}

/// A copy of the grid and its geometry from the handler's payload.
fn shared_of(data: &mut RefAny) -> Option<(CellGrid, Geometry)> {
    let s = data.downcast_ref::<GridShared>()?;
    Some((s.grid.clone(), s.geo.clone()))
}

/// Records `view` as the grid's view in the payload (a drag's progress).
fn store_view(data: &mut RefAny, view: &CellGridView) {
    if let Some(mut s) = data.downcast_mut::<GridShared>() {
        s.grid.view = view.clone();
    }
}

fn bounds_of<'a>(grid: &'a CellGrid, geo: &Geometry) -> Bounds<'a> {
    Bounds {
        row_count: grid.row_count,
        column_count: grid.column_count,
        content_rows: grid.content_rows,
        content_columns: grid.content_columns,
        frozen_rows: grid.frozen_rows,
        frozen_columns: grid.frozen_columns,
        column_widths: grid.column_widths.as_ref(),
        row_heights: grid.row_heights.as_ref(),
        page_rows: geo.page_rows,
        page_columns: geo.page_columns,
    }
}

/// Hands `event` to the app.
fn fire(grid: &CellGrid, info: CallbackInfo, event: CellGridEvent) -> Update {
    match grid.on_event.as_ref() {
        Some(CellGridOnEvent { refany, callback }) => callback.invoke(refany.clone(), info, event),
        None => Update::DoNothing,
    }
}

/// Whether `cell` holds data (what Ctrl + arrow stops at).
fn has_data(grid: &CellGrid, cell: CellGridCellRef) -> bool {
    let c = cell_content(&grid.data_source, cell);
    c.kind != CellGridCellKind::Empty || !c.text.as_str().is_empty()
}

fn zoom_of(grid: &CellGrid) -> f32 {
    if grid.zoom.is_finite() && grid.zoom > 0.0 {
        grid.zoom
    } else {
        1.0
    }
}

/// The view with no edit.
fn without_edit(view: &CellGridView) -> CellGridView {
    let mut v = view.clone();
    v.edit_mode = CellGridEditMode::None;
    v.edit_text = AzString::from_const_str("");
    v.edit_cursor = 0;
    v
}

/// Commits the edit and puts the cursor on `to`.
fn commit_to(grid: &CellGrid, b: &Bounds<'_>, to: CellGridCellRef) -> CellGridEvent {
    let view = &grid.view;
    let mut next = select(&without_edit(view), to, false, false);
    reveal(&mut next, b, to);
    let mut e = CellGridEvent::create(CellGridEventKind::EditCommit, next);
    e.text = view.edit_text.clone();
    e.range = CellGridRange::create(view.active);
    e
}

/// The view editing `cell` in `mode`, the edit starting as `text`.
fn start_edit(view: &CellGridView, cell: CellGridCellRef, mode: CellGridEditMode, text: &str) -> CellGridView {
    let mut next = if cell == view.active {
        view.clone()
    } else {
        select(view, cell, false, false)
    };
    next.edit_mode = mode;
    next.edit_text = AzString::from(String::from(text));
    next.edit_cursor = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
    next
}

/// A key while the active cell is edited.
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn edit_key(
    grid: &CellGrid,
    b: &Bounds<'_>,
    key: VirtualKeyCode,
    shift: bool,
) -> Option<CellGridEvent> {
    use VirtualKeyCode as K;
    let view = &grid.view;
    let mut chars: Vec<char> = view.edit_text.as_str().chars().collect();
    let caret = (view.edit_cursor as usize).min(chars.len());
    let edited = |chars: &[char], caret: usize| {
        let mut next = view.clone();
        next.edit_text = AzString::from(chars.iter().collect::<String>());
        next.edit_cursor = caret as u32;
        CellGridEvent::create(CellGridEventKind::EditText, next)
    };
    let enter_mode = view.edit_mode == CellGridEditMode::Enter;
    let moved = |dir: Dir| commit_to(grid, b, b.step(view.active, dir));
    Some(match key {
        K::Escape => CellGridEvent::create(CellGridEventKind::EditCancel, without_edit(view)),
        K::Return | K::NumpadEnter => moved(if shift { Dir::Up } else { Dir::Down }),
        K::Tab => moved(if shift { Dir::Left } else { Dir::Right }),
        K::Up if enter_mode => moved(Dir::Up),
        K::Down if enter_mode => moved(Dir::Down),
        K::Left if enter_mode => moved(Dir::Left),
        K::Right if enter_mode => moved(Dir::Right),
        K::Left => edited(&chars, caret.saturating_sub(1)),
        K::Right => edited(&chars, (caret + 1).min(chars.len())),
        K::Home => edited(&chars, 0),
        K::End => edited(&chars, chars.len()),
        K::Back => {
            if caret == 0 {
                return Some(edited(&chars, 0));
            }
            chars.remove(caret - 1);
            edited(&chars, caret - 1)
        }
        K::Delete => {
            if caret < chars.len() {
                chars.remove(caret);
            }
            edited(&chars, caret)
        }
        _ => return None,
    })
}

/// A key on the grid, nothing being edited.
pub(crate) fn grid_key(
    grid: &CellGrid,
    b: &Bounds<'_>,
    key: VirtualKeyCode,
    shift: bool,
    ctrl: bool,
) -> Option<CellGridEvent> {
    use VirtualKeyCode as K;
    let view = &grid.view;
    match key {
        K::F2 if !grid.read_only => Some(CellGridEvent::create(
            CellGridEventKind::EditStart,
            start_edit(view, view.active, CellGridEditMode::Edit, ""),
        )),
        K::Back if !grid.read_only => Some(CellGridEvent::create(
            CellGridEventKind::EditStart,
            start_edit(view, view.active, CellGridEditMode::Enter, ""),
        )),
        K::Delete if !grid.read_only => {
            let mut e = CellGridEvent::create(CellGridEventKind::Delete, view.clone());
            e.range = view.current_range();
            Some(e)
        }
        K::Escape if view.drag.kind != CellGridDragKind::None => {
            let mut next = view.clone();
            next.drag = CellGridDrag::default();
            Some(CellGridEvent::create(CellGridEventKind::Drag, next))
        }
        _ => {
            let navigated = navigate(view, b, key, shift, ctrl, &mut |c| has_data(grid, c))?;
            let mut e = CellGridEvent::create(navigated.kind, navigated.view);
            e.shift = shift;
            e.ctrl = ctrl;
            Some(e)
        }
    }
}

/// The keys (see the module's KEYBOARD).
extern "C" fn on_grid_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((grid, geo)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    let ks = info.get_current_keyboard_state();
    let Some(key) = ks.current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    if ks.alt_down() {
        return Update::DoNothing;
    }
    let shift = ks.shift_down();
    let ctrl = ks.primary_down();
    let b = bounds_of(&grid, &geo);
    let event = if grid.view.is_editing() {
        edit_key(&grid, &b, key, shift)
    } else {
        grid_key(&grid, &b, key, shift, ctrl)
    };
    match event {
        Some(event) => {
            // The key is the grid's: no spatial navigation, no scrolling.
            info.prevent_default();
            store_view(&mut data, &event.view);
            fire(&grid, info, event)
        }
        None => Update::DoNothing,
    }
}

/// The text a typed key inserts at `caret` of `view`'s edit (or the start
/// of an edit that replaces the cell).
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn typed(view: &CellGridView, text: &str) -> CellGridEvent {
    if view.is_editing() {
        let mut chars: Vec<char> = view.edit_text.as_str().chars().collect();
        let caret = (view.edit_cursor as usize).min(chars.len());
        let insert: Vec<char> = text.chars().collect();
        let n = insert.len();
        for (i, ch) in insert.into_iter().enumerate() {
            chars.insert(caret + i, ch);
        }
        let mut next = view.clone();
        next.edit_text = AzString::from(chars.iter().collect::<String>());
        next.edit_cursor = (caret + n) as u32;
        CellGridEvent::create(CellGridEventKind::EditText, next)
    } else {
        CellGridEvent::create(
            CellGridEventKind::EditStart,
            start_edit(view, view.active, CellGridEditMode::Enter, text),
        )
    }
}

/// A character typed on the grid: it starts an edit that replaces the
/// cell, or goes into the edit at the caret. The grid's node holds no text
/// of its own, so the engine's own insertion is cancelled.
extern "C" fn on_grid_text(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((grid, _)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    let Some(inserted) = info
        .get_text_changeset()
        .map(|c| String::from(c.inserted_text.as_str()))
    else {
        return Update::DoNothing;
    };
    info.prevent_default();
    let text: String = inserted.chars().filter(|c| !c.is_control()).collect();
    if text.is_empty() || grid.read_only {
        return Update::DoNothing;
    }
    let event = typed(&grid.view, &text);
    store_view(&mut data, &event.view);
    fire(&grid, info, event)
}

/// The current range's cells as text rows, a range of whole columns or rows
/// clipped to the data (a copy of column A does not copy a million rows).
fn selection_rows(grid: &CellGrid) -> (CellGridRange, Vec<Vec<String>>) {
    let mut r = grid.view.current_range();
    if r.row_count() > 10_000 {
        r.last.row = r.last.row.min(r.first.row.max(grid.content_rows.saturating_sub(1)));
    }
    if r.column_count() > 1_000 {
        r.last.column = r
            .last
            .column
            .min(r.first.column.max(grid.content_columns.saturating_sub(1)));
    }
    let rows = (r.first.row..=r.last.row)
        .map(|row| {
            (r.first.column..=r.last.column)
                .map(|column| {
                    String::from(
                        cell_content(&grid.data_source, CellGridCellRef::create(row, column))
                            .text
                            .as_str(),
                    )
                })
                .collect()
        })
        .collect();
    (r, rows)
}

/// Ctrl+C / Ctrl+X: the current range as tab-separated text and as an HTML
/// table onto the clipboard, then the app hears it.
fn copy_selection(mut data: RefAny, mut info: CallbackInfo, kind: CellGridEventKind) -> Update {
    let Some((grid, _)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    if grid.view.is_editing() {
        return Update::DoNothing;
    }
    let (range, rows) = selection_rows(&grid);
    info.set_clipboard_content(crate::managers::selection::ClipboardContent {
        plain_text: AzString::from(cells_to_tsv(&rows)),
        styled_runs: crate::managers::selection::StyledTextRunVec::from_const_slice(&[]),
        html: Some(AzString::from(cells_to_html(&rows))).into(),
    });
    let mut e = CellGridEvent::create(kind, grid.view.clone());
    e.range = range;
    fire(&grid, info, e)
}

extern "C" fn on_grid_copy(data: RefAny, info: CallbackInfo) -> Update {
    copy_selection(data, info, CellGridEventKind::Copy)
}

extern "C" fn on_grid_cut(data: RefAny, info: CallbackInfo) -> Update {
    copy_selection(data, info, CellGridEventKind::Cut)
}

/// Ctrl+V: the clipboard's text to the app, for the active cell.
extern "C" fn on_grid_paste(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((grid, _)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    let text = info
        .get_clipboard_content()
        .map(|c| c.plain_text.clone());
    info.prevent_default();
    let Some(text) = text else {
        return Update::DoNothing;
    };
    if grid.read_only {
        return Update::DoNothing;
    }
    if grid.view.is_editing() {
        let event = typed(&grid.view, text.as_str());
        store_view(&mut data, &event.view);
        return fire(&grid, info, event);
    }
    let mut e = CellGridEvent::create(CellGridEventKind::Paste, grid.view.clone());
    e.text = text;
    e.range = CellGridRange::create(grid.view.active);
    fire(&grid, info, e)
}

/// The pointer's position over the grid node.
fn cursor_in(info: &CallbackInfo) -> Option<(f32, f32)> {
    info.get_cursor_relative_to_node()
        .into_option()
        .map(|p| (p.x, p.y))
        .filter(|(x, y)| x.is_finite() && y.is_finite())
}

/// What a press at `hit` does to the view (the pure half of the handler);
/// `window_px` is the pointer's window position for a resize grip.
#[allow(clippy::too_many_lines)]
pub(crate) fn press(
    grid: &CellGrid,
    geo: &Geometry,
    hit: Hit,
    shift: bool,
    ctrl: bool,
    window_px: (f32, f32),
) -> Option<CellGridEvent> {
    let view = &grid.view;
    let b = bounds_of(grid, geo);
    let whole = |first: CellGridCellRef, last: CellGridCellRef, active: CellGridCellRef| {
        let range = CellGridRange::spanning(first, last);
        let mut next = view.clone();
        let mut ranges = if ctrl {
            view.ranges.as_ref().to_vec()
        } else {
            Vec::new()
        };
        ranges.push(range);
        next.ranges = CellGridRangeVec::from_vec(ranges);
        next.active = active;
        next.anchor = active;
        CellGridEvent::create(CellGridEventKind::Select, next)
    };
    let last_row = grid.row_count.saturating_sub(1);
    let last_column = grid.column_count.saturating_sub(1);
    let event = match hit {
        Hit::Nothing => return None,
        Hit::Corner => {
            let mut next = view.clone();
            next.ranges = CellGridRangeVec::from_vec(alloc::vec![CellGridRange::spanning(
                CellGridCellRef::create(0, 0),
                CellGridCellRef::create(last_row, last_column),
            )]);
            CellGridEvent::create(CellGridEventKind::Select, next)
        }
        Hit::ColumnHeader(c) => {
            let from = if shift { view.anchor.column } else { c };
            whole(
                CellGridCellRef::create(0, from),
                CellGridCellRef::create(last_row, c),
                CellGridCellRef::create(view.top_row.min(last_row), from),
            )
        }
        Hit::RowHeader(r) => {
            let from = if shift { view.anchor.row } else { r };
            whole(
                CellGridCellRef::create(from, 0),
                CellGridCellRef::create(r, last_column),
                CellGridCellRef::create(from, view.left_column.min(last_column)),
            )
        }
        Hit::ColumnEdge(c) => {
            let size = size_at(grid.column_widths.as_ref(), c, grid.default_column_width);
            let mut next = view.clone();
            next.drag = CellGridDrag {
                start_px: window_px.0,
                start_size: size,
                size,
                index: c,
                kind: CellGridDragKind::ResizeColumn,
                ..CellGridDrag::default()
            };
            CellGridEvent::create(CellGridEventKind::Drag, next)
        }
        Hit::RowEdge(r) => {
            let size = size_at(grid.row_heights.as_ref(), r, grid.default_row_height);
            let mut next = view.clone();
            next.drag = CellGridDrag {
                start_px: window_px.1,
                start_size: size,
                size,
                index: r,
                kind: CellGridDragKind::ResizeRow,
                ..CellGridDrag::default()
            };
            CellGridEvent::create(CellGridEventKind::Drag, next)
        }
        Hit::FillHandle => {
            let corner = view.current_range().last;
            let mut next = view.clone();
            next.drag = CellGridDrag {
                origin: corner,
                target: corner,
                kind: CellGridDragKind::Fill,
                ..CellGridDrag::default()
            };
            CellGridEvent::create(CellGridEventKind::Drag, next)
        }
        Hit::Cell(cell) => {
            if view.is_editing() {
                if cell == view.active {
                    return None;
                }
                return Some(commit_to(grid, &b, cell));
            }
            let mut next = select(view, cell, shift, ctrl);
            next.drag = CellGridDrag {
                origin: if shift { view.anchor } else { cell },
                target: cell,
                kind: CellGridDragKind::Select,
                ..CellGridDrag::default()
            };
            let mut e = CellGridEvent::create(CellGridEventKind::Select, next);
            e.shift = shift;
            e.ctrl = ctrl;
            e
        }
    };
    Some(event)
}

/// A press: select, start a range drag, a resize or a fill. The grid takes
/// the pointer until the button is up, so a drag that leaves it goes on.
extern "C" fn on_grid_mouse_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((grid, geo)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    let Some((x, y)) = cursor_in(&info) else {
        return Update::DoNothing;
    };
    let ks = info.get_current_keyboard_state();
    let (shift, ctrl) = (ks.shift_down(), ks.primary_down());
    let window_px = info
        .get_cursor_position()
        .map_or((x, y), |p| (p.x, p.y));
    let hit = hit_test(&geo, fill_handle_at(&grid, &geo), x, y);
    let Some(event) = press(&grid, &geo, hit, shift, ctrl, window_px) else {
        return Update::DoNothing;
    };
    if event.view.drag.kind != CellGridDragKind::None {
        let node = info.get_hit_node();
        info.capture_pointer(node);
    }
    store_view(&mut data, &event.view);
    fire(&grid, info, event)
}

/// What a pointer move during a drag does (the pure half): `cell` is the
/// cell under the pointer, `window_px` its window position.
pub(crate) fn drag_move(
    grid: &CellGrid,
    cell: Option<CellGridCellRef>,
    window_px: (f32, f32),
) -> Option<CellGridEvent> {
    let view = &grid.view;
    let drag = view.drag;
    let zoom = zoom_of(grid);
    match drag.kind {
        CellGridDragKind::None => None,
        CellGridDragKind::Select => {
            let cell = cell?;
            if cell == drag.target {
                return None;
            }
            let mut base = view.clone();
            base.anchor = drag.origin;
            let mut next = select(&base, cell, true, false);
            next.drag.target = cell;
            Some(CellGridEvent::create(CellGridEventKind::Select, next))
        }
        CellGridDragKind::Fill => {
            let cell = cell?;
            if cell == drag.target {
                return None;
            }
            let mut next = view.clone();
            next.drag.target = cell;
            Some(CellGridEvent::create(CellGridEventKind::Drag, next))
        }
        CellGridDragKind::Point => None,
        CellGridDragKind::ResizeColumn | CellGridDragKind::ResizeRow => {
            let at = if drag.kind == CellGridDragKind::ResizeColumn {
                window_px.0
            } else {
                window_px.1
            };
            let size = (drag.start_size + (at - drag.start_px) / zoom).max(MIN_RESIZE_PX);
            if !size.is_finite() || (size - drag.size).abs() < 0.5 {
                return None;
            }
            let mut next = view.clone();
            next.drag.size = size;
            Some(CellGridEvent::create(CellGridEventKind::Drag, next))
        }
    }
}

/// A move while a drag is in progress.
extern "C" fn on_grid_mouse_move(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((grid, geo)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    if grid.view.drag.kind == CellGridDragKind::None {
        return Update::DoNothing;
    }
    let pos = cursor_in(&info);
    let cell = pos.and_then(|(x, y)| nearest_cell(&geo, x, y));
    let window_px = info
        .get_cursor_position()
        .map(|p| (p.x, p.y))
        .or(pos)
        .unwrap_or((0.0, 0.0));
    let Some(event) = drag_move(&grid, cell, window_px) else {
        return Update::DoNothing;
    };
    store_view(&mut data, &event.view);
    fire(&grid, info, event)
}

/// What the release of a drag does (the pure half).
pub(crate) fn drag_end(grid: &CellGrid) -> Option<CellGridEvent> {
    let view = &grid.view;
    let drag = view.drag;
    let mut next = view.clone();
    next.drag = CellGridDrag::default();
    let event = match drag.kind {
        CellGridDragKind::None => return None,
        CellGridDragKind::Select | CellGridDragKind::Point => {
            CellGridEvent::create(CellGridEventKind::Drag, next)
        }
        CellGridDragKind::Fill => {
            let source = view.current_range();
            let reach = fill_range(source, drag.target);
            if reach == source {
                CellGridEvent::create(CellGridEventKind::Drag, next)
            } else {
                next.ranges = CellGridRangeVec::from_vec(alloc::vec![reach]);
                let mut e = CellGridEvent::create(CellGridEventKind::Fill, next);
                e.range = reach;
                e
            }
        }
        CellGridDragKind::ResizeColumn | CellGridDragKind::ResizeRow => {
            let kind = if drag.kind == CellGridDragKind::ResizeColumn {
                CellGridEventKind::ResizeColumn
            } else {
                CellGridEventKind::ResizeRow
            };
            let mut e = CellGridEvent::create(kind, next);
            e.index = drag.index;
            e.size = drag.size;
            e
        }
    };
    Some(event)
}

/// The release ends a drag: a fill fills, a resize resizes.
extern "C" fn on_grid_mouse_up(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((grid, _)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    let Some(event) = drag_end(&grid) else {
        return Update::DoNothing;
    };
    store_view(&mut data, &event.view);
    fire(&grid, info, event)
}

/// A double-click edits a cell; on a column's edge it fits the column.
extern "C" fn on_grid_double_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((grid, geo)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    let Some((x, y)) = cursor_in(&info) else {
        return Update::DoNothing;
    };
    let event = match hit_test(&geo, None, x, y) {
        Hit::ColumnEdge(c) => {
            let mut e = CellGridEvent::create(CellGridEventKind::AutoFitColumn, grid.view.clone());
            e.index = c;
            e
        }
        Hit::Cell(cell) if !grid.read_only => CellGridEvent::create(
            CellGridEventKind::EditStart,
            start_edit(&without_edit(&grid.view), cell, CellGridEditMode::Edit, ""),
        ),
        _ => return Update::DoNothing,
    };
    store_view(&mut data, &event.view);
    fire(&grid, info, event)
}

/// The wheel scrolls the grid by whole rows (Shift: columns). The grid IS
/// the scroll surface, so the page under it does not scroll as well.
extern "C" fn on_grid_wheel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((grid, geo)) = shared_of(&mut data) else {
        return Update::DoNothing;
    };
    let hit = info.get_hit_node();
    let Some(node_id) = hit.node.into_crate_internal() else {
        return Update::DoNothing;
    };
    let Some(delta) = info.get_scroll_delta(hit.dom, node_id) else {
        return Update::DoNothing;
    };
    // THE WHEEL HAS ONE CONSUMER (see the time picker): `stop_propagation`
    // keeps it from other callbacks, `prevent_default` cancels the scroll of
    // the box around the grid.
    info.prevent_default();
    info.stop_propagation();
    let shift = info.get_current_keyboard_state().shift_down();
    let (dx, dy) = if shift && delta.x.abs() < f32::EPSILON {
        (delta.y, 0.0)
    } else {
        (delta.x, delta.y)
    };
    let zoom = zoom_of(&grid);
    let (rows, columns) = take_wheel(
        dx,
        dy,
        WHEEL_PX_PER_ROW * zoom,
        grid.default_column_width.max(1.0) * zoom,
    );
    if rows == 0 && columns == 0 {
        return Update::DoNothing;
    }
    let b = bounds_of(&grid, &geo);
    let next = scroll_by(&grid.view, &b, rows, columns);
    if next == grid.view {
        return Update::DoNothing;
    }
    store_view(&mut data, &next);
    fire(
        &grid,
        info,
        CellGridEvent::create(CellGridEventKind::Scroll, next),
    )
}

#[cfg(test)]
pub(crate) mod fixtures {
    //! A small grid with data, for the widget's own tests and the lint
    //! manifest (`widgets::label_convention::every_widget_dom`).
    use super::*;

    /// A 5 x 3 block of numbers (row * 10 + column) at A1, the rest empty.
    pub(crate) extern "C" fn numbers(_: RefAny, cell: CellGridCellRef) -> CellGridCell {
        if cell.row < 5 && cell.column < 3 {
            CellGridCell::create(
                AzString::from(alloc::format!("{}", cell.row * 10 + cell.column)),
                CellGridCellKind::Number,
            )
        } else {
            CellGridCell::empty()
        }
    }

    /// Row 1 bold, the rest plain. (No app colours here: the fixture is
    /// also the lint manifest's, whose theme checks are about the grid's
    /// own looks; a user fill is a plain colour by design.)
    pub(crate) extern "C" fn looks(_: RefAny, cell: CellGridCellRef) -> CellGridCellStyle {
        CellGridCellStyle {
            bold: cell.row == 0,
            ..CellGridCellStyle::default()
        }
    }

    /// A 1000 x 50 grid, 400 x 200 px (6 columns and 9 rows in view), the
    /// block above as its data.
    pub(crate) fn small() -> CellGrid {
        CellGrid::create(1000, 50)
            .with_viewport(400.0, 200.0)
            .with_data_source(RefAny::new(()), numbers as CellGridDataSourceCallbackType)
            .with_style_source(RefAny::new(()), looks as CellGridStyleSourceCallbackType)
            .with_content_extent(5, 3)
            .with_accessibility_name(AzString::from_const_str("Sheet1"))
    }
}

#[cfg(test)]
mod cell_grid_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId, NodeType},
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::{fixtures::small, *};
    use crate::widgets::{
        roving::test_support as rv,
        themes::{theme_blocks::checks, theme_checks, UiTheme},
    };

    type Log = Arc<Mutex<Vec<CellGridEvent>>>;

    extern "C" fn record(mut data: RefAny, _: CallbackInfo, event: CellGridEvent) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(event);
        }
        Update::RefreshDom
    }

    fn grid(log: &Log) -> CellGrid {
        small().with_on_event(RefAny::new(log.clone()), record as CellGridOnEventCallbackType)
    }

    fn at(row: u32, column: u32) -> CellGridCellRef {
        CellGridCellRef::create(row, column)
    }

    fn id(n: NodeId) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(n)),
        }
    }

    fn nodes_with(styled: &StyledDom, class: &str) -> Vec<NodeId> {
        styled
            .node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter(|(_, nd)| {
                nd.get_ids_and_classes()
                    .as_ref()
                    .iter()
                    .any(|c| matches!(c, Class(s) if s.as_str() == class))
            })
            .map(|(i, _)| NodeId::new(i))
            .collect()
    }

    fn texts(node: &Dom, out: &mut Vec<String>) {
        if let NodeType::Text(s) = node.root.get_node_type() {
            if !s.as_ref().as_str().is_empty() {
                out.push(String::from(s.as_ref().as_str()));
            }
        }
        for c in node.children.as_ref() {
            texts(c, out);
        }
    }

    fn bounds(g: &CellGrid) -> (Geometry, CellGrid) {
        (geometry(g), g.clone())
    }

    fn indices(bands: &[Band]) -> Vec<u32> {
        bands.iter().map(|b| b.index).collect()
    }

    #[test]
    fn only_the_rows_and_columns_in_view_are_built() {
        let g = small();
        let geo = geometry(&g);
        assert_eq!(indices(&geo.rows), (0..9).collect::<Vec<_>>(), "200 px: header + 9 rows");
        assert_eq!(indices(&geo.columns), (0..6).collect::<Vec<_>>(), "400 px: header + 6 columns");
        assert_eq!(geo.page_rows, 9);
        assert_eq!(geo.page_columns, 5, "the sixth column straddles the edge");

        let styled = StyledDom::create_from_dom(small().with_theme(UiTheme::Flat).dom());
        assert_eq!(nodes_with(&styled, CELL_CLASS_NAME).len(), 9 * 6, "only the window is in the DOM");

        let scrolled = small().with_view(CellGridView::create().with_scroll(500, 20));
        let geo = geometry(&scrolled);
        assert_eq!(geo.rows.first().map(|b| b.index), Some(500));
        assert_eq!(geo.columns.first().map(|b| b.index), Some(20));
        let mut seen = Vec::new();
        texts(&scrolled.with_theme(UiTheme::Flat).dom(), &mut seen);
        assert!(seen.iter().any(|t| t == "501"), "row numbers follow the window: {seen:?}");
        assert!(seen.iter().any(|t| t == "U"), "column letters follow the window: {seen:?}");
    }

    #[test]
    fn frozen_rows_and_columns_stay_ahead_of_the_scrolled_window() {
        let g = small()
            .with_frozen(2, 1)
            .with_view(CellGridView::create().with_scroll(100, 10));
        let geo = geometry(&g);
        let rows = indices(&geo.rows);
        assert_eq!(&rows[..3], &[0, 1, 100], "the frozen rows, then the window");
        assert_eq!(geo.frozen_rows, 2);
        let columns = indices(&geo.columns);
        assert_eq!(&columns[..2], &[0, 10]);
        assert!(
            geo.rows[2].start >= geo.rows[1].end() + FREEZE_LINE_PX - 0.01,
            "the freeze line sits between them"
        );
        let styled = StyledDom::create_from_dom(g.with_theme(UiTheme::Flat).dom());
        assert!(
            !nodes_with(&styled, FREEZE_CLASS_NAME).is_empty(),
            "the freeze lines are drawn"
        );

        // A view that scrolled into the frozen rows is held below them.
        let held = small()
            .with_frozen(3, 0)
            .with_view(CellGridView::create().with_scroll(1, 0));
        assert_eq!(&indices(&geometry(&held).rows)[..4], &[0, 1, 2, 3]);
    }

    #[test]
    fn hidden_rows_and_columns_are_left_out_of_the_window_and_the_cursor_skips_them() {
        let g = small()
            .with_row_heights(CellGridSizeVec::from_vec(vec![CellGridSize::create(2, 0.0)]))
            .with_column_widths(CellGridSizeVec::from_vec(vec![CellGridSize::create(1, 120.0)]));
        let geo = geometry(&g);
        assert_eq!(&indices(&geo.rows)[..3], &[0, 1, 3]);
        assert!((geo.columns[1].size - 120.0).abs() < 0.01, "a wider column");
        let (geo, g) = bounds(&g);
        let b = bounds_of(&g, &geo);
        assert_eq!(b.step(at(1, 0), Dir::Down), at(3, 0), "row 3 hides row 2's place");
    }

    #[test]
    fn a_point_hits_the_cell_the_header_or_the_resize_grip_under_it() {
        // Header 40 x 20 px, columns 64 px, rows 20 px.
        let geo = geometry(&small());
        assert_eq!(hit_test(&geo, None, 10.0, 10.0), Hit::Corner);
        assert_eq!(hit_test(&geo, None, 50.0, 30.0), Hit::Cell(at(0, 0)));
        assert_eq!(hit_test(&geo, None, 110.0, 45.0), Hit::Cell(at(1, 1)));
        assert_eq!(hit_test(&geo, None, 70.0, 10.0), Hit::ColumnHeader(0));
        assert_eq!(hit_test(&geo, None, 102.0, 10.0), Hit::ColumnEdge(0));
        assert_eq!(hit_test(&geo, None, 105.0, 10.0), Hit::ColumnEdge(0), "the grip reaches into the next header");
        assert_eq!(hit_test(&geo, None, 20.0, 30.0), Hit::RowHeader(0));
        assert_eq!(hit_test(&geo, None, 20.0, 38.0), Hit::RowEdge(0));
        assert_eq!(
            hit_test(&geo, Some((104.0, 40.0)), 105.0, 41.0),
            Hit::FillHandle,
            "the fill handle wins over the cell under it"
        );
        assert_eq!(nearest_cell(&geo, 9999.0, 9999.0), Some(at(8, 5)), "a drag past the edge keeps the last cell");
    }

    fn nav(g: &CellGrid, key: VirtualKeyCode, shift: bool, ctrl: bool) -> CellGridView {
        let geo = geometry(g);
        let b = bounds_of(g, &geo);
        navigate(&g.view, &b, key, shift, ctrl, &mut |c| has_data(g, c))
            .expect("a navigation key")
            .view
    }

    #[test]
    fn the_arrows_move_the_cursor_and_shift_extends_the_range_from_the_anchor() {
        let g = small();
        let v = nav(&g, VirtualKeyCode::Down, false, false);
        assert_eq!(v.active, at(1, 0));
        assert_eq!(v.ranges.as_ref(), &[CellGridRange::create(at(1, 0))]);

        let g = small().with_view(v);
        let v = nav(&g, VirtualKeyCode::Right, true, false);
        assert_eq!(v.active, at(1, 0), "the cursor stays on the anchor");
        assert_eq!(v.current_range(), CellGridRange::spanning(at(1, 0), at(1, 1)));
        let g = small().with_view(v);
        let v = nav(&g, VirtualKeyCode::Down, true, false);
        assert_eq!(v.current_range(), CellGridRange::spanning(at(1, 0), at(2, 1)), "the moving end goes on");

        let edge = small();
        assert_eq!(nav(&edge, VirtualKeyCode::Up, false, false).active, at(0, 0), "held at the edge");
    }

    #[test]
    fn ctrl_arrows_jump_to_the_edge_of_the_data() {
        let g = small();
        assert_eq!(nav(&g, VirtualKeyCode::Down, false, true).active, at(4, 0), "the end of the run");
        let from_end = small().with_view(CellGridView::create().with_active(at(4, 0)));
        assert_eq!(
            nav(&from_end, VirtualKeyCode::Down, false, true).active,
            at(999, 0),
            "no data below: the sheet's last row"
        );
        let below = small().with_view(CellGridView::create().with_active(at(10, 0)));
        assert_eq!(nav(&below, VirtualKeyCode::Up, false, true).active, at(4, 0), "the next data up");
        assert_eq!(nav(&g, VirtualKeyCode::Right, false, true).active, at(0, 2));
        let extended = nav(&g, VirtualKeyCode::Down, true, true);
        assert_eq!(extended.current_range(), CellGridRange::spanning(at(0, 0), at(4, 0)), "Ctrl+Shift extends to it");
    }

    #[test]
    fn home_end_page_keys_select_all_and_whole_rows_and_columns() {
        let g = small().with_view(CellGridView::create().with_active(at(3, 4)));
        assert_eq!(nav(&g, VirtualKeyCode::Home, false, false).active, at(3, 0));
        assert_eq!(nav(&g, VirtualKeyCode::Home, false, true).active, at(0, 0));
        assert_eq!(nav(&g, VirtualKeyCode::End, false, true).active, at(4, 2), "the last cell with data");

        let paged = nav(&small(), VirtualKeyCode::PageDown, false, false);
        assert_eq!(paged.active, at(9, 0), "a screen of 9 rows");
        assert_eq!(paged.top_row, 9, "the window moves with it");

        let all = nav(&g, VirtualKeyCode::A, false, true);
        assert_eq!(
            all.current_range(),
            CellGridRange::spanning(at(0, 0), at(999, 49))
        );
        let column = nav(&g, VirtualKeyCode::Space, false, true);
        assert_eq!(column.current_range(), CellGridRange::spanning(at(0, 4), at(999, 4)));
        let row = nav(&g, VirtualKeyCode::Space, true, false);
        assert_eq!(row.current_range(), CellGridRange::spanning(at(3, 0), at(3, 49)));
    }

    #[test]
    fn the_cursor_scrolls_the_window_to_stay_in_view() {
        let g = small().with_view(CellGridView::create().with_active(at(8, 0)));
        let v = nav(&g, VirtualKeyCode::Down, false, false);
        assert_eq!(v.active, at(9, 0));
        assert_eq!(v.top_row, 1, "one row up, no more");
        let g = small().with_view(CellGridView::create().with_active(at(20, 0)).with_scroll(20, 0));
        let v = nav(&g, VirtualKeyCode::Up, false, false);
        assert_eq!(v.top_row, 19);
    }

    #[test]
    fn typing_starts_an_edit_that_replaces_the_cell_and_enter_commits_and_moves_down() {
        let g = small();
        let start = typed(&g.view, "4");
        assert_eq!(start.kind, CellGridEventKind::EditStart);
        assert_eq!(start.view.edit_mode, CellGridEditMode::Enter);
        assert_eq!(start.view.edit_text.as_str(), "4");
        assert_eq!(start.view.edit_cursor, 1);
        let more = typed(&start.view, "2");
        assert_eq!(more.kind, CellGridEventKind::EditText);
        assert_eq!(more.view.edit_text.as_str(), "42");

        let g = small().with_view(more.view);
        let geo = geometry(&g);
        let b = bounds_of(&g, &geo);
        let done = edit_key(&g, &b, VirtualKeyCode::Return, false).expect("Enter commits");
        assert_eq!(done.kind, CellGridEventKind::EditCommit);
        assert_eq!(done.text.as_str(), "42");
        assert_eq!(done.range.first, at(0, 0), "the cell that was edited");
        assert_eq!(done.view.active, at(1, 0), "then the cursor moves down");
        assert!(!done.view.is_editing());

        let tab = edit_key(&g, &b, VirtualKeyCode::Tab, false).expect("Tab commits");
        assert_eq!(tab.view.active, at(0, 1), "Tab moves right");
        let arrow = edit_key(&g, &b, VirtualKeyCode::Right, false).expect("an arrow commits in Enter mode");
        assert_eq!(arrow.kind, CellGridEventKind::EditCommit);
    }

    #[test]
    fn f2_edits_in_place_where_the_arrows_move_the_caret_and_escape_cancels() {
        let g = small();
        let geo = geometry(&g);
        let b = bounds_of(&g, &geo);
        let start = grid_key(&g, &b, VirtualKeyCode::F2, false, false).expect("F2 edits");
        assert_eq!(start.kind, CellGridEventKind::EditStart);
        assert_eq!(start.view.edit_mode, CellGridEditMode::Edit);
        assert_eq!(start.view.edit_text.as_str(), "", "the app puts the cell's content in");

        let mut view = start.view;
        view.edit_text = AzString::from_const_str("=A1+1");
        view.edit_cursor = 5;
        let g = small().with_view(view);
        let left = edit_key(&g, &b, VirtualKeyCode::Left, false).expect("Left moves the caret");
        assert_eq!(left.kind, CellGridEventKind::EditText);
        assert_eq!(left.view.edit_cursor, 4);
        let g2 = small().with_view(left.view);
        let back = edit_key(&g2, &b, VirtualKeyCode::Back, false).expect("Backspace deletes");
        assert_eq!(back.view.edit_text.as_str(), "=A11");
        assert_eq!(back.view.edit_cursor, 3);
        let inserted = typed(&back.view, "*");
        assert_eq!(inserted.view.edit_text.as_str(), "=A1*1");

        let cancel = edit_key(&g, &b, VirtualKeyCode::Escape, false).expect("Escape cancels");
        assert_eq!(cancel.kind, CellGridEventKind::EditCancel);
        assert!(!cancel.view.is_editing());
        assert_eq!(cancel.view.active, at(0, 0), "the cursor stays");
    }

    #[test]
    fn delete_clears_the_selection_and_a_read_only_grid_never_edits() {
        let g = small();
        let geo = geometry(&g);
        let b = bounds_of(&g, &geo);
        let del = grid_key(&g, &b, VirtualKeyCode::Delete, false, false).expect("Delete");
        assert_eq!(del.kind, CellGridEventKind::Delete);
        let ro = small().with_read_only(true);
        assert!(grid_key(&ro, &b, VirtualKeyCode::F2, false, false)
            .map_or(true, |e| e.kind != CellGridEventKind::EditStart));
        assert!(grid_key(&ro, &b, VirtualKeyCode::Delete, false, false)
            .map_or(true, |e| e.kind != CellGridEventKind::Delete));
    }

    #[test]
    fn keys_on_the_grid_node_reach_the_app_with_the_next_view() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let styled = StyledDom::create_from_dom(grid(&log).with_theme(UiTheme::Flat).dom());
        let node = nodes_with(&styled, GRID_CLASS_NAME)[0];
        let (update, changes) =
            rv::press(&styled, id(node), VirtualKeyCode::Down, &[]).expect("the grid hears keys");
        assert_eq!(update, Update::RefreshDom, "the app's answer is forwarded");
        assert!(rv::prevented(&changes), "the arrow is the grid's");
        let events = log.lock().expect("log").clone();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, CellGridEventKind::Select);
        assert_eq!(events[0].view.active, at(1, 0));

        let (_, changes) = rv::press(&styled, id(node), VirtualKeyCode::Q, &[])
            .expect("the handler runs");
        assert!(!rv::prevented(&changes), "a letter is not a key the grid takes (it is typed text)");
    }

    #[test]
    fn a_fill_drag_reaches_down_or_across_and_its_release_reports_the_range() {
        let source = CellGridRange::spanning(at(0, 0), at(0, 1));
        assert_eq!(fill_range(source, at(5, 0)), CellGridRange::spanning(at(0, 0), at(5, 1)));
        assert_eq!(fill_range(source, at(1, 6)), CellGridRange::spanning(at(0, 0), at(0, 6)));
        assert_eq!(fill_range(source, at(0, 1)), source, "inside the source: nothing");

        let mut view = CellGridView::create();
        view.ranges = CellGridRangeVec::from_vec(vec![source]);
        let g = small().with_view(view);
        let geo = geometry(&g);
        let press = press(&g, &geo, Hit::FillHandle, false, false, (0.0, 0.0)).expect("a fill starts");
        assert_eq!(press.view.drag.kind, CellGridDragKind::Fill);
        let g = small().with_view(press.view);
        let moved = drag_move(&g, Some(at(4, 1)), (0.0, 0.0)).expect("the target follows");
        let g = small().with_view(moved.view);
        let end = drag_end(&g).expect("the release fills");
        assert_eq!(end.kind, CellGridEventKind::Fill);
        assert_eq!(end.range, CellGridRange::spanning(at(0, 0), at(4, 1)));
        assert_eq!(end.view.drag.kind, CellGridDragKind::None);
    }

    #[test]
    fn a_resize_drag_shows_the_new_width_and_reports_it_on_release() {
        let g = small();
        let geo = geometry(&g);
        let press = press(&g, &geo, Hit::ColumnEdge(1), false, false, (200.0, 10.0)).expect("a resize starts");
        assert_eq!(press.view.drag.kind, CellGridDragKind::ResizeColumn);
        assert!((press.view.drag.start_size - 64.0).abs() < 0.01);
        let g = small().with_view(press.view);
        let moved = drag_move(&g, None, (220.0, 10.0)).expect("the size follows the pointer");
        assert!((moved.view.drag.size - 84.0).abs() < 0.01);
        let g = small().with_view(moved.view);
        assert!(
            (geometry(&resolve(g.clone()).grid).columns[1].size - 84.0).abs() < 0.01,
            "the column is drawn at the dragged width"
        );
        let end = drag_end(&g).expect("the release reports it");
        assert_eq!(end.kind, CellGridEventKind::ResizeColumn);
        assert_eq!(end.index, 1);
        assert!((end.size - 84.0).abs() < 0.01);
    }

    #[test]
    fn a_click_selects_shift_extends_ctrl_adds_and_a_header_selects_its_column() {
        let g = small();
        let geo = geometry(&g);
        let click = press(&g, &geo, Hit::Cell(at(2, 1)), false, false, (0.0, 0.0)).expect("a click");
        assert_eq!(click.view.active, at(2, 1));
        assert_eq!(click.view.drag.kind, CellGridDragKind::Select, "a drag may follow");
        let g2 = small().with_view(click.view.clone());
        let shift = press(&g2, &geo, Hit::Cell(at(4, 2)), true, false, (0.0, 0.0)).expect("shift-click");
        assert_eq!(shift.view.current_range(), CellGridRange::spanning(at(2, 1), at(4, 2)));
        let ctrl = press(&g2, &geo, Hit::Cell(at(0, 0)), false, true, (0.0, 0.0)).expect("ctrl-click");
        assert_eq!(ctrl.view.ranges.as_ref().len(), 2, "a second range");
        let column = press(&g, &geo, Hit::ColumnHeader(3), false, false, (0.0, 0.0)).expect("a header");
        assert_eq!(column.view.current_range(), CellGridRange::spanning(at(0, 3), at(999, 3)));

        // Dragging over cells grows the range from where the press was.
        let g3 = small().with_view(click.view);
        let dragged = drag_move(&g3, Some(at(3, 3)), (0.0, 0.0)).expect("the range grows");
        assert_eq!(dragged.view.current_range(), CellGridRange::spanning(at(2, 1), at(3, 3)));
    }

    /// Excel's point mode: while a formula waits for a reference (after
    /// `=`, `(`, `,` or an operator), a click puts the clicked cell's
    /// reference at the caret instead of committing; a second click
    /// replaces it; a drag makes it a range. The edited cell stays.
    #[test]
    fn a_click_while_a_formula_waits_for_a_reference_points_at_the_cell() {
        let mut view = CellGridView::create();
        view.edit_mode = CellGridEditMode::Enter;
        view.edit_text = AzString::from_const_str("=SUM(");
        view.edit_cursor = 5;
        let g = small().with_view(view);
        let geo = geometry(&g);
        let e = press(&g, &geo, Hit::Cell(at(2, 1)), false, false, (0.0, 0.0)).expect("a click");
        assert_eq!(e.kind, CellGridEventKind::EditText, "pointing, not committing");
        assert_eq!(e.view.edit_text.as_str(), "=SUM(B3");
        assert_eq!(e.view.edit_cursor, 7);
        assert_eq!(e.view.active, at(0, 0), "the edited cell stays");
        assert!(e.view.is_editing());
        assert_eq!(e.view.drag.kind, CellGridDragKind::Point, "a drag may follow");

        // A second click replaces the pointed reference.
        let mut pointed = e.view.clone();
        pointed.drag = CellGridDrag::default();
        let g2 = small().with_view(pointed);
        let again = press(&g2, &geo, Hit::Cell(at(4, 0)), false, false, (0.0, 0.0)).expect("a click");
        assert_eq!(again.view.edit_text.as_str(), "=SUM(A5");

        // A drag makes it a range (written top-left first), the release ends it.
        let g3 = small().with_view(e.view);
        let dragged = drag_move(&g3, Some(at(0, 0)), (0.0, 0.0)).expect("the range follows");
        assert_eq!(dragged.view.edit_text.as_str(), "=SUM(A1:B3");
        assert_eq!(dragged.view.edit_cursor, 10);
        let g4 = small().with_view(dragged.view);
        let end = drag_end(&g4).expect("the release");
        assert_eq!(end.view.drag.kind, CellGridDragKind::None);
        assert_eq!(end.view.edit_text.as_str(), "=SUM(A1:B3");
    }

    /// In Enter mode the arrows point too: from the edited cell first, then
    /// from the pointed one. After a value they commit, as before.
    #[test]
    fn the_arrows_point_while_a_formula_waits_for_a_reference() {
        let mut view = CellGridView::create().with_active(at(1, 1));
        view.edit_mode = CellGridEditMode::Enter;
        view.edit_text = AzString::from_const_str("=");
        view.edit_cursor = 1;
        let g = small().with_view(view);
        let geo = geometry(&g);
        let b = bounds_of(&g, &geo);
        let down = edit_key(&g, &b, VirtualKeyCode::Down, false).expect("Down points");
        assert_eq!(down.kind, CellGridEventKind::EditText);
        assert_eq!(down.view.edit_text.as_str(), "=B3", "the cell below the edited B2");
        assert_eq!(down.view.active, at(1, 1));
        let g2 = small().with_view(down.view);
        let right = edit_key(&g2, &b, VirtualKeyCode::Right, false).expect("Right points");
        assert_eq!(right.view.edit_text.as_str(), "=C3", "from the pointed cell");
        assert_eq!(right.view.edit_cursor, 3);

        let mut typed = CellGridView::create().with_active(at(1, 1));
        typed.edit_mode = CellGridEditMode::Enter;
        typed.edit_text = AzString::from_const_str("=1+2");
        typed.edit_cursor = 4;
        let g3 = small().with_view(typed);
        let commit = edit_key(&g3, &b, VirtualKeyCode::Down, false).expect("Down commits");
        assert_eq!(commit.kind, CellGridEventKind::EditCommit);
    }

    #[test]
    fn a_click_after_a_value_or_in_plain_text_still_commits() {
        for (text, cursor) in [("=1+2", 4u32), ("Total", 5), ("=SUM(A1", 4)] {
            let mut view = CellGridView::create();
            view.edit_mode = CellGridEditMode::Enter;
            view.edit_text = AzString::from(text);
            view.edit_cursor = cursor;
            let g = small().with_view(view);
            let geo = geometry(&g);
            let e = press(&g, &geo, Hit::Cell(at(3, 3)), false, false, (0.0, 0.0)).expect("a click");
            assert_eq!(e.kind, CellGridEventKind::EditCommit, "{text:?} at {cursor} commits");
        }
    }

    #[test]
    fn a_click_elsewhere_commits_the_edit_in_progress() {
        let g = small().with_view(typed(&CellGridView::create(), "7").view);
        let geo = geometry(&g);
        let e = press(&g, &geo, Hit::Cell(at(3, 3)), false, false, (0.0, 0.0)).expect("a click");
        assert_eq!(e.kind, CellGridEventKind::EditCommit);
        assert_eq!(e.text.as_str(), "7");
        assert_eq!(e.range.first, at(0, 0));
        assert_eq!(e.view.active, at(3, 3));
    }

    #[test]
    fn the_wheel_scrolls_whole_rows_and_keeps_the_remainder() {
        let mut travel = 0.0;
        assert_eq!(wheel_steps(&mut travel, 30.0, 20.0), 1);
        assert!((travel - 10.0).abs() < 0.01, "the rest waits");
        assert_eq!(wheel_steps(&mut travel, 30.0, 20.0), 2);
        assert_eq!(wheel_steps(&mut travel, -5.0, 20.0), 0);
        assert_eq!(wheel_steps(&mut travel, 1.0e9, 20.0), WHEEL_MAX_STEPS, "a burst is capped");
        assert_eq!(wheel_steps(&mut 0.0, f32::NAN, 20.0), 0);

        let g = small().with_frozen(1, 0);
        let geo = geometry(&g);
        let b = bounds_of(&g, &geo);
        let v = scroll_by(&g.view, &b, -5, 0);
        assert_eq!(v.top_row, 1, "never into the frozen rows");
        let v = scroll_by(&g.view, &b, 5000, 0);
        assert_eq!(v.top_row, 999, "never past the last row");
    }

    #[test]
    fn the_clipboard_gets_tab_separated_text_and_an_html_table() {
        let rows = vec![
            vec![String::from("a"), String::from("b\tc")],
            vec![String::from("\"q\""), String::from("<1>")],
        ];
        assert_eq!(cells_to_tsv(&rows), "a\t\"b\tc\"\n\"\"\"q\"\"\"\t<1>");
        assert_eq!(
            cells_to_html(&rows),
            "<table><tr><td>a</td><td>b\tc</td></tr><tr><td>\"q\"</td><td>&lt;1&gt;</td></tr></table>",
            "a cell is text: the one encoder escapes & < >, a quote is plain text there"
        );
        let mut view = CellGridView::create();
        view.ranges = CellGridRangeVec::from_vec(vec![CellGridRange::spanning(at(0, 0), at(999_999, 1))]);
        let (range, copied) = selection_rows(&small().with_view(view));
        assert_eq!(range.last.row, 4, "a whole column is clipped to the data");
        assert_eq!(copied[1], vec![String::from("10"), String::from("11")]);
    }

    #[test]
    fn a1_names_round_trip() {
        assert_eq!(column_letters(0), "A");
        assert_eq!(column_letters(25), "Z");
        assert_eq!(column_letters(26), "AA");
        assert_eq!(column_letters(16_383), "XFD");
        assert_eq!(parse_a1("B7"), Some(at(6, 1)));
        assert_eq!(parse_a1("$aa$10"), Some(at(9, 26)));
        assert_eq!(parse_a1("A0"), None);
        assert_eq!(parse_a1("7B"), None);
        assert_eq!(parse_a1(""), None);
        assert_eq!(CellGrid::cell_label(at(6, 1)).as_str(), "B7");
    }

    #[test]
    fn the_grid_is_one_focus_stop_with_the_grid_role_and_its_cells_carry_their_place() {
        let dom = small().with_view(CellGridView::create().with_scroll(41, 0)).with_theme(UiTheme::Flat).dom();
        let info = dom.root.get_accessibility_info().expect("a role");
        assert_eq!(info.role, azul_core::a11y::AccessibilityRole::Grid);
        assert_eq!(info.accessibility_name.as_ref().map(|n| n.as_str()), Some("Sheet1"));
        assert_eq!(dom.root.get_tab_index(), Some(azul_core::dom::TabIndex::Auto));
        // Row 0 of the children is the header row; row 1 the first data row
        // (sheet row 42, scrolled); its child 0 the row number, 1 the cell.
        let row = &dom.children.as_ref()[1];
        let cell = &row.children.as_ref()[1];
        let cell_info = cell.root.get_accessibility_info().expect("a cell role");
        assert_eq!(cell_info.role, azul_core::a11y::AccessibilityRole::GridCell);
        assert_eq!(cell_info.row_index.into_option(), Some(42), "its row in the WHOLE sheet");
        assert_eq!(cell_info.column_index.into_option(), Some(1));
    }

    /// Seen in the wave-6 look: every cell was an anonymous GridCell (one
    /// a11y-shape warning per cell per frame). A cell is named by its place
    /// ("B2", what Excel's screen reader says first) and carries its text as
    /// its value.
    #[test]
    fn every_cell_is_named_by_its_place_and_carries_its_text_as_its_value() {
        let dom = small().with_theme(UiTheme::Flat).dom();
        // Child 0 is the header row; child 2 sheet row 2; its child 0 the
        // row number, child 2 column B.
        let cell = &dom.children.as_ref()[2].children.as_ref()[2];
        let info = cell.root.get_accessibility_info().expect("a cell role");
        assert_eq!(info.accessibility_name.as_ref().map(|n| n.as_str()), Some("B2"));
        assert_eq!(info.accessibility_value.as_ref().map(|v| v.as_str()), Some("11"));
        let empty = &dom.children.as_ref()[7].children.as_ref()[1];
        let info = empty.root.get_accessibility_info().expect("a cell role");
        assert_eq!(
            info.accessibility_name.as_ref().map(|n| n.as_str()),
            Some("A7"),
            "an empty cell is named too"
        );
    }

    /// A1 holds a title wider than its column, B1 and C1 are empty, D1 has
    /// text; A2 a long number; A3 a long note.
    extern "C" fn long_title(_: RefAny, cell: CellGridCellRef) -> CellGridCell {
        let text = |t: &'static str, kind| CellGridCell::create(AzString::from_const_str(t), kind);
        match (cell.row, cell.column) {
            (0, 0) => text("Household budget 2027", CellGridCellKind::Text),
            (0, 3) => text("x", CellGridCellKind::Text),
            (1, 0) => text("1234567890123", CellGridCellKind::Number),
            (2, 0) => text("A very long note that runs on and on and on", CellGridCellKind::Text),
            _ => CellGridCell::empty(),
        }
    }

    fn titled() -> CellGrid {
        CellGrid::create(100, 20)
            .with_viewport(400.0, 200.0)
            .with_data_source(RefAny::new(()), long_title as CellGridDataSourceCallbackType)
    }

    fn is_cell(node: &Dom) -> bool {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, Class(s) if s.as_str() == CELL_CLASS_NAME))
    }

    /// Seen in the wave-6 look: the Budget sample's title was cut at A1's
    /// edge. Excel lets a text run on over the empty cells after it.
    #[test]
    fn a_text_too_wide_for_its_cell_spills_over_the_empty_cells_after_it() {
        let resolved = resolve(titled());
        let spans = spill_spans(&resolved, 0);
        assert_eq!(&spans[..4], &[3, 0, 0, 1], "A1 reaches over B1 and C1; D1 holds its own text");
        assert_eq!(spill_spans(&resolved, 1)[0], 1, "a number never spills");
        let long = spill_spans(&resolved, 2);
        assert_eq!(long.iter().sum::<u32>() as usize, long.len(), "every column is drawn once");
        assert_eq!(long[0] as usize, long.len(), "a long note runs to the edge of the window");

        // Drawn: the first data row has two cells fewer, A1 covers three.
        let in_view = geometry(&titled()).columns.len();
        let dom = titled().with_theme(UiTheme::Flat).dom();
        let first_row = &dom.children.as_ref()[1];
        let cells = first_row.children.as_ref().iter().filter(|c| is_cell(c)).count();
        assert_eq!(cells, in_view - 2);
    }

    #[test]
    fn cells_show_their_text_aligned_by_kind_and_a_fill_picks_a_readable_ink() {
        let mut seen = Vec::new();
        texts(&small().with_theme(UiTheme::Flat).dom(), &mut seen);
        for t in ["A", "B", "1", "2", "0", "11", "42"] {
            assert!(seen.iter().any(|s| s == t), "{t} in {seen:?}");
        }
        assert_eq!(
            auto_ink(ColorU { r: 255, g: 235, b: 59, a: 255 }),
            ColorU { r: 0, g: 0, b: 0, a: 255 },
            "black on yellow"
        );
        assert_eq!(
            auto_ink(ColorU { r: 20, g: 40, b: 90, a: 255 }),
            ColorU { r: 255, g: 255, b: 255, a: 255 },
            "white on navy"
        );
    }

    #[test]
    fn an_edit_shows_its_text_and_caret_over_the_cell() {
        let view = typed(&CellGridView::create().with_active(at(1, 1)), "=SUM(").view;
        let styled = StyledDom::create_from_dom(small().with_view(view).with_theme(UiTheme::Flat).dom());
        assert_eq!(nodes_with(&styled, EDITOR_CLASS_NAME).len(), 1);
        assert_eq!(nodes_with(&styled, CARET_CLASS_NAME).len(), 1);
        assert!(nodes_with(&styled, FILL_HANDLE_CLASS_NAME).is_empty(), "no fill handle while editing");
        let plain = StyledDom::create_from_dom(small().with_theme(UiTheme::Flat).dom());
        assert_eq!(nodes_with(&plain, FILL_HANDLE_CLASS_NAME).len(), 1);
        assert_eq!(nodes_with(&plain, OUTLINE_CLASS_NAME).len(), 1);
    }

    #[test]
    fn a_grid_without_a_theme_follows_the_app_theme_and_declares_its_structure_once() {
        checks::assert_follows_the_app_theme(
            "cell_grid",
            || small().dom(),
            |t: UiTheme| small().with_theme(t).dom(),
        );
        for theme in checks::BOTH {
            let dom = checks::under(theme, || small().dom());
            theme_checks::assert_structure_is_shared(
                &format!("cell_grid built for {}", theme.name()),
                &dom,
                &[],
            );
        }
    }
}
