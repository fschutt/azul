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
    let _ = (text, cursor);
    None
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
