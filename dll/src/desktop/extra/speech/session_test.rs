//! The session through its public face: the chain with scripted recognizers (the same chain code
//! a real run uses), the answers as the repr(C) types the C API hands out, the status lines, the
//! handles that share one session, and what `AZ_SPEECH_SCRIPT` makes of a test run.

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use azul_core::run_mode::TestHooks;
use azul_css::{AzString, StringVec};

use super::{
    builtin::{Microphone, MicrophoneSource},
    chain::Chain,
    keeper::MicKeeper,
    mock::{words_of, ManualClock, MockSpeech, Step},
    recognizer::{Kind, Outcome, SharedClock, SystemClock},
    answer_to_heard, heard_to_answer, with_timed_words, BuiltinSpeechStatus, HeardStatus, SpeechHeard,
    SpeechLimits, SpeechPreference, SpeechRecognizer, SpeechSession, SpeechStandby, SpeechState,
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
    // The refusal is what is under test here: the system's recognizer is asked first (English
    // would otherwise go to the built-in model first).
    session.set_preference(SpeechPreference::NativeFirst);
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
    assert!(lines(&session)[0].starts_with("native | ") && lines(&session)[0].contains("| ready"), "{:?}", lines(&session));
}

#[test]
fn a_clone_is_another_handle_on_the_same_session() {
    let session = session(vec![Step::Silence], vec![]);
    session.set_preference(SpeechPreference::NativeFirst);
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
            "builtin": [{"say": "navigate to Leipzig", "delay_ms": 0, "language": "en"}, {"silence": true}],
            "preference": "native_first"}"#,
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
/// once on every call, now with timed words and the scripted recognizer named. (The text itself
/// is the headless module's process-wide one, which its own tests set: this takes the answer
/// that module gave and checks what the chain makes of it.)
#[test]
fn the_old_stand_in_answer_gets_timed_words_and_a_quiet_one_stays_quiet() {
    let heard = with_timed_words(heard_to_answer(&SpeechHeard::heard("open the file", 1.0)), ORIGIN);
    assert_eq!(heard.text, "open the file");
    assert_eq!(heard.started_unix_ms, ORIGIN);
    assert_eq!(
        heard.words.iter().map(|w| (w.text.as_str(), w.start_ms, w.end_ms)).collect::<Vec<_>>(),
        vec![("open", 0, 300), ("the", 300, 600), ("file", 600, 900)]
    );
    let quiet = with_timed_words(heard_to_answer(&SpeechHeard::none(HeardStatus::NothingHeard, "nothing was said")), ORIGIN);
    assert!(quiet.words.is_empty());
    assert_eq!(quiet.started_unix_ms, 0);
    // The public answer carries who gave it.
    assert_eq!(answer_to_heard(heard, Some(Kind::Scripted)).recognizer, SpeechRecognizer::Scripted);
}

fn lines_of(session: &SpeechSession) -> Vec<String> {
    session.recognizers().as_slice().iter().map(|l| l.as_str().to_string()).collect()
}

fn words_list(words: &[&str]) -> StringVec {
    StringVec::from_vec(words.iter().map(|w| AzString::from((*w).to_string())).collect())
}

fn ask(session: &SpeechSession, language: &str) -> SpeechHeard {
    session.listen_blocking(AzString::from(language.to_string()), 5_000)
}

