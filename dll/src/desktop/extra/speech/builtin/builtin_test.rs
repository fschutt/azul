//! The built-in recognizer on a scripted microphone and a scripted engine: speech in a room is
//! cut out, handed to the engine once, and the engine's words come back with their times and the
//! moment the audio began. What the engine and the microphone do wrong is a status, never a crash.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use super::{
    default_places, engine_file_name, model_language, Builtin, BuiltinStatus, Microphone, MicrophoneSource, Tuning,
    MODEL_FILE_NAME,
};
use super::engine::Engine;
use super::super::{
    keeper::MicKeeper,
    mock::ManualClock,
    recognizer::{Clock, Kind, Listen, Outcome, Readiness, Recognizer},
};

const ORIGIN: u64 = 1_700_000_000_000;

/// The engine's own answer for the German clip (see json_test).
const GERMAN_REPLY: &str = r#"{"text":"Bitte verschiebe die Beschriftung des Flusses nach links.","language":"de","words":[{"word":"Bitte","start":0.40,"end":0.72,"probability":0.996},{"word":"verschiebe","start":0.72,"end":1.28,"probability":0.992},{"word":"die","start":1.28,"end":1.44,"probability":1.000},{"word":"Beschriftung","start":1.44,"end":2.00,"probability":0.999},{"word":"des","start":2.00,"end":2.24,"probability":1.000},{"word":"Flusses","start":2.24,"end":2.72,"probability":0.834},{"word":"nach","start":2.72,"end":2.96,"probability":0.998},{"word":"links.","start":2.96,"end":3.20,"probability":0.909}],"ttft_ms":54.7,"decode_tps":250.4}"#;

/// What the scripted engine was asked.
#[derive(Debug, Default, Clone, PartialEq)]
struct Seen {
    model_bytes: usize,
    threads: Option<u32>,
    /// (samples, language, keywords) of each transcription.
    calls: Vec<(usize, String, String)>,
}

struct FakeEngine {
    seen: Arc<Mutex<Seen>>,
    reply: Result<String, String>,
    load: Result<(), String>,
}

impl FakeEngine {
    fn new(reply: Result<String, String>) -> (Box<Self>, Arc<Mutex<Seen>>) {
        let seen = Arc::new(Mutex::new(Seen::default()));
        (
            Box::new(Self {
                seen: Arc::clone(&seen),
                reply,
                load: Ok(()),
            }),
            seen,
        )
    }
}

impl Engine for FakeEngine {
    fn load(&mut self, model: &[u8]) -> Result<(), String> {
        self.seen.lock().unwrap().model_bytes = model.len();
        self.load.clone()
    }

    fn set_threads(&mut self, threads: u32) {
        self.seen.lock().unwrap().threads = Some(threads);
    }

    fn transcribe(&mut self, pcm: &[f32], language: &str, keywords: &str) -> Result<String, String> {
        self.seen
            .lock()
            .unwrap()
            .calls
            .push((pcm.len(), language.to_string(), keywords.to_string()));
        self.reply.clone()
    }
}

/// What a scripted microphone does.
enum Take {
    /// This much sound.
    Sound(Vec<f32>),
    /// The microphone breaks.
    Break(String),
}

struct FakeMicrophone {
    rate: u32,
    takes: std::collections::VecDeque<Take>,
}

impl Microphone for FakeMicrophone {
    fn rate(&self) -> u32 {
        self.rate
    }

    fn read(&mut self, out: &mut Vec<f32>, _wait_ms: u32) -> Result<(), String> {
        match self.takes.pop_front() {
            Some(Take::Sound(samples)) => {
                out.extend(samples);
                Ok(())
            }
            Some(Take::Break(why)) => Err(why),
            // After the script nothing comes: a device that is open and silent.
            None => Ok(()),
        }
    }
}

struct FakeSource {
    rate: u32,
    takes: Option<Vec<Take>>,
    refuse: Option<(Outcome, String)>,
    opened: Arc<Mutex<u32>>,
}

impl MicrophoneSource for FakeSource {
    fn open(&mut self) -> Result<Box<dyn Microphone>, (Outcome, String)> {
        *self.opened.lock().unwrap() += 1;
        if let Some(refusal) = self.refuse.clone() {
            return Err(refusal);
        }
        Ok(Box::new(FakeMicrophone {
            rate: self.rate,
            takes: self.takes.take().unwrap_or_default().into(),
        }))
    }
}

fn tone(rate: u32, ms: u32, amplitude: f32) -> Vec<f32> {
    let n = (u64::from(rate) * u64::from(ms) / 1000) as usize;
    (0..n)
        .map(|i| amplitude * (2.0 * std::f32::consts::PI * 250.0 * i as f32 / rate as f32).sin())
        .collect()
}

fn quiet(rate: u32, ms: u32) -> Vec<f32> {
    // A very quiet room: a small fixed ripple.
    let n = (u64::from(rate) * u64::from(ms) / 1000) as usize;
    (0..n).map(|i| 0.0004 * (i as f32 * 0.37).sin()).collect()
}

/// `room` ms of room, `speech` ms of voice, `after` ms of room, as reads of 100 ms.
fn takes(rate: u32, room: u32, speech: u32, after: u32) -> Vec<Take> {
    let mut signal = quiet(rate, room);
    signal.extend(tone(rate, speech, 0.05));
    signal.extend(quiet(rate, after));
    let chunk = (rate / 10) as usize;
    signal.chunks(chunk).map(|c| Take::Sound(c.to_vec())).collect()
}

struct Rig {
    builtin: Builtin,
    seen: Arc<Mutex<Seen>>,
    opened: Arc<Mutex<u32>>,
    clock: Arc<ManualClock>,
}

fn rig(reply: Result<String, String>, rate: u32, script: Vec<Take>, refuse: Option<(Outcome, String)>) -> Rig {
    let (engine, seen) = FakeEngine::new(reply);
    let opened = Arc::new(Mutex::new(0));
    let source = FakeSource {
        rate,
        takes: Some(script),
        refuse,
        opened: Arc::clone(&opened),
    };
    let builtin = Builtin::with_engine(engine, &[0u8; 1234], Box::new(source), Tuning::default()).expect("a model");
    Rig {
        builtin,
        seen,
        opened,
        clock: ManualClock::shared(ORIGIN),
    }
}

