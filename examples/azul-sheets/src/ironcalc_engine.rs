//! [`SheetEngine`] on IronCalc 0.8.3 (`ironcalc_base::UserModel` plus the
//! `ironcalc` xlsx import / export).
//!
//! Embedding rules (planning/engines/ironcalc.md): the model is a
//! `UserModel<'static>` (built from string literals, `Send`), every mutating
//! call recalculates the workbook, rows and columns are 1-based. The pure
//! mapping functions between IronCalc's types and the engine trait's are
//! `pub(crate)` and tested on their own.
//!
//! Traps handled here (ironcalc.md §2.5): `BorderArea` has crate-private
//! fields, so it is built through serde; `save_to_xlsx` refuses to overwrite,
//! so saving goes through `save_xlsx_to_writer`; the xlsx import does not
//! evaluate, so a load evaluates once; `paste_csv_string` reads TAB-separated
//! text with a csv reader that drops a row of another length, so
//! `set_inputs` writes a padded rectangle with the same `csv` crate
//! (`model::tsv_of`).

use std::io::Cursor;

use ironcalc::base::{
    cell::CellValue as IcCellValue,
    expressions::types::Area,
    types::{BorderItem, CellType, Color, HorizontalAlignment, Style, VerticalAlignment},
    BorderArea, Model, UserModel,
};

use crate::engine::{
    BorderPreset, CellAddr, CellArea, CellBorders, CellStyle, CellValue, DefinedName, EngineError,
    FillTo, HAlign, SheetEngine, SheetInfo, StylePatch, VAlign,
};

/// IronCalc's default column width, px (its constant is crate-private).
const DEFAULT_COLUMN_WIDTH: f64 = 90.0;
/// IronCalc's default row height, px.
const DEFAULT_ROW_HEIGHT: f64 = 25.0;
/// The locale, time zone and formula language of new workbooks.
const LOCALE: &str = "en";
const TIMEZONE: &str = "UTC";
const LANGUAGE: &str = "en";

/// The real engine.
pub struct IronCalcEngine {
    model: UserModel<'static>,
}

impl Default for IronCalcEngine {
    fn default() -> Self {
        Self::new_empty()
    }
}

impl IronCalcEngine {
    /// An empty workbook "Book1" with one sheet.
    ///
    /// # Panics
    /// Never in practice: the locale, time zone and language are IronCalc's
    /// own built-ins.
    #[must_use]
    pub fn new_empty() -> Self {
        Self {
            model: empty_model("Book1").expect("the built-in locale, time zone and language"),
        }
    }

    /// The model of an `.xlsx` file, evaluated.
    pub fn from_xlsx_bytes(bytes: &[u8], name: &str) -> Result<Self, EngineError> {
        let workbook = ironcalc::import::load_from_xlsx_bytes(bytes, name, LOCALE, TIMEZONE)
            .map_err(|e| format!("Could not read the workbook: {e}"))?;
        let model = Model::from_workbook(workbook, LANGUAGE)
            .map_err(|e| format!("Could not open the workbook: {e}"))?;
        let mut model = UserModel::from_model(model);
        // The import keeps the cached values; recalculate once so every
        // formula shows what IronCalc computes.
        model.evaluate();
        Ok(Self { model })
    }

    /// The `#RRGGBB` of `color` (theme colours resolved), `None` for no colour.
    fn color(&self, color: &Color) -> Option<String> {
        non_empty(self.model.resolve_color(color))
    }
}

/// An empty `UserModel<'static>` named `name` (set on the `Model`, so the
/// new workbook has no undo history).
fn empty_model(name: &str) -> Result<UserModel<'static>, EngineError> {
    let mut model = Model::new_empty("Book1", LOCALE, TIMEZONE, LANGUAGE)?;
    model.workbook.name = name.to_string();
    Ok(UserModel::from_model(model))
}

/// `Some(s)` unless `s` is empty.
fn non_empty(s: String) -> Option<String> {
    (!s.is_empty()).then_some(s)
}

