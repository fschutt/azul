//! The streaming listen through the session's public face, on scripted recognizers (the same
//! stream code a real run uses): the events and their order, utterances cut inside the stream,
//! refusals before the first event, a failure part-way, a cancel during a partial, the blocking wait.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use azul_css::AzString;

use super::{
    chain::Chain,
    mock::{words_of, ManualClock, MockSpeech, Step, Then},
    recognizer::{Kind, Outcome, SharedClock, SystemClock},
    HeardStatus, SpeechEvent, SpeechEventKind, SpeechRecognizer, SpeechSession, SpeechState, SpeechStream,
    SpeechStreamOptions,
};

const ORIGIN: u64 = 1_700_000_000_000;

/// "move the label", the words 300 ms long from 100 ms on (they end at 400, 700 and 1000 ms), the
/// utterance over at `over_ms`.
fn utterance(over_ms: u32, text: &str) -> Step {
    Step::Say {
        delay_ms: over_ms,
        words: words_of(text, 100, 300, 0.9),
        language: "en".to_string(),
    }
}

fn options(partials: bool, max_utterances: u32, max_ms: u32) -> SpeechStreamOptions {
    SpeechStreamOptions {
        partials,
        endpoint_ms: 0,
        max_ms,
        max_utterances,
        pre_roll_ms: 1_000,
    }
}

fn session_of(native: Vec<Step>, builtin: Option<Vec<Step>>) -> SpeechSession {
    let clock = ManualClock::shared(ORIGIN);
    let mut chain = Chain::new(clock).with(Box::new(MockSpeech::new(Kind::Native, native)));
    if let Some(steps) = builtin {
        chain = chain.with(Box::new(MockSpeech::new(Kind::Builtin, steps)));
    }
    SpeechSession::on_chain(chain)
}

/// Every event until the stream ends (a scripted stream on a manual clock is over in no real time).
fn events_until_ended(stream: &SpeechStream) -> Vec<SpeechEvent> {
    let mut all = Vec::new();
    let began = Instant::now();
    loop {
        all.extend(stream.wait_events(50).as_slice().iter().cloned());
        if all.last().is_some_and(|e| e.kind == SpeechEventKind::Ended) {
            return all;
        }
        assert!(began.elapsed() < Duration::from_secs(5), "the stream never ended: {all:?}");
    }
}

fn kinds(events: &[SpeechEvent]) -> Vec<SpeechEventKind> {
    events.iter().map(|e| e.kind).collect()
}

#[test]
fn a_stream_gives_ready_then_a_partial_for_each_word_then_the_final_then_the_end() {
    let session = session_of(vec![utterance(1_500, "move the label")], None);
    let stream = session.start_stream(AzString::from("en".to_string()), options(true, 1, 0));
    let events = events_until_ended(&stream);
    use SpeechEventKind::{Ended, Final, Partial, Ready};
    assert_eq!(kinds(&events), [Ready, Partial, Partial, Partial, Final, Ended]);
    let texts: Vec<&str> = events.iter().map(|e| e.heard.text.as_str()).collect();
    assert_eq!(texts[1..5], ["move", "move the", "move the label", "move the label"]);
    // The times are the stream's: partials as the words end, the final when the utterance is over.
    let at: Vec<u32> = events.iter().map(|e| e.at_ms).collect();
    assert_eq!(at[1..5], [400, 700, 1_000, 1_500]);
    // Who said it, and the words with their times, on the final.
    let last = &events[4];
    assert_eq!(last.heard.recognizer, SpeechRecognizer::Native);
    assert_eq!(last.heard.status, HeardStatus::Heard);
    assert_eq!(last.heard.words.as_slice().len(), 3);
    assert_eq!(last.heard.words.as_slice()[1].text.as_str(), "the");
    // The end says why: the utterances asked for were heard.
    assert_eq!(events[5].heard.status, HeardStatus::Heard);
    assert!(!stream.is_running());
}

#[test]
fn partials_are_left_out_when_not_asked_for() {
    let session = session_of(vec![utterance(1_500, "move the label")], None);
    let stream = session.start_stream(AzString::from("en".to_string()), options(false, 1, 0));
    let events = events_until_ended(&stream);
    use SpeechEventKind::{Ended, Final, Ready};
    assert_eq!(kinds(&events), [Ready, Final, Ended]);
}

