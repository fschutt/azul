//! The address under a click in a terminal's row.

use super::url_at;

#[test]
fn the_address_under_the_column_is_found_without_its_trailing_punctuation() {
    let row = "see https://example.org/docs?a=1&b=2, or (http://x.example/y).";
    let at = |needle: &str| row.find(needle).unwrap();
    assert_eq!(
        url_at(row, at("example.org")).as_deref(),
        Some("https://example.org/docs?a=1&b=2")
    );
    assert_eq!(url_at(row, at("https")).as_deref(), Some("https://example.org/docs?a=1&b=2"));
    assert_eq!(url_at(row, at("x.example")).as_deref(), Some("http://x.example/y"), "the bracket is not in it");
    assert_eq!(url_at(row, at("see")), None, "a word is no address");
    assert_eq!(url_at(row, row.len() + 5), None);
}

#[test]
fn only_web_mail_and_file_addresses_open() {
    assert_eq!(url_at("mailto:ada@example.org", 3).as_deref(), Some("mailto:ada@example.org"));
    assert_eq!(url_at("file:///tmp/report.pdf", 9).as_deref(), Some("file:///tmp/report.pdf"));
    assert_eq!(url_at("javascript:alert(1)", 3), None);
    assert_eq!(url_at("https://", 3), None, "a scheme alone");
    // A wiki address keeps its own closing bracket.
    assert_eq!(
        url_at("https://en.wikipedia.org/wiki/Rust_(language)", 10).as_deref(),
        Some("https://en.wikipedia.org/wiki/Rust_(language)")
    );
}
