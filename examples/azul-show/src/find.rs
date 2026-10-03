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
    if needle.is_empty() {
        return None;
    }
    let all = places(deck);
    let n = all.len();
    let start = after.and_then(|a| all.iter().position(|p| *p == a));
    (1..=n)
        .map(|k| match (start, backwards) {
            (Some(s), false) => (s + k) % n,
            (Some(s), true) => (s + n - k) % n,
            (None, false) => k - 1,
            (None, true) => n - k,
        })
        .map(|i| all[i])
        .find(|p| holds_at(deck, *p, needle, how))
}

/// Replaces every match in the texts of `place`; whether anything changed.
pub fn replace_in(deck: &mut Deck, place: Place, needle: &str, replacement: &str, how: TextMatch) -> bool {
    let Some(slide) = deck.slides.get_mut(place.slide) else {
        return false;
    };
    match place.element {
        None => edit(&mut slide.notes, needle, replacement, how),
        Some(id) => slide
            .elements
            .iter_mut()
            .find(|e| e.id == id)
            .is_some_and(|e| replace_texts(e, needle, replacement, how)),
    }
}

/// Replace All: every match of the deck; how many places changed.
pub fn replace_all(deck: &mut Deck, needle: &str, replacement: &str, how: TextMatch) -> usize {
    if needle.is_empty() {
        return 0;
    }
    places(deck)
        .into_iter()
        .filter(|p| replace_in(deck, *p, needle, replacement, how))
        .count()
}

/// Every place of the deck in reading order: each slide's elements, then
/// its notes.
fn places(deck: &Deck) -> Vec<Place> {
    let mut out = Vec::new();
    for (slide, s) in deck.slides.iter().enumerate() {
        out.extend(s.elements.iter().map(|e| Place {
            slide,
            element: Some(e.id),
        }));
        out.push(Place { slide, element: None });
    }
    out
}

/// Whether a text of `place` holds `needle`.
fn holds_at(deck: &Deck, place: Place, needle: &str, how: TextMatch) -> bool {
    let Some(slide) = deck.slides.get(place.slide) else {
        return false;
    };
    match place.element {
        None => find::holds(&slide.notes, needle, how),
        Some(id) => slide.elements.iter().find(|e| e.id == id).is_some_and(|e| {
            let mut texts = Vec::new();
            texts_of(e, &mut texts);
            texts.iter().any(|t| find::holds(t, needle, how))
        }),
    }
}

/// The texts of an element (a group's: its children's): runs, cells, a
/// chart's title.
fn texts_of(e: &Element, out: &mut Vec<String>) {
    match &e.kind {
        ElementKind::Text { body } | ElementKind::Shape { body, .. } => {
            for p in &body.paragraphs {
                out.extend(p.runs.iter().map(|r| r.text.clone()));
            }
        }
        ElementKind::Table { rows, .. } => out.extend(rows.iter().flatten().cloned()),
        ElementKind::Chart { title, .. } => out.push(title.clone()),
        ElementKind::Group { children } => {
            for c in children {
                texts_of(c, out);
            }
        }
        ElementKind::Image { .. } | ElementKind::Video { .. } => {}
    }
}

/// Replaces the matches in one text; whether it changed.
fn edit(text: &mut String, needle: &str, replacement: &str, how: TextMatch) -> bool {
    match find::replace(text, needle, replacement, how) {
        Some(new) => {
            *text = new;
            true
        }
        None => false,
    }
}

/// Replaces the matches in every text of an element; whether any changed.
/// (A match across two differently formatted runs is not found: each run is
/// matched on its own, so a replacement keeps its run's format.)
fn replace_texts(e: &mut Element, needle: &str, replacement: &str, how: TextMatch) -> bool {
    let mut changed = false;
    match &mut e.kind {
        ElementKind::Text { body } | ElementKind::Shape { body, .. } => {
            for p in &mut body.paragraphs {
                for r in &mut p.runs {
                    changed |= edit(&mut r.text, needle, replacement, how);
                }
            }
        }
        ElementKind::Table { rows, .. } => {
            for cell in rows.iter_mut().flatten() {
                changed |= edit(cell, needle, replacement, how);
            }
        }
        ElementKind::Chart { title, .. } => changed |= edit(title, needle, replacement, how),
        ElementKind::Group { children } => {
            for c in children {
                changed |= replace_texts(c, needle, replacement, how);
            }
        }
        ElementKind::Image { .. } | ElementKind::Video { .. } => {}
    }
    changed
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