#[test]
fn utterances_follow_each_other_inside_one_stream_until_the_count_is_reached() {
    let session = session_of(vec![utterance(1_200, "move the label"), utterance(900, "delete it"), utterance(900, "never heard")], None);
    let stream = session.start_stream(AzString::from("en".to_string()), options(false, 2, 0));
    let events = events_until_ended(&stream);
    use SpeechEventKind::{Ended, Final, Ready};
    assert_eq!(kinds(&events), [Ready, Final, Final, Ended], "one Ready for the stream, not one per utterance");
    assert_eq!(events[1].heard.text.as_str(), "move the label");
    assert_eq!(events[2].heard.text.as_str(), "delete it");
    assert!(events[2].at_ms > events[1].at_ms);
}

#[test]
fn a_quiet_room_gives_quiet_and_the_stream_ends_when_its_time_is_up() {
    let session = session_of(vec![Step::Silence], None);
    let stream = session.start_stream(AzString::from("en".to_string()), options(true, 0, 3_000));
    let events = events_until_ended(&stream);
    assert_eq!(events.first().map(|e| e.kind), Some(SpeechEventKind::Ready));
    assert!(events.iter().any(|e| e.kind == SpeechEventKind::Quiet), "{:?}", kinds(&events));
    assert!(!events.iter().any(|e| e.kind == SpeechEventKind::Final));
    let ended = events.last().unwrap();
    assert_eq!(ended.heard.status, HeardStatus::NothingHeard);
}

#[test]
fn a_stop_ends_a_stream_that_has_no_end_of_its_own_after_the_utterance_that_runs() {
    let clock = ManualClock::shared(ORIGIN);
    let session = SpeechSession::on_chain(
        Chain::new(clock).with(Box::new(MockSpeech::new(Kind::Native, vec![utterance(600, "again")]).then(Then::RepeatLast))),
    );
    let stream = session.start_stream(AzString::from("en".to_string()), options(false, 0, 0));
    let mut finals = 0;
    let mut all = Vec::new();
    let began = Instant::now();
    while finals < 3 {
        let batch = stream.wait_events(50);
        for event in batch.as_slice() {
            if event.kind == SpeechEventKind::Final {
                finals += 1;
            }
            all.push(event.clone());
        }
        assert!(began.elapsed() < Duration::from_secs(5), "no third utterance: {:?}", kinds(&all));
    }
    stream.stop();
    all.extend(events_until_ended(&stream));
    assert_eq!(all.last().unwrap().kind, SpeechEventKind::Ended);
    assert!(finals >= 3);
}

#[test]
fn a_refusal_before_the_first_event_hands_the_stream_to_the_next_recognizer() {
    let session = session_of(
        vec![Step::Refuse {
            outcome: Outcome::Denied,
            message: "the microphone is turned off for Terminal".to_string(),
        }],
        Some(vec![utterance(900, "delete it")]),
    );
    session.set_preference(super::SpeechPreference::NativeFirst);
    let stream = session.start_stream(AzString::from("en".to_string()), options(false, 1, 0));
    let events = events_until_ended(&stream);
    use SpeechEventKind::{Ended, Final, Ready};
    assert_eq!(kinds(&events), [Ready, Final, Ended]);
    assert_eq!(events[1].heard.recognizer, SpeechRecognizer::Builtin);
    let ended = &events[2];
    assert!(ended.heard.message.as_str().contains("refused (the microphone is turned off for Terminal)"), "{}", ended.heard.message.as_str());
}

#[test]
fn when_nobody_can_listen_the_stream_ends_at_once_with_the_refusal() {
    let session = session_of(
        vec![Step::Refuse {
            outcome: Outcome::Unavailable,
            message: "no usage text".to_string(),
        }],
        None,
    );
    let stream = session.start_stream(AzString::from("en".to_string()), options(true, 1, 0));
    let events = events_until_ended(&stream);
    assert_eq!(kinds(&events), [SpeechEventKind::Ended], "no Ready: no sound ever flowed");
    assert_eq!(events[0].heard.status, HeardStatus::Unavailable);
    assert!(events[0].heard.message.as_str().contains("no usage text"));
}

