//! What AzSheets does on top of any [`SheetEngine`]: the status bar's
//! statistics, sort, remove duplicates, filter, find, CSV export and the
//! AutoSum range. IronCalc has no sort / filter / find (excel.md §1), so these
//! run app-side over the engine's cells, each write as ONE undo step
//! (`set_inputs`).

use std::{cmp::Ordering, collections::HashSet};

use azul_appkit::find::{self, TextMatch};

use crate::engine::{CellAddr, CellArea, CellValue, EngineError, HAlign, SheetEngine, StylePatch};

/// Count / Sum / Min / Max of a selection (the status bar's Average comes
/// from [`SelectionStats::average`]).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct SelectionStats {
    /// Non-empty cells.
    pub count: usize,
    /// Cells holding a number.
    pub numbers: usize,
    pub sum: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

impl SelectionStats {
    /// The mean of the numbers; `None` without numbers.
    #[must_use]
    pub fn average(&self) -> Option<f64> {
        (self.numbers > 0).then(|| self.sum / self.numbers as f64)
    }
}

/// The statistics of `areas` (each clipped to its sheet's data; a cell in
/// two areas counts once).
#[must_use]
pub fn selection_stats(engine: &dyn SheetEngine, areas: &[CellArea]) -> SelectionStats {
    let mut seen = HashSet::new();
    let mut stats = SelectionStats::default();
    for area in areas {
        let (max_row, max_column) = engine.extent(area.sheet);
        let Some(clipped) = area.clip(max_row, max_column) else {
            continue;
        };
        for row in clipped.row..=clipped.last_row() {
            for column in clipped.column..=clipped.last_column() {
                if !seen.insert((area.sheet, row, column)) {
                    continue;
                }
                match engine.cell_value(CellAddr::new(area.sheet, row, column)) {
                    CellValue::Empty => {}
                    CellValue::Number(n) => {
                        stats.count += 1;
                        stats.numbers += 1;
                        stats.sum += n;
                        stats.min = Some(stats.min.map_or(n, |m| m.min(n)));
                        stats.max = Some(stats.max.map_or(n, |m| m.max(n)));
                    }
                    _ => stats.count += 1,
                }
            }
        }
    }
    stats
}

/// Sort order of the kinds of value: numbers, text, booleans, errors; empty
/// cells always go last.
fn rank(value: &CellValue) -> u8 {
    match value {
        CellValue::Number(_) => 0,
        CellValue::Text(_) => 1,
        CellValue::Boolean(_) => 2,
        CellValue::Error(_) => 3,
        CellValue::Empty => 4,
    }
}

/// Ascending order of two non-empty values.
fn compare(a: &CellValue, b: &CellValue) -> Ordering {
    match (a, b) {
        (CellValue::Number(x), CellValue::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (CellValue::Text(x), CellValue::Text(y)) => x.to_lowercase().cmp(&y.to_lowercase()),
        (CellValue::Boolean(x), CellValue::Boolean(y)) => x.cmp(y),
        (CellValue::Error(x), CellValue::Error(y)) => x.cmp(y),
        _ => rank(a).cmp(&rank(b)),
    }
}

/// The rows of `area` below its header (if `has_header`), clipped to the
/// sheet's data: (first row, the area), or `None` when there are none.
fn data_rows(
    engine: &dyn SheetEngine,
    area: CellArea,
    has_header: bool,
) -> Option<(i32, CellArea)> {
    let (max_row, max_column) = engine.extent(area.sheet);
    let clipped = area.clip(max_row, max_column)?;
    let first = area.row + i32::from(has_header);
    (first.max(clipped.row) <= clipped.last_row()).then_some((first.max(clipped.row), clipped))
}

/// The inputs of row `row` across `area`'s columns.
fn row_inputs(engine: &dyn SheetEngine, area: CellArea, row: i32) -> Vec<String> {
    (area.column..=area.last_column())
        .map(|column| engine.cell_input(CellAddr::new(area.sheet, row, column)))
        .collect()
}

/// Sorts the rows of `area` (the header row stays) by the VALUE in
/// `key_column`: numbers before text (case-insensitive) before booleans
/// before errors, empty cells last in both directions; equal keys keep their
/// order. The rows' INPUTS move (a formula moves as its text: its references
/// are not rewritten), written back as one undo step.
pub fn sort_area(
    engine: &mut dyn SheetEngine,
    area: CellArea,
    key_column: i32,
    descending: bool,
    has_header: bool,
) -> Result<(), EngineError> {
    if key_column < area.column || key_column > area.last_column() {
        return Err(String::from("The sort column is outside the selection."));
    }
    let Some((first, clipped)) = data_rows(engine, area, has_header) else {
        return Ok(());
    };
    let mut rows: Vec<(CellValue, Vec<String>)> = (first..=clipped.last_row())
        .map(|row| {
            (
                engine.cell_value(CellAddr::new(area.sheet, row, key_column)),
                row_inputs(engine, clipped, row),
            )
        })
        .collect();
    rows.sort_by(
        |a, b| match (a.0 == CellValue::Empty, b.0 == CellValue::Empty) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => {
                let order = compare(&a.0, &b.0);
                if descending {
                    order.reverse()
                } else {
                    order
                }
            }
        },
    );
    let out: Vec<Vec<String>> = rows.into_iter().map(|(_, inputs)| inputs).collect();
    engine.set_inputs(CellAddr::new(area.sheet, first, clipped.column), &out)
}

