//! The scripted recognizer on a clock the test moves: delays cost no real time, a cut-short call
//! keeps what was about to be said, a failure part-way carries the words so far.

use std::sync::atomic::Ordering;

use super::{words_of, ManualClock, MockSpeech, Step, Then};
use super::super::recognizer::{Clock, Kind, Listen, Outcome, Readiness, Recognizer, Word};

const ORIGIN: u64 = 1_700_000_000_000;

fn say(delay_ms: u32, text: &str) -> Step {
    Step::Say {
        delay_ms,
        words: words_of(text, 100, 300, 0.9),
        language: "en-US".to_string(),
    }
}

/// Listens once with a flag that is never set.
fn listen_once(mock: &mut MockSpeech, clock: &ManualClock, timeout_ms: u32) -> super::super::recognizer::Answer {
    let never = || false;
    mock.listen(&Listen {
        language: "en-US",
        timeout_ms,
        clock,
        cancel: &never,
        flowing: &|| {},
    })
}

#[test]
fn words_come_with_their_times_and_the_moment_the_audio_began() {
    let clock = ManualClock::new(ORIGIN);
    let mut mock = MockSpeech::new(Kind::Scripted, vec![say(1_200, "move the label")]);
    clock.advance(50);
    let answer = listen_once(&mut mock, &clock, 5_000);
    assert_eq!(answer.outcome, Outcome::Heard);
    assert_eq!(answer.text, "move the label");
    assert_eq!(answer.language, "en-US");
    assert_eq!(answer.started_unix_ms, ORIGIN + 50, "the audio began when the call began");
    assert_eq!(
        answer.words,
        vec![
            Word::new("move", 100, 400, 0.9),
            Word::new("the", 400, 700, 0.9),
            Word::new("label", 700, 1_000, 0.9)
        ]
    );
    assert!((answer.confidence - 0.9).abs() < 1e-6, "the mean probability of the words");
}

#[test]
fn a_delay_costs_the_clock_and_not_a_real_second() {
    let clock = ManualClock::new(ORIGIN);
    let mut mock = MockSpeech::new(Kind::Scripted, vec![say(3_000, "hello")]);
    let real = std::time::Instant::now();
    let answer = listen_once(&mut mock, &clock, 5_000);
    assert_eq!(answer.outcome, Outcome::Heard);
    assert_eq!(clock.now_ms(), 3_000, "the clock moved by the delay");
    assert!(real.elapsed() < std::time::Duration::from_millis(500), "and nothing really slept");
}

#[test]
fn words_that_come_after_the_time_is_up_come_in_the_next_call() {
    let clock = ManualClock::new(ORIGIN);
    let mut mock = MockSpeech::new(Kind::Scripted, vec![say(8_000, "late words")]);
    let first = listen_once(&mut mock, &clock, 5_000);
    assert_eq!(first.outcome, Outcome::NothingHeard);
    assert_eq!(clock.now_ms(), 5_000, "the call took its whole time");
    let second = listen_once(&mut mock, &clock, 5_000);
    assert_eq!(second.outcome, Outcome::Heard, "3 s more, inside the next call");
    assert_eq!(second.text, "late words");
    assert_eq!(clock.now_ms(), 8_000);
}

#[test]
fn a_cancelled_call_ends_early_and_keeps_what_was_about_to_be_said() {
    let clock = ManualClock::new(ORIGIN);
    let mut mock = MockSpeech::new(Kind::Scripted, vec![say(4_000, "kept words")]);
    // The flag is set from the fifth look on: 4 slices of 20 ms are waited.
    let looks = std::cell::Cell::new(0u32);
    let cancel = || {
        looks.set(looks.get() + 1);
        looks.get() > 4
    };
    let answer = mock.listen(&Listen {
        language: "",
        timeout_ms: 10_000,
        clock: &clock,
        cancel: &cancel,
        flowing: &|| {},
    });
    assert_eq!(answer.outcome, Outcome::NothingHeard);
    assert_eq!(clock.now_ms(), 80, "ended after 4 slices of 20 ms, not after 4 s");
    let again = listen_once(&mut mock, &clock, 10_000);
    assert_eq!(again.text, "kept words");
    assert_eq!(clock.now_ms(), 80 + 3_920, "only the rest of the delay is waited");
}

#[test]
fn a_refusal_is_immediate_and_a_silence_takes_the_whole_time() {
    let clock = ManualClock::new(ORIGIN);
    let mut mock = MockSpeech::new(
        Kind::Native,
        vec![
            Step::Refuse {
                outcome: Outcome::Denied,
                message: "the microphone is turned off".to_string(),
            },
            Step::Silence,
        ],
    );
    let refused = listen_once(&mut mock, &clock, 5_000);
    assert_eq!(refused.outcome, Outcome::Denied);
    assert_eq!(refused.message, "the microphone is turned off");
    assert_eq!(clock.now_ms(), 0);
    let quiet = listen_once(&mut mock, &clock, 5_000);
    assert_eq!(quiet.outcome, Outcome::NothingHeard);
    assert_eq!(clock.now_ms(), 5_000);
}

