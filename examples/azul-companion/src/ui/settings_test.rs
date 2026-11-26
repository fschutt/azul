//! The settings screen's helpers: the lines of the connection log are cut to fit.

use super::*;

#[test]
fn a_line_that_fits_stays_as_it_is() {
    assert_eq!(shorten("abc", 3), "abc");
    assert_eq!(shorten("abc", 10), "abc");
    assert_eq!(shorten("", 0), "");
}

#[test]
fn a_line_that_does_not_fit_is_cut_in_characters_with_an_ellipsis_that_counts_in_the_length() {
    assert_eq!(shorten("abcdef", 4), "abc…");
    assert_eq!(shorten("äöüß-äöüß", 4), "äöü…", "characters, not bytes");
    assert_eq!(shorten("abc", 0), "");
    assert_eq!(shorten("abc", 1), "…");
}

#[test]
fn the_longest_line_the_screen_shows_is_the_length_it_is_cut_to() {
    let long = "x".repeat(500);
    assert_eq!(shorten(&long, LOG_LINE_CHARS).chars().count(), LOG_LINE_CHARS);
}
