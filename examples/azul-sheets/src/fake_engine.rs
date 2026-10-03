//! A small in-memory [`SheetEngine`] for the tests: cells hold their input,
//! numbers and booleans parse, a formula stays unevaluated EXCEPT
//! `=SUM(<range>)`, which it sums (enough for the status bar and AutoSum).
//! Every mutating call snapshots the workbook for undo (`set_inputs` and
//! `paste_tsv` snapshot once). `save_xlsx` writes JSON, not xlsx: the fake
//! only has to read back what it wrote.
//!
//! Public so the UI's tests can drive the worker with it.

use std::collections::{BTreeMap, BTreeSet};

use crate::engine::{
    BorderPreset, CellAddr, CellArea, CellStyle, CellValue, CondLook, CondRule, ConditionalFormat, DefinedName,
    EngineError, FillTo,
    SheetEngine, SheetInfo, StylePatch, LAST_COLUMN, LAST_ROW,
};

/// The fake's default column width (IronCalc's), px.
pub const FAKE_COLUMN_WIDTH: f64 = 90.0;
/// The fake's default row height (IronCalc's), px.
pub const FAKE_ROW_HEIGHT: f64 = 25.0;
/// How far an area touching the whole sheet is walked by the fake.
const FAKE_WALK_ROWS: i32 = 10_000;
const FAKE_WALK_COLUMNS: i32 = 1_000;
/// How deep `=SUM` follows other `=SUM`s before it calls it a cycle.
const MAX_DEPTH: u32 = 64;

#[derive(Clone, Debug, Default, PartialEq)]
struct FakeSheet {
    name: String,
    color: Option<String>,
    hidden: bool,
    inputs: BTreeMap<(i32, i32), String>,
    styles: BTreeMap<(i32, i32), CellStyle>,
    widths: BTreeMap<i32, f64>,
    heights: BTreeMap<i32, f64>,
    hidden_rows: BTreeSet<i32>,
    frozen: (i32, i32),
    grid_lines: bool,
    /// The merged areas (their `sheet` is not kept up to date: the sheet
    /// they belong to is the one holding them).
    merges: Vec<CellArea>,
    /// The conditional formats, in order (kept, listed, cleared - the fake
    /// does not evaluate them; IronCalc does).
    conditional: Vec<(CellArea, CondRule, CondLook)>,
}

