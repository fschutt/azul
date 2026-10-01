//! The spreadsheet engine behind a trait.
//!
//! The UI never talks to IronCalc directly: it sends commands to the engine
//! thread (`crate::worker`), which drives a [`SheetEngine`]. The real one is
//! `crate::ironcalc_engine::IronCalcEngine`; the tests drive
//! `crate::fake_engine::FakeEngine`. Everything here is plain Rust (no azul).
//!
//! Addresses follow IronCalc: the sheet is 0-based, rows and columns are
//! 1-based (rows up to 1,048,576, columns up to 16,384).

/// What an engine call answers when it fails: a sentence for the user.
pub type EngineError = String;

/// The last row of a sheet (Excel's and IronCalc's limit).
pub const LAST_ROW: i32 = 1_048_576;
/// The last column of a sheet (`XFD`).
pub const LAST_COLUMN: i32 = 16_384;

/// One cell: sheet 0-based, row and column 1-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct CellAddr {
    pub sheet: u32,
    pub row: i32,
    pub column: i32,
}

impl CellAddr {
    #[must_use]
    pub const fn new(sheet: u32, row: i32, column: i32) -> Self {
        Self { sheet, row, column }
    }
}

/// An inclusive rectangle of cells; `width` and `height` are at least 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct CellArea {
    pub sheet: u32,
    pub row: i32,
    pub column: i32,
    pub width: i32,
    pub height: i32,
}

impl CellArea {
    /// The one cell `at`.
    #[must_use]
    pub const fn cell(at: CellAddr) -> Self {
        Self {
            sheet: at.sheet,
            row: at.row,
            column: at.column,
            width: 1,
            height: 1,
        }
    }

    /// The area between two corners, given in any order.
    #[must_use]
    pub fn spanning(sheet: u32, r0: i32, c0: i32, r1: i32, c1: i32) -> Self {
        let (top, bottom) = (r0.min(r1), r0.max(r1));
        let (left, right) = (c0.min(c1), c0.max(c1));
        Self {
            sheet,
            row: top,
            column: left,
            width: right - left + 1,
            height: bottom - top + 1,
        }
    }

    /// The bottom row, inclusive.
    #[must_use]
    pub const fn last_row(&self) -> i32 {
        self.row + self.height - 1
    }

    /// The right column, inclusive.
    #[must_use]
    pub const fn last_column(&self) -> i32 {
        self.column + self.width - 1
    }

    /// Whether (`row`, `column`) lies inside (the sheet is not compared).
    #[must_use]
    pub const fn contains(&self, row: i32, column: i32) -> bool {
        row >= self.row
            && row <= self.last_row()
            && column >= self.column
            && column <= self.last_column()
    }

    /// The part inside rows `1..=max_row` and columns `1..=max_column`;
    /// `None` when nothing is left (an empty sheet, an area past the data).
    #[must_use]
    pub fn clip(&self, max_row: i32, max_column: i32) -> Option<CellArea> {
        let top = self.row.max(1);
        let left = self.column.max(1);
        let bottom = self.last_row().min(max_row);
        let right = self.last_column().min(max_column);
        if top > bottom || left > right {
            return None;
        }
        Some(Self {
            sheet: self.sheet,
            row: top,
            column: left,
            width: right - left + 1,
            height: bottom - top + 1,
        })
    }
}

/// A cell's evaluated value.
#[derive(Clone, Debug, PartialEq)]
pub enum CellValue {
    Empty,
    Number(f64),
    Text(String),
    Boolean(bool),
    /// An error value, as displayed (`#DIV/0!`, `#NAME?`).
    Error(String),
}

/// Horizontal alignment; `General` puts numbers right and text left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum HAlign {
    #[default]
    General,
    Left,
    Center,
    Right,
}

/// Vertical alignment inside the row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum VAlign {
    #[default]
    Bottom,
    Center,
    Top,
}

/// A cell's border, edge by edge: the `#RRGGBB` colour of a thin line, or
/// `None` for no line.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct CellBorders {
    pub top: Option<String>,
    pub right: Option<String>,
    pub bottom: Option<String>,
    pub left: Option<String>,
}

/// IronCalc's default font size (`Font::default().sz`); rendered as px.
pub const DEFAULT_FONT_SIZE: i32 = 12;