/// Drops the rows of `area` (below its header) that repeat an earlier row in
/// every column, moves the rest up and clears the freed rows: one undo step.
/// Returns how many rows went.
pub fn remove_duplicates(
    engine: &mut dyn SheetEngine,
    area: CellArea,
    has_header: bool,
) -> Result<usize, EngineError> {
    let Some((first, clipped)) = data_rows(engine, area, has_header) else {
        return Ok(0);
    };
    let rows: Vec<Vec<String>> = (first..=clipped.last_row())
        .map(|row| row_inputs(engine, clipped, row))
        .collect();
    let mut seen = HashSet::new();
    let mut out: Vec<Vec<String>> = rows
        .iter()
        .filter(|row| seen.insert((*row).clone()))
        .cloned()
        .collect();
    let removed = rows.len() - out.len();
    if removed == 0 {
        return Ok(0);
    }
    let width = clipped.width.max(0) as usize;
    out.extend(std::iter::repeat(vec![String::new(); width]).take(removed));
    engine.set_inputs(CellAddr::new(area.sheet, first, clipped.column), &out)?;
    Ok(removed)
}

/// A filter on one column of `area` (its first row is the header): with
/// `keep`, the data rows whose displayed value in `column` is not `keep`
/// (case-insensitive) are hidden and the rest shown; without, every data row
/// is shown again. Returns how many rows are hidden.
pub fn filter_rows(
    engine: &mut dyn SheetEngine,
    area: CellArea,
    column: i32,
    keep: Option<&str>,
) -> Result<usize, EngineError> {
    let Some((first, clipped)) = data_rows(engine, area, true) else {
        return Ok(0);
    };
    let last = clipped.last_row();
    let Some(keep) = keep else {
        engine.set_rows_hidden(area.sheet, first, last, false)?;
        return Ok(0);
    };
    let wanted = keep.to_lowercase();
    let hide: Vec<bool> = (first..=last)
        .map(|row| {
            engine
                .cell_formatted(CellAddr::new(area.sheet, row, column))
                .to_lowercase()
                != wanted
        })
        .collect();
    // One call per run of rows in the same state.
    let mut start = 0usize;
    while start < hide.len() {
        let mut end = start;
        while end + 1 < hide.len() && hide[end + 1] == hide[start] {
            end += 1;
        }
        engine.set_rows_hidden(
            area.sheet,
            first + start as i32,
            first + end as i32,
            hide[start],
        )?;
        start = end + 1;
    }
    Ok(hide.iter().filter(|h| **h).count())
}

/// The next cell after `from` (row by row, wrapping once, `from` itself
/// last) whose displayed value or input contains `needle`, ignoring case:
/// [`find_match`] with the default options.
#[must_use]
pub fn find_next(engine: &dyn SheetEngine, from: CellAddr, needle: &str) -> Option<CellAddr> {
    find_match(engine, from, needle, FindOptions::default())
}

/// How Find / Replace match (the standard FindReplaceDialog's options).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct FindOptions {
    /// Upper and lower case differ.
    pub match_case: bool,
    /// The needle must stand as a whole word (not inside a longer one).
    pub whole_word: bool,
    /// Find previous: row by row backwards.
    pub backwards: bool,
}