/// The engine area as IronCalc's.
pub(crate) const fn area(a: CellArea) -> Area {
    Area {
        sheet: a.sheet,
        row: a.row,
        column: a.column,
        width: a.width,
        height: a.height,
    }
}

/// IronCalc's horizontal alignment as the engine's (the justified and
/// distributed kinds read as left, centre-across as centre).
pub(crate) fn h_align_from(h: &HorizontalAlignment) -> HAlign {
    match h {
        HorizontalAlignment::General => HAlign::General,
        HorizontalAlignment::Center | HorizontalAlignment::CenterContinuous => HAlign::Center,
        HorizontalAlignment::Right => HAlign::Right,
        HorizontalAlignment::Left
        | HorizontalAlignment::Fill
        | HorizontalAlignment::Justify
        | HorizontalAlignment::Distributed => HAlign::Left,
    }
}

/// IronCalc's vertical alignment as the engine's.
pub(crate) fn v_align_from(v: &VerticalAlignment) -> VAlign {
    match v {
        VerticalAlignment::Bottom => VAlign::Bottom,
        VerticalAlignment::Top => VAlign::Top,
        VerticalAlignment::Center | VerticalAlignment::Distributed | VerticalAlignment::Justify => {
            VAlign::Center
        }
    }
}

/// An IronCalc `Style` as the engine's, colours through `resolve` (the
/// model's `resolve_color`: `""` for no colour).
pub(crate) fn style_from(style: &Style, resolve: &dyn Fn(&Color) -> String) -> CellStyle {
    let color = |c: &Color| non_empty(resolve(c));
    // A border line without a colour is drawn in black (Excel's automatic).
    let edge = |item: &Option<BorderItem>| {
        item.as_ref()
            .map(|i| color(&i.color).unwrap_or_else(|| String::from("#000000")))
    };
    let alignment = style.alignment.as_ref();
    CellStyle {
        bold: style.font.b,
        italic: style.font.i,
        underline: style.font.u,
        strike: style.font.strike,
        font_size: style.font.sz,
        font_color: color(&style.font.color),
        fill: color(&style.fill.color),
        h_align: alignment.map_or(HAlign::General, |a| h_align_from(&a.horizontal)),
        v_align: alignment.map_or(VAlign::Bottom, |a| v_align_from(&a.vertical)),
        wrap: alignment.is_some_and(|a| a.wrap_text),
        num_fmt: style.num_fmt.clone(),
        borders: CellBorders {
            top: edge(&style.border.top),
            right: edge(&style.border.right),
            bottom: edge(&style.border.bottom),
            left: edge(&style.border.left),
        },
    }
}

fn flag(b: bool) -> String {
    String::from(if b { "true" } else { "false" })
}

/// The `update_range_style` path and value of `patch`; `None` for the
/// borders, which go through `set_area_with_border`.
pub(crate) fn style_path(patch: &StylePatch) -> Option<(&'static str, String)> {
    Some(match patch {
        StylePatch::Bold(b) => ("font.b", flag(*b)),
        StylePatch::Italic(b) => ("font.i", flag(*b)),
        StylePatch::Underline(b) => ("font.u", flag(*b)),
        StylePatch::Strike(b) => ("font.strike", flag(*b)),
        StylePatch::FontSize(s) => ("font.size", s.to_string()),
        StylePatch::FontSizeDelta(d) => ("font.size_delta", d.to_string()),
        StylePatch::FontColor(c) => ("font.color", c.clone().unwrap_or_default()),
        StylePatch::Fill(c) => ("fill.color", c.clone().unwrap_or_default()),
        StylePatch::HAlign(h) => (
            "alignment.horizontal",
            String::from(match h {
                HAlign::General => "general",
                HAlign::Left => "left",
                HAlign::Center => "center",
                HAlign::Right => "right",
            }),
        ),
        StylePatch::VAlign(v) => (
            "alignment.vertical",
            String::from(match v {
                VAlign::Bottom => "bottom",
                VAlign::Center => "center",
                VAlign::Top => "top",
            }),
        ),
        StylePatch::Wrap(b) => ("alignment.wrap_text", flag(*b)),
        StylePatch::NumberFormat(f) => ("num_fmt", f.clone()),
        StylePatch::Borders { .. } => return None,
    })
}

