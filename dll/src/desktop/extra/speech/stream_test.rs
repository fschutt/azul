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
    HeadlessRecognizer, HeardStatus, SpeechEvent, SpeechEventKind, SpeechHeard, SpeechRecognizer, SpeechSession,
    SpeechState, SpeechStream, SpeechStreamOptions,
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
    drop(closed.clone());
    // A stream of a closed session ends at once with that.
    let events = events_until_ended(&SpeechSession::default().start_stream(AzString::from("en".to_string()), options(true, 1, 0)));
    assert_eq!(kinds(&events), [SpeechEventKind::Ended]);
    assert_eq!(events[0].heard.status, HeardStatus::Unavailable);
}

/// `is_running` is false once the `Ended` event can be read: the thread of a stream pushed the end and
/// cleared the flag afterwards, so a caller that had just read `Ended` could still see `true` (the
/// documented meaning of the flag is "events may still wait, but nothing is listening"). Many streams
/// in a row, the flag read at once after each end.
#[test]
fn a_stream_is_not_running_any_more_when_its_ended_event_can_be_read() {
    let session = session_of(vec![Step::Silence], None);
    for round in 0..3_000 {
        let stream = session.start_stream(AzString::from("en".to_string()), options(false, 0, 500));
        let mut ended = false;
        let began = Instant::now();
        while !ended {
            ended = stream.wait_events(50).as_slice().iter().any(|e| e.kind == SpeechEventKind::Ended);
            assert!(began.elapsed() < Duration::from_secs(5), "round {round}: the stream never ended");
        }
        assert!(!stream.is_running(), "round {round}: Ended was readable while the stream still ran");
    }
}


// ---- the stand-in of a headless run (AZ_SPEECH_HEARD) as a stream ----

/// A stand-in that hears `text` (or nothing, when empty) and whose text a test can change.
fn stand_in(text: &Arc<std::sync::Mutex<String>>) -> HeadlessRecognizer {
    let text = Arc::clone(text);
    HeadlessRecognizer::hearing(Box::new(move || {
        let text = text.lock().unwrap().clone();
        if text.is_empty() {
            SpeechHeard::none(HeardStatus::NothingHeard, "nothing was said")
        } else {
            SpeechHeard::heard(&text, 1.0)
        }
    }))
}

/// One utterance per AZ_SPEECH_HEARD, then the room is quiet: the default loop of a recognizer that
/// only knows `listen` answered the stand-in again and again, as fast as it could.
#[test]
fn a_stream_over_the_stand_in_hears_what_is_set_once_and_is_then_quiet_until_it_ends() {
    let heard = Arc::new(std::sync::Mutex::new("open the file".to_string()));
    let clock = ManualClock::shared(ORIGIN);
    let session = SpeechSession::on_chain(Chain::new(clock).with(Box::new(stand_in(&heard))));
    let stream = session.start_stream(AzString::from("en".to_string()), options(true, 0, 3_500));
    let events = events_until_ended(&stream);
    use SpeechEventKind::{Ended, Final, Partial, Quiet, Ready};
    assert_eq!(
        kinds(&events),
        [Ready, Partial, Partial, Partial, Final, Quiet, Quiet, Quiet, Ended],
        "one utterance, then a quiet room every second until the time is up"
    );
    let finals: Vec<&str> = events.iter().filter(|e| e.kind == Final).map(|e| e.heard.text.as_str()).collect();
    assert_eq!(finals, ["open the file"]);
    assert_eq!(events[1].heard.text.as_str(), "open", "a partial per word");
    assert_eq!(events[3].heard.text.as_str(), "open the file");
    let quiet_at: Vec<u32> = events.iter().filter(|e| e.kind == Quiet).map(|e| e.at_ms).collect();
    assert_eq!(quiet_at, [1_000, 2_000, 3_000]);
    assert_eq!(events[4].heard.recognizer, SpeechRecognizer::Scripted);
    assert_eq!(events[8].heard.status, HeardStatus::Heard, "something was heard");
}

#[test]
fn the_stand_in_stream_ends_after_the_utterances_asked_for_and_hears_nothing_in_a_quiet_room() {
    let heard = Arc::new(std::sync::Mutex::new("open the file".to_string()));
    let clock = ManualClock::shared(ORIGIN);
    let session = SpeechSession::on_chain(Chain::new(clock).with(Box::new(stand_in(&heard))));
    let events = events_until_ended(&session.start_stream(AzString::from("en".to_string()), options(false, 1, 0)));
    assert_eq!(kinds(&events), [SpeechEventKind::Ready, SpeechEventKind::Final, SpeechEventKind::Ended]);

    // Nothing set: nothing heard, only the room.
    let nothing = Arc::new(std::sync::Mutex::new(String::new()));
    let clock = ManualClock::shared(ORIGIN);
    let session = SpeechSession::on_chain(Chain::new(clock).with(Box::new(stand_in(&nothing))));
    let events = events_until_ended(&session.start_stream(AzString::from("en".to_string()), options(true, 0, 2_500)));
    use SpeechEventKind::{Ended, Quiet, Ready};
    assert_eq!(kinds(&events), [Ready, Quiet, Quiet, Ended]);
    assert_eq!(events[3].heard.status, HeardStatus::NothingHeard);
}

