//! The session through its public face: the chain with scripted recognizers (the same chain code
//! a real run uses), the answers as the repr(C) types the C API hands out, the status lines, the
//! handles that share one session, and what `AZ_SPEECH_SCRIPT` makes of a test run.

use std::sync::Arc;

use azul_core::run_mode::TestHooks;
use azul_css::{AzString, StringVec};

use super::{
    chain::Chain,
    mock::{words_of, ManualClock, MockSpeech, Step},
    recognizer::{Kind, Outcome, SharedClock, SystemClock},
    BuiltinSpeechStatus, HeardStatus, SpeechLimits, SpeechRecognizer, SpeechSession,
};

const ORIGIN: u64 = 1_700_000_000_000;

fn say(delay_ms: u32, text: &str) -> Step {
    Step::Say {
        delay_ms,
        words: words_of(text, 0, 300, 0.9),
        language: "en".to_string(),
    }
}

fn session(native: Vec<Step>, builtin: Vec<Step>) -> SpeechSession {
    let clock = ManualClock::shared(ORIGIN);
    SpeechSession::on_chain(
        Chain::new(clock)
            .with(Box::new(MockSpeech::new(Kind::Native, native)))
            .with(Box::new(MockSpeech::new(Kind::Builtin, builtin))),
    )
}

#[test]
fn the_answer_says_who_gave_it_with_the_words_and_when_the_audio_began() {
    let session = session(
        vec![Step::Refuse {
            outcome: Outcome::Denied,
            message: "the microphone is turned off for Terminal".to_string(),
        }],
        vec![say(400, "move the label"), say(400, "and delete it")],
    );
    let heard = session.listen_blocking(AzString::from("en-US".to_string()), 5_000);
    assert_eq!(heard.status, HeardStatus::Heard);
    assert_eq!(heard.recognizer, SpeechRecognizer::Builtin);
    assert_eq!(heard.text.as_str(), "move the label");
    assert_eq!(heard.language.as_str(), "en");
    assert_eq!(heard.started_unix_ms, ORIGIN);
    let words = heard.words.as_slice();
    assert_eq!(words.len(), 3);
    assert_eq!(words[0].text.as_str(), "move");
    assert_eq!((words[0].start_ms, words[0].end_ms), (0, 300));
    assert!((words[1].probability - 0.9).abs() < 1e-6);
    assert!(heard.message.as_str().contains("refused (the microphone is turned off for Terminal)"), "{}", heard.message.as_str());
    // The second call does not say it again.
    let again = session.listen_blocking(AzString::from("en-US".to_string()), 5_000);
    assert_eq!(again.message.as_str(), "");
}

#[test]
fn the_status_lines_name_each_recognizer_and_why_one_is_not_used() {
    let session = session(
        vec![Step::Refuse {
            outcome: Outcome::Unavailable,
            message: "no usage text".to_string(),
        }],
        vec![say(100, "hello")],
    );
    let lines = |s: &SpeechSession| -> Vec<String> { s.recognizers().as_slice().iter().map(|l| l.as_str().to_string()).collect() };
    let before = lines(&session);
    assert_eq!(before.len(), 2);
    assert!(before[0].starts_with("native | ") && before[0].ends_with("| unavailable: no usage text"), "{before:?}");
    assert!(before[1].starts_with("built-in | ") && before[1].ends_with("| ready"), "{before:?}");
    let _ = session.listen_blocking(AzString::from(String::new()), 5_000);
    let after = lines(&session);
    assert!(after[0].contains("unavailable: no usage text"), "{after:?}");
    session.reset();
    // The script had one refusal only: after the reset the native recognizer is asked again.
    assert!(lines(&session)[0].ends_with("| ready"), "{:?}", lines(&session));
}

#[test]
fn a_clone_is_another_handle_on_the_same_session() {
    let session = session(vec![Step::Silence], vec![]);
    let clone = session.clone();
    assert!(clone.run_destructor && !clone.ptr.is_null());
    assert_eq!(clone.ptr, session.ptr, "the same session");
    drop(session);
    // The session lives on in the clone.
    let heard = clone.listen_blocking(AzString::from("en".to_string()), 1_000);
    assert_eq!(heard.status, HeardStatus::NothingHeard);
    assert_eq!(heard.recognizer, SpeechRecognizer::Native);
    drop(clone);
    // A default handle is closed, and says so.
    let closed = SpeechSession::default();
    let heard = closed.listen_blocking(AzString::from("en".to_string()), 1_000);
    assert_eq!(heard.status, HeardStatus::Unavailable);
    assert!(heard.message.as_str().contains("closed"), "{}", heard.message.as_str());
    assert!(closed.recognizers().as_slice().is_empty());
    closed.cancel();
    closed.reset();
    let _ = closed.clone();
}

#[test]
fn a_cancel_ends_the_call_that_runs() {
    // A clock that really waits: the call of a minute ends only because of the flag.
    let clock: SharedClock = Arc::new(SystemClock::new());
    let session = SpeechSession::on_chain(Chain::new(clock).with(Box::new(MockSpeech::new(Kind::Native, vec![Step::Silence]))));
    let worker = session.clone();
    let began = std::time::Instant::now();
    let handle = std::thread::spawn(move || worker.listen_blocking(AzString::from("en".to_string()), 60_000));
    std::thread::sleep(std::time::Duration::from_millis(60));
    session.cancel();
    let heard = handle.join().expect("the worker");
    assert_eq!(heard.status, HeardStatus::NothingHeard);
    assert!(began.elapsed() < std::time::Duration::from_secs(5), "ended by the flag, not by the minute");
}

