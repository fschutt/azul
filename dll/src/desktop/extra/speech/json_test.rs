//! The engine's JSON, read with the answers the engine really gave (Whistle in cactus 4.1.4 on
//! 2026-11-25: a synthetic English and German clip, and a microphone take).

use super::{parse, transcript, Value, MAX_JSON_BYTES};
use super::super::recognizer::Word;

/// The engine's answer for "Please move the label of the river to the left and delete the
/// duplicate street name." (4.02 s of audio, one thread).
const ENGLISH: &str = r#"{"text":"Please move the label of the river to the left and delete the duplicate street name.","language":"en","words":[{"word":"Please","start":0.00,"end":0.32,"probability":0.954},{"word":"move","start":0.32,"end":0.56,"probability":0.999},{"word":"the","start":0.56,"end":0.72,"probability":1.000},{"word":"label","start":0.72,"end":1.04,"probability":0.992},{"word":"of","start":1.04,"end":1.20,"probability":1.000},{"word":"the","start":1.20,"end":1.28,"probability":0.998},{"word":"river","start":1.28,"end":1.52,"probability":0.952},{"word":"to","start":1.52,"end":1.76,"probability":1.000},{"word":"the","start":1.76,"end":1.84,"probability":0.999},{"word":"left","start":1.84,"end":2.08,"probability":0.994},{"word":"and","start":2.08,"end":2.24,"probability":0.378},{"word":"delete","start":2.24,"end":2.48,"probability":0.994},{"word":"the","start":2.48,"end":2.72,"probability":0.999},{"word":"duplicate","start":2.72,"end":3.28,"probability":0.867},{"word":"street","start":3.28,"end":3.68,"probability":0.974},{"word":"name.","start":3.68,"end":3.92,"probability":0.996}],"ttft_ms":18.0,"decode_tps":423.7}"#;

/// The German clip: umlauts, a sharp s, a comma.
const GERMAN: &str = r#"{"text":"Bitte verschiebe die Beschriftung des Flusses nach links und lösche den doppelten Straßennamen.","language":"de","words":[{"word":"Bitte","start":0.08,"end":0.32,"probability":0.996},{"word":"verschiebe","start":0.32,"end":0.88,"probability":0.992},{"word":"die","start":0.88,"end":1.04,"probability":1.000},{"word":"Beschriftung","start":1.04,"end":1.60,"probability":0.999},{"word":"des","start":1.60,"end":1.84,"probability":1.000},{"word":"Flusses","start":1.84,"end":2.32,"probability":0.834},{"word":"nach","start":2.32,"end":2.56,"probability":0.998},{"word":"links","start":2.56,"end":2.80,"probability":0.909},{"word":"und","start":2.80,"end":3.04,"probability":0.993},{"word":"lösche","start":3.04,"end":3.36,"probability":0.935},{"word":"den","start":3.36,"end":3.52,"probability":1.000},{"word":"doppelten","start":3.52,"end":4.00,"probability":0.974},{"word":"Straßennamen.","start":4.00,"end":4.80,"probability":0.899}],"ttft_ms":54.7,"decode_tps":250.4}"#;

#[test]
fn the_english_answer_gives_its_text_language_and_every_word_with_its_time() {
    let t = transcript(ENGLISH).expect("the engine's own answer");
    assert_eq!(t.text, "Please move the label of the river to the left and delete the duplicate street name.");
    assert_eq!(t.language, "en");
    assert_eq!(t.words.len(), 16);
    assert_eq!(t.words[0], Word::new("Please", 0, 320, 0.954));
    assert_eq!(t.words[10], Word::new("and", 2_080, 2_240, 0.378));
    assert_eq!(t.words[15], Word::new("name.", 3_680, 3_920, 0.996));
}

#[test]
fn the_german_answer_keeps_umlauts_and_the_sharp_s() {
    let t = transcript(GERMAN).expect("the engine's own answer");
    assert_eq!(t.language, "de");
    assert_eq!(t.words.len(), 13);
    assert_eq!(t.words[9].text, "lösche");
    assert_eq!(t.words[12], Word::new("Straßennamen.", 4_000, 4_800, 0.899));
    assert!(t.text.contains("Straßennamen"));
}