/// On a clock that really waits: a new AZ_SPEECH_HEARD is a new utterance, the same one is not heard
/// twice, stop and cancel end the stream at once.
#[test]
fn a_new_text_is_a_new_utterance_and_stop_and_cancel_end_the_stand_in_stream_at_once() {
    for how in ["stop", "cancel"] {
        let heard = Arc::new(std::sync::Mutex::new("first".to_string()));
        let clock: SharedClock = Arc::new(SystemClock::new());
        let session = SpeechSession::on_chain(Chain::new(clock).with(Box::new(stand_in(&heard))));
        let stream = session.start_stream(AzString::from("en".to_string()), options(false, 0, 0));
        let mut all = Vec::new();
        let began = Instant::now();
        let wait_for = |all: &mut Vec<SpeechEvent>, finals: usize| {
            while all.iter().filter(|e| e.kind == SpeechEventKind::Final).count() < finals {
                all.extend(stream.wait_events(50).as_slice().iter().cloned());
                assert!(began.elapsed() < Duration::from_secs(5), "{how}: no final {finals}: {:?}", kinds(all));
            }
        };
        wait_for(&mut all, 1);
        // The same text is not an utterance again; a changed one is.
        std::thread::sleep(Duration::from_millis(350));
        all.extend(stream.wait_events(10).as_slice().iter().cloned());
        assert_eq!(all.iter().filter(|e| e.kind == SpeechEventKind::Final).count(), 1, "{how}: heard twice: {:?}", kinds(&all));
        *heard.lock().unwrap() = "second".to_string();
        wait_for(&mut all, 2);
        let acted = Instant::now();
        if how == "stop" { stream.stop() } else { stream.cancel() }
        while stream.is_running() {
            all.extend(stream.wait_events(50).as_slice().iter().cloned());
            assert!(acted.elapsed() < Duration::from_secs(2), "{how}: the stream outlived it");
        }
        all.extend(stream.poll_events().as_slice().iter().cloned());
        let finals: Vec<&str> = all.iter().filter(|e| e.kind == SpeechEventKind::Final).map(|e| e.heard.text.as_str()).collect();
        assert_eq!(finals, ["first", "second"], "{how}");
        assert_eq!(all.last().map(|e| e.kind), Some(SpeechEventKind::Ended), "{how}");
    }
}

/// A recognizer that only knows `listen` and answers at once, without a pause (a platform's
/// recognizer that has no stream of its own, or a stand-in): the default stream must not loop it as fast
/// as it can.
struct AnswersAtOnce {
    calls: Arc<std::sync::atomic::AtomicU32>,
}

impl super::recognizer::Recognizer for AnswersAtOnce {
    fn kind(&self) -> Kind {
        Kind::Scripted
    }

    fn label(&self) -> String {
        "answers at once".to_string()
    }

    fn readiness(&mut self) -> super::recognizer::Readiness {
        super::recognizer::Readiness::Ready
    }

    fn listen(&mut self, call: &super::recognizer::Listen<'_>) -> super::recognizer::Answer {
        let calls = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        assert!(calls < 10_000, "listened {calls} times in no time: the stream loops an instant answer as fast as it can");
        super::recognizer::Answer::heard_words(words_of("again", 0, 300, 1.0), "en", call.clock.unix_ms())
    }
}

#[test]
fn the_default_stream_of_a_recognizer_that_answers_at_once_keeps_a_pace() {
    let calls = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let clock = ManualClock::shared(ORIGIN);
    let session = SpeechSession::on_chain(Chain::new(clock).with(Box::new(AnswersAtOnce { calls: Arc::clone(&calls) })));
    let events = events_until_ended(&session.start_stream(AzString::from("en".to_string()), options(false, 0, 5_000)));
    let finals = events.iter().filter(|e| e.kind == SpeechEventKind::Final).count();
    assert!((10..=25).contains(&finals), "{finals} finals in 5 s: a pause of about a quarter of a second between two");
    assert_eq!(events.last().map(|e| e.kind), Some(SpeechEventKind::Ended));
}
