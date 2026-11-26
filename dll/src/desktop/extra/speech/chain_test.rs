//! The chain of recognizers: the order they are tried in, what a refusal does (once per session),
//! what the answer says about who gave it. Driven by scripted recognizers on a clock the test
//! moves, so the same code runs here that runs with the system's recognizer and the built-in model.

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use super::{Chain, Health, Heard};
use super::super::{
    mock::{words_of, ManualClock, MockSpeech, Step, Then},
    recognizer::{Clock, Kind, Limits, Outcome, Word},
};

const ORIGIN: u64 = 1_700_000_000_000;

fn say(delay_ms: u32, text: &str, language: &str) -> Step {
    Step::Say {
        delay_ms,
        words: words_of(text, 0, 300, 0.95),
        language: language.to_string(),
    }
}

fn refuse(outcome: Outcome, message: &str) -> Step {
    Step::Refuse {
        outcome,
        message: message.to_string(),
    }
}

fn fail(message: &str) -> Step {
    Step::Fail {
        after_ms: 100,
        message: message.to_string(),
        partial: Vec::new(),
    }
}

/// A chain of a scripted native and a scripted built-in recognizer, and how often each was asked.
struct Rig {
    chain: Chain,
    clock: Arc<ManualClock>,
    native_calls: Arc<AtomicU32>,
    builtin_calls: Arc<AtomicU32>,
}

fn rig(native: Vec<Step>, builtin: Vec<Step>) -> Rig {
    let clock = ManualClock::shared(ORIGIN);
    let native = MockSpeech::new(Kind::Native, native);
    let builtin = MockSpeech::new(Kind::Builtin, builtin);
    let (native_calls, builtin_calls) = (native.calls(), builtin.calls());
    let chain = Chain::new(clock.clone())
        .with(Box::new(native))
        .with(Box::new(builtin));
    Rig {
        chain,
        clock,
        native_calls,
        builtin_calls,
    }
}

fn listen(rig: &mut Rig, timeout_ms: u32) -> Heard {
    rig.chain.listen("en-US", timeout_ms, &|| false)
}

fn calls(counter: &Arc<AtomicU32>) -> u32 {
    counter.load(Ordering::SeqCst)
}

#[test]
fn the_native_recognizer_answers_first_and_the_built_in_one_is_not_asked() {
    let mut rig = rig(vec![say(500, "move the label", "en-US")], vec![say(500, "from the model", "en")]);
    let heard = listen(&mut rig, 5_000);
    assert_eq!(heard.by, Some(Kind::Native));
    assert_eq!(heard.answer.outcome, Outcome::Heard);
    assert_eq!(heard.answer.text, "move the label");
    assert_eq!(heard.answer.message, "", "nothing to explain when the first one answers");
    assert_eq!((calls(&rig.native_calls), calls(&rig.builtin_calls)), (1, 0));
}

#[test]
fn a_refused_native_recognizer_hands_the_call_to_the_built_in_one_and_the_answer_says_who() {
    let mut rig = rig(
        vec![refuse(Outcome::Denied, "the microphone is turned off for Terminal")],
        vec![say(800, "delete the second label", "en")],
    );
    let heard = listen(&mut rig, 5_000);
    assert_eq!(heard.by, Some(Kind::Builtin), "the built-in recognizer answered");
    assert_eq!(heard.answer.outcome, Outcome::Heard);
    assert_eq!(heard.answer.text, "delete the second label");
    assert_eq!(heard.answer.language, "en");
    assert_eq!(heard.answer.words.len(), 4, "the words come with their times");
    assert_eq!(heard.answer.words[0], Word::new("delete", 0, 300, 0.95));
    assert!(
        heard.answer.message.contains("the system's recognizer refused (the microphone is turned off for Terminal)"),
        "{}",
        heard.answer.message
    );
    assert!(heard.answer.message.contains("the built-in recognizer answered"), "{}", heard.answer.message);
    // The built-in one had the whole time: the refusal cost none of it.
    assert_eq!(rig.clock.now_ms(), 800);
    let states = rig.chain.states();
    assert_eq!(
        states[0].health,
        Health::Refused {
            outcome: Outcome::Denied,
            message: "the microphone is turned off for Terminal".to_string()
        }
    );
    assert_eq!(states[1].health, Health::Ready);
}

