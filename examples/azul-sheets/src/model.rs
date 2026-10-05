//! The UI model's arithmetic, without azul: which cells to fetch for the
//! window in view, what a fill drag means for the engine, number-format
//! steps, the status bar's numbers, colours. Tested on its own.

use crate::{
    engine::{CellArea, FillTo, SheetInfo, LAST_COLUMN, LAST_ROW},
    ops::SelectionStats,
};

/// Rows fetched beyond the window, above and below (a few wheel steps
/// scroll without waiting for the engine).
pub const OVERSCAN_ROWS: i32 = 30;
/// Columns fetched beyond the window.
pub const OVERSCAN_COLUMNS: i32 = 8;
/// A sheet whose data has at most this many cells is fetched whole.
pub const WHOLE_SHEET_CELLS: i64 = 20_000;

/// The row and column spans (1-based, inclusive) to fetch for a window
/// whose first scrolled row / column is `top` / `left` (1-based), showing
/// `rows` x `columns`, below `frozen` (rows, columns) frozen ones, on a
/// sheet whose data reaches `extent` (max row, max column). A small sheet
/// is fetched whole (plus the window, if it is scrolled past the data).
#[must_use]
pub fn spans_to_fetch(
    top: i32,
    left: i32,
    rows: i32,
    columns: i32,
    frozen: (i32, i32),
    extent: (i32, i32),
) -> (Vec<(i32, i32)>, Vec<(i32, i32)>) {
    let span = |first: i32, count: i32, over: i32, limit: i32| -> (i32, i32) {
        let lo = (first - over).max(1);
        let hi = (first + count + over).min(limit).max(lo);
        (lo, hi)
    };
    let mut row_spans = Vec::new();
    let mut column_spans = Vec::new();
    if frozen.0 > 0 {
        row_spans.push((1, frozen.0.min(LAST_ROW)));
    }
    if frozen.1 > 0 {
        column_spans.push((1, frozen.1.min(LAST_COLUMN)));
    }
    let whole = i64::from(extent.0.max(0)) * i64::from(extent.1.max(0)) <= WHOLE_SHEET_CELLS;
    if whole && extent.0 > 0 && extent.1 > 0 {
        row_spans.push((1, extent.0.max(1)));
        column_spans.push((1, extent.1.max(1)));
    }
    row_spans.push(span(top, rows, OVERSCAN_ROWS, LAST_ROW));
    column_spans.push(span(left, columns, OVERSCAN_COLUMNS, LAST_COLUMN));
    (merge(row_spans), merge(column_spans))
}

/// Spans sorted, overlapping or touching ones joined.
#[must_use]
pub fn merge(mut spans: Vec<(i32, i32)>) -> Vec<(i32, i32)> {
    spans.sort_unstable();
    let mut out: Vec<(i32, i32)> = Vec::with_capacity(spans.len());
    for (lo, hi) in spans {
        match out.last_mut() {
            Some(last) if lo <= last.1 + 1 => last.1 = last.1.max(hi),
            _ => out.push((lo, hi)),
        }
    }
    out
}

/// Whether `have` covers every line of `want`.
#[must_use]
pub fn covers(have: &[(i32, i32)], want: &[(i32, i32)]) -> bool {
    want.iter()
        .all(|&(lo, hi)| have.iter().any(|&(a, b)| a <= lo && hi <= b))
}

/// What a fill drag from `source` to `reach` (the source plus the dragged
/// extension) asks of the engine; `None` when nothing was extended.
#[must_use]
pub fn fill_to(source: CellArea, reach: CellArea) -> Option<FillTo> {
    if reach.last_row() > source.last_row() {
        Some(FillTo::Row(reach.last_row()))
    } else if reach.row < source.row {
        Some(FillTo::Row(reach.row))
    } else if reach.last_column() > source.last_column() {
        Some(FillTo::Column(reach.last_column()))
    } else if reach.column < source.column {
        Some(FillTo::Column(reach.column))
    } else {
        None
    }
}

/// One more (or one fewer) decimal in a number format: "general" and
/// "#,##0" become "#,##0.0"; "0.00%" becomes "0.000%" / "0.0%".
#[must_use]
pub fn step_decimals(format: &str, more: bool) -> String {
    let base = if format.eq_ignore_ascii_case("general") || format.is_empty() {
        "#,##0"
    } else {
        format
    };
    // The first numeric section only ("#,##0.00;[Red]-#,##0.00" keeps its
    // other sections untouched).
    let (first, rest) = match base.find(';') {
        Some(i) => (&base[..i], &base[i..]),
        None => (base, ""),
    };
    let last_zero = first.rfind('0');
    let Some(at) = last_zero else {
        return base.to_string();
    };
    let (head, tail) = first.split_at(at + 1);
    let decimals = head
        .rfind('.')
        .map_or(0, |dot| head[dot + 1..].chars().filter(|c| *c == '0').count());
    let integer_part = head.rfind('.').map_or(head, |dot| &head[..dot]);
    let wanted = if more {
        decimals + 1
    } else {
        decimals.saturating_sub(1)
    };
    let mut out = String::from(integer_part);
    if wanted > 0 {
        out.push('.');
        out.push_str(&"0".repeat(wanted));
    }
    out.push_str(tail);
    out.push_str(rest);
    out
}

