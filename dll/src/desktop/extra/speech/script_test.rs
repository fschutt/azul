//! The script of a scripted run: every kind of step, the plain text of the old hook, and what is
//! refused loudly (a misspelt member in a test script must not pass for a recognizer that is quiet).

use super::{heard_every_time, parse};
use super::{
    super::{
        mock::{Step, Then},
        recognizer::{Outcome, Word},
    },
};

#[test]
fn a_script_with_every_kind_of_step_is_read() {
    let script = parse(
        r#"{
          "native":  [ {"refuse": "denied", "message": "the microphone is turned off for Terminal"} ],
          "builtin": [ {"say": "navigate to Leipzig", "delay_ms": 300, "language": "en"},
                       {"say": [["delete", 0, 300, 0.99], ["it", 300, 500, 0.8]]},
                       {"silence": true},
                       {"fail": "the model ran out of memory", "after_ms": 400, "partial": "delete the"},
                       {"refuse": "unavailable", "message": "no model"} ],
          "then": "repeat"
        }"#,
    )
    .expect("a good script");
    assert_eq!(script.then, Then::RepeatLast);
    assert_eq!(
        script.native,
        Some(vec![Step::Refuse {
            outcome: Outcome::Denied,
            message: "the microphone is turned off for Terminal".to_string()
        }])
    );
    let builtin = script.builtin.expect("builtin steps");
    assert_eq!(builtin.len(), 5);
    match &builtin[0] {
        Step::Say { delay_ms, words, language } => {
            assert_eq!(*delay_ms, 300);
            assert_eq!(language, "en");
            assert_eq!(words.len(), 3);
            assert_eq!(words[0], Word::new("navigate", 0, 300, 0.95));
            assert_eq!(words[2], Word::new("Leipzig", 600, 900, 0.95));
        }
        other => panic!("{other:?}"),
    }
    match &builtin[1] {
        Step::Say { delay_ms, words, .. } => {
            assert_eq!(*delay_ms, 200, "the default delay");
            assert_eq!(words, &vec![Word::new("delete", 0, 300, 0.99), Word::new("it", 300, 500, 0.8)]);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(builtin[2], Step::Silence);
    match &builtin[3] {
        Step::Fail { after_ms, message, partial } => {
            assert_eq!((*after_ms, message.as_str()), (400, "the model ran out of memory"));
            assert_eq!(partial.len(), 2);
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(&builtin[4], Step::Refuse { outcome: Outcome::Unavailable, message } if message == "no model"));
}

#[test]
fn a_recognizer_without_a_list_hears_nothing_and_silence_is_the_default_ending() {
    let script = parse(r#"{"builtin": [{"say": "hello"}]}"#).unwrap();
    assert_eq!(script.native, None);
    assert!(script.builtin.is_some());
    assert_eq!(script.then, Then::Silence);
    let empty = parse("{}").unwrap();
    assert_eq!((empty.native, empty.builtin), (None, None));
}

#[test]
fn the_old_plain_text_is_heard_on_every_call() {
    let steps = heard_every_time("  navigate to Leipzig ");
    assert_eq!(steps.len(), 1);
    match &steps[0] {
        Step::Say { delay_ms, words, .. } => {
            assert_eq!(*delay_ms, 0, "no wait: the old hook answered at once");
            assert_eq!(words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), ["navigate", "to", "Leipzig"]);
            assert!((words[0].probability - 1.0).abs() < 1e-6, "it was sure, as the old hook said");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(heard_every_time("   "), vec![Step::Silence], "a blank text is a quiet room");
}

#[test]
fn what_is_wrong_is_refused_with_a_message_that_says_where() {
    for (script, wanted) in [
        ("[1]", "not a JSON object"),
        ("{", "invalid JSON"),
        (r#"{"natve": []}"#, "nobody knows: \"natve\""),
        (r#"{"native": {}}"#, "is a list of steps"),
        (r#"{"native": [5]}"#, "\"native\" step 0: a step is an object"),
        (r#"{"builtin": [{"say": "a", "silence": true}]}"#, "not several"),
        (r#"{"builtin": [{}]}"#, "one of say, silence, refuse and fail"),
        (r#"{"builtin": [{"say": "a", "dealy_ms": 3}]}"#, "nobody knows: \"dealy_ms\""),
        (r#"{"builtin": [{"say": "a", "delay_ms": -1}]}"#, "number of milliseconds"),
        (r#"{"builtin": [{"say": "a", "delay_ms": 99999999}]}"#, "number of milliseconds"),
        (r#"{"builtin": [{"refuse": "maybe"}]}"#, "\"denied\" or \"unavailable\""),
        (r#"{"builtin": [{"say": [["a", 0, 1]]}]}"#, "a word is"),
        (r#"{"builtin": [{"say": [["a", 0, 1, 7]]}]}"#, "probability"),
        (r#"{"builtin": [{"say": 4}]}"#, "text or a list of words"),
        (r#"{"then": "forever"}"#, "\"silence\" or \"repeat\""),
    ] {
        let error = parse(script).expect_err(script);
        assert!(error.contains(wanted), "{script}: {error}");
    }
}

#[test]
fn a_script_of_absurd_size_is_refused_not_read() {
    let many = format!(r#"{{"native": [{}]}}"#, vec![r#"{"silence": true}"#; 1_001].join(","));
    assert!(parse(&many).unwrap_err().contains("more than the 1000"));
    let long = format!(r#"{{"builtin": [{{"say": "{}"}}]}}"#, "x".repeat(9_000));
    assert!(parse(&long).unwrap_err().contains("more than the 8192"));
    let huge = format!("{{\"native\": [], \"pad\": \"{}\"}}", "x".repeat(1024 * 1024));
    assert!(parse(&huge).unwrap_err().contains("more than the"));
}

#[test]
fn a_word_that_ends_before_it_starts_is_made_to_end_where_it_starts() {
    let script = parse(r#"{"builtin": [{"say": [["a", 500, 100, 0.5]]}]}"#).unwrap();
    match &script.builtin.unwrap()[0] {
        Step::Say { words, .. } => assert_eq!(words[0], Word::new("a", 500, 500, 0.5)),
        other => panic!("{other:?}"),
    }
}