/// The next (or previous) cell after `from` - row by row, wrapping once,
/// `from` itself last - whose displayed value or input holds `needle`.
#[must_use]
pub fn find_match(engine: &dyn SheetEngine, from: CellAddr, needle: &str, opts: FindOptions) -> Option<CellAddr> {
    if needle.is_empty() {
        return None;
    }
    let (max_row, max_column) = engine.extent(from.sheet);
    if max_row < 1 || max_column < 1 {
        return None;
    }
    let columns = i64::from(max_column);
    let total = i64::from(max_row) * columns;
    let inside =
        from.row >= 1 && from.row <= max_row && from.column >= 1 && from.column <= max_column;
    let start = match (inside, opts.backwards) {
        (true, _) => i64::from(from.row - 1) * columns + i64::from(from.column - 1),
        (false, false) => -1,
        (false, true) => total,
    };
    for step in 1..=total {
        let i = (if opts.backwards { start - step } else { start + step }).rem_euclid(total);
        #[allow(clippy::cast_possible_truncation)]
        let at = CellAddr::new(from.sheet, (i / columns) as i32 + 1, (i % columns) as i32 + 1);
        if holds(&engine.cell_formatted(at), needle, opts) || holds(&engine.cell_input(at), needle, opts) {
            return Some(at);
        }
    }
    None
}

/// How the shared matcher (azul-appkit's `find`) reads the options.
const fn how(opts: FindOptions) -> TextMatch {
    TextMatch {
        match_case: opts.match_case,
        whole_word: opts.whole_word,
    }
}

/// Whether `text` holds `needle` under `opts`.
fn holds(text: &str, needle: &str, opts: FindOptions) -> bool {
    find::holds(text, needle, how(opts))
}

/// `text` with every match of `needle` replaced by `replacement`; `None`
/// when nothing matches.
#[must_use]
pub fn replace_text(text: &str, needle: &str, replacement: &str, opts: FindOptions) -> Option<String> {
    find::replace(text, needle, replacement, how(opts))
}

/// Replace All on `sheet`: every cell whose INPUT holds `needle` gets it
/// replaced (formulas too, as Excel does), as ONE undo step; how many cells
/// changed.
pub fn replace_all(
    engine: &mut dyn SheetEngine,
    sheet: u32,
    needle: &str,
    replacement: &str,
    opts: FindOptions,
) -> Result<usize, EngineError> {
    let (max_row, max_column) = engine.extent(sheet);
    let mut changed: std::collections::HashMap<(i32, i32), String> = std::collections::HashMap::new();
    for row in 1..=max_row {
        for column in 1..=max_column {
            let input = engine.cell_input(CellAddr::new(sheet, row, column));
            if let Some(new) = replace_text(&input, needle, replacement, opts) {
                changed.insert((row, column), new);
            }
        }
    }
    if changed.is_empty() {
        return Ok(0);
    }
    // One paste of the changed cells' bounding block = one undo step; the
    // cells in it without a match are written back as they were.
    let r0 = changed.keys().map(|k| k.0).min().unwrap_or(1);
    let r1 = changed.keys().map(|k| k.0).max().unwrap_or(1);
    let c0 = changed.keys().map(|k| k.1).min().unwrap_or(1);
    let c1 = changed.keys().map(|k| k.1).max().unwrap_or(1);
    let rows: Vec<Vec<String>> = (r0..=r1)
        .map(|row| {
            (c0..=c1)
                .map(|column| {
                    changed
                        .get(&(row, column))
                        .cloned()
                        .unwrap_or_else(|| engine.cell_input(CellAddr::new(sheet, row, column)))
                })
                .collect()
        })
        .collect();
    engine.set_inputs(CellAddr::new(sheet, r0, c0), &rows)?;
    Ok(changed.len())
}

/// Merge & Center: `area` becomes one cell holding its top-left cell's input
/// (the other inputs are cleared, one undo step), centred.
pub fn merge_and_center(engine: &mut dyn SheetEngine, area: CellArea) -> Result<(), EngineError> {
    let keep = engine.cell_input(CellAddr::new(area.sheet, area.row, area.column));
    let others_hold_input = (area.row..=area.last_row()).any(|r| {
        (area.column..=area.last_column())
            .any(|c| (r, c) != (area.row, area.column) && !engine.cell_input(CellAddr::new(area.sheet, r, c)).is_empty())
    });
    if others_hold_input {
        let rows: Vec<Vec<String>> = (0..area.height)
            .map(|r| {
                (0..area.width)
                    .map(|c| if r == 0 && c == 0 { keep.clone() } else { String::new() })
                    .collect()
            })
            .collect();
        engine.set_inputs(CellAddr::new(area.sheet, area.row, area.column), &rows)?;
    }
    engine.update_style(area, &StylePatch::HAlign(HAlign::Center))?;
    engine.merge(area)
}

