//! The debug op that adds a note to the open file without a microphone: what its arguments say, held to what a page has.

use super::*;

#[test]
fn the_arguments_are_read_and_the_page_and_the_height_are_held_to_the_page() {
    let r = parse("{\"text\":\"rename this\",\"page\":1,\"y\":120.5,\"ask\":true,\"open\":true}", 3).expect("valid");
    assert_eq!(r.text, "rename this");
    assert_eq!((r.page, r.y, r.ask, r.open), (1, 120.5, true, true));
    let r = parse("{\"text\":\"x\",\"page\":99,\"y\":99999}", 3).expect("valid");
    assert_eq!(r.page, 2, "the last page of three");
    assert_eq!(r.y, crate::ui::page_h() - crate::ui::PAGE_PAD, "the foot of the page's code");
    let r = parse("{\"text\":\"x\",\"y\":-5}", 3).expect("valid");
    assert_eq!((r.page, r.y), (0, crate::ui::PAGE_PAD), "the first page, the top of its code");
    let r = parse("{\"text\":\"x\"}", 1).expect("valid");
    assert!(!r.ask && !r.open);
}

#[test]
fn a_note_needs_words_and_the_words_are_capped_with_a_word_about_it() {
    assert!(parse("{}", 1).is_err());
    assert!(parse("{\"text\":\"   \"}", 1).is_err());
    assert!(parse("not json", 1).is_err());
    let long = "é".repeat(MAX_TEXT_BYTES);
    let r = parse(&format!("{{\"text\":\"{long}\"}}"), 1).expect("valid");
    assert!(r.text.len() <= MAX_TEXT_BYTES);
    assert!(r.text.chars().all(|c| c == 'é'), "cut at a letter");
    assert!(r.cut.as_ref().is_some_and(|w| w.contains("bytes")), "{:?}", r.cut);
}

#[test]
fn a_scripted_dictation_takes_its_utterances_a_place_and_a_gap() {
    let r = parse_dictate("{\"words\":[\"rename step_3\",\"to first_step\"],\"page\":1,\"y\":100,\"gap_ms\":250}", 3).expect("valid");
    assert_eq!(r.words, vec!["rename step_3".to_string(), "to first_step".to_string()]);
    assert_eq!((r.page, r.y, r.gap_ms), (1, 100.0, 250));
    let r = parse_dictate("{\"words\":[\"x\"]}", 1).expect("valid");
    assert_eq!((r.page, r.gap_ms), (0, DEFAULT_GAP_MS));
    let r = parse_dictate("{\"words\":[\"x\"],\"gap_ms\":999999}", 1).expect("valid");
    assert_eq!(r.gap_ms, MAX_GAP_MS, "a gap is held to what a script needs");
}

#[test]
fn a_scripted_dictation_needs_words() {
    assert!(parse_dictate("{}", 1).is_err());
    assert!(parse_dictate("{\"words\":[]}", 1).is_err());
    assert!(parse_dictate("{\"words\":[\"  \"]}", 1).is_err());
    assert!(parse_dictate("{\"words\":\"not a list\"}", 1).is_err());
}
