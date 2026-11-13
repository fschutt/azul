//! Find in the scrollback: the newest match first, then older ones, round again.

use alacritty_terminal::index::Line;

use super::{find, literal_pattern};
use crate::{session::Session, vt::GridSize};

#[test]
fn find_goes_from_the_newest_match_back_and_round() {
    let s = Session::replay(b"alpha one\r\nbeta\r\nAlpha two\r\n", GridSize::new(20, 4), 100);
    let term = s.term.lock();
    // No capital in the query: any case.
    let newest = find(&*term, "alpha", None, true).expect("a match");
    assert_eq!(newest.start().line, Line(2));
    let older = find(&*term, "alpha", Some(&newest), true).expect("an older one");
    assert_eq!(older.start().line, Line(0));
    let round = find(&*term, "alpha", Some(&older), true).expect("round again");
    assert_eq!(round.start().line, Line(2));
    // Forward from the older one: the newer one.
    assert_eq!(find(&*term, "alpha", Some(&older), false).unwrap().start().line, Line(2));
    // A capital: that case only.
    let exact = find(&*term, "Alpha", None, true).unwrap();
    assert_eq!(exact.start().line, Line(2));
    assert_eq!(find(&*term, "Alpha", Some(&exact), true).unwrap().start().line, Line(2));
    assert!(find(&*term, "gamma", None, true).is_none());
    assert!(find(&*term, "", None, true).is_none());
}

#[test]
fn what_is_typed_is_found_as_it_is_not_as_a_pattern() {
    assert_eq!(literal_pattern("a.b*(c)"), r"a\.b\*\(c\)");
    let s = Session::replay(b"axb\r\na.b\r\n", GridSize::new(20, 3), 100);
    let term = s.term.lock();
    assert_eq!(find(&*term, "a.b", None, true).unwrap().start().line, Line(1), "a dot is a dot");
}