/// The serde form of IronCalc's `BorderArea` (its fields are crate-private):
/// a thin line in `color` on the edges `preset` names.
pub(crate) fn border_area_json(preset: BorderPreset, color: &str) -> serde_json::Value {
    let kind = match preset {
        BorderPreset::All => "All",
        BorderPreset::Outer => "Outer",
        BorderPreset::Top => "Top",
        BorderPreset::Right => "Right",
        BorderPreset::Bottom => "Bottom",
        BorderPreset::Left => "Left",
        BorderPreset::None => "None",
    };
    let color = if color.is_empty() { "#000000" } else { color };
    serde_json::json!({
        "item": { "style": "thin", "color": color },
        "type": kind,
    })
}

/// [`border_area_json`] as IronCalc's type.
pub(crate) fn border_area(preset: BorderPreset, color: &str) -> Result<BorderArea, EngineError> {
    serde_json::from_value(border_area_json(preset, color))
        .map_err(|e| format!("Could not build the border: {e}"))
}

/// An IronCalc value as the engine's; an error value arrives as text, which
/// `is_error` (the cell's type) tells apart.
pub(crate) fn value_from(value: IcCellValue, is_error: bool) -> CellValue {
    match value {
        IcCellValue::None => CellValue::Empty,
        IcCellValue::Number(n) => CellValue::Number(n),
        IcCellValue::Boolean(b) => CellValue::Boolean(b),
        IcCellValue::String(s) if is_error => CellValue::Error(s),
        IcCellValue::String(s) => CellValue::Text(s),
    }
}

impl SheetEngine for IronCalcEngine {
    fn new_workbook(&mut self, name: &str) -> Result<(), EngineError> {
        self.model = empty_model(name)?;
        Ok(())
    }

    fn load_xlsx(&mut self, bytes: &[u8], name: &str) -> Result<(), EngineError> {
        *self = Self::from_xlsx_bytes(bytes, name)?;
        Ok(())
    }

    fn save_xlsx(&self) -> Result<Vec<u8>, EngineError> {
        ironcalc::export::save_xlsx_to_writer(self.model.get_model(), Cursor::new(Vec::new()))
            .map(Cursor::into_inner)
            .map_err(|e| format!("Could not write the workbook: {e}"))
    }

    fn workbook_name(&self) -> String {
        self.model.get_name()
    }

    fn evaluate(&mut self) {
        self.model.evaluate();
    }

    fn undo(&mut self) -> Result<(), EngineError> {
        self.model.undo()
    }

    fn redo(&mut self) -> Result<(), EngineError> {
        self.model.redo()
    }

    fn can_undo(&self) -> bool {
        self.model.can_undo()
    }

    fn can_redo(&self) -> bool {
        self.model.can_redo()
    }

    fn sheets(&self) -> Vec<SheetInfo> {
        self.model
            .get_worksheets_properties()
            .into_iter()
            .map(|p| SheetInfo {
                color: self.color(&p.color),
                hidden: p.state != "visible",
                name: p.name,
            })
            .collect()
    }

    fn add_sheet(&mut self) -> Result<(), EngineError> {
        self.model.new_sheet()
    }

    fn rename_sheet(&mut self, sheet: u32, name: &str) -> Result<(), EngineError> {
        self.model.rename_sheet(sheet, name)
    }

    fn delete_sheet(&mut self, sheet: u32) -> Result<(), EngineError> {
        self.model.delete_sheet(sheet)
    }

    fn move_sheet(&mut self, sheet: u32, to: u32) -> Result<(), EngineError> {
        self.model.move_sheet(sheet, to)
    }

    fn set_sheet_color(&mut self, sheet: u32, color: Option<&str>) -> Result<(), EngineError> {
        let color = Color::from_param(color.unwrap_or(""))?;
        self.model.set_sheet_color(sheet, &color)
    }