fn listen(rig: &mut Rig, language: &str, timeout_ms: u32) -> super::super::recognizer::Answer {
    let never = || false;
    rig.builtin.listen(&Listen {
        language,
        timeout_ms,
        clock: &*rig.clock,
        cancel: &never,
        flowing: &|| {},
    })
}

#[test]
fn speech_in_a_room_is_cut_out_and_transcribed_once_with_words_dated_by_the_clock() {
    let mut rig = rig(Ok(GERMAN_REPLY.to_string()), 48_000, takes(48_000, 600, 1_200, 1_500), None);
    let answer = listen(&mut rig, "de-DE", 10_000);
    assert_eq!(answer.outcome, Outcome::Heard, "{}", answer.message);
    assert_eq!(answer.text, "Bitte verschiebe die Beschriftung des Flusses nach links.");
    assert_eq!(answer.language, "de");
    assert_eq!(answer.words.len(), 8);
    assert_eq!(answer.words[0].text, "Bitte");
    assert_eq!((answer.words[0].start_ms, answer.words[0].end_ms), (400, 720));
    // 600 ms of room, speech from 600 on, 300 ms kept before it: the audio handed over began at 300 ms.
    assert!((answer.started_unix_ms as i64 - (ORIGIN as i64 + 300)).abs() <= 40, "{}", answer.started_unix_ms - ORIGIN);
    let seen = rig.seen.lock().unwrap().clone();
    assert_eq!(seen.model_bytes, 1234);
    assert_eq!(seen.threads, Some(1), "one engine thread by default");
    assert_eq!(seen.calls.len(), 1, "one utterance, one transcription");
    let (samples, language, keywords) = &seen.calls[0];
    assert_eq!(language, "de", "de-DE is the model's de");
    assert_eq!(keywords, "");
    // 300 before + 1200 of speech + 300 after, at 16 kHz.
    let ms = *samples as i64 * 1000 / 16_000;
    assert!((ms - 1_800).abs() <= 80, "{ms} ms of audio");
}

#[test]
fn a_quiet_room_is_nothing_heard_and_the_engine_is_not_run() {
    let mut rig = rig(Ok(GERMAN_REPLY.to_string()), 48_000, takes(48_000, 3_000, 0, 0), None);
    let answer = listen(&mut rig, "de", 2_000);
    assert_eq!(answer.outcome, Outcome::NothingHeard);
    assert!(rig.seen.lock().unwrap().calls.is_empty());
}

#[test]
fn a_language_the_model_does_not_know_is_detected_and_the_answer_says_so() {
    let mut rig = rig(Ok(GERMAN_REPLY.to_string()), 16_000, takes(16_000, 500, 1_000, 1_500), None);
    let answer = listen(&mut rig, "ja-JP", 10_000);
    assert_eq!(answer.outcome, Outcome::Heard);
    assert_eq!(rig.seen.lock().unwrap().calls[0].1, "", "the engine detects it");
    assert!(answer.message.contains("does not know ja-JP"), "{}", answer.message);
    // The system's language (empty) is detected without a word about it.
    let mut again = rig_default_with(16_000, takes(16_000, 500, 1_000, 1_500));
    let answer = listen(&mut again, "", 10_000);
    assert_eq!(answer.message, "");
}

fn rig_default_with(rate: u32, script: Vec<Take>) -> Rig {
    rig(Ok(GERMAN_REPLY.to_string()), rate, script, None)
}