#[test]
fn a_refusal_is_remembered_for_the_session_and_the_native_recognizer_is_not_asked_again() {
    let mut rig = rig(
        vec![
            refuse(Outcome::Unavailable, "no usage text"),
            // Were it asked again, it would answer: the chain must not ask.
            say(100, "native now", "en-US"),
        ],
        vec![say(100, "first", "en"), say(100, "second", "en")],
    );
    let first = listen(&mut rig, 5_000);
    assert_eq!(first.by, Some(Kind::Builtin));
    assert!(first.answer.message.contains("refused"), "the first call says what happened");
    let second = listen(&mut rig, 5_000);
    assert_eq!(second.by, Some(Kind::Builtin));
    assert_eq!(second.answer.text, "second");
    assert_eq!(second.answer.message, "", "said once, not on every call");
    assert_eq!(calls(&rig.native_calls), 1, "tried once per session");
    assert_eq!(calls(&rig.builtin_calls), 2);
}

#[test]
fn forgetting_the_refusals_asks_the_native_recognizer_again() {
    let mut rig = rig(
        vec![refuse(Outcome::Denied, "off"), say(100, "native now", "en-US")],
        vec![say(100, "from the model", "en")],
    );
    assert_eq!(listen(&mut rig, 5_000).by, Some(Kind::Builtin));
    // The user changed the setting.
    rig.chain.reset();
    let again = listen(&mut rig, 5_000);
    assert_eq!(again.by, Some(Kind::Native));
    assert_eq!(again.answer.text, "native now");
}

#[test]
fn a_quiet_room_is_not_a_refusal_and_does_not_hand_over() {
    let mut rig = rig(vec![Step::Silence], vec![say(100, "never asked", "en")]);
    let heard = listen(&mut rig, 3_000);
    assert_eq!(heard.by, Some(Kind::Native));
    assert_eq!(heard.answer.outcome, Outcome::NothingHeard);
    assert_eq!(calls(&rig.builtin_calls), 0);
    assert_eq!(rig.chain.states()[0].health, Health::Ready);
}

#[test]
fn a_failing_recognizer_is_passed_over_and_refused_after_two_failures_in_a_row() {
    let mut rig = rig(
        vec![fail("the service is not reachable"), fail("the service is not reachable"), say(100, "would answer", "en-US")],
        vec![say(100, "one", "en"), say(100, "two", "en"), say(100, "three", "en")],
    );
    let first = listen(&mut rig, 5_000);
    assert_eq!(first.by, Some(Kind::Builtin), "a failure is passed over within the call");
    assert_eq!(rig.chain.states()[0].health, Health::Ready, "once is not a pattern");
    let second = listen(&mut rig, 5_000);
    assert_eq!(second.by, Some(Kind::Builtin));
    assert!(
        matches!(rig.chain.states()[0].health, Health::Refused { outcome: Outcome::Failed, .. }),
        "twice in a row is"
    );
    let third = listen(&mut rig, 5_000);
    assert_eq!(third.by, Some(Kind::Builtin));
    assert_eq!(calls(&rig.native_calls), 2, "not asked a third time");
}

#[test]
fn an_answer_in_between_resets_the_count_of_failures() {
    let mut rig = rig(
        vec![fail("hiccup"), say(100, "back", "en-US"), fail("hiccup"), say(100, "back again", "en-US")],
        vec![say(100, "stand-in", "en")],
    );
    assert_eq!(listen(&mut rig, 5_000).by, Some(Kind::Builtin));
    assert_eq!(listen(&mut rig, 5_000).by, Some(Kind::Native));
    assert_eq!(listen(&mut rig, 5_000).by, Some(Kind::Builtin));
    assert_eq!(rig.chain.states()[0].health, Health::Ready, "failed twice, but not in a row");
    assert_eq!(listen(&mut rig, 5_000).by, Some(Kind::Native));
}

#[test]
fn when_nobody_can_listen_the_answer_is_the_refusal_that_helps_most() {
    let mut rig = rig(
        vec![refuse(Outcome::Denied, "the microphone is turned off for Terminal")],
        vec![refuse(Outcome::Unavailable, "no model is registered")],
    );
    let heard = listen(&mut rig, 5_000);
    assert_eq!(heard.by, None);
    assert_eq!(heard.answer.outcome, Outcome::Denied, "something the user can change comes before something the app has to");
    assert!(heard.answer.message.contains("the system's recognizer: the microphone is turned off for Terminal"), "{}", heard.answer.message);
    assert!(heard.answer.message.contains("the built-in recognizer: no model is registered"), "{}", heard.answer.message);
    assert_eq!(heard.answer.text, "");
    // And the next call, with both remembered as refused, says the same without asking either.
    let again = listen(&mut rig, 5_000);
    assert_eq!(again.answer.outcome, Outcome::Denied);
    assert_eq!(again.answer.message, heard.answer.message);
    assert_eq!((calls(&rig.native_calls), calls(&rig.builtin_calls)), (1, 1));
}