/// A number as the status bar shows it: thousands separated, at most two
/// decimals, no trailing zeros ("1,234.5", "-0.25", "12").
#[must_use]
pub fn format_number(v: f64) -> String {
    if !v.is_finite() {
        return String::from("#NUM!");
    }
    let rounded = (v * 100.0).round() / 100.0;
    let negative = rounded < 0.0;
    let text = format!("{:.2}", rounded.abs());
    let (int, frac) = text.split_once('.').unwrap_or((text.as_str(), ""));
    let mut grouped = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    let frac = frac.trim_end_matches('0');
    let mut out = String::new();
    if negative && (grouped != "0" || !frac.is_empty()) {
        out.push('-');
    }
    out.push_str(&grouped);
    if !frac.is_empty() {
        out.push('.');
        out.push_str(frac);
    }
    out
}

/// The status bar's statistics of a selection: Excel shows Average and Sum
/// only when numbers are selected, Count when more than one cell is.
#[must_use]
pub fn stats_segments(stats: &SelectionStats) -> Vec<String> {
    let mut out = Vec::new();
    if stats.numbers > 0 {
        if let Some(avg) = stats.average() {
            out.push(format!("Average: {}", format_number(avg)));
        }
    }
    if stats.count > 1 || stats.numbers > 0 {
        out.push(format!("Count: {}", stats.count));
    }
    if stats.numbers > 0 {
        out.push(format!("Sum: {}", format_number(stats.sum)));
    }
    out
}

/// The width that fits the widest of `texts` at `font_px` (an estimate
/// from the character count: the engine has no autofit and the grid's text
/// is measured only at layout), clamped to 30..=600 px.
#[must_use]
pub fn autofit_px<'a>(texts: impl Iterator<Item = &'a str>, font_px: f64) -> f64 {
    let widest = texts.map(|t| t.chars().count()).max().unwrap_or(0);
    #[allow(clippy::cast_precision_loss)]
    let px = widest as f64 * font_px * 0.6 + 12.0;
    px.clamp(30.0, 600.0)
}

/// Rows of cell inputs as the engine's tab-separated paste text (the csv
/// crate, the same that reads it on the engine side) - the ONE encoder of
/// the app (the ribbon's Paste and the engine adapter's `set_inputs`).
///
/// The block is padded to a rectangle with empty fields: IronCalc reads a
/// paste with a csv reader that is not flexible and drops every record
/// whose length differs from the first one (the Budget sample's one-cell
/// title row cost it every row under it).
#[must_use]
pub fn tsv_of(rows: &[Vec<String>]) -> String {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut writer = csv::WriterBuilder::new()
        .delimiter(b'\t')
        .terminator(csv::Terminator::Any(b'\n'))
        .has_headers(false)
        .from_writer(Vec::new());
    for row in rows {
        let mut record: Vec<&str> = row.iter().map(String::as_str).collect();
        record.resize(width, "");
        if writer.write_record(&record).is_err() {
            return String::new();
        }
    }
    writer
        .into_inner()
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default()
}

/// The sheet Ctrl+PageDown (`forward`) or Ctrl+PageUp goes to from `from`:
/// the next visible one, wrapping; `None` when no other sheet is visible.
#[must_use]
pub fn step_sheet(sheets: &[SheetInfo], from: u32, forward: bool) -> Option<u32> {
    let n = sheets.len();
    if n < 2 {
        return None;
    }
    let from = (from as usize).min(n - 1);
    (1..n)
        .map(|k| if forward { (from + k) % n } else { (from + n - k) % n })
        .find(|i| !sheets[*i].hidden)
        .and_then(|i| u32::try_from(i).ok())
}

