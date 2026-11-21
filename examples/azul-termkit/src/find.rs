//! Find in a terminal's text, the scrollback with it: what is typed is found as it is (a dot is
//! a dot), in any case unless it holds a capital (alacritty's smart case), from the newest
//! match back to older ones and round again.

use alacritty_terminal::{
    grid::Dimensions,
    index::{Boundary, Column, Direction, Point, Side},
    term::search::{Match, RegexSearch},
    Term,
};

/// What a regular expression reads as `query` itself: every character with a meaning of its
/// own escaped.
#[must_use]
pub fn literal_pattern(query: &str) -> String {
    let mut out = String::with_capacity(query.len() * 2);
    for c in query.chars() {
        if matches!(
            c,
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$'
                | '#' | '&' | '-' | '~'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The match of `query` after `from` (the one shown) - older (`backwards`) or newer - round
/// the text's ends; with no `from` the newest (backwards) or the oldest. `None` when the
/// query is empty or found nowhere.
pub fn find<T>(term: &Term<T>, query: &str, from: Option<&Match>, backwards: bool) -> Option<Match> {
    if query.is_empty() {
        return None;
    }
    let mut regex = RegexSearch::new(&literal_pattern(query)).ok()?;
    let (origin, direction, side) = match (from, backwards) {
        (Some(shown), true) => (
            shown.start().sub(term, Boundary::None, 1),
            Direction::Left,
            Side::Right,
        ),
        (Some(shown), false) => (
            shown.end().add(term, Boundary::None, 1),
            Direction::Right,
            Side::Left,
        ),
        (None, true) => (
            Point::new(term.bottommost_line(), term.last_column()),
            Direction::Left,
            Side::Right,
        ),
        (None, false) => (
            Point::new(term.topmost_line(), Column(0)),
            Direction::Right,
            Side::Left,
        ),
    };
    term.search_next(&mut regex, origin, direction, side, None)
}

#[cfg(test)]
#[path = "find_tests.rs"]
mod tests;
