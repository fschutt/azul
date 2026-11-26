//! What the system's recognizer reports, and what is made of it: the words of the final report
//! carry the times and the confidences (the partial ones carry zeros), also when the final text is
//! the one the last partial report had.

use std::time::{Duration, Instant};

use super::{words_of_segments, Hearing, Segment};

fn segments(list: &[(&str, f64, f64, f32)]) -> Vec<Segment> {
    list.iter().map(|(text, at, long, sure)| ((*text).to_string(), *at, *long, *sure)).collect()
}

#[test]
fn the_final_report_gives_the_times_and_confidences_even_when_its_text_is_the_one_already_known() {
    let mut hearing = Hearing::default();
    let began = Instant::now();
    // The partial reports: the words, with no time and no confidence yet.
    hearing.update(Some("the speech session".to_string()), false, segments(&[("the", 0.0, 0.0, 0.0), ("speech", 0.0, 0.0, 0.0), ("session", 0.0, 0.0, 0.0)]), None, began);
    assert!(!hearing.done);
    assert_eq!(hearing.changed_at, Some(began));
    // The final one says the same and has the numbers.
    let later = began + Duration::from_secs(2);
    hearing.update(
        Some("the speech session".to_string()),
        true,
        segments(&[("the", 0.32, 0.16, 0.9), ("speech", 0.48, 0.32, 0.95), ("session", 0.8, 0.32, 0.97)]),
        None,
        later,
    );
    assert!(hearing.done);
    assert_eq!(hearing.changed_at, Some(began), "the text did not change: neither did the moment");
    let words = words_of_segments(&hearing.segments);
    assert_eq!(words.len(), 3);
    assert_eq!((words[1].text.as_str(), words[1].start_ms, words[1].end_ms), ("speech", 480, 800));
    assert!((words[2].probability - 0.97).abs() < 1e-6, "{:?}", words[2]);
}

#[test]
fn a_changed_text_takes_its_words_and_its_moment_and_a_failure_ends_the_task() {
    let mut hearing = Hearing::default();
    let at = Instant::now();
    hearing.update(Some("the".to_string()), false, segments(&[("the", 0.0, 0.0, 0.0)]), None, at);
    let next = at + Duration::from_millis(400);
    hearing.update(Some("the speech".to_string()), false, segments(&[("the", 0.0, 0.0, 0.0), ("speech", 0.0, 0.0, 0.0)]), None, next);
    assert_eq!((hearing.text.as_str(), hearing.segments.len(), hearing.changed_at), ("the speech", 2, Some(next)));
    assert!(!hearing.done && hearing.error.is_none());
    // An empty report changes nothing; a failure is the end.
    hearing.update(None, false, Vec::new(), None, next);
    assert_eq!(hearing.text, "the speech");
    hearing.update(None, false, Vec::new(), Some("no speech detected".to_string()), next);
    assert!(hearing.done);
    assert_eq!(hearing.error.as_deref(), Some("no speech detected"));
    assert_eq!(hearing.text, "the speech", "what was heard before stays");
}

#[test]
fn a_segment_that_holds_several_words_is_cut_into_them_with_equal_shares_of_its_time() {
    let words = words_of_segments(&segments(&[("is a", 1.0, 0.4, 0.8), ("closed", 1.4, 0.3, 0.9)]));
    let shown: Vec<(&str, u32, u32)> = words.iter().map(|w| (w.text.as_str(), w.start_ms, w.end_ms)).collect();
    assert_eq!(shown, [("is", 1000, 1200), ("a", 1200, 1400), ("closed", 1400, 1700)]);
}