#[test]
fn a_built_in_recognizer_that_is_not_set_up_is_a_clear_status() {
    let mut rig = rig(
        vec![refuse(Outcome::Unavailable, "no Info.plist")],
        vec![refuse(Outcome::Unavailable, "the built-in recognizer has no model: register one with set_builtin")],
    );
    let heard = listen(&mut rig, 5_000);
    assert_eq!(heard.answer.outcome, Outcome::Unavailable);
    assert!(heard.answer.message.contains("no Info.plist"), "{}", heard.answer.message);
    assert!(heard.answer.message.contains("register one with set_builtin"), "{}", heard.answer.message);
}

#[test]
fn an_empty_chain_says_there_is_no_recognizer() {
    let clock = ManualClock::shared(ORIGIN);
    let mut chain = Chain::new(clock);
    let heard = chain.listen("en", 1_000, &|| false);
    assert_eq!(heard.by, None);
    assert_eq!(heard.answer.outcome, Outcome::Unavailable);
    assert!(heard.answer.message.contains("no speech recognizer"), "{}", heard.answer.message);
}

#[test]
fn the_failure_with_words_so_far_keeps_the_words_when_nobody_else_answers() {
    let partial = words_of("delete the", 0, 250, 0.8);
    let clock = ManualClock::shared(ORIGIN);
    let mut chain = Chain::new(clock).with(Box::new(MockSpeech::new(
        Kind::Native,
        vec![Step::Fail {
            after_ms: 700,
            message: "the recognizer stopped".to_string(),
            partial,
        }],
    )));
    let heard = chain.listen("en", 5_000, &|| false);
    assert_eq!(heard.answer.outcome, Outcome::Failed);
    assert_eq!(heard.answer.text, "delete the", "what was understood before the failure is not thrown away");
    assert_eq!(heard.answer.words.len(), 2);
}

#[test]
fn the_status_of_each_link_is_known_without_listening() {
    let mut rig = rig(
        vec![say(100, "ok", "en")],
        vec![refuse(Outcome::Unavailable, "no model")],
    );
    let states = rig.chain.states();
    assert_eq!(states.len(), 2);
    assert_eq!(states[0].kind, Kind::Native);
    assert_eq!(states[0].health, Health::Ready);
    assert_eq!(states[1].kind, Kind::Builtin);
    assert_eq!(
        states[1].health,
        Health::Refused {
            outcome: Outcome::Unavailable,
            message: "no model".to_string()
        }
    );
    assert!(!states[0].label.is_empty() && !states[1].label.is_empty());
    assert_eq!((calls(&rig.native_calls), calls(&rig.builtin_calls)), (0, 0), "asking the status is not listening");
}

#[test]
fn a_recognizer_registered_later_replaces_its_refused_predecessor() {
    let mut rig = rig(
        vec![refuse(Outcome::Unavailable, "no usage text")],
        vec![refuse(Outcome::Unavailable, "no model is registered")],
    );
    assert_eq!(listen(&mut rig, 5_000).answer.outcome, Outcome::Unavailable);
    // The app registers the model after the first try.
    rig.chain.replace(Box::new(MockSpeech::new(Kind::Builtin, vec![say(200, "from the model", "de")])));
    let heard = listen(&mut rig, 5_000);
    assert_eq!(heard.by, Some(Kind::Builtin), "the new one is not held back by the old one's refusal");
    assert_eq!(heard.answer.text, "from the model");
    assert_eq!(rig.chain.states().len(), 2, "replaced, not added");
}