    fn set_cell_input(&mut self, at: CellAddr, input: &str) -> Result<(), EngineError> {
        self.model
            .set_user_input(at.sheet, at.row, at.column, input)
    }

    fn set_inputs(&mut self, top_left: CellAddr, rows: &[Vec<String>]) -> Result<(), EngineError> {
        if rows.is_empty() {
            return Ok(());
        }
        // Rectangular: IronCalc's paste drops a row of another length.
        let tsv = crate::model::tsv_of(rows);
        self.paste_tsv(top_left, &tsv)
    }

    fn paste_tsv(&mut self, top_left: CellAddr, tsv: &str) -> Result<(), EngineError> {
        self.model
            .paste_csv_string(&area(CellArea::cell(top_left)), tsv)
    }

    fn cell_input(&self, at: CellAddr) -> String {
        self.model
            .get_cell_content(at.sheet, at.row, at.column)
            .unwrap_or_default()
    }

    fn cell_value(&self, at: CellAddr) -> CellValue {
        let value = self
            .model
            .get_model()
            .get_cell_value_by_index(at.sheet, at.row, at.column)
            .unwrap_or(IcCellValue::None);
        let is_error = matches!(
            self.model.get_cell_type(at.sheet, at.row, at.column),
            Ok(CellType::ErrorValue)
        );
        value_from(value, is_error)
    }

    fn cell_formatted(&self, at: CellAddr) -> String {
        self.model
            .get_formatted_cell_value(at.sheet, at.row, at.column)
            .unwrap_or_default()
    }

    fn cell_style(&self, at: CellAddr) -> CellStyle {
        match self.model.get_cell_style(at.sheet, at.row, at.column) {
            Ok(style) => style_from(&style, &|c: &Color| self.model.resolve_color(c)),
            Err(_) => CellStyle::default(),
        }
    }

    fn update_style(&mut self, a: CellArea, patch: &StylePatch) -> Result<(), EngineError> {
        if let StylePatch::Borders { preset, color } = patch {
            let border = border_area(*preset, color)?;
            return self.model.set_area_with_border(&area(a), &border);
        }
        match style_path(patch) {
            Some((path, value)) => self.model.update_range_style(&area(a), path, &value),
            None => Ok(()),
        }
    }

    fn clear_contents(&mut self, a: CellArea) -> Result<(), EngineError> {
        self.model.range_clear_contents(&area(a))
    }

    fn clear_formats(&mut self, a: CellArea) -> Result<(), EngineError> {
        self.model.range_clear_formatting(&area(a))
    }

    fn auto_fill(&mut self, source: CellArea, to: FillTo) -> Result<(), EngineError> {
        match to {
            FillTo::Row(row) => self.model.auto_fill_rows(&area(source), row),
            FillTo::Column(column) => self.model.auto_fill_columns(&area(source), column),
        }
    }

    fn insert_rows(&mut self, sheet: u32, row: i32, count: i32) -> Result<(), EngineError> {
        self.model.insert_rows(sheet, row, count)
    }

    fn delete_rows(&mut self, sheet: u32, row: i32, count: i32) -> Result<(), EngineError> {
        self.model.delete_rows(sheet, row, count)
    }

    fn insert_columns(&mut self, sheet: u32, column: i32, count: i32) -> Result<(), EngineError> {
        self.model.insert_columns(sheet, column, count)
    }

    fn delete_columns(&mut self, sheet: u32, column: i32, count: i32) -> Result<(), EngineError> {
        self.model.delete_columns(sheet, column, count)
    }

    fn column_width(&self, sheet: u32, column: i32) -> f64 {
        self.model
            .get_column_width(sheet, column)
            .unwrap_or(DEFAULT_COLUMN_WIDTH)
    }

    fn set_column_width(
        &mut self,
        sheet: u32,
        first: i32,
        last: i32,
        px: f64,
    ) -> Result<(), EngineError> {
        self.model.set_columns_width(sheet, first, last, px)
    }

    fn row_height(&self, sheet: u32, row: i32) -> f64 {
        self.model
            .get_row_height(sheet, row)
            .unwrap_or(DEFAULT_ROW_HEIGHT)
    }