/// The title a new workbook gets: "Book1", "Book2", ... - the first one no
/// workbook in `taken` uses.
#[must_use]
pub fn next_book_title(taken: &[String]) -> String {
    (1..)
        .map(|n| format!("Book{n}"))
        .find(|t| !taken.iter().any(|x| x == t))
        .unwrap_or_else(|| String::from("Book"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_sheet_is_fetched_whole_and_a_big_one_around_the_window() {
        let (rows, columns) = spans_to_fetch(1, 1, 30, 12, (0, 0), (20, 6));
        assert_eq!(rows, vec![(1, 61)], "the data and the window with its overscan");
        assert_eq!(columns, vec![(1, 21)]);
        let (rows, columns) = spans_to_fetch(5000, 3, 30, 12, (2, 1), (100_000, 20));
        assert_eq!(rows, vec![(1, 2), (4970, 5060)], "the frozen rows and the window");
        assert_eq!(columns, vec![(1, 23)]);
    }

    #[test]
    fn spans_merge_and_cover() {
        assert_eq!(merge(vec![(5, 9), (1, 3), (4, 4), (20, 30)]), vec![(1, 9), (20, 30)]);
        assert!(covers(&[(1, 100)], &[(3, 9), (50, 60)]));
        assert!(!covers(&[(1, 100)], &[(90, 110)]));
    }

    #[test]
    fn a_fill_drag_down_up_right_or_left_is_the_engine_fill_to_that_line() {
        let source = CellArea::spanning(0, 3, 2, 4, 3);
        assert_eq!(fill_to(source, CellArea::spanning(0, 3, 2, 9, 3)), Some(FillTo::Row(9)));
        assert_eq!(fill_to(source, CellArea::spanning(0, 1, 2, 4, 3)), Some(FillTo::Row(1)));
        assert_eq!(fill_to(source, CellArea::spanning(0, 3, 2, 4, 7)), Some(FillTo::Column(7)));
        assert_eq!(fill_to(source, source), None);
    }

    #[test]
    fn decimals_step_up_and_down_in_the_first_section_only() {
        assert_eq!(step_decimals("general", true), "#,##0.0");
        assert_eq!(step_decimals("#,##0.00", true), "#,##0.000");
        assert_eq!(step_decimals("#,##0.00", false), "#,##0.0");
        assert_eq!(step_decimals("#,##0.0", false), "#,##0");
        assert_eq!(step_decimals("0.00%", false), "0.0%");
        assert_eq!(
            step_decimals("#,##0.00;[Red]-#,##0.00", true),
            "#,##0.000;[Red]-#,##0.00"
        );
    }

    /// Grouping goes through azul's money formatter, whose amounts are whole cents in an i64:
    /// a sum beyond that is written as Excel's status bar writes it.
    #[test]
    fn a_status_bar_number_beyond_whole_cents_is_written_in_scientific_notation() {
        assert_eq!(format_number(1e20), "1E+20");
        assert_eq!(format_number(-2.5e19), "-2.5E+19");
        assert_eq!(format_number(9e15), "9,000,000,000,000,000");
    }

    #[test]
    fn the_status_bar_numbers_are_grouped_and_trimmed() {
        assert_eq!(format_number(1234.5), "1,234.5");
        assert_eq!(format_number(1_000_000.0), "1,000,000");
        assert_eq!(format_number(-0.25), "-0.25");
        assert_eq!(format_number(0.004), "0");
        assert_eq!(format_number(f64::NAN), "#NUM!");
        assert_eq!(format_number(-1_234_567.891), "-1,234,567.89");
        let stats = SelectionStats {
            count: 4,
            numbers: 3,
            sum: 15.0,
            min: Some(4.0),
            max: Some(6.0),
        };
        assert_eq!(stats_segments(&stats), vec!["Average: 5", "Count: 4", "Sum: 15"]);
        let text_only = SelectionStats {
            count: 1,
            ..SelectionStats::default()
        };
        assert!(stats_segments(&text_only).is_empty(), "one text cell: nothing to say");
    }

    #[test]
    fn colours_widths_tsv_and_titles() {
        // The engine's "#RRGGBB" colours are read by `ColorU::parse_hex`
        // (azul-css's tests).
        assert!((autofit_px(["ab", "abcdefghij"].into_iter(), 12.0) - 84.0).abs() < 0.01);
        assert!((autofit_px(std::iter::empty(), 12.0) - 30.0).abs() < 0.01);
        assert_eq!(
            tsv_of(&[vec![String::from("a"), String::from("=SUM(A1:A2)")], vec![String::from("x\ty")]]),
            "a\t=SUM(A1:A2)\n\"x\ty\"\t\n",
            "a ragged block is padded to a rectangle: IronCalc's paste drops a row of another length"
        );
        assert_eq!(next_book_title(&[String::from("Book1")]), "Book2");
    }

    #[test]
    fn the_sheet_keys_step_over_hidden_sheets_and_wrap() {
        let sheet = |name: &str, hidden: bool| SheetInfo {
            name: name.to_string(),
            color: None,
            hidden,
        };
        let sheets = vec![sheet("A", false), sheet("B", true), sheet("C", false), sheet("D", false)];
        assert_eq!(step_sheet(&sheets, 0, true), Some(2), "B is hidden");
        assert_eq!(step_sheet(&sheets, 3, true), Some(0), "wraps");
        assert_eq!(step_sheet(&sheets, 0, false), Some(3), "wraps back");
        assert_eq!(step_sheet(&sheets, 2, false), Some(0));
        assert_eq!(step_sheet(&[sheet("Only", false)], 0, true), None);
    }
}
