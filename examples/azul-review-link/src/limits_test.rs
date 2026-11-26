//! The caps: what a cap cuts and what it says.

use super::*;

#[test]
fn the_protocol_this_build_speaks_is_at_least_the_oldest_it_understands() {
    assert!(PROTOCOL_VERSION >= MIN_PROTOCOL_VERSION);
    assert!(ALPN.ends_with(&format!("/{PROTOCOL_VERSION}")), "the protocol name carries the major version");
}

#[test]
fn a_frame_may_hold_a_full_thumbnail_and_its_header() {
    assert!(MAX_FRAME_BYTES > MAX_THUMBNAIL_BYTES + MAX_STRING_BYTES);
    assert!(MAX_ROWS * (MAX_ENTRY_NAME_BYTES + 3) < MAX_FRAME_BYTES, "a full listing fits in a frame");
}

#[test]
fn text_inside_its_cap_is_unchanged_and_says_nothing() {
    let (text, capped) = clean_text("Tone: please", MAX_LABEL_CHARS, false, "button label");
    assert_eq!(text, "Tone: please");
    assert_eq!(capped, None);
}

#[test]
fn text_over_its_cap_is_cut_in_characters_not_bytes_and_says_how_much_it_was() {
    let (text, capped) = clean_text("ÄÖÜäöüßÄÖÜäöüßÄÖ", MAX_LABEL_CHARS, false, "button label");
    assert_eq!(text.chars().count(), MAX_LABEL_CHARS);
    assert_eq!(text, "ÄÖÜäöüßÄÖÜäö");
    assert_eq!(capped, Some(Capped { what: "button label", got: 16, limit: MAX_LABEL_CHARS }));
}

#[test]
fn control_characters_are_dropped_and_a_newline_stays_only_where_it_is_allowed() {
    let (label, _) = clean_text("To\u{0}n\u{7}e\nx", 12, false, "button label");
    assert_eq!(label, "Tonex");
    let (text, capped) = clean_text("line one\nline\u{1b}[31m two", 280, true, "button text");
    assert_eq!(text, "line one\nline[31m two");
    assert_eq!(capped, None, "dropped control characters do not count against the cap");
}

#[test]
fn bytes_are_cut_on_a_character_boundary() {
    let (cut, capped) = cut_bytes("aÄb", 2, "name");
    assert_eq!(cut, "a", "the two bytes of Ä do not fit in the one byte left");
    assert_eq!(capped, Some(Capped { what: "name", got: 4, limit: 2 }));
    assert_eq!(cut_bytes("abc", 3, "name"), ("abc".to_string(), None));
    assert_eq!(cut_bytes("", 0, "name"), (String::new(), None));
}

#[test]
fn a_cap_says_what_it_cut_in_words() {
    let said = Capped { what: "button text", got: 300, limit: 280 }.to_string();
    assert!(said.contains("button text") && said.contains("300") && said.contains("280"), "{said}");
}