/// How a cell looks.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CellStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// IronCalc's `font.sz`.
    pub font_size: i32,
    /// `#RRGGBB`; `None` = automatic.
    pub font_color: Option<String>,
    /// `#RRGGBB`; `None` = no fill.
    pub fill: Option<String>,
    pub h_align: HAlign,
    pub v_align: VAlign,
    pub wrap: bool,
    /// The number format code (`general`, `#,##0.00`, `0%`).
    pub num_fmt: String,
    pub borders: CellBorders,
}

impl Default for CellStyle {
    fn default() -> Self {
        Self {
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            font_size: DEFAULT_FONT_SIZE,
            font_color: None,
            fill: None,
            h_align: HAlign::General,
            v_align: VAlign::Bottom,
            wrap: false,
            num_fmt: String::from("general"),
            borders: CellBorders::default(),
        }
    }
}

/// Which edges a border command draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BorderPreset {
    /// Every edge of every cell.
    All,
    /// The outline of the area.
    Outer,
    Top,
    Right,
    Bottom,
    Left,
    /// Remove the borders.
    None,
}

/// One change to the style of an area.
#[derive(Clone, Debug, PartialEq)]
pub enum StylePatch {
    Bold(bool),
    Italic(bool),
    Underline(bool),
    Strike(bool),
    FontSize(i32),
    FontSizeDelta(i32),
    FontColor(Option<String>),
    Fill(Option<String>),
    HAlign(HAlign),
    VAlign(VAlign),
    Wrap(bool),
    NumberFormat(String),
    Borders { preset: BorderPreset, color: String },
}

/// A sheet tab.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetInfo {
    pub name: String,
    /// The tab colour, `#RRGGBB`.
    pub color: Option<String>,
    pub hidden: bool,
}

/// A defined name (`GrowthRate` = `Assumptions!$C$2`).
#[derive(Clone, Debug, PartialEq)]
pub struct DefinedName {
    pub name: String,
    /// `None` = the workbook, `Some(sheet)` = local to that sheet.
    pub scope: Option<u32>,
    pub formula: String,
}

/// Where the fill handle was dragged to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillTo {
    /// Extend the source down (or up) to this row.
    Row(i32),
    /// Extend the source right (or left) to this column.
    Column(i32),
}

/// A workbook the UI can drive. Every mutating call is one undo step unless
/// it says otherwise, and recalculates what it must before it returns.
pub trait SheetEngine: Send {
    // ---- the workbook ----

    /// Replaces the workbook with an empty one (one sheet, "Sheet1").
    fn new_workbook(&mut self, name: &str) -> Result<(), EngineError>;
    /// Replaces the workbook with an `.xlsx` file's (evaluated after loading).
    fn load_xlsx(&mut self, bytes: &[u8], name: &str) -> Result<(), EngineError>;
    /// The workbook as an `.xlsx` file.
    fn save_xlsx(&self) -> Result<Vec<u8>, EngineError>;
    fn workbook_name(&self) -> String;
    /// Recalculates every formula.
    fn evaluate(&mut self);
    fn undo(&mut self) -> Result<(), EngineError>;
    fn redo(&mut self) -> Result<(), EngineError>;
    fn can_undo(&self) -> bool;
    fn can_redo(&self) -> bool;

    // ---- sheets ----

    fn sheets(&self) -> Vec<SheetInfo>;
    fn add_sheet(&mut self) -> Result<(), EngineError>;
    fn rename_sheet(&mut self, sheet: u32, name: &str) -> Result<(), EngineError>;
    fn delete_sheet(&mut self, sheet: u32) -> Result<(), EngineError>;
    fn move_sheet(&mut self, sheet: u32, to: u32) -> Result<(), EngineError>;
    fn set_sheet_color(&mut self, sheet: u32, color: Option<&str>) -> Result<(), EngineError>;

    // ---- cells ----