#[test]
fn what_the_engine_does_wrong_is_a_failed_answer_with_its_words() {
    let mut failing = rig(Err("out of memory".to_string()), 16_000, takes(16_000, 500, 1_000, 1_500), None);
    let answer = listen(&mut failing, "en", 10_000);
    assert_eq!(answer.outcome, Outcome::Failed);
    assert!(answer.message.contains("the speech engine failed: out of memory"), "{}", answer.message);

    let mut garbled = rig(Ok("not json at all".to_string()), 16_000, takes(16_000, 500, 1_000, 1_500), None);
    let answer = listen(&mut garbled, "en", 10_000);
    assert_eq!(answer.outcome, Outcome::Failed);
    assert!(answer.message.contains("could not be read"), "{}", answer.message);

    let mut empty = rig(
        Ok(r#"{"text":"","language":"","words":[]}"#.to_string()),
        16_000,
        takes(16_000, 500, 1_000, 1_500),
        None,
    );
    assert_eq!(listen(&mut empty, "en", 10_000).outcome, Outcome::NothingHeard, "the engine heard nothing in it");
}

#[test]
fn a_refused_microphone_is_the_answer_and_the_engine_is_not_run() {
    let mut rig = rig(
        Ok(GERMAN_REPLY.to_string()),
        16_000,
        Vec::new(),
        Some((Outcome::Denied, "the microphone is turned off for Terminal".to_string())),
    );
    let answer = listen(&mut rig, "en", 5_000);
    assert_eq!(answer.outcome, Outcome::Denied);
    assert_eq!(answer.message, "the microphone is turned off for Terminal");
    assert!(rig.seen.lock().unwrap().calls.is_empty());
    assert_eq!(*rig.opened.lock().unwrap(), 1);
}

#[test]
fn a_microphone_that_delivers_nothing_is_a_failure_after_a_few_seconds() {
    let mut rig = rig(Ok(GERMAN_REPLY.to_string()), 16_000, Vec::new(), None);
    let answer = listen(&mut rig, "en", 30_000);
    assert_eq!(answer.outcome, Outcome::Failed);
    assert!(answer.message.contains("delivers no sound"), "{}", answer.message);
    let waited = rig.clock.now_ms();
    assert!((3_000..4_000).contains(&waited), "gave up after {waited} ms of the clock");
}

#[test]
fn a_microphone_that_breaks_is_a_failure_that_says_why() {
    let mut script = takes(16_000, 400, 0, 0);
    script.push(Take::Break("the device was unplugged".to_string()));
    let mut rig = rig(Ok(GERMAN_REPLY.to_string()), 16_000, script, None);
    let answer = listen(&mut rig, "en", 10_000);
    assert_eq!(answer.outcome, Outcome::Failed);
    assert!(answer.message.contains("the device was unplugged"), "{}", answer.message);
}

#[test]
fn cancelling_in_the_middle_of_speech_transcribes_what_was_said() {
    let mut rig = rig(Ok(GERMAN_REPLY.to_string()), 16_000, takes(16_000, 500, 3_000, 1_500), None);
    // The flag is set after 1.5 s of audio were read (15 reads of 100 ms).
    let reads = std::cell::Cell::new(0u32);
    let cancel = || {
        reads.set(reads.get() + 1);
        reads.get() > 15
    };
    let answer = rig.builtin.listen(&Listen {
        language: "de",
        timeout_ms: 10_000,
        clock: &*rig.clock,
        cancel: &cancel,
        flowing: &|| {},
    });
    assert_eq!(answer.outcome, Outcome::Heard, "what was said until the stop is not thrown away");
    let calls = rig.seen.lock().unwrap().calls.clone();
    assert_eq!(calls.len(), 1);
    let ms = calls[0].0 as i64 * 1000 / 16_000;
    assert!((900..=1_500).contains(&ms), "{ms} ms of audio");
}

#[test]
fn the_time_to_listen_is_audio_time_and_ends_the_call() {
    // 10 s of speech, but only 2 s are asked for: the utterance is cut there.
    let mut rig = rig(Ok(GERMAN_REPLY.to_string()), 16_000, takes(16_000, 200, 10_000, 0), None);
    let answer = listen(&mut rig, "de", 2_000);
    assert_eq!(answer.outcome, Outcome::Heard);
    let ms = rig.seen.lock().unwrap().calls[0].0 as i64 * 1000 / 16_000;
    assert!(ms <= 2_100, "{ms} ms of audio");
}

#[test]
fn a_recognizer_that_is_not_set_up_says_how_to_set_it_up() {
    let source = FakeSource {
        rate: 16_000,
        takes: None,
        refuse: None,
        opened: Arc::new(Mutex::new(0)),
    };
    let mut builtin = Builtin::not_set_up(Box::new(source));
    let readiness = builtin.readiness();
    let Readiness::Refused { outcome, message } = readiness else {
        panic!("not ready");
    };
    assert_eq!(outcome, Outcome::Unavailable);
    assert!(message.contains("set_builtin_files") && message.contains(MODEL_FILE_NAME), "{message}");
    let clock = ManualClock::new(ORIGIN);
    let never = || false;
    let answer = builtin.listen(&Listen {
        language: "en",
        timeout_ms: 1_000,
        clock: &clock,
        cancel: &never,
        flowing: &|| {},
    });
    assert_eq!(answer.outcome, Outcome::Unavailable);
    assert_eq!(answer.message, message);
    assert_eq!(builtin.kind(), Kind::Builtin);
    assert!(builtin.label().contains("Whistle"));
    assert_eq!(clock.now_ms(), 0, "refusing costs no listening time");
}

#[test]
fn the_noise_floor_of_one_call_starts_the_next() {
    let mut rig = rig(Ok(GERMAN_REPLY.to_string()), 16_000, takes(16_000, 2_000, 0, 0), None);
    let before = rig.builtin_floor();
    let _ = listen(&mut rig, "en", 2_000);
    assert!(rig.builtin_floor() != before, "the room was measured");
}

impl Rig {
    fn builtin_floor(&self) -> f32 {
        self.builtin.floor
    }
}

#[test]
fn the_vocabulary_is_handed_to_the_engine_and_kept_to_the_limits() {
    let mut rig = rig(Ok(GERMAN_REPLY.to_string()), 16_000, takes(16_000, 500, 1_000, 1_500), None);
    let note = rig.builtin.set_keywords(&[
        "AzReview".to_string(),
        "  Neckarbrücke ".to_string(),
        "".to_string(),
        "line\nbreak".to_string(),
    ]);
    assert_eq!(note, None);
    let _ = listen(&mut rig, "de", 10_000);
    assert_eq!(
        rig.seen.lock().unwrap().calls[0].2,
        "AzReview\naz review\nNeckarbrücke\nlinebreak",
        "an identifier goes with its spoken parts"
    );

    let many: Vec<String> = (0..500).map(|i| format!("word{i}")).collect();
    let note = rig.builtin.set_keywords(&many).expect("cut");
    assert!(note.contains("cut to 200 words"), "{note}");
    assert!(note.starts_with("the built-in recognizer"), "it says who cut it: {note}");
    assert_eq!(rig.builtin.vocabulary.joined().lines().count(), 200);
    let long = vec!["x".repeat(500)];
    assert!(rig.builtin.set_keywords(&long).is_some());
    assert_eq!(rig.builtin.vocabulary.joined().chars().count(), 64);
}

#[test]
fn a_model_that_is_too_large_or_that_the_engine_refuses_is_rejected_with_its_reason() {
    let (engine, _) = FakeEngine::new(Ok(String::new()));
    let source = FakeSource {
        rate: 16_000,
        takes: None,
        refuse: None,
        opened: Arc::new(Mutex::new(0)),
    };
    let tuning = Tuning {
        max_model_bytes: 100,
        ..Tuning::default()
    };
    let failure = Builtin::with_engine(engine, &[0u8; 101], Box::new(source), tuning).err().expect("too large");
    assert_eq!(failure.status, BuiltinStatus::ModelRejected);
    assert!(failure.message.contains("101 bytes, more than the 100 bytes"), "{}", failure.message);

    let (mut engine, _) = FakeEngine::new(Ok(String::new()));
    engine.load = Err("invalid .cact model".to_string());
    let source = FakeSource {
        rate: 16_000,
        takes: None,
        refuse: None,
        opened: Arc::new(Mutex::new(0)),
    };
    let failure = Builtin::with_engine(engine, b"junk", Box::new(source), Tuning::default()).err().expect("junk");
    assert_eq!(failure.status, BuiltinStatus::ModelRejected);
    assert!(failure.message.contains("invalid .cact model"), "{}", failure.message);
}

#[test]
fn the_files_are_looked_for_next_to_the_program_and_in_the_bundle() {
    let places = default_places(Path::new("/Applications/AzReview.app/Contents/MacOS"));
    assert_eq!(
        places,
        vec![
            PathBuf::from("/Applications/AzReview.app/Contents/MacOS"),
            PathBuf::from("/Applications/AzReview.app/Contents/MacOS/speech"),
            PathBuf::from("/Applications/AzReview.app/Contents/MacOS/../Resources"),
            PathBuf::from("/Applications/AzReview.app/Contents/MacOS/../Resources/speech"),
        ]
    );
    assert_eq!(engine_file_name("macos"), "libcactus.dylib");
    assert_eq!(engine_file_name("linux"), "libcactus.so");
    assert_eq!(engine_file_name("windows"), "libcactus.dll");
}

#[test]
fn registering_files_that_are_not_there_names_what_is_missing_and_where_it_looked() {
    let source = || {
        Box::new(FakeSource {
            rate: 16_000,
            takes: None,
            refuse: None,
            opened: Arc::new(Mutex::new(0)),
        })
    };
    // No engine library at the given path.
    let failure = Builtin::from_files("/nowhere/libcactus.so", "/nowhere/whistle.cact", None, source(), Tuning::default())
        .err()
        .expect("no engine");
    assert_eq!(failure.status, BuiltinStatus::EngineMissing);
    assert!(failure.message.contains("/nowhere/libcactus.so"), "{}", failure.message);
    // Nothing named, nothing next to the program, nothing linked in.
    let dir = std::env::temp_dir().join(format!("azul-speech-builtin-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    let failure = Builtin::from_files("", "", Some(&dir), source(), Tuning::default()).err().expect("no files");
    assert_eq!(failure.status, BuiltinStatus::EngineMissing);
    assert!(failure.message.contains(engine_file_name(std::env::consts::OS)), "{}", failure.message);
    assert!(failure.message.contains("speech"), "the places it looked in are named: {}", failure.message);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_model_codes() {
    assert_eq!(model_language("de-DE"), Some("de"));
    assert_eq!(model_language("EN_us"), Some("en"));
    assert_eq!(model_language(" pl "), Some("pl"));
    assert_eq!(model_language("nl-BE"), Some("nl"));
    assert_eq!(model_language("ja-JP"), None);
    assert_eq!(model_language(""), None);
    assert_eq!(model_language("-"), None);
}

#[test]
fn a_recognizer_that_looks_for_its_files_says_where_it_looked_and_opens_no_microphone() {
    let dir = std::env::temp_dir().join(format!("azul-speech-discover-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    let opened = Arc::new(Mutex::new(0));
    let source = FakeSource {
        rate: 16_000,
        takes: None,
        refuse: None,
        opened: Arc::clone(&opened),
    };
    let mut builtin = Builtin::discover(Some(dir.clone()), Box::new(source), Tuning::default());
    let Readiness::Refused { outcome, message } = builtin.readiness() else {
        panic!("there are no files in {dir:?}");
    };
    assert_eq!(outcome, Outcome::Unavailable);
    assert!(message.contains(&dir.display().to_string()), "{message}");
    assert!(message.contains("set_builtin_files"), "{message}");
    let clock = ManualClock::new(ORIGIN);
    let never = || false;
    let answer = builtin.listen(&Listen {
        language: "en",
        timeout_ms: 1_000,
        clock: &clock,
        cancel: &never,
        flowing: &|| {},
    });
    assert_eq!(answer.outcome, Outcome::Unavailable);
    assert!(answer.message.contains(&dir.display().to_string()), "{}", answer.message);
    assert_eq!(*opened.lock().unwrap(), 0, "the microphone stays off when there is no model");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Favouring a word pulls the probability of the RIGHT word down (a correct "enum" came out at
/// 0.32 with the vocabulary): the word says it was favoured, so nobody reads the number as doubt.
#[test]
fn a_word_of_the_vocabulary_is_marked_so_its_probability_is_not_read_as_doubt() {
    let reply = r#"{"text":"Rename the enum, AzReview and the speech session.","language":"en","words":[{"word":"Rename","start":0.0,"end":0.4,"probability":0.99},{"word":"the","start":0.4,"end":0.5,"probability":1.0},{"word":"enum,","start":0.5,"end":0.9,"probability":0.32},{"word":"AzReview","start":0.9,"end":1.5,"probability":0.55},{"word":"and","start":1.5,"end":1.6,"probability":1.0},{"word":"the","start":1.6,"end":1.7,"probability":1.0},{"word":"speech","start":1.7,"end":2.0,"probability":0.9},{"word":"session.","start":2.0,"end":2.4,"probability":0.7}]}"#;
    let mut rig = rig(Ok(reply.to_string()), 16_000, takes(16_000, 500, 1_000, 1_500), None);
    // Case and punctuation do not matter; a phrase marks each of its words.
    let _ = rig.builtin.set_keywords(&["ENUM".to_string(), "azreview".to_string(), "speech session".to_string()]);
    let answer = listen(&mut rig, "en", 10_000);
    let marked: Vec<(&str, bool)> = answer.words.iter().map(|w| (w.text.as_str(), w.biased)).collect();
    assert_eq!(
        marked,
        vec![
            ("Rename", false),
            ("the", false),
            ("enum,", true),
            ("AzReview", true),
            ("and", false),
            ("the", false),
            ("speech", true),
            ("session.", true)
        ]
    );
    // Without a vocabulary nothing is marked.
    let mut plain = rig_with_reply(reply);
    let answer = listen(&mut plain, "en", 10_000);
    assert!(answer.words.iter().all(|w| !w.biased));
}

fn rig_with_reply(reply: &str) -> Rig {
    rig(Ok(reply.to_string()), 16_000, takes(16_000, 500, 1_000, 1_500), None)
}

/// What a UI shows "speak now" from: the first sound of the microphone, not the start of the call
/// (the microphone takes a moment to start), once; and never when no microphone could be opened.
#[test]
fn the_flowing_signal_comes_once_with_the_first_sound_and_not_with_the_start_of_the_call() {
    // The microphone is open but delivers nothing for two reads.
    let mut script = vec![Take::Sound(Vec::new()), Take::Sound(Vec::new())];
    script.extend(takes(16_000, 500, 1_000, 1_500));
    let mut rig = rig(Ok(GERMAN_REPLY.to_string()), 16_000, script, None);
    let signalled = Arc::new(Mutex::new(Vec::new()));
    let flowing = {
        let (signalled, clock) = (Arc::clone(&signalled), rig.clock.clone());
        move || signalled.lock().unwrap().push(clock.now_ms())
    };
    let never = || false;
    let answer = rig.builtin.listen(&Listen {
        language: "de",
        timeout_ms: 10_000,
        clock: &*rig.clock,
        cancel: &never,
        flowing: &flowing,
    });
    assert_eq!(answer.outcome, Outcome::Heard, "{}", answer.message);
    let at = signalled.lock().unwrap().clone();
    assert_eq!(at.len(), 1, "once: {at:?}");
    assert!(at[0] >= 20, "after the two empty reads, not at the start: {at:?}");

    // No microphone: nothing flowed, nothing is signalled.
    let mut refused = rig_with_refusal();
    let count = Arc::new(Mutex::new(0u32));
    let counter = {
        let count = Arc::clone(&count);
        move || *count.lock().unwrap() += 1
    };
    let answer = refused.builtin.listen(&Listen {
        language: "de",
        timeout_ms: 10_000,
        clock: &*refused.clock,
        cancel: &never,
        flowing: &counter,
    });
    assert_eq!(answer.outcome, Outcome::Denied);
    assert_eq!(*count.lock().unwrap(), 0);
}

fn rig_with_refusal() -> Rig {
    rig(
        Ok(String::new()),
        16_000,
        Vec::new(),
        Some((Outcome::Denied, "the microphone is turned off".to_string())),
    )
}

/// A microphone whose sound the test pushes in from outside, a chunk per read, and that counts how
/// often it was opened and closed.
#[derive(Clone, Default)]
struct Room {
    sound: Arc<Mutex<std::collections::VecDeque<Vec<f32>>>>,
    opened: Arc<Mutex<u32>>,
    closed: Arc<Mutex<u32>>,
}

impl Room {
    /// `room` ms of room, `speech` ms of voice, `after` ms of room, as reads of 100 ms.
    fn push_takes(&self, room: u32, speech: u32, after: u32) {
        for take in takes(16_000, room, speech, after) {
            if let Take::Sound(chunk) = take {
                self.sound.lock().unwrap().push_back(chunk);
            }
        }
    }

    /// Waits until everything pushed was read, and a little more for the chunk in hand.
    fn wait_until_read(&self) {
        let began = std::time::Instant::now();
        while !self.sound.lock().unwrap().is_empty() {
            assert!(began.elapsed() < std::time::Duration::from_secs(5), "nobody read what was pushed");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        std::thread::sleep(std::time::Duration::from_millis(80));
    }
}

struct RoomSource(Room);

impl MicrophoneSource for RoomSource {
    fn open(&mut self) -> Result<Box<dyn Microphone>, (Outcome, String)> {
        *self.0.opened.lock().unwrap() += 1;
        Ok(Box::new(RoomMicrophone(self.0.clone())))
    }
}

struct RoomMicrophone(Room);

impl Microphone for RoomMicrophone {
    fn rate(&self) -> u32 {
        16_000
    }

    fn read(&mut self, out: &mut Vec<f32>, _wait_ms: u32) -> Result<(), String> {
        if let Some(chunk) = self.0.sound.lock().unwrap().pop_front() {
            out.extend(chunk);
        }
        Ok(())
    }
}

impl Drop for RoomMicrophone {
    fn drop(&mut self) {
        *self.0.closed.lock().unwrap() += 1;
    }
}

struct KeptRig {
    builtin: Builtin,
    keeper: MicKeeper,
    room: Room,
    seen: Arc<Mutex<Seen>>,
    clock: Arc<ManualClock>,
}

/// A recognizer whose microphone stays open for `keep_ms` after a call.
fn kept_rig(keep_ms: u32) -> KeptRig {
    let room = Room::default();
    let keeper = MicKeeper::new(Box::new(RoomSource(room.clone())));
    keeper.set_keep_ms(keep_ms);
    let (engine, seen) = FakeEngine::new(Ok(GERMAN_REPLY.to_string()));
    let builtin = Builtin::with_engine(engine, &[0u8; 1234], keeper.clone(), Tuning::default()).expect("a model");
    KeptRig {
        builtin,
        keeper,
        room,
        seen,
        clock: ManualClock::shared(ORIGIN),
    }
}

fn kept_listen(rig: &mut KeptRig, timeout_ms: u32) -> super::super::recognizer::Answer {
    let never = || false;
    rig.builtin.listen(&Listen {
        language: "de",
        timeout_ms,
        clock: &*rig.clock,
        cancel: &never,
        flowing: &|| {},
    })
}

#[test]
fn a_kept_microphone_serves_the_next_call_and_speech_that_began_in_the_gap_is_not_lost() {
    let mut rig = kept_rig(5_000);
    rig.room.push_takes(600, 1_200, 1_500);
    let first = kept_listen(&mut rig, 10_000);
    assert_eq!(first.outcome, Outcome::Heard, "{}", first.message);
    assert_eq!(*rig.room.opened.lock().unwrap(), 1);
    assert!(rig.keeper.is_held(), "the microphone stays open for the next call");
    assert_eq!(*rig.room.closed.lock().unwrap(), 0);

    // The speaker goes on while the first answer is being used: 600 ms of a second sentence come in
    // before the second call begins. The rest of it, and the quiet after, come with the call.
    rig.room.push_takes(0, 600, 0);
    rig.room.wait_until_read();
    rig.room.push_takes(0, 600, 1_500);
    let began = rig.clock.now_ms();
    let flowed = Arc::new(Mutex::new(None));
    let flowing = {
        let (flowed, clock) = (Arc::clone(&flowed), rig.clock.clone());
        move || *flowed.lock().unwrap() = Some(clock.now_ms())
    };
    let never = || false;
    let second = rig.builtin.listen(&Listen {
        language: "de",
        timeout_ms: 10_000,
        clock: &*rig.clock,
        cancel: &never,
        flowing: &flowing,
    });
    assert_eq!(second.outcome, Outcome::Heard, "{}", second.message);
    assert_eq!(*rig.room.opened.lock().unwrap(), 1, "no microphone was opened for the second call");
    assert_eq!(*flowed.lock().unwrap(), Some(began), "its sound was flowing before the call began");
    // The engine got the whole sentence: 300 ms before it, 1200 ms of voice, the tail. Without what
    // came in during the gap it would have had half of it.
    let seen = rig.seen.lock().unwrap().clone();
    assert_eq!(seen.calls.len(), 2);
    assert!(seen.calls[1].0 >= 16_000 * 3 / 2, "{} samples of the second call: the gap was lost", seen.calls[1].0);
}

#[test]
fn what_was_kept_for_the_next_call_does_not_use_up_its_time() {
    let mut rig = kept_rig(5_000);
    rig.room.push_takes(0, 600, 1_200);
    let _ = kept_listen(&mut rig, 10_000);
    // A second and a half of room comes in while nobody listens; then 400 ms of room and 600 ms of
    // voice with the call, which has two seconds: the voice is inside them, only if the time that
    // was kept does not count.
    rig.room.push_takes(1_500, 0, 0);
    rig.room.wait_until_read();
    rig.room.push_takes(400, 600, 1_500);
    let answer = kept_listen(&mut rig, 2_000);
    assert_eq!(answer.outcome, Outcome::Heard, "{}", answer.message);
    let seen = rig.seen.lock().unwrap().clone();
    assert!(seen.calls[1].0 >= 16_000 * 11 / 10, "{} samples: the voice was cut", seen.calls[1].0);
}

#[test]
fn a_cancelled_call_gives_the_microphone_up_instead_of_keeping_it() {
    let mut rig = kept_rig(5_000);
    rig.room.push_takes(600, 1_200, 1_500);
    let reads = std::cell::Cell::new(0);
    let cancel = || {
        reads.set(reads.get() + 1);
        reads.get() > 3
    };
    let answer = rig.builtin.listen(&Listen {
        language: "de",
        timeout_ms: 10_000,
        clock: &*rig.clock,
        cancel: &cancel,
        flowing: &|| {},
    });
    assert!(matches!(answer.outcome, Outcome::NothingHeard | Outcome::Heard), "{}", answer.message);
    assert!(!rig.keeper.is_held(), "the user cancelled: the microphone indicator goes out");
    assert_eq!(*rig.room.closed.lock().unwrap(), 1);
}

#[test]
fn a_microphone_that_is_not_kept_is_closed_before_the_engine_runs() {
    // The old behaviour with a keeper that keeps nothing: opened for the call, closed at its end.
    let mut rig = kept_rig(0);
    rig.room.push_takes(600, 1_200, 1_500);
    let _ = kept_listen(&mut rig, 10_000);
    assert_eq!((*rig.room.opened.lock().unwrap(), *rig.room.closed.lock().unwrap()), (1, 1));
    assert!(!rig.keeper.is_held());
}

/// The model cannot write `CssError` but writes "css error" right when it is given it: the engine
/// gets both, and a word of either is marked.
#[test]
fn an_identifier_goes_to_the_engine_with_its_spoken_parts_and_the_words_of_both_are_marked() {
    let reply = r#"{"text":"We had a CSS error in the enum.","language":"en","words":[{"word":"We","start":0.0,"end":0.2,"probability":0.99},{"word":"had","start":0.2,"end":0.4,"probability":0.99},{"word":"a","start":0.4,"end":0.5,"probability":0.99},{"word":"CSS","start":0.5,"end":0.9,"probability":0.5},{"word":"error","start":0.9,"end":1.2,"probability":0.8},{"word":"in","start":1.2,"end":1.3,"probability":0.99},{"word":"the","start":1.3,"end":1.4,"probability":0.99},{"word":"enum.","start":1.4,"end":1.8,"probability":0.4}]}"#;
    let mut rig = rig_with_reply(reply);
    let note = rig.builtin.set_keywords(&["CssError".to_string(), "enum".to_string()]);
    assert_eq!(note, None);
    let answer = listen(&mut rig, "en", 10_000);
    assert_eq!(rig.seen.lock().unwrap().calls[0].2, "CssError\ncss error\nenum");
    let marked: Vec<&str> = answer.words.iter().filter(|w| w.biased).map(|w| w.text.as_str()).collect();
    assert_eq!(marked, ["CSS", "error", "enum."]);
    // What is really sent counts against the limit, and the note says so.
    let identifiers: Vec<String> = (0..150).map(|i| format!("DataTable{i}Cell")).collect();
    let note = rig.builtin.set_keywords(&identifiers).expect("cut");
    assert!(note.contains("spoken parts"), "{note}");
    assert_eq!(rig.builtin.vocabulary.entries().len(), 200);
}

// ---- the stream ----

use super::super::recognizer::{Answer, EventKind, StreamOptions};

/// Every event of a stream over the microphone of `rig`, and how it ended. `stop` and `cancel` are
/// asked at every turn of the loop.
fn run_stream(
    rig: &mut KeptRig,
    options: &StreamOptions,
    timeout_ms: u32,
    stop: &dyn Fn() -> bool,
    cancel: &dyn Fn() -> bool,
) -> (Vec<(EventKind, Answer, usize)>, Answer) {
    let events = std::cell::RefCell::new(Vec::new());
    let ended = rig.builtin.listen_stream(
        &Listen {
            language: "de",
            timeout_ms,
            clock: &*rig.clock,
            cancel,
            flowing: &|| {},
        },
        options,
        stop,
        &|kind, answer| {
            // How much of the microphone's sound was still to be read when the event came: the
            // moment of the event in the audio.
            let unread = rig.room.sound.lock().unwrap().len();
            events.borrow_mut().push((kind, answer, unread));
        },
    );
    (events.into_inner(), ended)
}

fn stream_options(partials: bool, max_utterances: u32) -> StreamOptions {
    StreamOptions {
        partials,
        endpoint_ms: 0,
        max_ms: 0,
        max_utterances,
        pre_roll_ms: 2_000,
    }
}

fn kinds_of(events: &[(EventKind, Answer, usize)]) -> Vec<EventKind> {
    events.iter().map(|(kind, _, _)| *kind).collect()
}

#[test]
fn a_stream_gives_partials_while_an_utterance_goes_on_then_its_final_then_the_next_utterance() {
    let mut rig = kept_rig(0);
    // A sentence of 1.5 s, a pause, a second sentence of 1.5 s, the room.
    rig.room.push_takes(400, 1_500, 1_000);
    rig.room.push_takes(0, 1_500, 1_500);
    let never = || false;
    let (events, ended) = run_stream(&mut rig, &stream_options(true, 2), 30_000, &never, &never);
    assert_eq!(ended.outcome, Outcome::Heard, "{}", ended.message);
    let kinds = kinds_of(&events);
    let finals: Vec<usize> = kinds.iter().enumerate().filter(|(_, k)| **k == EventKind::Final).map(|(i, _)| i).collect();
    assert_eq!(finals.len(), 2, "{kinds:?}");
    let first_partials = kinds[..finals[0]].iter().filter(|k| **k == EventKind::Partial).count();
    let second_partials = kinds[finals[0]..finals[1]].iter().filter(|k| **k == EventKind::Partial).count();
    assert!(first_partials >= 2, "1.5 s of speech gives a partial about every 500 ms: {kinds:?}");
    assert!(second_partials >= 2, "{kinds:?}");
    // The engine was asked for each: the partials of an utterance grow, its final is the longest.
    let seen = rig.seen.lock().unwrap().clone();
    let lengths: Vec<usize> = seen.calls.iter().map(|(samples, _, _)| *samples).collect();
    assert_eq!(lengths.len(), first_partials + second_partials + 2, "{lengths:?}");
    assert!(lengths[..first_partials].windows(2).all(|w| w[0] < w[1]), "{lengths:?}");
    assert!(lengths[first_partials] >= lengths[first_partials - 1], "the final holds all of it");
    // A partial is the utterance so far: its words are the engine's, with the audio's moment.
    assert!(events[0].1.started_unix_ms >= ORIGIN, "{}", events[0].1.started_unix_ms);
    assert_eq!(events[finals[0]].1.language, "de");
}

#[test]
fn partials_are_not_decoded_when_not_asked_for() {
    let mut rig = kept_rig(0);
    rig.room.push_takes(400, 1_500, 1_500);
    let never = || false;
    let (events, _) = run_stream(&mut rig, &stream_options(false, 1), 30_000, &never, &never);
    assert_eq!(kinds_of(&events), [EventKind::Final]);
    assert_eq!(rig.seen.lock().unwrap().calls.len(), 1, "one decode for the one utterance");
}

#[test]
fn a_shorter_pause_ends_an_utterance_sooner() {
    let unread_at_final = |endpoint_ms: u32| {
        let mut rig = kept_rig(0);
        rig.room.push_takes(400, 1_000, 3_000);
        let options = StreamOptions {
            endpoint_ms,
            ..stream_options(false, 1)
        };
        let never = || false;
        let (events, _) = run_stream(&mut rig, &options, 30_000, &never, &never);
        events.iter().find(|(kind, _, _)| *kind == EventKind::Final).map(|(_, _, unread)| *unread).expect("a final")
    };
    // 100 ms of audio per chunk: 400 ms against 800 ms of end pause is 4 chunks that were not needed.
    let (short, long) = (unread_at_final(400), unread_at_final(800));
    assert!(short >= long + 3, "the final came with {short} chunks unread at 400 ms and {long} at 800");
    // The default is 600 ms.
    assert_eq!(unread_at_final(0), unread_at_final(600));
    assert!(unread_at_final(600) > long && unread_at_final(600) < short);
    // What is asked is kept to what an end-point can be: 5 ms is taken as 200 ms.
    assert_eq!(unread_at_final(5), unread_at_final(200));
    assert!(unread_at_final(200) >= short);
}

#[test]
fn a_stop_ends_the_utterance_that_goes_on_with_what_was_said_and_a_cancel_drops_it() {
    // 400 ms of room, then a sentence that has not ended when the stop comes after 14 reads.
    let reads = std::cell::Cell::new(0);
    let after = |n: u32| {
        let reads = &reads;
        move || {
            reads.set(reads.get() + 1);
            reads.get() > n
        }
    };
    let never = || false;
    let mut rig = kept_rig(0);
    rig.room.push_takes(400, 3_000, 0);
    let stop = after(14);
    let (events, ended) = run_stream(&mut rig, &stream_options(false, 0), 30_000, &stop, &never);
    assert_eq!(kinds_of(&events), [EventKind::Final], "the speech so far is the last utterance");
    assert_eq!(ended.outcome, Outcome::Heard);
    let said = rig.seen.lock().unwrap().calls[0].0;
    assert!(said < 16_000 * 3, "{said} samples: only what was said until the stop");

    reads.set(0);
    let mut rig = kept_rig(0);
    rig.room.push_takes(400, 3_000, 0);
    let cancel = after(14);
    let (events, ended) = run_stream(&mut rig, &stream_options(false, 0), 30_000, &never, &cancel);
    assert!(events.is_empty(), "cancelled: no final for the utterance that was cut: {:?}", kinds_of(&events));
    assert_eq!(ended.outcome, Outcome::NothingHeard);
    assert!(rig.seen.lock().unwrap().calls.is_empty(), "nothing was decoded");
}

#[test]
fn a_room_without_speech_is_reported_every_three_seconds_and_nothing_is_decoded() {
    let mut rig = kept_rig(0);
    rig.room.push_takes(7_500, 0, 0);
    let never = || false;
    let (events, ended) = run_stream(&mut rig, &stream_options(true, 0), 7_000, &never, &never);
    assert_eq!(kinds_of(&events), [EventKind::Quiet, EventKind::Quiet], "at 3 s and at 6 s");
    assert_eq!(ended.outcome, Outcome::NothingHeard);
    assert!(rig.seen.lock().unwrap().calls.is_empty());
}

#[test]
fn an_utterance_longer_than_the_model_takes_is_cut_into_utterances() {
    let mut rig = kept_rig(0);
    // Speech-like: 400 ms of voice and 100 ms of quiet over and over (a steady tone would be taken
    // for the room once the noise floor, the quietest of the last 3 s, rose to it).
    rig.room.push_takes(300, 0, 0);
    for _ in 0..54 {
        rig.room.push_takes(0, 400, 100);
    }
    rig.room.push_takes(0, 0, 1_500);
    let never = || false;
    let (events, _) = run_stream(&mut rig, &stream_options(false, 0), 40_000, &never, &never);
    let finals = events.iter().filter(|(kind, _, _)| *kind == EventKind::Final).count();
    assert_eq!(finals, 2, "{:?}", kinds_of(&events));
    let longest = rig.seen.lock().unwrap().calls.iter().map(|(samples, _, _)| *samples).max().unwrap();
    assert!(longest <= 16_000 * 27, "{longest} samples in one decode");
}

#[test]
fn what_the_engine_fails_at_while_a_partial_is_decoded_ends_the_stream_with_that() {
    let room = Room::default();
    let keeper = MicKeeper::new(Box::new(RoomSource(room.clone())));
    let (engine, _) = FakeEngine::new(Err("the model ran out of memory".to_string()));
    let mut builtin = Builtin::with_engine(engine, &[0u8; 1234], keeper, Tuning::default()).expect("a model");
    room.push_takes(400, 3_000, 1_500);
    let clock = ManualClock::shared(ORIGIN);
    let never = || false;
    let seen = std::cell::RefCell::new(Vec::new());
    let ended = builtin.listen_stream(
        &Listen {
            language: "en",
            timeout_ms: 30_000,
            clock: &*clock,
            cancel: &never,
            flowing: &|| {},
        },
        &stream_options(true, 0),
        &never,
        &|kind, _| seen.borrow_mut().push(kind),
    );
    assert_eq!(ended.outcome, Outcome::Failed);
    assert!(ended.message.contains("speech engine failed") && ended.message.contains("out of memory"), "{}", ended.message);
    assert!(seen.borrow().is_empty());
    assert_eq!(*room.closed.lock().unwrap(), 1, "the microphone of a failed stream is closed");
}

#[test]
fn what_an_armed_microphone_heard_before_the_stream_began_is_part_of_its_first_utterance() {
    let mut rig = kept_rig(5_000);
    assert!(rig.keeper.arm().is_ok());
    // 300 ms of room and 600 ms of a sentence came in while armed; the other 600 ms come with the
    // stream (with some room before: a tone with none would make the room's level the tone's).
    rig.room.push_takes(300, 600, 0);
    rig.room.wait_until_read();
    rig.room.push_takes(0, 600, 1_500);
    let flowed_at = std::cell::Cell::new(None);
    let never = || false;
    let events = std::cell::RefCell::new(Vec::new());
    let began = rig.clock.now_ms();
    let ended = rig.builtin.listen_stream(
        &Listen {
            language: "de",
            timeout_ms: 30_000,
            clock: &*rig.clock,
            cancel: &never,
            flowing: &|| flowed_at.set(Some(rig.clock.now_ms())),
        },
        &stream_options(false, 1),
        &never,
        &|kind, answer| events.borrow_mut().push((kind, answer)),
    );
    assert_eq!(ended.outcome, Outcome::Heard);
    assert_eq!(flowed_at.get(), Some(began), "the armed microphone was flowing already");
    assert_eq!(*rig.room.opened.lock().unwrap(), 1);
    let said = rig.seen.lock().unwrap().calls[0].0;
    assert!(said >= 16_000 * 3 / 2, "{said} samples: the part said while it was armed is in it");
}

/// An armed microphone hears the room: a stream is handed the last `pre_roll_ms` of what it heard
/// (a second by default), not all of its 2 s.
#[test]
fn a_stream_is_handed_only_the_pre_roll_it_asked_for() {
    let decoded_samples = |pre_roll_ms: u32| {
        let mut rig = kept_rig(5_000);
        assert!(rig.keeper.arm().is_ok());
        // 1.3 s before the stream: 300 ms of room, then a second of speech-like sound (400 ms of
        // voice and 100 ms of quiet, twice: a steady tone would be taken for the room); the sentence
        // goes on into the stream for another second, then the room.
        rig.room.push_takes(300, 0, 0);
        rig.room.push_takes(0, 400, 100);
        rig.room.push_takes(0, 400, 100);
        rig.room.wait_until_read();
        rig.room.push_takes(0, 400, 100);
        rig.room.push_takes(0, 400, 100);
        rig.room.push_takes(0, 0, 1_500);
        let never = || false;
        let options = StreamOptions {
            pre_roll_ms,
            ..stream_options(false, 1)
        };
        let (events, _) = run_stream(&mut rig, &options, 30_000, &never, &never);
        assert!(events.iter().any(|(kind, _, _)| *kind == EventKind::Final));
        let calls = rig.seen.lock().unwrap().calls.clone();
        calls[0].0
    };
    let (all, one_second, none) = (decoded_samples(2_000), decoded_samples(1_000), decoded_samples(0));
    assert!(all > one_second && one_second > none, "{all} samples with all of it, {one_second} with a second, {none} with none");
    // With none the first utterance is what came after the stream began: 600 ms of voice and the tails.
    assert!(none < 16_000 * 3 / 2, "{none}");
    // With a second the voice that was said just before the stream is in it.
    assert!(one_second > none + 16_000 / 2, "{one_second} against {none}");
}

/// The microphone goes away in the middle of a sentence: what was said comes as a last final, and the
/// stream ends with the failure (an app that dictates keeps the words).
#[test]
fn a_microphone_that_fails_mid_stream_gives_what_was_said_and_then_the_failure() {
    let mut script = takes(16_000, 300, 1_000, 0);
    script.push(Take::Break("the device went away".to_string()));
    let mut rig = rig(Ok(GERMAN_REPLY.to_string()), 16_000, script, None);
    let events = std::cell::RefCell::new(Vec::new());
    let never = || false;
    let ended = rig.builtin.listen_stream(
        &Listen {
            language: "de",
            timeout_ms: 30_000,
            clock: &*rig.clock,
            cancel: &never,
            flowing: &|| {},
        },
        &stream_options(false, 0),
        &never,
        &|kind, answer| events.borrow_mut().push((kind, answer.text)),
    );
    assert_eq!(ended.outcome, Outcome::Failed);
    assert!(ended.message.contains("device went away"), "{}", ended.message);
    let events = events.into_inner();
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].0, EventKind::Final);
    assert!(events[0].1.starts_with("Bitte verschiebe"), "{}", events[0].1);
    assert_eq!(*rig.opened.lock().unwrap(), 1);
}