#[test]
fn a_quiet_clip_is_an_empty_text_and_no_words() {
    // The engine's answer below its loudness threshold: an empty transcript and an empty language.
    let t = transcript(r#"{"text":"","language":"","words":[],"ttft_ms":0.0,"decode_tps":0.0}"#).unwrap();
    assert_eq!((t.text.as_str(), t.language.as_str(), t.words.len()), ("", "", 0));
    // Members that are left out are empty too.
    let t = transcript(r#"{"text":"  hello  "}"#).unwrap();
    assert_eq!((t.text.as_str(), t.words.len()), ("hello", 0));
}

#[test]
fn the_streaming_shape_is_read_too_with_its_unknown_members_skipped() {
    let t = transcript(
        r#"{"text":"Good","words":[{"word":"Good","start":0.00,"end":0.32,"probability":0.989}],"pending":"morning, I have opened a map of","language":"en","received":2.00,"pass_ms":62.0}"#,
    )
    .unwrap();
    assert_eq!(t.text, "Good");
    assert_eq!(t.words, vec![Word::new("Good", 0, 320, 0.989)]);
}

#[test]
fn a_word_without_a_text_is_skipped_and_odd_numbers_are_kept_to_what_a_time_can_be() {
    let t = transcript(
        r#"{"text":"a b","language":"en","words":[{"start":0.1,"end":0.2},{"word":"a","start":-3,"end":1e12,"probability":7},{"word":"b","start":2.0}]}"#,
    )
    .unwrap();
    assert_eq!(t.words.len(), 2);
    assert_eq!(t.words[0], Word::new("a", 0, u32::MAX, 1.0), "negative time is 0, a huge one is the most, a probability is kept to 1");
    assert_eq!(t.words[1], Word::new("b", 2_000, 2_000, 0.0), "a missing end is the start");
}

#[test]
fn escapes_and_surrogate_pairs_are_read() {
    let v = parse(r#""tab\there \"quoted\" back\\slash ä ß 😀 line\nbreak \/""#).unwrap();
    assert_eq!(v, Value::Text("tab\there \"quoted\" back\\slash ä ß \u{1F600} line\nbreak /".to_string()));
}

#[test]
fn every_kind_of_value_is_read() {
    let v = parse(r#" { "n": -12.5e1, "t": true, "f": false, "z": null, "l": [1, [2], {}], "o": {"k": "v"} } "#).unwrap();
    assert_eq!(v.get("n"), Some(&Value::Number(-125.0)));
    assert_eq!(v.get("t"), Some(&Value::Bool(true)));
    assert_eq!(v.get("f"), Some(&Value::Bool(false)));
    assert_eq!(v.get("z"), Some(&Value::Null));
    assert_eq!(v.get("l").and_then(Value::as_list).map(<[Value]>::len), Some(3));
    assert_eq!(v.get("o").and_then(|o| o.get("k")).and_then(Value::as_text), Some("v"));
    assert_eq!(v.get("missing"), None);
}

#[test]
fn broken_answers_are_errors_that_say_where_and_never_a_panic() {
    for broken in [
        "",
        "{",
        r#"{"text":"unterminated"#,
        r#"{"text" "no colon"}"#,
        r#"{"a":1,}"#,
        "[1 2]",
        r#"{"a":nul}"#,
        r#""\u12""#,
        r#""\ud83d alone""#,
        r#""\x""#,
        "1 2",
        "{\"a\":\u{1}}",
        "-",
        "1e999",
    ] {
        let error = parse(broken).expect_err(broken);
        assert!(error.starts_with("invalid JSON at byte"), "{broken:?}: {error}");
    }
    assert!(transcript("[1,2]").unwrap_err().contains("not a JSON object"));
    assert!(transcript("not json").is_err());
}

#[test]
fn deep_nesting_and_huge_answers_are_refused_not_followed() {
    let deep = format!("{}1{}", "[".repeat(10_000), "]".repeat(10_000));
    assert!(parse(&deep).unwrap_err().contains("nested too deeply"));
    let huge = " ".repeat(MAX_JSON_BYTES + 1);
    assert!(parse(&huge).unwrap_err().contains("more than"));
}