#[test]
fn the_limits_are_kept_to_and_can_be_changed() {
    let session = session(vec![Step::Silence, Step::Silence], vec![]);
    let heard = session.listen_blocking(AzString::from("en".to_string()), 5);
    assert!(heard.message.as_str().contains("too short"), "{}", heard.message.as_str());
    session.set_limits(SpeechLimits {
        min_listen_ms: 5,
        ..SpeechLimits::default()
    });
    let heard = session.listen_blocking(AzString::from("en".to_string()), 5);
    assert!(!heard.message.as_str().contains("too short"), "{}", heard.message.as_str());
    let defaults = SpeechLimits::default();
    assert_eq!((defaults.min_listen_ms, defaults.max_listen_ms, defaults.max_keywords, defaults.max_model_mb), (500, 60_000, 200, 128));
}

#[test]
fn a_scripted_session_registers_nothing_real_and_answers_from_the_script() {
    // The script has a built-in recognizer: a registration is answered Ready and loads nothing.
    let with_builtin = session(vec![Step::Silence], vec![Step::Silence]);
    assert_eq!(
        with_builtin.set_builtin_files(AzString::from("/no/engine".to_string()), AzString::from("/no/model".to_string())),
        BuiltinSpeechStatus::Ready
    );
    // A script without one is honest about it.
    let clock = ManualClock::shared(ORIGIN);
    let only_native = SpeechSession::on_chain(Chain::new(clock).with(Box::new(MockSpeech::new(Kind::Native, vec![]))));
    assert_eq!(
        only_native.set_builtin_files(AzString::from(String::new()), AzString::from(String::new())),
        BuiltinSpeechStatus::EngineMissing
    );
    let note = only_native.set_vocabulary(StringVec::from_vec(vec![AzString::from("AzReview".to_string())]));
    assert_eq!(note.as_str(), "", "nothing was cut");
}

/// What `AZ_SPEECH_SCRIPT` names becomes the chain of a test run: here a script of a refusing
/// native recognizer and a built-in one that hears two things, the second one late.
#[test]
fn a_test_run_follows_the_script_its_hook_names() {
    let dir = std::env::temp_dir().join(format!("azul-speech-session-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    let file = dir.join("script.json");
    std::fs::write(
        &file,
        r#"{"native": [{"refuse": "denied", "message": "no mic for you"}],
            "builtin": [{"say": "navigate to Leipzig", "delay_ms": 0, "language": "en"}, {"silence": true}]}"#,
    )
    .expect("a script");
    let hooks = TestHooks {
        speech_script: Some(file.clone().into_os_string()),
        ..TestHooks::HEADLESS
    };
    let session = SpeechSession::with_hooks(&hooks);
    let first = session.listen_blocking(AzString::from("en".to_string()), 1_000);
    assert_eq!(first.status, HeardStatus::Heard, "{}", first.message.as_str());
    assert_eq!(first.recognizer, SpeechRecognizer::Builtin);
    assert_eq!(first.text.as_str(), "navigate to Leipzig");
    assert!(first.message.as_str().contains("no mic for you"), "{}", first.message.as_str());
    let second = session.listen_blocking(AzString::from("en".to_string()), 500);
    assert_eq!(second.status, HeardStatus::NothingHeard);

    // A script that is not there or not valid is a refusal that names the file, not a quiet room.
    let missing = TestHooks {
        speech_script: Some(dir.join("nope.json").into_os_string()),
        ..TestHooks::HEADLESS
    };
    let heard = SpeechSession::with_hooks(&missing).listen_blocking(AzString::from("en".to_string()), 500);
    assert_eq!(heard.status, HeardStatus::Unavailable);
    assert!(heard.message.as_str().contains("AZ_SPEECH_SCRIPT") && heard.message.as_str().contains("nope.json"), "{}", heard.message.as_str());
    std::fs::write(&file, r#"{"natve": []}"#).expect("a bad script");
    let bad = TestHooks {
        speech_script: Some(file.into_os_string()),
        ..TestHooks::HEADLESS
    };
    let heard = SpeechSession::with_hooks(&bad).listen_blocking(AzString::from("en".to_string()), 500);
    assert!(heard.message.as_str().contains("nobody knows"), "{}", heard.message.as_str());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Without a script a headless run is the old stand-in: what `AZ_SPEECH_HEARD` holds is heard at
/// once on every call, now with timed words and the scripted recognizer named.
#[test]
fn a_headless_run_without_a_script_hears_the_old_variable_as_the_scripted_recognizer() {
    let hooks = TestHooks {
        speech_heard: Some("open the file".to_string()),
        ..TestHooks::HEADLESS
    };
    // The text a test hears is kept by the headless stand-in: set it through its own door.
    super::headless::set_heard(Some("open the file"));
    let session = SpeechSession::with_hooks(&hooks);
    let heard = session.listen_blocking(AzString::from("en".to_string()), 1_000);
    super::headless::set_heard(None);
    assert_eq!(heard.status, HeardStatus::Heard);
    assert_eq!(heard.recognizer, SpeechRecognizer::Scripted);
    assert_eq!(heard.text.as_str(), "open the file");
    assert_eq!(heard.words.as_slice().len(), 3);
}