/// Who is asked first follows the preference, and the first status line says which order the last
/// call had and why.
#[test]
fn the_preference_decides_who_is_asked_first_and_the_first_status_line_says_why() {
    let session = session(
        vec![say(100, "from the system"), say(100, "from the system"), say(100, "from the system")],
        vec![say(100, "from the model"), say(100, "from the model")],
    );
    // Automatic: English goes to the built-in model, German to the system's recognizer.
    let heard = ask(&session, "en-US");
    assert_eq!((heard.recognizer, heard.text.as_str()), (SpeechRecognizer::Builtin, "from the model"));
    let lines = lines_of(&session);
    assert!(lines[0].starts_with("built-in | ") && lines[0].contains("tried first: English"), "{lines:?}");
    assert!(lines[1].starts_with("native | ") && !lines[1].contains("tried first"), "{lines:?}");
    let heard = ask(&session, "de-DE");
    assert_eq!((heard.recognizer, heard.text.as_str()), (SpeechRecognizer::Native, "from the system"));
    let lines = lines_of(&session);
    assert!(lines[0].starts_with("native | ") && lines[0].contains("tried first: German"), "{lines:?}");
    // What the app asked for wins.
    session.set_preference(SpeechPreference::BuiltinFirst);
    assert_eq!(ask(&session, "de-DE").recognizer, SpeechRecognizer::Builtin);
    assert!(lines_of(&session)[0].contains("tried first: as the app asked"), "{:?}", lines_of(&session));
    session.set_preference(SpeechPreference::NativeFirst);
    assert_eq!(ask(&session, "en-US").recognizer, SpeechRecognizer::Native);
    // The status line follows a change at once, before any call.
    session.set_preference(SpeechPreference::BuiltinFirst);
    assert!(lines_of(&session)[0].starts_with("built-in | "), "{:?}", lines_of(&session));
}

#[test]
fn an_identifier_in_the_vocabulary_puts_the_system_recognizer_first_even_in_english() {
    let session = session(
        vec![say(100, "from the system"), say(100, "from the system")],
        vec![say(100, "from the model"), say(100, "from the model")],
    );
    assert_eq!(session.set_vocabulary(words_list(&["enum", "CSS"])).as_str(), "");
    assert_eq!(ask(&session, "en-US").recognizer, SpeechRecognizer::Builtin, "acronyms and words are no identifiers");
    assert_eq!(session.set_vocabulary(words_list(&["enum", "CssError"])).as_str(), "");
    assert_eq!(ask(&session, "en-US").recognizer, SpeechRecognizer::Native);
    let lines = lines_of(&session);
    assert!(lines[0].starts_with("native | ") && lines[0].contains("tried first: an identifier is in the vocabulary"), "{lines:?}");
    // Without it the language decides again.
    let _ = session.set_vocabulary(words_list(&[]));
    assert!(lines_of(&session)[0].starts_with("built-in | "), "{:?}", lines_of(&session));
}

#[test]
fn a_built_in_model_that_is_not_there_is_never_asked_first_and_the_line_says_so() {
    let clock = ManualClock::shared(ORIGIN);
    let session = SpeechSession::on_chain(
        Chain::new(clock)
            .with(Box::new(MockSpeech::new(Kind::Native, vec![say(100, "from the system")])))
            .with(Box::new(MockSpeech::new(
                Kind::Builtin,
                vec![Step::Refuse {
                    outcome: Outcome::Unavailable,
                    message: "no model".to_string(),
                }],
            ))),
    );
    let heard = ask(&session, "en-US");
    assert_eq!(heard.recognizer, SpeechRecognizer::Native);
    assert_eq!(heard.message.as_str(), "", "the built-in recognizer was not even tried");
    // One recognizer ready: there was no choice to explain.
    let lines = lines_of(&session);
    assert!(!lines[0].contains("tried first"), "{lines:?}");
}