impl FakeSheet {
    fn named(name: &str) -> Self {
        Self {
            name: name.to_string(),
            grid_lines: true,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct FakeBook {
    name: String,
    sheets: Vec<FakeSheet>,
    names: Vec<DefinedName>,
}

impl FakeBook {
    fn empty(name: &str) -> Self {
        Self {
            name: name.to_string(),
            sheets: vec![FakeSheet::named("Sheet1")],
            names: Vec::new(),
        }
    }
}

/// The test engine.
#[derive(Debug)]
pub struct FakeEngine {
    book: FakeBook,
    undo: Vec<FakeBook>,
    redo: Vec<FakeBook>,
}

impl Default for FakeEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeEngine {
    /// An empty workbook "Book1" with one sheet, "Sheet1".
    #[must_use]
    pub fn new() -> Self {
        Self {
            book: FakeBook::empty("Book1"),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Before a change: remember the workbook for undo.
    fn checkpoint(&mut self) {
        self.undo.push(self.book.clone());
        self.redo.clear();
    }

    fn sheet(&self, sheet: u32) -> Option<&FakeSheet> {
        self.book.sheets.get(sheet as usize)
    }

    fn sheet_mut(&mut self, sheet: u32) -> Result<&mut FakeSheet, EngineError> {
        self.book
            .sheets
            .get_mut(sheet as usize)
            .ok_or_else(|| format!("There is no sheet {sheet}."))
    }

    fn check_sheet(&self, sheet: u32) -> Result<(), EngineError> {
        if self.sheet(sheet).is_some() {
            Ok(())
        } else {
            Err(format!("There is no sheet {sheet}."))
        }
    }

    fn input(&self, at: CellAddr) -> &str {
        self.sheet(at.sheet)
            .and_then(|s| s.inputs.get(&(at.row, at.column)))
            .map_or("", String::as_str)
    }

    fn evaluate_at(&self, at: CellAddr, depth: u32) -> CellValue {
        let input = self.input(at);
        if input.is_empty() {
            return CellValue::Empty;
        }
        if let Some(formula) = input.strip_prefix('=') {
            return self.evaluate_formula(at.sheet, formula, depth);
        }
        parse_constant(input)
    }

    fn evaluate_formula(&self, sheet: u32, formula: &str, depth: u32) -> CellValue {
        let upper = formula.trim().to_ascii_uppercase();
        let Some(args) = upper.strip_prefix("SUM(").and_then(|r| r.strip_suffix(')')) else {
            // The fake evaluates nothing else: the formula is its own value.
            return CellValue::Text(format!("={formula}"));
        };
        if depth >= MAX_DEPTH {
            return CellValue::Error(String::from("#CIRC!"));
        }
        let Some(area) = parse_range(sheet, args) else {
            return CellValue::Error(String::from("#NAME?"));
        };
        let mut sum = 0.0;
        for row in area.row..=area.last_row() {
            for column in area.column..=area.last_column() {
                match self.evaluate_at(CellAddr::new(sheet, row, column), depth + 1) {
                    CellValue::Number(n) => sum += n,
                    CellValue::Error(e) => return CellValue::Error(e),
                    _ => {}
                }
            }
        }
        CellValue::Number(sum)
    }

    /// Writes `rows` from `top_left` without a checkpoint.
    fn write_rows(&mut self, top_left: CellAddr, rows: &[Vec<String>]) -> Result<(), EngineError> {
        let sheet = self.sheet_mut(top_left.sheet)?;
        for (i, row) in rows.iter().enumerate() {
            for (j, value) in row.iter().enumerate() {
                let key = (top_left.row + i as i32, top_left.column + j as i32);
                if value.is_empty() {
                    sheet.inputs.remove(&key);
                } else {
                    sheet.inputs.insert(key, value.clone());
                }
            }
        }
        Ok(())
    }
}

/// A typed constant: a number, `TRUE` / `FALSE`, or text.
fn parse_constant(input: &str) -> CellValue {
    let trimmed = input.trim();
    if trimmed.eq_ignore_ascii_case("true") {
        return CellValue::Boolean(true);
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return CellValue::Boolean(false);
    }
    match trimmed.parse::<f64>() {
        Ok(n) if n.is_finite() => CellValue::Number(n),
        _ => CellValue::Text(input.to_string()),
    }
}

/// `B7` / `$B$7` -> (row 7, column 2).
fn parse_a1(text: &str) -> Option<(i32, i32)> {
    let text = text.trim().replace('$', "");
    let letters: String = text.chars().take_while(char::is_ascii_alphabetic).collect();
    let digits = &text[letters.len()..];
    if letters.is_empty() || digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut column = 0i32;
    for c in letters.chars() {
        column = column * 26 + (c.to_ascii_uppercase() as i32 - 'A' as i32 + 1);
    }
    let row: i32 = digits.parse().ok()?;
    (row >= 1 && column >= 1).then_some((row, column))
}

/// `A1:B3` (or a single cell) on `sheet`.
fn parse_range(sheet: u32, text: &str) -> Option<CellArea> {
    let (a, b) = text.split_once(':').unwrap_or((text, text));
    let (r0, c0) = parse_a1(a)?;
    let (r1, c1) = parse_a1(b)?;
    Some(CellArea::spanning(sheet, r0, c0, r1, c1))
}

/// The fake's number formats: `general`, fixed decimals with or without
/// thousands separators, percentages.
fn format_number(n: f64, num_fmt: &str) -> String {
    let decimals = num_fmt.split_once('.').map_or(0, |(_, after)| {
        after.chars().take_while(|c| *c == '0').count()
    });
    if num_fmt.contains('%') {
        return format!("{:.*}%", decimals, n * 100.0);
    }
    if num_fmt.eq_ignore_ascii_case("general") || num_fmt.is_empty() {
        if n.fract() == 0.0 && n.abs() < 1e15 {
            return format!("{}", n as i64);
        }
        return format!("{n}");
    }
    let fixed = format!("{:.*}", decimals, n.abs());
    let (int_part, frac_part) = fixed.split_once('.').unwrap_or((fixed.as_str(), ""));
    let int_part = if num_fmt.contains(',') {
        let digits: Vec<char> = int_part.chars().collect();
        let mut out = String::new();
        for (i, c) in digits.iter().enumerate() {
            if i > 0 && (digits.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(*c);
        }
        out
    } else {
        int_part.to_string()
    };
    let sign = if n < 0.0 { "-" } else { "" };
    if frac_part.is_empty() {
        format!("{sign}{int_part}")
    } else {
        format!("{sign}{int_part}.{frac_part}")
    }
}

/// The cells of `area` the fake walks (a whole-sheet area is cut down).
fn walk(area: CellArea) -> impl Iterator<Item = (i32, i32)> {
    let last_row = area.last_row().min(area.row + FAKE_WALK_ROWS);
    let last_column = area.last_column().min(area.column + FAKE_WALK_COLUMNS);
    (area.row..=last_row).flat_map(move |r| (area.column..=last_column).map(move |c| (r, c)))
}

/// `patch` applied to the style of the cell at (`row`, `column`) of `area`.
fn patched(
    mut style: CellStyle,
    patch: &StylePatch,
    area: CellArea,
    row: i32,
    column: i32,
) -> CellStyle {
    match patch {
        StylePatch::Bold(b) => style.bold = *b,
        StylePatch::Italic(b) => style.italic = *b,
        StylePatch::Underline(b) => style.underline = *b,
        StylePatch::Strike(b) => style.strike = *b,
        StylePatch::FontSize(s) => style.font_size = (*s).max(1),
        StylePatch::FontSizeDelta(d) => style.font_size = (style.font_size + d).max(1),
        StylePatch::FontColor(c) => style.font_color = c.clone(),
        StylePatch::Fill(c) => style.fill = c.clone(),
        StylePatch::HAlign(h) => style.h_align = *h,
        StylePatch::VAlign(v) => style.v_align = *v,
        StylePatch::Wrap(b) => style.wrap = *b,
        StylePatch::NumberFormat(f) => style.num_fmt = f.clone(),
        StylePatch::Borders { preset, color } => {
            let line = Some(color.clone());
            let b = &mut style.borders;
            match preset {
                BorderPreset::All => {
                    b.top = line.clone();
                    b.right = line.clone();
                    b.bottom = line.clone();
                    b.left = line;
                }
                BorderPreset::Outer => {
                    if row == area.row {
                        b.top = line.clone();
                    }
                    if row == area.last_row() {
                        b.bottom = line.clone();
                    }
                    if column == area.column {
                        b.left = line.clone();
                    }
                    if column == area.last_column() {
                        b.right = line;
                    }
                }
                BorderPreset::Top => {
                    if row == area.row {
                        b.top = line;
                    }
                }
                BorderPreset::Bottom => {
                    if row == area.last_row() {
                        b.bottom = line;
                    }
                }
                BorderPreset::Left => {
                    if column == area.column {
                        b.left = line;
                    }
                }
                BorderPreset::Right => {
                    if column == area.last_column() {
                        b.right = line;
                    }
                }
                BorderPreset::None => *b = Default::default(),
            }
        }
    }
    style
}

/// Moves every key at or past `at` on its axis by `by` (negative: deleting
/// `-by` lines at `at` drops the lines in `at..at - by`).
fn shift_keys<V: Clone>(
    map: &BTreeMap<(i32, i32), V>,
    on_rows: bool,
    at: i32,
    by: i32,
) -> BTreeMap<(i32, i32), V> {
    let mut out = BTreeMap::new();
    for (&(r, c), v) in map {
        let line = if on_rows { r } else { c };
        let moved = if line < at {
            Some(line)
        } else if by < 0 && line < at - by {
            None
        } else {
            Some(line + by)
        };
        if let Some(line) = moved {
            let key = if on_rows { (line, c) } else { (r, line) };
            out.insert(key, v.clone());
        }
    }
    out
}

/// [`shift_keys`] for a map keyed by one line.
fn shift_lines<V: Clone>(map: &BTreeMap<i32, V>, at: i32, by: i32) -> BTreeMap<i32, V> {
    let mut out = BTreeMap::new();
    for (&line, v) in map {
        if line < at {
            out.insert(line, v.clone());
        } else if by > 0 || line >= at - by {
            out.insert(line + by, v.clone());
        }
    }
    out
}

impl SheetEngine for FakeEngine {
    fn new_workbook(&mut self, name: &str) -> Result<(), EngineError> {
        self.book = FakeBook::empty(name);
        self.undo.clear();
        self.redo.clear();
        Ok(())
    }

    fn load_xlsx(&mut self, bytes: &[u8], name: &str) -> Result<(), EngineError> {
        let json: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| format!("Not a fake workbook: {e}"))?;
        let mut book = FakeBook {
            name: json["name"].as_str().unwrap_or(name).to_string(),
            sheets: Vec::new(),
            names: Vec::new(),
        };
        for s in json["sheets"].as_array().cloned().unwrap_or_default() {
            let mut sheet = FakeSheet::named(s["name"].as_str().unwrap_or("Sheet"));
            sheet.color = s["color"].as_str().map(str::to_string);
            sheet.hidden = s["hidden"].as_bool().unwrap_or(false);
            sheet.grid_lines = s["grid_lines"].as_bool().unwrap_or(true);
            sheet.frozen = (
                s["frozen"][0].as_i64().unwrap_or(0) as i32,
                s["frozen"][1].as_i64().unwrap_or(0) as i32,
            );
            for cell in s["cells"].as_array().cloned().unwrap_or_default() {
                let (Some(r), Some(c), Some(input)) =
                    (cell[0].as_i64(), cell[1].as_i64(), cell[2].as_str())
                else {
                    continue;
                };
                sheet.inputs.insert((r as i32, c as i32), input.to_string());
            }
            for w in s["widths"].as_array().cloned().unwrap_or_default() {
                if let (Some(c), Some(px)) = (w[0].as_i64(), w[1].as_f64()) {
                    sheet.widths.insert(c as i32, px);
                }
            }
            for h in s["heights"].as_array().cloned().unwrap_or_default() {
                if let (Some(r), Some(px)) = (h[0].as_i64(), h[1].as_f64()) {
                    sheet.heights.insert(r as i32, px);
                }
            }
            for r in s["hidden_rows"].as_array().cloned().unwrap_or_default() {
                if let Some(r) = r.as_i64() {
                    sheet.hidden_rows.insert(r as i32);
                }
            }
            for m in s["merges"].as_array().cloned().unwrap_or_default() {
                if let (Some(row), Some(column), Some(width), Some(height)) =
                    (m[0].as_i64(), m[1].as_i64(), m[2].as_i64(), m[3].as_i64())
                {
                    sheet.merges.push(CellArea {
                        sheet: 0,
                        row: row as i32,
                        column: column as i32,
                        width: width as i32,
                        height: height as i32,
                    });
                }
            }
            book.sheets.push(sheet);
        }
        for n in json["names"].as_array().cloned().unwrap_or_default() {
            book.names.push(DefinedName {
                name: n["name"].as_str().unwrap_or_default().to_string(),
                scope: n["scope"].as_u64().map(|s| s as u32),
                formula: n["formula"].as_str().unwrap_or_default().to_string(),
            });
        }
        if book.sheets.is_empty() {
            return Err(String::from("A workbook needs at least one sheet."));
        }
        self.book = book;
        self.undo.clear();
        self.redo.clear();
        Ok(())
    }

    fn save_xlsx(&self) -> Result<Vec<u8>, EngineError> {
        let sheets: Vec<serde_json::Value> = self
            .book
            .sheets
            .iter()
            .map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "color": s.color,
                    "hidden": s.hidden,
                    "grid_lines": s.grid_lines,
                    "frozen": [s.frozen.0, s.frozen.1],
                    "cells": s.inputs.iter().map(|(&(r, c), v)| serde_json::json!([r, c, v])).collect::<Vec<_>>(),
                    "widths": s.widths.iter().map(|(c, px)| serde_json::json!([c, px])).collect::<Vec<_>>(),
                    "heights": s.heights.iter().map(|(r, px)| serde_json::json!([r, px])).collect::<Vec<_>>(),
                    "hidden_rows": s.hidden_rows.iter().collect::<Vec<_>>(),
                    "merges": s.merges.iter().map(|m| serde_json::json!([m.row, m.column, m.width, m.height])).collect::<Vec<_>>(),
                })
            })
            .collect();
        let names: Vec<serde_json::Value> = self
            .book
            .names
            .iter()
            .map(|n| serde_json::json!({ "name": n.name, "scope": n.scope, "formula": n.formula }))
            .collect();
        serde_json::to_vec(&serde_json::json!({
            "name": self.book.name,
            "sheets": sheets,
            "names": names,
        }))
        .map_err(|e| e.to_string())
    }

    fn workbook_name(&self) -> String {
        self.book.name.clone()
    }

    fn evaluate(&mut self) {}

    fn undo(&mut self) -> Result<(), EngineError> {
        let previous = self
            .undo
            .pop()
            .ok_or_else(|| String::from("Nothing to undo."))?;
        let current = core::mem::replace(&mut self.book, previous);
        self.redo.push(current);
        Ok(())
    }

    fn redo(&mut self) -> Result<(), EngineError> {
        let next = self
            .redo
            .pop()
            .ok_or_else(|| String::from("Nothing to redo."))?;
        let current = core::mem::replace(&mut self.book, next);
        self.undo.push(current);
        Ok(())
    }

    fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn sheets(&self) -> Vec<SheetInfo> {
        self.book
            .sheets
            .iter()
            .map(|s| SheetInfo {
                name: s.name.clone(),
                color: s.color.clone(),
                hidden: s.hidden,
            })
            .collect()
    }

    fn add_sheet(&mut self) -> Result<(), EngineError> {
        self.checkpoint();
        let mut n = self.book.sheets.len() + 1;
        while self
            .book
            .sheets
            .iter()
            .any(|s| s.name == format!("Sheet{n}"))
        {
            n += 1;
        }
        self.book
            .sheets
            .push(FakeSheet::named(&format!("Sheet{n}")));
        Ok(())
    }

    fn rename_sheet(&mut self, sheet: u32, name: &str) -> Result<(), EngineError> {
        self.check_sheet(sheet)?;
        let name = name.trim();
        if name.is_empty() {
            return Err(String::from("A sheet needs a name."));
        }
        if self
            .book
            .sheets
            .iter()
            .enumerate()
            .any(|(i, s)| i != sheet as usize && s.name.eq_ignore_ascii_case(name))
        {
            return Err(format!("There is already a sheet named \"{name}\"."));
        }
        self.checkpoint();
        self.sheet_mut(sheet)?.name = name.to_string();
        Ok(())
    }

    fn delete_sheet(&mut self, sheet: u32) -> Result<(), EngineError> {
        self.check_sheet(sheet)?;
        if self.book.sheets.len() == 1 {
            return Err(String::from("A workbook keeps at least one sheet."));
        }
        self.checkpoint();
        self.book.sheets.remove(sheet as usize);
        Ok(())
    }

    fn move_sheet(&mut self, sheet: u32, to: u32) -> Result<(), EngineError> {
        self.check_sheet(sheet)?;
        self.checkpoint();
        let moved = self.book.sheets.remove(sheet as usize);
        let to = (to as usize).min(self.book.sheets.len());
        self.book.sheets.insert(to, moved);
        Ok(())
    }

    fn set_sheet_color(&mut self, sheet: u32, color: Option<&str>) -> Result<(), EngineError> {
        self.check_sheet(sheet)?;
        self.checkpoint();
        self.sheet_mut(sheet)?.color = color.map(str::to_string);
        Ok(())
    }

    fn set_cell_input(&mut self, at: CellAddr, input: &str) -> Result<(), EngineError> {
        self.check_sheet(at.sheet)?;
        self.checkpoint();
        self.write_rows(at, &[vec![input.to_string()]])
    }

    fn set_inputs(&mut self, top_left: CellAddr, rows: &[Vec<String>]) -> Result<(), EngineError> {
        self.check_sheet(top_left.sheet)?;
        self.checkpoint();
        self.write_rows(top_left, rows)
    }

    fn paste_tsv(&mut self, top_left: CellAddr, tsv: &str) -> Result<(), EngineError> {
        self.check_sheet(top_left.sheet)?;
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(b'\t')
            .has_headers(false)
            .flexible(true)
            .from_reader(tsv.as_bytes());
        let rows: Vec<Vec<String>> = reader
            .records()
            .flatten()
            .map(|r| r.iter().map(str::to_string).collect())
            .collect();
        if rows.is_empty() {
            return Ok(());
        }
        self.checkpoint();
        self.write_rows(top_left, &rows)
    }

    fn cell_input(&self, at: CellAddr) -> String {
        self.input(at).to_string()
    }

    fn cell_value(&self, at: CellAddr) -> CellValue {
        self.evaluate_at(at, 0)
    }

    fn cell_formatted(&self, at: CellAddr) -> String {
        match self.cell_value(at) {
            CellValue::Empty => String::new(),
            CellValue::Number(n) => format_number(n, &self.cell_style(at).num_fmt),
            CellValue::Text(s) | CellValue::Error(s) => s,
            CellValue::Boolean(b) => String::from(if b { "TRUE" } else { "FALSE" }),
        }
    }

    fn cell_style(&self, at: CellAddr) -> CellStyle {
        self.sheet(at.sheet)
            .and_then(|s| s.styles.get(&(at.row, at.column)))
            .cloned()
            .unwrap_or_default()
    }

    fn update_style(&mut self, area: CellArea, patch: &StylePatch) -> Result<(), EngineError> {
        self.check_sheet(area.sheet)?;
        self.checkpoint();
        let sheet = self.sheet_mut(area.sheet)?;
        for (row, column) in walk(area) {
            let old = sheet
                .styles
                .get(&(row, column))
                .cloned()
                .unwrap_or_default();
            let new = patched(old, patch, area, row, column);
            if new == CellStyle::default() {
                sheet.styles.remove(&(row, column));
            } else {
                sheet.styles.insert((row, column), new);
            }
        }
        Ok(())
    }

    fn clear_contents(&mut self, area: CellArea) -> Result<(), EngineError> {
        self.check_sheet(area.sheet)?;
        self.checkpoint();
        self.sheet_mut(area.sheet)?
            .inputs
            .retain(|&(r, c), _| !area.contains(r, c));
        Ok(())
    }

    fn clear_formats(&mut self, area: CellArea) -> Result<(), EngineError> {
        self.check_sheet(area.sheet)?;
        self.checkpoint();
        self.sheet_mut(area.sheet)?
            .styles
            .retain(|&(r, c), _| !area.contains(r, c));
        Ok(())
    }

    fn auto_fill(&mut self, source: CellArea, to: FillTo) -> Result<(), EngineError> {
        self.check_sheet(source.sheet)?;
        self.checkpoint();
        let sheet = self.sheet_mut(source.sheet)?;
        let mut writes = Vec::new();
        match to {
            FillTo::Row(to) => {
                let targets: Vec<i32> = if to > source.last_row() {
                    (source.last_row() + 1..=to).collect()
                } else if to < source.row {
                    (to..source.row).collect()
                } else {
                    Vec::new()
                };
                for t in targets {
                    let from = source.row + (t - source.row).rem_euclid(source.height);
                    for c in source.column..=source.last_column() {
                        writes.push(((t, c), sheet.inputs.get(&(from, c)).cloned()));
                    }
                }
            }
            FillTo::Column(to) => {
                let targets: Vec<i32> = if to > source.last_column() {
                    (source.last_column() + 1..=to).collect()
                } else if to < source.column {
                    (to..source.column).collect()
                } else {
                    Vec::new()
                };
                for t in targets {
                    let from = source.column + (t - source.column).rem_euclid(source.width);
                    for r in source.row..=source.last_row() {
                        writes.push(((r, t), sheet.inputs.get(&(r, from)).cloned()));
                    }
                }
            }
        }
        for (key, value) in writes {
            match value {
                Some(v) => {
                    sheet.inputs.insert(key, v);
                }
                None => {
                    sheet.inputs.remove(&key);
                }
            }
        }
        Ok(())
    }

    fn insert_rows(&mut self, sheet: u32, row: i32, count: i32) -> Result<(), EngineError> {
        self.check_sheet(sheet)?;
        if count < 1 {
            return Err(String::from("Insert at least one row."));
        }
        self.checkpoint();
        let s = self.sheet_mut(sheet)?;
        s.inputs = shift_keys(&s.inputs, true, row, count);
        s.styles = shift_keys(&s.styles, true, row, count);
        s.heights = shift_lines(&s.heights, row, count);
        s.hidden_rows = s
            .hidden_rows
            .iter()
            .map(|&r| if r >= row { r + count } else { r })
            .collect();
        Ok(())
    }

    fn delete_rows(&mut self, sheet: u32, row: i32, count: i32) -> Result<(), EngineError> {
        self.check_sheet(sheet)?;
        if count < 1 {
            return Err(String::from("Delete at least one row."));
        }
        self.checkpoint();
        let s = self.sheet_mut(sheet)?;
        s.inputs = shift_keys(&s.inputs, true, row, -count);
        s.styles = shift_keys(&s.styles, true, row, -count);
        s.heights = shift_lines(&s.heights, row, -count);
        s.hidden_rows = s
            .hidden_rows
            .iter()
            .filter(|&&r| r < row || r >= row + count)
            .map(|&r| if r >= row + count { r - count } else { r })
            .collect();
        Ok(())
    }

    fn insert_columns(&mut self, sheet: u32, column: i32, count: i32) -> Result<(), EngineError> {
        self.check_sheet(sheet)?;
        if count < 1 {
            return Err(String::from("Insert at least one column."));
        }
        self.checkpoint();
        let s = self.sheet_mut(sheet)?;
        s.inputs = shift_keys(&s.inputs, false, column, count);
        s.styles = shift_keys(&s.styles, false, column, count);
        s.widths = shift_lines(&s.widths, column, count);
        Ok(())
    }

    fn delete_columns(&mut self, sheet: u32, column: i32, count: i32) -> Result<(), EngineError> {
        self.check_sheet(sheet)?;
        if count < 1 {
            return Err(String::from("Delete at least one column."));
        }
        self.checkpoint();
        let s = self.sheet_mut(sheet)?;
        s.inputs = shift_keys(&s.inputs, false, column, -count);
        s.styles = shift_keys(&s.styles, false, column, -count);
        s.widths = shift_lines(&s.widths, column, -count);
        Ok(())
    }

    fn column_width(&self, sheet: u32, column: i32) -> f64 {
        self.sheet(sheet)
            .and_then(|s| s.widths.get(&column).copied())
            .unwrap_or(FAKE_COLUMN_WIDTH)
    }

    fn set_column_width(
        &mut self,
        sheet: u32,
        first: i32,
        last: i32,
        px: f64,
    ) -> Result<(), EngineError> {
        self.check_sheet(sheet)?;
        if px.is_nan() || px < 0.0 {
            return Err(format!("Can not set a width of {px}."));
        }
        self.checkpoint();
        let s = self.sheet_mut(sheet)?;
        for c in first.max(1)..=last.min(LAST_COLUMN) {
            s.widths.insert(c, px);
        }
        Ok(())
    }

    fn row_height(&self, sheet: u32, row: i32) -> f64 {
        let Some(s) = self.sheet(sheet) else {
            return FAKE_ROW_HEIGHT;
        };
        if s.hidden_rows.contains(&row) {
            return 0.0;
        }
        s.heights.get(&row).copied().unwrap_or(FAKE_ROW_HEIGHT)
    }

    fn set_row_height(
        &mut self,
        sheet: u32,
        first: i32,
        last: i32,
        px: f64,
    ) -> Result<(), EngineError> {
        self.check_sheet(sheet)?;
        if px.is_nan() || px < 0.0 {
            return Err(format!("Can not set a height of {px}."));
        }
        self.checkpoint();
        let s = self.sheet_mut(sheet)?;
        for r in first.max(1)..=last.min(LAST_ROW) {
            s.heights.insert(r, px);
        }
        Ok(())
    }

    fn set_rows_hidden(
        &mut self,
        sheet: u32,
        first: i32,
        last: i32,
        hidden: bool,
    ) -> Result<(), EngineError> {
        self.check_sheet(sheet)?;
        self.checkpoint();
        let s = self.sheet_mut(sheet)?;
        for r in first.max(1)..=last.min(LAST_ROW) {
            if hidden {
                s.hidden_rows.insert(r);
            } else {
                s.hidden_rows.remove(&r);
            }
        }
        Ok(())
    }

    fn frozen(&self, sheet: u32) -> (i32, i32) {
        self.sheet(sheet).map_or((0, 0), |s| s.frozen)
    }

    fn set_frozen(&mut self, sheet: u32, rows: i32, columns: i32) -> Result<(), EngineError> {
        self.check_sheet(sheet)?;
        if rows < 0 || columns < 0 {
            return Err(String::from("Frozen panes can not be negative."));
        }
        self.checkpoint();
        self.sheet_mut(sheet)?.frozen = (rows, columns);
        Ok(())
    }

    fn show_grid_lines(&self, sheet: u32) -> bool {
        self.sheet(sheet).map_or(true, |s| s.grid_lines)
    }

    fn set_show_grid_lines(&mut self, sheet: u32, show: bool) -> Result<(), EngineError> {
        self.check_sheet(sheet)?;
        self.checkpoint();
        self.sheet_mut(sheet)?.grid_lines = show;
        Ok(())
    }

    fn extent(&self, sheet: u32) -> (i32, i32) {
        let Some(s) = self.sheet(sheet) else {
            return (0, 0);
        };
        s.inputs
            .keys()
            .chain(s.styles.keys())
            .fold((0, 0), |(mr, mc), &(r, c)| (mr.max(r), mc.max(c)))
    }

    fn defined_names(&self) -> Vec<DefinedName> {
        self.book.names.clone()
    }

    fn add_defined_name(
        &mut self,
        name: &str,
        scope: Option<u32>,
        formula: &str,
    ) -> Result<(), EngineError> {
        let name = name.trim();
        if name.is_empty() || !name.chars().next().is_some_and(char::is_alphabetic) {
            return Err(format!("\"{name}\" is not a valid name."));
        }
        if self
            .book
            .names
            .iter()
            .any(|n| n.scope == scope && n.name.eq_ignore_ascii_case(name))
        {
            return Err(format!("The name \"{name}\" exists already."));
        }
        self.checkpoint();
        self.book.names.push(DefinedName {
            name: name.to_string(),
            scope,
            formula: formula.to_string(),
        });
        Ok(())
    }

    fn delete_defined_name(&mut self, name: &str, scope: Option<u32>) -> Result<(), EngineError> {
        if !self
            .book
            .names
            .iter()
            .any(|n| n.scope == scope && n.name.eq_ignore_ascii_case(name))
        {
            return Err(format!("There is no name \"{name}\"."));
        }
        self.checkpoint();
        self.book
            .names
            .retain(|n| !(n.scope == scope && n.name.eq_ignore_ascii_case(name)));
        Ok(())
    }

    fn merges(&self, sheet: u32) -> Vec<CellArea> {
        let mut out: Vec<CellArea> = self
            .book
            .sheets
            .get(sheet as usize)
            .map(|s| s.merges.iter().map(|m| CellArea { sheet, ..*m }).collect())
            .unwrap_or_default();
        out.sort_by_key(|m| (m.row, m.column));
        out
    }

    fn merge(&mut self, area: CellArea) -> Result<(), EngineError> {
        self.check_sheet(area.sheet)?;
        if area.width * area.height <= 1 {
            return Ok(());
        }
        self.checkpoint();
        let merges = &mut self.book.sheets[area.sheet as usize].merges;
        merges.retain(|m| !CellArea { sheet: area.sheet, ..*m }.overlaps(&area));
        merges.push(area);
        Ok(())
    }

    fn unmerge(&mut self, area: CellArea) -> Result<(), EngineError> {
        self.check_sheet(area.sheet)?;
        self.checkpoint();
        self.book.sheets[area.sheet as usize]
            .merges
            .retain(|m| !CellArea { sheet: area.sheet, ..*m }.overlaps(&area));
        Ok(())
    }

    fn conditional_formats(&self, sheet: u32) -> Vec<ConditionalFormat> {
        self.book
            .sheets
            .get(sheet as usize)
            .map(|s| {
                s.conditional
                    .iter()
                    .enumerate()
                    .map(|(index, (area, rule, _))| ConditionalFormat {
                        index,
                        area: CellArea { sheet, ..*area },
                        description: rule.describe(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn add_conditional_format(&mut self, area: CellArea, rule: &CondRule, look: CondLook) -> Result<(), EngineError> {
        self.check_sheet(area.sheet)?;
        self.checkpoint();
        self.book.sheets[area.sheet as usize]
            .conditional
            .push((area, rule.clone(), look));
        Ok(())
    }

    fn clear_conditional_formats(&mut self, area: CellArea) -> Result<(), EngineError> {
        self.check_sheet(area.sheet)?;
        self.checkpoint();
        self.book.sheets[area.sheet as usize]
            .conditional
            .retain(|(a, _, _)| !CellArea { sheet: area.sheet, ..*a }.overlaps(&area));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(row: i32, column: i32) -> CellAddr {
        CellAddr::new(0, row, column)
    }

    #[test]
    fn numbers_booleans_and_text_parse_into_their_values() {
        let mut e = FakeEngine::new();
        e.set_cell_input(at(1, 1), "41.5").unwrap();
        e.set_cell_input(at(2, 1), "true").unwrap();
        e.set_cell_input(at(3, 1), "Rent").unwrap();
        assert_eq!(e.cell_value(at(1, 1)), CellValue::Number(41.5));
        assert_eq!(e.cell_value(at(2, 1)), CellValue::Boolean(true));
        assert_eq!(
            e.cell_value(at(3, 1)),
            CellValue::Text(String::from("Rent"))
        );
        assert_eq!(e.cell_value(at(4, 1)), CellValue::Empty);
        assert_eq!(e.cell_formatted(at(2, 1)), "TRUE");
    }

    #[test]
    fn a_sum_formula_adds_the_numbers_of_its_range_and_skips_text() {
        let mut e = FakeEngine::new();
        e.set_inputs(
            at(1, 1),
            &[
                vec![String::from("1")],
                vec![String::from("2")],
                vec![String::from("note")],
                vec![String::from("=SUM(A1:A3)")],
            ],
        )
        .unwrap();
        assert_eq!(e.cell_value(at(4, 1)), CellValue::Number(3.0));
        assert_eq!(e.cell_formatted(at(4, 1)), "3");
        assert_eq!(e.cell_input(at(4, 1)), "=SUM(A1:A3)");
        e.set_cell_input(at(5, 1), "=SUM(A1:A4)").unwrap();
        assert_eq!(
            e.cell_value(at(5, 1)),
            CellValue::Number(6.0),
            "a SUM over a SUM"
        );
    }

    #[test]
    fn a_number_format_shapes_the_displayed_text() {
        assert_eq!(format_number(1234.5, "#,##0.00"), "1,234.50");
        assert_eq!(format_number(-1234.5, "#,##0.00"), "-1,234.50");
        assert_eq!(format_number(0.03, "0.0%"), "3.0%");
        assert_eq!(format_number(7.0, "general"), "7");
        assert_eq!(format_number(0.25, "general"), "0.25");
    }

    #[test]
    fn every_change_is_one_undo_step_and_redo_brings_it_back() {
        let mut e = FakeEngine::new();
        assert!(!e.can_undo());
        e.set_cell_input(at(1, 1), "1").unwrap();
        e.set_cell_input(at(1, 1), "2").unwrap();
        e.undo().unwrap();
        assert_eq!(e.cell_input(at(1, 1)), "1");
        assert!(e.can_redo());
        e.redo().unwrap();
        assert_eq!(e.cell_input(at(1, 1)), "2");
        e.undo().unwrap();
        e.undo().unwrap();
        assert_eq!(e.cell_input(at(1, 1)), "");
        assert!(e.undo().is_err(), "nothing left to undo");
    }

    #[test]
    fn set_inputs_and_a_tsv_paste_are_one_undo_step_each() {
        let mut e = FakeEngine::new();
        e.set_inputs(
            at(1, 1),
            &[
                vec![String::from("a"), String::from("b")],
                vec![String::from("c"), String::from("d")],
            ],
        )
        .unwrap();
        e.paste_tsv(at(3, 1), "x\t\"y\tz\"\n1\t2\n").unwrap();
        assert_eq!(
            e.cell_input(at(3, 2)),
            "y\tz",
            "a quoted field keeps its tab"
        );
        assert_eq!(e.cell_input(at(4, 2)), "2");
        e.undo().unwrap();
        assert_eq!(e.cell_input(at(3, 1)), "");
        assert_eq!(e.cell_input(at(2, 2)), "d");
        e.undo().unwrap();
        assert_eq!(e.extent(0), (0, 0));
    }

    #[test]
    fn inserting_rows_shifts_the_cells_below_down_and_deleting_moves_them_back() {
        let mut e = FakeEngine::new();
        e.set_cell_input(at(1, 1), "top").unwrap();
        e.set_cell_input(at(3, 1), "below").unwrap();
        e.set_row_height(0, 3, 3, 40.0).unwrap();
        e.insert_rows(0, 2, 2).unwrap();
        assert_eq!(e.cell_input(at(1, 1)), "top");
        assert_eq!(e.cell_input(at(5, 1)), "below");
        assert_eq!(e.row_height(0, 5), 40.0, "the height moves with its row");
        e.delete_rows(0, 2, 2).unwrap();
        assert_eq!(e.cell_input(at(3, 1)), "below");
        e.delete_rows(0, 3, 1).unwrap();
        assert_eq!(e.extent(0), (1, 1));
    }

    #[test]
    fn a_hidden_row_has_no_height() {
        let mut e = FakeEngine::new();
        e.set_rows_hidden(0, 2, 3, true).unwrap();
        assert_eq!(e.row_height(0, 2), 0.0);
        assert_eq!(e.row_height(0, 4), FAKE_ROW_HEIGHT);
        e.set_rows_hidden(0, 2, 3, false).unwrap();
        assert_eq!(e.row_height(0, 3), FAKE_ROW_HEIGHT);
    }

    #[test]
    fn the_fill_handle_repeats_the_source_down() {
        let mut e = FakeEngine::new();
        e.set_inputs(
            at(1, 1),
            &[vec![String::from("a")], vec![String::from("b")]],
        )
        .unwrap();
        e.auto_fill(CellArea::spanning(0, 1, 1, 2, 1), FillTo::Row(5))
            .unwrap();
        let col: Vec<String> = (1..=5).map(|r| e.cell_input(at(r, 1))).collect();
        assert_eq!(col, vec!["a", "b", "a", "b", "a"]);
    }

    #[test]
    fn a_saved_fake_workbook_reads_back() {
        let mut e = FakeEngine::new();
        e.set_cell_input(at(2, 3), "=SUM(A1:A2)").unwrap();
        e.set_column_width(0, 1, 1, 140.0).unwrap();
        e.set_frozen(0, 2, 0).unwrap();
        e.add_defined_name("Rate", None, "Sheet1!$C$2").unwrap();
        let bytes = e.save_xlsx().unwrap();
        let mut back = FakeEngine::new();
        back.load_xlsx(&bytes, "copy").unwrap();
        assert_eq!(back.cell_input(at(2, 3)), "=SUM(A1:A2)");
        assert_eq!(back.column_width(0, 1), 140.0);
        assert_eq!(back.frozen(0), (2, 0));
        assert_eq!(back.defined_names().len(), 1);
        assert!(!back.can_undo(), "a loaded workbook starts without history");
    }
}