    fn set_row_height(
        &mut self,
        sheet: u32,
        first: i32,
        last: i32,
        px: f64,
    ) -> Result<(), EngineError> {
        self.model.set_rows_height(sheet, first, last, px)
    }

    fn set_rows_hidden(
        &mut self,
        sheet: u32,
        first: i32,
        last: i32,
        hidden: bool,
    ) -> Result<(), EngineError> {
        self.model.set_rows_hidden(sheet, first, last, hidden)
    }

    fn frozen(&self, sheet: u32) -> (i32, i32) {
        (
            self.model.get_frozen_rows_count(sheet).unwrap_or(0),
            self.model.get_frozen_columns_count(sheet).unwrap_or(0),
        )
    }

    fn set_frozen(&mut self, sheet: u32, rows: i32, columns: i32) -> Result<(), EngineError> {
        let (old_rows, old_columns) = self.frozen(sheet);
        if rows != old_rows {
            self.model.set_frozen_rows_count(sheet, rows)?;
        }
        if columns != old_columns {
            self.model.set_frozen_columns_count(sheet, columns)?;
        }
        Ok(())
    }

    fn show_grid_lines(&self, sheet: u32) -> bool {
        self.model.get_show_grid_lines(sheet).unwrap_or(true)
    }

    fn set_show_grid_lines(&mut self, sheet: u32, show: bool) -> Result<(), EngineError> {
        self.model.set_show_grid_lines(sheet, show)
    }

    fn extent(&self, sheet: u32) -> (i32, i32) {
        let Ok(worksheet) = self.model.get_model().workbook.worksheet(sheet) else {
            return (0, 0);
        };
        if worksheet.sheet_data.is_empty() {
            return (0, 0);
        }
        let dimension = worksheet.dimension();
        (dimension.max_row, dimension.max_column)
    }

    fn defined_names(&self) -> Vec<DefinedName> {
        self.model
            .get_defined_name_list()
            .into_iter()
            .map(|(name, scope, formula)| DefinedName {
                name,
                scope,
                formula,
            })
            .collect()
    }

    fn add_defined_name(
        &mut self,
        name: &str,
        scope: Option<u32>,
        formula: &str,
    ) -> Result<(), EngineError> {
        self.model.new_defined_name(name, scope, formula)
    }

    fn delete_defined_name(&mut self, name: &str, scope: Option<u32>) -> Result<(), EngineError> {
        self.model.delete_defined_name(name, scope)
    }

    fn merges(&self, sheet: u32) -> Vec<CellArea> {
        let _ = sheet;
        Vec::new()
    }

    fn merge(&mut self, area: CellArea) -> Result<(), EngineError> {
        let _ = area;
        Ok(())
    }