#[test]
fn the_time_to_listen_and_the_language_are_kept_to_the_limits_and_the_answer_says_so() {
    let mut rig = rig(vec![Step::Silence, Step::Silence, Step::Silence], vec![]);
    // Too short: raised to the least.
    let short = rig.chain.listen("en-US", 0, &|| false);
    assert_eq!(rig.clock.now_ms(), u64::from(Limits::default().min_listen_ms));
    assert!(short.answer.message.contains("too short"), "{}", short.answer.message);
    // Too long: cut to the most (a mock that waits the whole time moves the clock by it).
    let before = rig.clock.now_ms();
    let long = rig.chain.listen("en-US", 10_000_000, &|| false);
    assert_eq!(rig.clock.now_ms() - before, u64::from(Limits::default().max_listen_ms));
    assert!(long.answer.message.contains("longer than one call may take"), "{}", long.answer.message);
    // Not a language tag.
    let tag = "x".repeat(200);
    let odd = rig.chain.listen(&tag, 1_000, &|| false);
    assert!(odd.answer.message.contains("language tag is longer than 35 characters"), "{}", odd.answer.message);
    // Inside the limits nothing is said about them.
    let mut quiet = self::rig(vec![Step::Silence], vec![]);
    let fine = quiet.chain.listen("en-US", 2_000, &|| false);
    assert!(!fine.answer.message.contains("using"), "{}", fine.answer.message);
}

#[test]
fn a_cancelled_listen_comes_back_at_once() {
    let mut rig = rig(vec![Step::Silence], vec![]);
    let looks = std::cell::Cell::new(0u32);
    let cancel = || {
        looks.set(looks.get() + 1);
        looks.get() > 3
    };
    let heard = rig.chain.listen("en", 30_000, &cancel);
    assert_eq!(heard.answer.outcome, Outcome::NothingHeard);
    assert_eq!(heard.by, Some(Kind::Native));
    assert!(rig.clock.now_ms() <= 100, "ended after a few slices, not after 30 s: {}", rig.clock.now_ms());
}

#[test]
fn a_chain_of_the_same_scripts_gives_the_same_answers_every_time() {
    // The e2e relies on it: the same script, the same chain, the same words, the same times.
    let run = || {
        let mut rig = rig(
            vec![refuse(Outcome::Denied, "off")],
            vec![say(1_000, "navigate to Leipzig", "en"), Step::Silence],
        );
        (listen(&mut rig, 5_000), listen(&mut rig, 2_000))
    };
    assert_eq!(run(), run());
    let (first, second) = run();
    assert_eq!(first.answer.started_unix_ms, ORIGIN, "dated by the clock of the chain");
    assert_eq!(second.answer.outcome, Outcome::NothingHeard);
}

#[test]
fn repeating_scripts_serve_a_headless_run_that_hears_the_same_words_every_time() {
    let clock = ManualClock::shared(ORIGIN);
    let mut chain = Chain::new(clock).with(Box::new(
        MockSpeech::new(Kind::Scripted, vec![say(0, "open the file", "en")]).then(Then::RepeatLast),
    ));
    for _ in 0..3 {
        let heard = chain.listen("en", 1_000, &|| false);
        assert_eq!((heard.by, heard.answer.text.as_str()), (Some(Kind::Scripted), "open the file"));
    }
}

/// A recognizer that takes a vocabulary and keeps it.
struct TakesWords {
    seen: Arc<std::sync::Mutex<Vec<String>>>,
}

impl super::super::recognizer::Recognizer for TakesWords {
    fn kind(&self) -> Kind {
        Kind::Builtin
    }

    fn label(&self) -> String {
        "takes words".to_string()
    }

    fn readiness(&mut self) -> super::super::recognizer::Readiness {
        super::super::recognizer::Readiness::Ready
    }

    fn listen(&mut self, _call: &super::super::recognizer::Listen<'_>) -> super::super::recognizer::Answer {
        super::super::recognizer::Answer::none(Outcome::NothingHeard, "nothing was said")
    }

    fn set_vocabulary(&mut self, words: &[String]) -> Option<String> {
        *self.seen.lock().unwrap() = words.to_vec();
        Some("the vocabulary was cut".to_string())
    }
}

#[test]
fn the_vocabulary_goes_to_the_recognizers_that_take_one_and_their_words_about_it_come_back() {
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let clock = ManualClock::shared(ORIGIN);
    let mut chain = Chain::new(clock)
        .with(Box::new(MockSpeech::new(Kind::Native, vec![])))
        .with(Box::new(TakesWords { seen: Arc::clone(&seen) }));
    let words = vec!["AzReview".to_string(), "Neckarbrücke".to_string()];
    assert_eq!(chain.set_vocabulary(&words).as_deref(), Some("the vocabulary was cut"));
    assert_eq!(*seen.lock().unwrap(), words);
    // A chain whose recognizers take none has nothing to say about it.
    let clock = ManualClock::shared(ORIGIN);
    let mut quiet = Chain::new(clock).with(Box::new(MockSpeech::new(Kind::Native, vec![])));
    assert_eq!(quiet.set_vocabulary(&words), None);
}