/// The sheet's used range as CSV (RFC 4180 quoting, `\n` lines), the
/// displayed values.
#[must_use]
pub fn export_csv(engine: &dyn SheetEngine, sheet: u32) -> String {
    let (max_row, max_column) = engine.extent(sheet);
    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(Vec::new());
    for row in 1..=max_row {
        let record: Vec<String> = (1..=max_column)
            .map(|column| engine.cell_formatted(CellAddr::new(sheet, row, column)))
            .collect();
        if writer.write_record(&record).is_err() {
            break;
        }
    }
    let bytes = writer.into_inner().unwrap_or_default();
    String::from_utf8(bytes).unwrap_or_default()
}

/// AutoSum's range for a formula in `at`: the run of numbers directly above
/// it (one empty cell right above is skipped), else the run to its left;
/// `None` when neither has a number.
#[must_use]
pub fn sum_range_above(engine: &dyn SheetEngine, at: CellAddr) -> Option<CellArea> {
    let value = |row: i32, column: i32| engine.cell_value(CellAddr::new(at.sheet, row, column));
    let is_number = |row: i32, column: i32| matches!(value(row, column), CellValue::Number(_));
    let is_empty = |row: i32, column: i32| value(row, column) == CellValue::Empty;

    let mut row = at.row - 1;
    if row >= 1 && is_empty(row, at.column) {
        row -= 1;
    }
    let bottom = row;
    while row >= 1 && is_number(row, at.column) {
        row -= 1;
    }
    if row < bottom {
        return Some(CellArea::spanning(
            at.sheet,
            row + 1,
            at.column,
            bottom,
            at.column,
        ));
    }

    let mut column = at.column - 1;
    if column >= 1 && is_empty(at.row, column) {
        column -= 1;
    }
    let right = column;
    while column >= 1 && is_number(at.row, column) {
        column -= 1;
    }
    (column < right).then(|| CellArea::spanning(at.sheet, at.row, column + 1, at.row, right))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake_engine::FakeEngine;

    fn at(row: i32, column: i32) -> CellAddr {
        CellAddr::new(0, row, column)
    }

    fn strings(row: &[&str]) -> Vec<String> {
        row.iter().map(|s| s.to_string()).collect()
    }

    fn engine_with(rows: &[&[&str]]) -> FakeEngine {
        let mut e = FakeEngine::new();
        let rows: Vec<Vec<String>> = rows.iter().map(|r| strings(r)).collect();
        e.set_inputs(at(1, 1), &rows).unwrap();
        e
    }

    fn column(e: &FakeEngine, column: i32, rows: std::ops::RangeInclusive<i32>) -> Vec<String> {
        rows.map(|r| e.cell_input(at(r, column))).collect()
    }

    #[test]
    fn the_statistics_count_a_cell_in_two_overlapping_areas_once() {
        let e = engine_with(&[&["1", "x"], &["2", ""], &["3", "TRUE"]]);
        let a = CellArea::spanning(0, 1, 1, 3, 1);
        let b = CellArea::spanning(0, 2, 1, 3, 2);
        let stats = selection_stats(&e, &[a, b]);
        assert_eq!(stats.numbers, 3);
        assert_eq!(
            stats.count, 4,
            "1, 2, 3 and TRUE; the empty cell is not counted"
        );
        assert_eq!(stats.sum, 6.0);
        assert_eq!(stats.average(), Some(2.0));
        assert_eq!((stats.min, stats.max), (Some(1.0), Some(3.0)));
    }

    #[test]
    fn the_statistics_of_a_whole_column_stop_at_the_data() {
        let e = engine_with(&[&["5"], &["7"]]);
        let whole = CellArea::spanning(0, 1, 1, crate::engine::LAST_ROW, 1);
        assert_eq!(selection_stats(&e, &[whole]).sum, 12.0);
        assert_eq!(selection_stats(&e, &[]).average(), None);
    }

    #[test]
    fn sorting_puts_numbers_before_text_keeps_the_header_and_is_one_undo_step() {
        let mut e = engine_with(&[
            &["Item", "Cost"],
            &["pear", "3"],
            &["Apple", "x"],
            &["fig", ""],
            &["kiwi", "1"],
        ]);
        let area = CellArea::spanning(0, 1, 1, 5, 2);
        sort_area(&mut e, area, 2, false, true).unwrap();
        assert_eq!(
            column(&e, 1, 1..=5),
            strings(&["Item", "kiwi", "pear", "Apple", "fig"])
        );
        e.undo().unwrap();
        assert_eq!(
            column(&e, 1, 1..=5),
            strings(&["Item", "pear", "Apple", "fig", "kiwi"])
        );

        sort_area(&mut e, area, 1, true, true).unwrap();
        assert_eq!(
            column(&e, 1, 2..=5),
            strings(&["pear", "kiwi", "fig", "Apple"])
        );
        assert_eq!(e.cell_input(at(2, 2)), "3", "a row moves as a whole");
        assert!(sort_area(&mut e, area, 7, false, true).is_err());
    }

    #[test]
    fn removing_duplicates_moves_the_unique_rows_up_and_counts_the_rest() {
        let mut e = engine_with(&[&["Name"], &["a"], &["b"], &["a"], &["c"], &["b"]]);
        let removed = remove_duplicates(&mut e, CellArea::spanning(0, 1, 1, 6, 1), true).unwrap();
        assert_eq!(removed, 2);
        assert_eq!(
            column(&e, 1, 1..=6),
            strings(&["Name", "a", "b", "c", "", ""])
        );
        assert_eq!(
            remove_duplicates(&mut e, CellArea::spanning(0, 1, 1, 4, 1), true).unwrap(),
            0
        );
    }

    #[test]
    fn a_filter_hides_the_other_rows_and_clearing_it_shows_them_again() {
        let mut e = engine_with(&[&["City"], &["Graz"], &["Wien"], &["graz"], &["Linz"]]);
        let area = CellArea::spanning(0, 1, 1, 5, 1);
        assert_eq!(filter_rows(&mut e, area, 1, Some("Graz")).unwrap(), 2);
        let heights: Vec<f64> = (1..=5).map(|r| e.row_height(0, r)).collect();
        assert!(heights[0] > 0.0, "the header stays");
        assert!(heights[1] > 0.0 && heights[3] > 0.0);
        assert_eq!((heights[2], heights[4]), (0.0, 0.0));
        filter_rows(&mut e, area, 1, None).unwrap();
        assert!((1..=5).all(|r| e.row_height(0, r) > 0.0));
    }

    #[test]
    fn find_goes_row_by_row_and_wraps_to_the_top() {
        let e = engine_with(&[&["apple", "x"], &["", "Pineapple"], &["", "y"]]);
        assert_eq!(find_next(&e, at(1, 1), "APPLE"), Some(at(2, 2)));
        assert_eq!(find_next(&e, at(2, 2), "apple"), Some(at(1, 1)), "wraps");
        assert_eq!(find_next(&e, at(1, 1), "nothing"), None);
        assert_eq!(find_next(&e, at(1, 1), ""), None);
    }

    #[test]
    fn the_csv_export_quotes_what_needs_quoting() {
        let e = engine_with(&[&["a,b", "say \"hi\""], &["1", ""]]);
        assert_eq!(export_csv(&e, 0), "\"a,b\",\"say \"\"hi\"\"\"\n1,\n");
        assert_eq!(export_csv(&FakeEngine::new(), 0), "");
    }

    #[test]
    fn autosum_takes_the_numbers_above_and_else_the_numbers_to_the_left() {
        let e = engine_with(&[
            &["Jan", "Feb", ""],
            &["1", "2", ""],
            &["3", "4", ""],
            &["", "", ""],
            &["5", "6", ""],
        ]);
        assert_eq!(
            sum_range_above(&e, at(4, 1)),
            Some(CellArea::spanning(0, 2, 1, 3, 1))
        );
        assert_eq!(
            sum_range_above(&e, at(5, 1)),
            Some(CellArea::spanning(0, 2, 1, 3, 1)),
            "one empty cell right above is skipped"
        );
        assert_eq!(
            sum_range_above(&e, at(5, 3)),
            Some(CellArea::spanning(0, 5, 1, 5, 2)),
            "nothing above: the run to the left"
        );
        assert_eq!(sum_range_above(&e, at(1, 3)), None);
    }

    /// The standard FindReplaceDialog asks for match case, whole word and
    /// Find previous; the side panel's Find knew only "next, any case".
    #[test]
    fn merge_and_center_keeps_the_top_left_input_centred_over_the_area() {
        let mut e = engine_with(&[&["Budget", "x"], &["1", "2"]]);
        let area = CellArea::spanning(0, 1, 1, 2, 2);
        merge_and_center(&mut e, area).unwrap();
        assert_eq!(e.merges(0), vec![area]);
        assert_eq!(e.cell_input(at(1, 1)), "Budget");
        for (r, c) in [(1, 2), (2, 1), (2, 2)] {
            assert_eq!(e.cell_input(at(r, c)), "", "the other cells are cleared");
        }
        assert_eq!(e.cell_style(at(1, 1)).h_align, crate::engine::HAlign::Center);
        e.unmerge(area).unwrap();
        assert!(e.merges(0).is_empty());
    }

    #[test]
    fn find_honours_case_whole_words_and_the_direction() {
        let e = engine_with(&[&["Rent", "rental"], &["rent", "x"], &["", "RENT"]]);
        let any = FindOptions::default();
        assert_eq!(find_match(&e, at(1, 1), "rent", any), Some(at(1, 2)), "the next cell, any case");
        let case = FindOptions { match_case: true, ..any };
        assert_eq!(find_match(&e, at(1, 1), "rent", case), Some(at(1, 2)));
        assert_eq!(find_match(&e, at(1, 2), "rent", case), Some(at(2, 1)), "rental's 'rent', then row 2");
        let whole = FindOptions { whole_word: true, ..any };
        assert_eq!(find_match(&e, at(1, 1), "rent", whole), Some(at(2, 1)), "'rental' is no whole word");
        let back = FindOptions { backwards: true, ..any };
        assert_eq!(find_match(&e, at(2, 1), "rent", back), Some(at(1, 2)), "the previous cell");
        assert_eq!(find_match(&e, at(1, 1), "rent", back), Some(at(3, 2)), "wraps to the end");
        assert_eq!(find_match(&e, at(1, 1), "", any), None);
    }

    #[test]
    fn replacing_keeps_the_rest_of_the_text_and_respects_the_options() {
        let any = FindOptions::default();
        assert_eq!(replace_text("Rent and rent", "rent", "Lease", any).as_deref(), Some("Lease and Lease"));
        let case = FindOptions { match_case: true, ..any };
        assert_eq!(replace_text("Rent and rent", "rent", "lease", case).as_deref(), Some("Rent and lease"));
        let whole = FindOptions { whole_word: true, ..any };
        assert_eq!(replace_text("rental rent", "rent", "x", whole).as_deref(), Some("rental x"));
        assert_eq!(replace_text("nothing", "rent", "x", any), None);
        assert_eq!(replace_text("=SUM(A1:A2)", "A2", "A3", any).as_deref(), Some("=SUM(A1:A3)"), "formulas too");
    }

    #[test]
    fn replace_all_rewrites_every_matching_cell_as_one_undo_step() {
        let mut e = engine_with(&[&["Rent", "rent 2"], &["Food", "=1+1"], &["", "RENT"]]);
        let before_undo = e.can_undo();
        let n = replace_all(&mut e, 0, "rent", "Lease", FindOptions::default()).unwrap();
        assert_eq!(n, 3);
        assert_eq!(e.cell_input(at(1, 1)), "Lease");
        assert_eq!(e.cell_input(at(1, 2)), "Lease 2");
        assert_eq!(e.cell_input(at(3, 2)), "Lease");
        assert_eq!(e.cell_input(at(2, 1)), "Food", "a cell without a match is untouched");
        assert_eq!(e.cell_input(at(2, 2)), "=1+1");
        assert!(e.can_undo());
        e.undo().unwrap();
        assert_eq!(e.cell_input(at(1, 1)), "Rent", "one undo brings all of it back");
        assert_eq!(e.cell_input(at(3, 2)), "RENT");
        let _ = before_undo;
    }
}