    fn unmerge(&mut self, area: CellArea) -> Result<(), EngineError> {
        let _ = area;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use ironcalc::base::types::{Alignment, BorderStyle};

    use super::*;
    use crate::worker::{spawn_engine, Command, EngineMsg, ViewRequest};

    fn at(row: i32, column: i32) -> CellAddr {
        CellAddr::new(0, row, column)
    }

    // ---- the mapping functions ----

    #[test]
    fn an_ironcalc_style_maps_with_its_theme_colours_resolved() {
        let mut style = Style::default();
        style.font.b = true;
        style.font.sz = 16;
        style.font.color = Color::Theme(1, 0.0);
        style.fill.color = Color::Rgb(String::from("#FF0000"));
        style.alignment = Some(Alignment {
            horizontal: HorizontalAlignment::Right,
            vertical: VerticalAlignment::Top,
            wrap_text: true,
        });
        style.border.top = Some(BorderItem {
            style: BorderStyle::Thin,
            color: Color::None,
        });
        style.num_fmt = String::from("#,##0.00");
        let resolve = |c: &Color| match c {
            Color::Theme(1, _) => String::from("#123456"),
            Color::Rgb(s) => s.clone(),
            _ => String::new(),
        };
        let mapped = style_from(&style, &resolve);
        assert!(mapped.bold && !mapped.italic);
        assert_eq!(mapped.font_size, 16);
        assert_eq!(mapped.font_color.as_deref(), Some("#123456"));
        assert_eq!(mapped.fill.as_deref(), Some("#FF0000"));
        assert_eq!(
            (mapped.h_align, mapped.v_align, mapped.wrap),
            (HAlign::Right, VAlign::Top, true)
        );
        assert_eq!(
            mapped.borders.top.as_deref(),
            Some("#000000"),
            "an automatic line is black"
        );
        assert_eq!(mapped.borders.left, None);
        assert_eq!(mapped.num_fmt, "#,##0.00");
        assert_eq!(
            style_from(&Style::default(), &resolve),
            CellStyle::default()
        );
    }

    #[test]
    fn a_style_patch_names_its_ironcalc_style_path() {
        assert_eq!(
            style_path(&StylePatch::Bold(true)),
            Some(("font.b", String::from("true")))
        );
        assert_eq!(
            style_path(&StylePatch::FontColor(None)),
            Some(("font.color", String::new()))
        );
        assert_eq!(
            style_path(&StylePatch::Fill(Some(String::from("#DDEBF7")))),
            Some(("fill.color", String::from("#DDEBF7")))
        );
        assert_eq!(
            style_path(&StylePatch::HAlign(HAlign::Center)),
            Some(("alignment.horizontal", String::from("center")))
        );
        assert_eq!(
            style_path(&StylePatch::NumberFormat(String::from("0%"))),
            Some(("num_fmt", String::from("0%")))
        );
        assert_eq!(
            style_path(&StylePatch::FontSizeDelta(-1)),
            Some(("font.size_delta", String::from("-1")))
        );
        assert_eq!(
            style_path(&StylePatch::Borders {
                preset: BorderPreset::All,
                color: String::new()
            }),
            None
        );
    }

    #[test]
    fn a_border_area_is_built_through_serde_for_every_preset() {
        for preset in [
            BorderPreset::All,
            BorderPreset::Outer,
            BorderPreset::Top,
            BorderPreset::Right,
            BorderPreset::Bottom,
            BorderPreset::Left,
            BorderPreset::None,
        ] {
            assert!(border_area(preset, "#5B7DB1").is_ok(), "{preset:?}");
        }
        assert_eq!(
            border_area_json(BorderPreset::Top, "")["item"]["color"],
            "#000000"
        );
    }

    #[test]
    fn an_error_value_is_told_apart_from_text_by_the_cell_type() {
        assert_eq!(value_from(IcCellValue::None, false), CellValue::Empty);
        assert_eq!(
            value_from(IcCellValue::Number(2.5), false),
            CellValue::Number(2.5)
        );
        assert_eq!(
            value_from(IcCellValue::Boolean(true), false),
            CellValue::Boolean(true)
        );
        assert_eq!(
            value_from(IcCellValue::String(String::from("#DIV/0!")), true),
            CellValue::Error(String::from("#DIV/0!"))
        );
        assert_eq!(
            value_from(IcCellValue::String(String::from("Rent")), false),
            CellValue::Text(String::from("Rent"))
        );
    }

    #[test]
    fn rows_become_tab_separated_text_with_quoting_where_needed() {
        let rows = vec![
            vec![String::from("a"), String::from("b\tc")],
            vec![String::from("=IF(A1=\"x\",1,2)"), String::new()],
        ];
        assert_eq!(
            crate::model::tsv_of(&rows),
            "a\t\"b\tc\"\n\"=IF(A1=\"\"x\"\",1,2)\"\t\n"
        );
    }

    // ---- the real engine ----

    #[test]
    fn a_formula_evaluates_as_soon_as_it_is_typed() {
        let mut e = IronCalcEngine::new_empty();
        e.set_cell_input(at(1, 1), "=1+2").unwrap();
        assert_eq!(e.cell_formatted(at(1, 1)), "3");
        assert_eq!(e.cell_input(at(1, 1)), "=1+2");
        assert_eq!(e.cell_value(at(1, 1)), CellValue::Number(3.0));
        e.set_cell_input(at(2, 1), "=1/0").unwrap();
        assert!(matches!(e.cell_value(at(2, 1)), CellValue::Error(_)));
        assert_eq!(e.extent(0), (2, 1));
    }

    /// Seen in the sample (wave 6): only the title "Household budget 2027"
    /// arrived, the rows under it were blank. IronCalc's paste reader is not
    /// flexible: a record whose length differs from the first is dropped
    /// whole. Every row of a ragged block reaches the sheet.
    #[test]
    fn ragged_rows_all_reach_the_sheet() {
        let mut e = IronCalcEngine::new_empty();
        e.set_inputs(
            at(1, 1),
            &[
                vec![String::from("Title")],
                vec![String::from("a"), String::from("b"), String::from("c")],
                vec![String::from("1"), String::from("2")],
            ],
        )
        .unwrap();
        assert_eq!(e.cell_input(at(1, 1)), "Title");
        assert_eq!(e.cell_input(at(2, 1)), "a");
        assert_eq!(e.cell_input(at(2, 3)), "c");
        assert_eq!(e.cell_input(at(3, 2)), "2");
        assert_eq!(e.extent(0), (3, 3));
    }

    #[test]
    fn many_inputs_are_one_undo_step() {
        let mut e = IronCalcEngine::new_empty();
        assert!(!e.can_undo(), "a new workbook has no history");
        e.set_inputs(
            at(1, 1),
            &[
                vec![String::from("1"), String::from("2")],
                vec![String::from("x\ty"), String::from("=A1+B1")],
            ],
        )
        .unwrap();
        assert_eq!(e.cell_formatted(at(2, 2)), "3");
        assert_eq!(e.cell_input(at(2, 1)), "x\ty", "a quoted tab survives");
        e.undo().unwrap();
        assert_eq!(e.cell_input(at(1, 1)), "");
        assert_eq!(e.cell_input(at(2, 2)), "");
        assert!(!e.can_undo());
    }

    #[test]
    fn styles_and_borders_reach_the_cells() {
        let mut e = IronCalcEngine::new_empty();
        let a1 = CellArea::cell(at(1, 1));
        e.update_style(a1, &StylePatch::Bold(true)).unwrap();
        e.update_style(a1, &StylePatch::Fill(Some(String::from("#DDEBF7"))))
            .unwrap();
        e.update_style(
            a1,
            &StylePatch::Borders {
                preset: BorderPreset::All,
                color: String::from("#5B7DB1"),
            },
        )
        .unwrap();
        let style = e.cell_style(at(1, 1));
        assert!(style.bold);
        assert_eq!(
            style.fill.as_deref().map(str::to_uppercase).as_deref(),
            Some("#DDEBF7")
        );
        assert!(style.borders.top.is_some() && style.borders.left.is_some());
    }

    #[test]
    fn a_saved_workbook_loads_back_with_its_values_and_column_widths() {
        let mut e = IronCalcEngine::new_empty();
        e.set_cell_input(at(1, 1), "42").unwrap();
        e.set_cell_input(at(1, 2), "=A1*2").unwrap();
        e.set_column_width(0, 2, 2, 150.0).unwrap();
        e.set_frozen(0, 1, 0).unwrap();
        let bytes = e.save_xlsx().unwrap();
        assert!(bytes.starts_with(b"PK"), "an xlsx is a zip");
        let mut back = IronCalcEngine::new_empty();
        back.load_xlsx(&bytes, "copy").unwrap();
        assert_eq!(back.cell_formatted(at(1, 1)), "42");
        assert_eq!(back.cell_formatted(at(1, 2)), "84");
        assert!(
            (back.column_width(0, 2) - 150.0).abs() < 0.5,
            "{}",
            back.column_width(0, 2)
        );
        assert_eq!(back.frozen(0), (1, 0));
    }

    /// IronCalc keeps a sheet's merges (`<mergeCells>`) but its UserModel
    /// cannot change them: the engine keeps them by the sheet's stable id
    /// and writes them into the saved file.
    #[test]
    fn merges_are_saved_into_the_xlsx_and_follow_their_sheet() {
        let mut e = IronCalcEngine::new_empty();
        e.add_sheet().unwrap();
        e.merge(CellArea::spanning(1, 2, 2, 3, 3)).unwrap(); // Sheet2!B2:C3
        e.merge(CellArea::spanning(0, 1, 1, 1, 4)).unwrap(); // Sheet1!A1:D1
        e.merge(CellArea::spanning(0, 1, 2, 1, 2)).unwrap(); // one cell: no merge
        assert_eq!(e.merges(0), vec![CellArea::spanning(0, 1, 1, 1, 4)]);
        e.move_sheet(1, 0).unwrap();
        assert_eq!(e.merges(0), vec![CellArea::spanning(0, 2, 2, 3, 3)], "the merge moved with its sheet");

        let bytes = e.save_xlsx().unwrap();
        let mut back = IronCalcEngine::new_empty();
        back.load_xlsx(&bytes, "copy").unwrap();
        assert_eq!(back.merges(0), vec![CellArea::spanning(0, 2, 2, 3, 3)]);
        assert_eq!(back.merges(1), vec![CellArea::spanning(1, 1, 1, 1, 4)]);

        back.merge(CellArea::spanning(1, 1, 3, 2, 5)).unwrap(); // overlaps A1:D1: replaces it
        assert_eq!(back.merges(1), vec![CellArea::spanning(1, 1, 3, 2, 5)]);
        back.unmerge(CellArea::cell(CellAddr::new(1, 2, 4))).unwrap();
        assert!(back.merges(1).is_empty());
    }

    #[test]
    fn sheets_can_be_added_renamed_and_listed() {
        let mut e = IronCalcEngine::new_empty();
        e.add_sheet().unwrap();
        e.rename_sheet(1, "Data").unwrap();
        let names: Vec<String> = e.sheets().into_iter().map(|s| s.name).collect();
        assert_eq!(names.len(), 2);
        assert_eq!(names[1], "Data");
        assert!(e.sheets().iter().all(|s| !s.hidden));
    }

    #[test]
    fn the_budget_sample_builds_and_evaluates_on_ironcalc() {
        let mut e = IronCalcEngine::new_empty();
        crate::sample::budget(&mut e).unwrap();
        // Rent Q1 = 3 x 1250; the Q2 projection reads GrowthRate (3%).
        assert_eq!(
            e.cell_value(CellAddr::new(0, 3, 5)),
            CellValue::Number(3750.0)
        );
        match e.cell_value(CellAddr::new(0, 3, 6)) {
            CellValue::Number(n) => assert!((n - 3862.5).abs() < 1e-6, "{n}"),
            other => panic!("the projection is {other:?}"),
        }
        assert_eq!(e.frozen(0), (2, 0));
    }

    /// The crash the engine study found: a chain of 5,000 cells evaluated
    /// from its far end (A1 reads the last link) needs ~15 MB of stack -
    /// more than a main thread has. On the engine thread's 256 MB it
    /// evaluates.
    #[test]
    fn a_five_thousand_cell_chain_evaluates_on_the_engine_thread() {
        let engine = spawn_engine(|| Box::new(IronCalcEngine::new_empty())).expect("engine thread");
        let mut tsv = String::from("=A5001\n1\n");
        for row in 3..=5001 {
            tsv.push_str(&format!("=A{}+1\n", row - 1));
        }
        let (tx, rx) = std::sync::mpsc::channel();
        engine
            .send(EngineMsg {
                seq: 1,
                command: Command::Paste { at: at(1, 1), tsv },
                view: ViewRequest {
                    sheet: 0,
                    rows: vec![(1, 1)],
                    columns: vec![(1, 1)],
                    selection: Vec::new(),
                },
                reply: tx,
            })
            .unwrap();
        let reply = rx.recv().expect("the engine thread survived");
        assert_eq!(reply.result, Ok(()));
        assert_eq!(reply.snapshot.cells[0].formatted, "5000");
    }
}
