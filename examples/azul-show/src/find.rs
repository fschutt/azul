//! Find and Replace across a deck (the standard Find / Replace dialog,
//! Mod+F / Mod+H): every text of every slide in reading order - the text
//! boxes' and shapes' runs, the tables' cells, the charts' titles, inside
//! groups too - then the slide's notes. The matcher is azul-appkit's (the
//! one AzSheets uses). Pure: no azul.

use azul_appkit::find::{self, TextMatch};

use crate::model::{Deck, Element, ElementKind};

/// Where a match is: the slide, and the element (the top-level one, a group
/// for a text inside it) or `None` for the slide's notes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Place {
    pub slide: usize,
    pub element: Option<u64>,
}

/// The next place after `after` (in reading order, wrapping; from the start
/// when `None`) whose text holds `needle`; the previous one when
/// `backwards`. `after` itself comes last.
#[must_use]
pub fn find_next(deck: &Deck, after: Option<Place>, needle: &str, how: TextMatch, backwards: bool) -> Option<Place> {
    let _ = (deck, after, needle, how, backwards);
    None
}

/// Replaces every match in the texts of `place`; whether anything changed.
pub fn replace_in(deck: &mut Deck, place: Place, needle: &str, replacement: &str, how: TextMatch) -> bool {
    let _ = (deck, place, needle, replacement, how);
    false
}

/// Replace All: every match of the deck; how many places changed.
pub fn replace_all(deck: &mut Deck, needle: &str, replacement: &str, how: TextMatch) -> usize {
    let _ = (deck, needle, replacement, how);
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{sample_deck, Frame, Theme};

    fn deck() -> Deck {
        let mut d = sample_deck("find", Theme::office());
        // Slide 0 gets a table, slide 1 a note.
        let id = d.mint();
        d.slides[0].elements.push(Element::new(
            id,
            Frame::new(0.0, 0.0, 100.0, 100.0),
            ElementKind::Table {
                rows: vec![vec![String::from("Budget"), String::from("Plan B")]],
                header: true,
            },
        ));
        d.slides[1].notes = String::from("Mention the plan.");
        d
    }

    fn table_id(d: &Deck) -> u64 {
        d.slides[0]
            .elements
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Table { .. }))
            .map(|e| e.id)
            .expect("the table")
    }

    #[test]
    fn find_walks_the_slides_texts_tables_and_notes_in_order_and_wraps() {
        let d = deck();
        let any = TextMatch::default();
        let table = Place { slide: 0, element: Some(table_id(&d)) };
        let notes = Place { slide: 1, element: None };
        let first = find_next(&d, None, "plan", any, false).expect("a match");
        assert_eq!(first, table, "the table's \"Plan B\" on slide 1 comes first");
        assert_eq!(find_next(&d, Some(table), "plan", any, false), Some(notes), "then slide 2's notes");
        assert_eq!(find_next(&d, Some(notes), "plan", any, false), Some(table), "wraps");
        assert_eq!(find_next(&d, Some(notes), "plan", any, true), Some(table), "backwards");
        let case = TextMatch { match_case: true, ..any };
        assert_eq!(find_next(&d, None, "Plan", case, false), Some(table));
        assert_eq!(find_next(&d, Some(table), "Plan", case, false), Some(table), "the notes say \"plan\"");
        assert_eq!(find_next(&d, None, "zebra", any, false), None);
    }

    #[test]
    fn replace_rewrites_a_place_or_every_place() {
        let mut d = deck();
        let any = TextMatch::default();
        let notes = Place { slide: 1, element: None };
        assert!(replace_in(&mut d, notes, "plan", "roadmap", any));
        assert_eq!(d.slides[1].notes, "Mention the roadmap.");
        assert!(!replace_in(&mut d, notes, "plan", "x", any), "nothing left there");

        let n = replace_all(&mut d, "budget", "Forecast", any);
        assert!(n >= 1);
        let table = table_id(&d);
        let cells = d.slides[0]
            .elements
            .iter()
            .find(|e| e.id == table)
            .and_then(|e| match &e.kind {
                ElementKind::Table { rows, .. } => Some(rows.clone()),
                _ => None,
            })
            .expect("the table");
        assert_eq!(cells[0][0], "Forecast");
        assert_eq!(find_next(&d, None, "budget", any, false), None, "none left anywhere");
    }
}