    fn set_cell_input(&mut self, at: CellAddr, input: &str) -> Result<(), EngineError>;
    /// Many inputs as ONE undo step (sort, remove duplicates): `rows[i][j]`
    /// goes to (`top_left.row + i`, `top_left.column + j`).
    fn set_inputs(&mut self, top_left: CellAddr, rows: &[Vec<String>]) -> Result<(), EngineError>;
    /// Tab-separated text (a clipboard paste) as ONE undo step.
    fn paste_tsv(&mut self, top_left: CellAddr, tsv: &str) -> Result<(), EngineError>;
    /// The formula (`=SUM(A1:A3)`) or the typed text; empty for an empty cell.
    fn cell_input(&self, at: CellAddr) -> String;
    fn cell_value(&self, at: CellAddr) -> CellValue;
    /// The displayed text: the value with its number format applied.
    fn cell_formatted(&self, at: CellAddr) -> String;
    fn cell_style(&self, at: CellAddr) -> CellStyle;
    fn update_style(&mut self, area: CellArea, patch: &StylePatch) -> Result<(), EngineError>;
    fn clear_contents(&mut self, area: CellArea) -> Result<(), EngineError>;
    fn clear_formats(&mut self, area: CellArea) -> Result<(), EngineError>;
    fn auto_fill(&mut self, source: CellArea, to: FillTo) -> Result<(), EngineError>;

    // ---- rows and columns ----

    fn insert_rows(&mut self, sheet: u32, row: i32, count: i32) -> Result<(), EngineError>;
    fn delete_rows(&mut self, sheet: u32, row: i32, count: i32) -> Result<(), EngineError>;
    fn insert_columns(&mut self, sheet: u32, column: i32, count: i32) -> Result<(), EngineError>;
    fn delete_columns(&mut self, sheet: u32, column: i32, count: i32) -> Result<(), EngineError>;
    /// In px; 0 for a hidden column.
    fn column_width(&self, sheet: u32, column: i32) -> f64;
    fn set_column_width(
        &mut self,
        sheet: u32,
        first: i32,
        last: i32,
        px: f64,
    ) -> Result<(), EngineError>;
    /// In px; 0 for a hidden row.
    fn row_height(&self, sheet: u32, row: i32) -> f64;
    fn set_row_height(
        &mut self,
        sheet: u32,
        first: i32,
        last: i32,
        px: f64,
    ) -> Result<(), EngineError>;
    fn set_rows_hidden(
        &mut self,
        sheet: u32,
        first: i32,
        last: i32,
        hidden: bool,
    ) -> Result<(), EngineError>;
    /// The frozen panes: (rows, columns).
    fn frozen(&self, sheet: u32) -> (i32, i32);
    fn set_frozen(&mut self, sheet: u32, rows: i32, columns: i32) -> Result<(), EngineError>;
    fn show_grid_lines(&self, sheet: u32) -> bool;
    fn set_show_grid_lines(&mut self, sheet: u32, show: bool) -> Result<(), EngineError>;
    /// (max_row, max_column) of the stored cells; (0, 0) for an empty sheet.
    fn extent(&self, sheet: u32) -> (i32, i32);

    // ---- defined names ----

    fn defined_names(&self) -> Vec<DefinedName>;
    fn add_defined_name(
        &mut self,
        name: &str,
        scope: Option<u32>,
        formula: &str,
    ) -> Result<(), EngineError>;
    fn delete_defined_name(&mut self, name: &str, scope: Option<u32>) -> Result<(), EngineError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_area_spanning_two_corners_in_any_order_is_the_same_rectangle() {
        let a = CellArea::spanning(0, 5, 4, 2, 1);
        let b = CellArea::spanning(0, 2, 1, 5, 4);
        assert_eq!(a, b);
        assert_eq!((a.row, a.column, a.width, a.height), (2, 1, 4, 4));
        assert_eq!((a.last_row(), a.last_column()), (5, 4));
    }

    #[test]
    fn an_area_contains_its_corners_and_nothing_outside() {
        let a = CellArea::spanning(0, 2, 2, 4, 3);
        assert!(a.contains(2, 2) && a.contains(4, 3) && a.contains(3, 2));
        assert!(!a.contains(1, 2) && !a.contains(5, 3) && !a.contains(3, 4) && !a.contains(3, 1));
    }

    #[test]
    fn clipping_keeps_the_part_inside_the_data_and_drops_an_area_past_it() {
        let whole_column = CellArea::spanning(0, 1, 2, LAST_ROW, 2);
        assert_eq!(
            whole_column.clip(10, 5),
            Some(CellArea::spanning(0, 1, 2, 10, 2))
        );
        let past = CellArea::spanning(0, 20, 1, 30, 1);
        assert_eq!(past.clip(10, 5), None);
        assert_eq!(
            CellArea::cell(CellAddr::new(0, 1, 1)).clip(0, 0),
            None,
            "an empty sheet"
        );
    }
}
