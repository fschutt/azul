//! Cell references in formula text: F4's cycling of the reference at the
//! caret (`A1` -> `$A$1` -> `A$1` -> `$A1` -> `A1`, as Excel does) and the
//! reference a click on the grid inserts while a formula is being typed
//! (point mode). Pure text; no azul, no engine.

/// The reference under (or just before) the caret `cursor` (in characters)
/// of the formula `text`, cycled one step through Excel's four anchorings;
/// a range `A1:B2` turns as one. The new text and the caret after the
/// reference, or `None` when the text is no formula or no reference touches
/// the caret.
#[must_use]
pub fn cycle_reference(text: &str, cursor: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let span = references(&chars)
        .into_iter()
        .find(|r| r.start < cursor && cursor <= r.end)?;
    let (col_abs, row_abs) = span.parts[0].anchoring;
    let next = match (col_abs, row_abs) {
        (false, false) => (true, true),
        (true, true) => (false, true),
        (false, true) => (true, false),
        (true, false) => (false, false),
    };
    let turned: Vec<String> = span.parts.iter().map(|p| p.spelled(next)).collect();
    let replacement = turned.join(":");
    let mut out: String = chars[..span.start].iter().collect();
    out.push_str(&replacement);
    out.extend(&chars[span.end..]);
    Some((out, span.start + replacement.chars().count()))
}

/// One cell reference: its column letters, its row digits and whether each
/// is anchored (`$`).
#[derive(Clone, Debug, PartialEq, Eq)]
struct CellRef {
    column: String,
    row: String,
    anchoring: (bool, bool),
}

impl CellRef {
    fn spelled(&self, (col_abs, row_abs): (bool, bool)) -> String {
        format!(
            "{}{}{}{}",
            if col_abs { "$" } else { "" },
            self.column,
            if row_abs { "$" } else { "" },
            self.row
        )
    }
}

/// A reference in the text: a cell, or a range of two (`A1:B2`), over the
/// characters `start..end`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RefSpan {
    start: usize,
    end: usize,
    parts: Vec<CellRef>,
}

/// A character that continues a name or a number: a reference may not
/// start right after one, nor end right before one.
fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.'
}

/// The cell reference starting at `i` (`$`? 1-3 letters `$`? digits), not
/// glued to a name before or after it and not a function call (`LOG10(`):
/// the reference and the index after it.
fn cell_at(chars: &[char], i: usize) -> Option<(CellRef, usize)> {
    if i > 0 && (is_name_char(chars[i - 1]) || chars[i - 1] == '$') {
        return None;
    }
    let mut j = i;
    let col_abs = chars.get(j) == Some(&'$');
    if col_abs {
        j += 1;
    }
    let letters = j;
    while chars.get(j).is_some_and(char::is_ascii_alphabetic) {
        j += 1;
    }
    if j == letters || j - letters > 3 {
        return None;
    }
    let column: String = chars[letters..j].iter().collect();
    let row_abs = chars.get(j) == Some(&'$');
    if row_abs {
        j += 1;
    }
    let digits = j;
    while chars.get(j).is_some_and(char::is_ascii_digit) {
        j += 1;
    }
    if j == digits {
        return None;
    }
    if chars.get(j).is_some_and(|c| is_name_char(*c) || *c == '(' || *c == '$') {
        return None;
    }
    let row: String = chars[digits..j].iter().collect();
    Some((
        CellRef {
            column,
            row,
            anchoring: (col_abs, row_abs),
        },
        j,
    ))
}

/// Every reference of a formula outside its strings, left to right; none
/// when the text is no formula.
fn references(chars: &[char]) -> Vec<RefSpan> {
    let mut out = Vec::new();
    if chars.first() != Some(&'=') {
        return out;
    }
    let mut in_string = false;
    let mut i = 1;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            in_string = !in_string;
            i += 1;
            continue;
        }
        if in_string {
            i += 1;
            continue;
        }
        match cell_at(chars, i) {
            Some((first, end)) => {
                let mut span = RefSpan {
                    start: i,
                    end,
                    parts: vec![first],
                };
                if chars.get(end) == Some(&':') {
                    if let Some((second, end2)) = cell_at(chars, end + 1) {
                        span.end = end2;
                        span.parts.push(second);
                    }
                }
                i = span.end;
                out.push(span);
            }
            None => i += 1,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f4(text: &str, cursor: usize) -> Option<(String, usize)> {
        cycle_reference(text, cursor)
    }

    #[test]
    fn f4_cycles_a_reference_through_the_four_anchorings() {
        assert_eq!(f4("=A1", 3), Some((String::from("=$A$1"), 5)));
        assert_eq!(f4("=$A$1", 5), Some((String::from("=A$1"), 4)));
        assert_eq!(f4("=A$1", 4), Some((String::from("=$A1"), 4)));
        assert_eq!(f4("=$A1", 4), Some((String::from("=A1"), 3)));
    }

    #[test]
    fn the_reference_under_the_caret_turns_and_the_rest_stays() {
        // The caret inside B12 (after "B1"): only B12 turns.
        assert_eq!(f4("=A1+B12*2", 6), Some((String::from("=A1+$B$12*2"), 9)));
        // The caret right after A1.
        assert_eq!(f4("=A1+B12*2", 3), Some((String::from("=$A$1+B12*2"), 5)));
        // A range turns as one.
        assert_eq!(f4("=SUM(A1:B2)", 8), Some((String::from("=SUM($A$1:$B$2)"), 14)));
        // A sheet-qualified reference keeps its sheet.
        assert_eq!(f4("=Sheet2!C3", 10), Some((String::from("=Sheet2!$C$3"), 12)));
        // Lower case is a reference too (Excel upper-cases it on commit).
        assert_eq!(f4("=a1", 3), Some((String::from("=$a$1"), 5)));
    }

    #[test]
    fn no_reference_at_the_caret_is_nothing() {
        assert_eq!(f4("=A1+B2", 4), None, "the caret after the +");
        assert_eq!(f4("=SUM(1,2)", 4), None, "a function name is no reference");
        assert_eq!(f4("=LOG10(5)", 6), None, "LOG10( is a function, not a cell");
        assert_eq!(f4("=\"A1\"", 3), None, "inside a string");
        assert_eq!(f4("A1", 2), None, "not a formula");
        assert_eq!(f4("=ABCD1", 6), None, "four letters are no column");
    }
}