#[test]
fn a_script_can_pin_the_order_and_by_default_follows_the_language() {
    let dir = std::env::temp_dir().join(format!("azul-speech-order-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    let run = |name: &str, script: &str| {
        let file = dir.join(name);
        std::fs::write(&file, script).expect("a script");
        let hooks = TestHooks {
            speech_script: Some(file.into_os_string()),
            ..TestHooks::HEADLESS
        };
        SpeechSession::with_hooks(&hooks)
    };
    let both = r#""native": [{"say": "from the system", "delay_ms": 0}], "builtin": [{"say": "from the model", "delay_ms": 0}]"#;
    let by_language = run("auto.json", &format!("{{{both}}}"));
    assert_eq!(ask(&by_language, "en").recognizer, SpeechRecognizer::Builtin);
    let pinned = run("pinned.json", &format!("{{{both}, \"preference\": \"native_first\"}}"));
    assert_eq!(ask(&pinned, "en").recognizer, SpeechRecognizer::Native);
    let other = run("builtin.json", &format!("{{{both}, \"preference\": \"builtin_first\"}}"));
    assert_eq!(ask(&other, "de").recognizer, SpeechRecognizer::Builtin);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A UI polls this while the call runs on a worker thread: false before sound reaches the
/// recognizer, true from then until the call ends.
#[test]
fn the_session_says_when_sound_is_flowing_and_stops_saying_it_when_the_call_ends() {
    let clock: SharedClock = Arc::new(SystemClock::new());
    let session = SpeechSession::on_chain(Chain::new(clock).with(Box::new(MockSpeech::new(Kind::Native, vec![Step::Silence]))));
    assert!(!session.audio_flowing(), "nothing listens yet");
    let worker = session.clone();
    let handle = std::thread::spawn(move || worker.listen_blocking(AzString::from("de".to_string()), 60_000));
    let began = std::time::Instant::now();
    while !session.audio_flowing() {
        assert!(began.elapsed() < std::time::Duration::from_secs(5), "the sound never flowed");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    session.cancel();
    let heard = handle.join().expect("the worker");
    assert_eq!(heard.status, HeardStatus::NothingHeard);
    assert!(!session.audio_flowing(), "the call ended: no sound flows to it");
    // A closed session has no sound.
    assert!(!SpeechSession::default().audio_flowing());
}

/// A source of microphones that count how many were closed.
struct CountingSource(Arc<AtomicU32>);

struct CountingMicrophone(Arc<AtomicU32>);

impl MicrophoneSource for CountingSource {
    fn open(&mut self) -> Result<Box<dyn Microphone>, (Outcome, String)> {
        Ok(Box::new(CountingMicrophone(Arc::clone(&self.0))))
    }
}

impl Microphone for CountingMicrophone {
    fn rate(&self) -> u32 {
        16_000
    }

    fn read(&mut self, _out: &mut Vec<f32>, _wait_ms: u32) -> Result<(), String> {
        Ok(())
    }
}

impl Drop for CountingMicrophone {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn the_session_decides_how_long_the_microphone_stays_open_and_lets_it_go_on_cancel_reset_and_zero() {
    assert_eq!(SpeechLimits::default().mic_keep_ms, 5_000, "five seconds unless the app says otherwise");
    let closed = Arc::new(AtomicU32::new(0));
    let keeper = MicKeeper::new(Box::new(CountingSource(Arc::clone(&closed))));
    let clock = ManualClock::shared(ORIGIN);
    let session = SpeechSession::on_chain_with_keeper(
        Chain::new(clock).with(Box::new(MockSpeech::new(Kind::Native, vec![]))),
        keeper.clone(),
    );
    session.set_limits(SpeechLimits::default());
    assert_eq!(keeper.keep_ms(), 5_000);

    keeper.acquire().expect("a microphone").finish(true);
    assert!(keeper.is_held());
    session.reset();
    assert!(!keeper.is_held(), "a reset lets the microphone go");
    assert_eq!(closed.load(Ordering::SeqCst), 1);

    keeper.acquire().expect("a microphone").finish(true);
    session.cancel();
    assert!(!keeper.is_held(), "so does a cancel when nothing is listening");
    assert_eq!(closed.load(Ordering::SeqCst), 2);

    keeper.acquire().expect("a microphone").finish(true);
    session.set_limits(SpeechLimits {
        mic_keep_ms: 0,
        ..SpeechLimits::default()
    });
    assert!(!keeper.is_held(), "a time of zero closes it at once");
    assert_eq!(closed.load(Ordering::SeqCst), 3);

    session.set_limits(SpeechLimits {
        mic_keep_ms: u32::MAX,
        ..SpeechLimits::default()
    });
    assert_eq!(keeper.keep_ms(), 60_000, "a minute is the most");

    // The end of the session closes what is kept.
    keeper.acquire().expect("a microphone").finish(true);
    drop(keeper);
    drop(session);
    let began = std::time::Instant::now();
    while closed.load(Ordering::SeqCst) < 4 {
        assert!(began.elapsed() < std::time::Duration::from_secs(2), "the microphone outlived its session");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn watched_session(native: Vec<Step>) -> (SpeechSession, MicKeeper, Arc<AtomicU32>) {
    let closed = Arc::new(AtomicU32::new(0));
    let keeper = MicKeeper::new(Box::new(CountingSource(Arc::clone(&closed))));
    let clock = ManualClock::shared(ORIGIN);
    let session = SpeechSession::on_chain_with_keeper(
        Chain::new(clock).with(Box::new(MockSpeech::new(Kind::Native, native))),
        keeper.clone(),
    );
    session.set_limits(SpeechLimits::default());
    (session, keeper, closed)
}

#[test]
fn the_state_goes_from_cold_through_armed_and_warm_to_listening_and_back() {
    let (session, keeper, closed) = watched_session(vec![say(100, "first")]);
    assert_eq!(session.state(), SpeechState::Cold, "nothing was done yet");
    // Arming opens the microphone: armed, whatever was initialised.
    assert_eq!(session.arm().as_str(), "", "armed");
    assert_eq!(session.state(), SpeechState::Armed);
    assert!(keeper.is_held());
    session.disarm();
    assert_eq!((session.state(), closed.load(Ordering::SeqCst)), (SpeechState::Cold, 1));
    // A call initialises the recognizer (a scripted one does not touch the microphone).
    let _ = ask(&session, "en");
    assert_eq!(session.state(), SpeechState::Warm, "initialised, microphone closed");
    // A call that kept the microphone leaves the session armed until it is let go.
    keeper.acquire().expect("a microphone").finish(true);
    assert_eq!(session.state(), SpeechState::Armed);
    session.reset();
    assert_eq!(session.state(), SpeechState::Warm);
    // While a call runs it is listening.
    let clock: SharedClock = Arc::new(SystemClock::new());
    let listening = SpeechSession::on_chain(Chain::new(clock).with(Box::new(MockSpeech::new(Kind::Native, vec![Step::Silence]))));
    let worker = listening.clone();
    let handle = std::thread::spawn(move || worker.listen_blocking(AzString::from("en".to_string()), 60_000));
    let began = std::time::Instant::now();
    while listening.state() != SpeechState::Listening {
        assert!(began.elapsed() < std::time::Duration::from_secs(5), "the call never listened");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    listening.cancel();
    let _ = handle.join().expect("the worker");
    assert_ne!(listening.state(), SpeechState::Listening, "the call ended");
    assert_eq!(SpeechSession::default().state(), SpeechState::Cold, "a closed session has nothing");
}

#[test]
fn the_standby_decides_what_stays_open_and_arm_says_why_when_it_cannot() {
    let (session, keeper, _closed) = watched_session(vec![say(100, "first"), say(100, "second"), say(100, "third")]);
    // Off: nothing is kept, and an arm says so.
    session.set_standby(SpeechStandby::Off);
    assert!(session.arm().as_str().contains("off"), "{}", session.arm().as_str());
    let _ = ask(&session, "en");
    assert!(!keeper.is_held(), "off: the call closed the microphone");
    // On demand (the default): kept for mic_keep_ms after a call (a scripted call does not use the
    // microphone: the keeper is asked as a recognizer that listens with it would).
    session.set_standby(SpeechStandby::OnDemand);
    keeper.acquire().expect("a microphone").finish(true);
    assert!(keeper.is_held());
    session.reset();
    // While armed: an arm holds it for the armed time, which a short keep time does not end.
    session.set_limits(SpeechLimits {
        mic_keep_ms: 50,
        armed_idle_ms: 5_000,
        ..SpeechLimits::default()
    });
    session.set_standby(SpeechStandby::WhileArmed);
    assert_eq!(session.arm().as_str(), "");
    std::thread::sleep(std::time::Duration::from_millis(250));
    assert!(keeper.is_held(), "armed: the 50 ms of a call do not apply");
    session.disarm();
    assert!(!keeper.is_held());
    // Always: no limit; a cancel does not close it, a disarm does.
    session.set_standby(SpeechStandby::Always);
    assert_eq!(session.arm().as_str(), "");
    session.cancel();
    assert!(keeper.is_held(), "always open: a cancel of a call is not the end of it");
    session.disarm();
    assert!(!keeper.is_held());
    // Leaving Always puts the limits back.
    session.set_standby(SpeechStandby::OnDemand);
    assert_eq!(SpeechStandby::default(), SpeechStandby::OnDemand);
    assert_eq!(SpeechLimits::default().armed_idle_ms, 60_000);
}