#[test]
fn a_failure_part_way_carries_the_words_so_far() {
    let clock = ManualClock::new(ORIGIN);
    let partial = words_of("delete the", 0, 250, 0.8);
    let mut mock = MockSpeech::new(
        Kind::Builtin,
        vec![Step::Fail {
            after_ms: 1_500,
            message: "the model ran out of memory".to_string(),
            partial: partial.clone(),
        }],
    );
    let answer = listen_once(&mut mock, &clock, 5_000);
    assert_eq!(answer.outcome, Outcome::Failed);
    assert_eq!(answer.message, "the model ran out of memory");
    assert_eq!(answer.text, "delete the");
    assert_eq!(answer.words, partial);
    assert_eq!(clock.now_ms(), 1_500);
}

#[test]
fn what_happens_after_the_script_is_silence_or_the_last_step_again() {
    let clock = ManualClock::new(ORIGIN);
    let mut quiet = MockSpeech::new(Kind::Scripted, vec![say(100, "once")]);
    assert_eq!(listen_once(&mut quiet, &clock, 1_000).outcome, Outcome::Heard);
    assert_eq!(listen_once(&mut quiet, &clock, 1_000).outcome, Outcome::NothingHeard);

    let mut repeating = MockSpeech::new(Kind::Scripted, vec![say(100, "again")]).then(Then::RepeatLast);
    for _ in 0..3 {
        let answer = listen_once(&mut repeating, &clock, 1_000);
        assert_eq!((answer.outcome, answer.text.as_str()), (Outcome::Heard, "again"));
    }
}

#[test]
fn calls_are_counted_and_readiness_does_not_use_a_step_up() {
    let clock = ManualClock::new(ORIGIN);
    let mut mock = MockSpeech::new(
        Kind::Builtin,
        vec![Step::Refuse {
            outcome: Outcome::Unavailable,
            message: "no model".to_string(),
        }],
    );
    let calls = mock.calls();
    assert_eq!(
        mock.readiness(),
        Readiness::Refused {
            outcome: Outcome::Unavailable,
            message: "no model".to_string()
        }
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0, "asking is not listening");
    assert_eq!(listen_once(&mut mock, &clock, 1_000).outcome, Outcome::Unavailable);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(mock.readiness(), Readiness::Ready, "the script has no more refusals");
}

#[test]
fn words_of_spaces_the_words_one_after_the_other() {
    let words = words_of("  turn  left now ", 200, 100, 1.5);
    assert_eq!(words.len(), 3);
    assert_eq!((words[0].text.as_str(), words[0].start_ms, words[0].end_ms), ("turn", 200, 300));
    assert_eq!((words[2].text.as_str(), words[2].start_ms, words[2].end_ms), ("now", 400, 500));
}

/// What a UI shows "speak now" from: a scripted recognizer that listens says so at the start of
/// the call (its sound flows at once), one that refuses never does.
#[test]
fn a_scripted_recognizer_says_sound_flows_when_it_listens_and_not_when_it_refuses() {
    use std::sync::atomic::AtomicU32;
    let clock = ManualClock::new(ORIGIN);
    let flowed = AtomicU32::new(0);
    let call = |mock: &mut MockSpeech| {
        let never = || false;
        mock.listen(&Listen {
            language: "en",
            timeout_ms: 1_000,
            clock: &clock,
            cancel: &never,
            flowing: &|| {
                flowed.fetch_add(1, Ordering::SeqCst);
            },
        })
    };
    let mut mock = MockSpeech::new(
        Kind::Scripted,
        vec![
            say(100, "one"),
            Step::Silence,
            Step::Refuse {
                outcome: Outcome::Denied,
                message: "no".to_string(),
            },
            Step::Fail {
                after_ms: 100,
                message: "broke".to_string(),
                partial: Vec::new(),
            },
        ],
    );
    let _ = call(&mut mock);
    assert_eq!(flowed.load(Ordering::SeqCst), 1, "a say");
    let _ = call(&mut mock);
    assert_eq!(flowed.load(Ordering::SeqCst), 2, "a silence");
    let _ = call(&mut mock);
    assert_eq!(flowed.load(Ordering::SeqCst), 2, "a refusal never listened");
    let _ = call(&mut mock);
    assert_eq!(flowed.load(Ordering::SeqCst), 3, "a failure part-way listened first");
}