#[test]
fn a_device_that_fails_part_way_ends_the_stream_with_what_was_heard_and_the_failure() {
    let session = session_of(
        vec![Step::Fail {
            after_ms: 800,
            message: "the microphone went away".to_string(),
            partial: words_of("move the", 100, 300, 0.9),
        }],
        None,
    );
    let stream = session.start_stream(AzString::from("en".to_string()), options(true, 0, 0));
    let events = events_until_ended(&stream);
    assert_eq!(events.first().map(|e| e.kind), Some(SpeechEventKind::Ready));
    assert!(events.iter().any(|e| e.kind == SpeechEventKind::Partial && e.heard.text.as_str() == "move the"), "{:?}", kinds(&events));
    let ended = events.last().unwrap();
    assert_eq!(ended.heard.status, HeardStatus::Failed);
    assert!(ended.heard.message.as_str().contains("went away"), "{}", ended.heard.message.as_str());
}

/// On a clock that really waits: the first partial comes after 400 ms, the cancel after it ends the
/// stream at once, with no final for the utterance that was cut.
#[test]
fn a_cancel_during_a_partial_ends_the_stream_at_once_without_a_final() {
    let clock: SharedClock = Arc::new(SystemClock::new());
    let session = SpeechSession::on_chain(Chain::new(clock).with(Box::new(MockSpeech::new(Kind::Native, vec![utterance(30_000, "move the label")]))));
    let stream = session.start_stream(AzString::from("en".to_string()), options(true, 1, 0));
    let mut all = Vec::new();
    let began = Instant::now();
    while !all.iter().any(|e: &SpeechEvent| e.kind == SpeechEventKind::Partial) {
        all.extend(stream.wait_events(100).as_slice().iter().cloned());
        assert!(began.elapsed() < Duration::from_secs(5), "no partial: {:?}", kinds(&all));
    }
    let cancelled = Instant::now();
    stream.cancel();
    while stream.is_running() {
        all.extend(stream.wait_events(50).as_slice().iter().cloned());
        assert!(cancelled.elapsed() < Duration::from_secs(3), "the stream outlived its cancel");
    }
    all.extend(stream.poll_events().as_slice().iter().cloned());
    assert!(!all.iter().any(|e| e.kind == SpeechEventKind::Final), "{:?}", kinds(&all));
    assert_eq!(all.last().map(|e| e.kind), Some(SpeechEventKind::Ended));
}

#[test]
fn the_blocking_wait_returns_at_once_when_there_is_an_event_and_after_its_time_when_there_is_none() {
    let clock: SharedClock = Arc::new(SystemClock::new());
    let session = SpeechSession::on_chain(Chain::new(clock).with(Box::new(MockSpeech::new(Kind::Native, vec![Step::Silence]))));
    let stream = session.start_stream(AzString::from("en".to_string()), options(true, 0, 0));
    // The Ready event comes at once.
    let began = Instant::now();
    let first = stream.wait_events(2_000);
    assert_eq!(first.as_slice().first().map(|e| e.kind), Some(SpeechEventKind::Ready));
    assert!(began.elapsed() < Duration::from_secs(2), "waited {:?} for an event that was there", began.elapsed());
    // Nothing else is going to come for a while: the wait gives up when its time is up.
    let began = Instant::now();
    let none = stream.wait_events(60);
    assert!(none.as_slice().is_empty());
    assert!(began.elapsed() >= Duration::from_millis(55) && began.elapsed() < Duration::from_secs(2), "{:?}", began.elapsed());
    // poll never waits.
    let began = Instant::now();
    assert!(stream.poll_events().as_slice().is_empty());
    assert!(began.elapsed() < Duration::from_secs(1));
    assert_eq!(session.state(), SpeechState::Listening, "a running stream is listening");
    stream.cancel();
    while stream.is_running() {
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_closed_stream_has_no_events_and_is_not_running() {
    let closed = SpeechStream::default();
    assert!(closed.poll_events().as_slice().is_empty());
    assert!(closed.wait_events(10).as_slice().is_empty());
    assert!(!closed.is_running());
    closed.stop();
    closed.cancel();
    let _ = closed.clone();
    // A stream of a closed session ends at once with that.
    let events = events_until_ended(&SpeechSession::default().start_stream(AzString::from("en".to_string()), options(true, 1, 0)));
    assert_eq!(kinds(&events), [SpeechEventKind::Ended]);
    assert_eq!(events[0].heard.status, HeardStatus::Unavailable);
}
