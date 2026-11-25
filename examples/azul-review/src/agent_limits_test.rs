//! The agents' limits: the defaults are inside what a person can mean, the override scales every
//! cap and is held to 1..8, a cut is on a character boundary, the notes are bounded.

use super::*;

#[test]
fn the_defaults_are_realistic_and_none_is_zero() {
    let l = Limits::default();
    for (name, n) in [
        ("strokes", l.max_strokes),
        ("samples", l.max_samples_per_stroke),
        ("points", l.max_svg_points),
        ("context lines", l.max_context_lines),
        ("line chars", l.max_line_chars),
        ("words under", l.max_words_under),
        ("utterances", l.max_utterances),
        ("transcript", l.max_transcript_chars),
        ("turns", l.max_turns),
        ("turn chars", l.max_turn_chars),
        ("prompt", l.max_prompt_bytes),
        ("reply", l.max_reply_bytes),
        ("suggestions", l.max_suggestions),
        ("insert lines", l.max_insert_lines),
        ("suggestion bytes", l.max_suggestion_bytes),
        ("answer", l.max_answer_chars),
        ("rationale", l.max_rationale_chars),
        ("stdout", l.max_stdout_bytes),
        ("stderr", l.max_stderr_bytes),
        ("http", l.max_http_response_bytes),
        ("cut segments", l.max_cut_segments),
        ("export lines", l.max_export_lines),
        ("export items", l.max_export_items),
    ] {
        assert!(n > 0, "{name} is 0");
    }
    // a path of 96 points is a gesture; a prompt of 64 KiB is a few hundred lines of code
    assert!(l.max_svg_points >= 32 && l.max_svg_points <= 512);
    assert!(l.max_prompt_bytes >= 16 * 1024 && l.max_prompt_bytes <= 1024 * 1024);
}

#[test]
fn the_override_scales_every_cap_and_is_held_to_one_to_eight() {
    let base = Limits::default();
    let three = base.scaled(3);
    assert_eq!(three.max_strokes, base.max_strokes * 3);
    assert_eq!(three.max_prompt_bytes, base.max_prompt_bytes * 3);
    assert_eq!(
        three.max_http_response_bytes,
        base.max_http_response_bytes * 3
    );
    assert_eq!(base.scaled(1), base);
    assert_eq!(base.scaled(0), base, "0 is held to 1");
    assert_eq!(
        base.scaled(1_000_000),
        base.scaled(8),
        "a silly factor is held to 8"
    );
}

#[test]
fn a_text_is_cut_on_a_character_boundary_with_an_ellipsis() {
    assert_eq!(cap_chars("short", 10), ("short".to_string(), false));
    assert_eq!(cap_chars("exactly", 7), ("exactly".to_string(), false));
    let (cut, was) = cap_chars("ünïcödé text", 5);
    assert!(was);
    assert_eq!(cut, "ünïc\u{2026}");
    assert_eq!(cut.chars().count(), 5);
    assert_eq!(cap_chars("abc", 0), (String::new(), true));
    let (emoji, _) = cap_chars("😀😀😀😀", 3);
    assert_eq!(emoji, "😀😀\u{2026}");
}

#[test]
fn bytes_are_cut_on_a_character_boundary_without_an_ellipsis() {
    assert_eq!(cap_bytes("abc", 3), ("abc", false));
    assert_eq!(cap_bytes("abcdef", 4), ("abcd", true));
    // 'ä' is two bytes: a cut at byte 1 of it goes back to before it
    assert_eq!(cap_bytes("aäb", 2), ("a", true));
    assert_eq!(cap_bytes("😀", 3), ("", true));
    assert_eq!(cap_bytes("", 0), ("", false));
}

#[test]
fn the_same_note_is_kept_once() {
    let mut n = Notes::new();
    n.push("a stroke was thinned");
    n.push("a stroke was thinned");
    n.push("a line was cut");
    assert_eq!(n.len(), 2);
    assert_eq!(
        n.finish(),
        vec![
            "a stroke was thinned".to_string(),
            "a line was cut".to_string()
        ]
    );
}

#[test]
fn the_notes_are_bounded_and_say_how_many_did_not_fit() {
    let mut n = Notes::new();
    for i in 0..(MAX_NOTES + 10) {
        n.push(format!("note {i}"));
    }
    assert_eq!(n.len(), MAX_NOTES);
    let all = n.finish();
    assert_eq!(all.len(), MAX_NOTES + 1);
    assert_eq!(
        all.last().map(String::as_str),
        Some("10 more notes were not kept")
    );
}

#[test]
fn no_notes_is_empty_and_finishes_empty() {
    let n = Notes::new();
    assert!(n.is_empty());
    assert!(n.finish().is_empty());
}
