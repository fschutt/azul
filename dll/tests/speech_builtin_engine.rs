//! The built-in recognizer on the REAL engine and the REAL model (Whistle in Cactus 4.1.4), listening
//! to a recording instead of a microphone.
//!
//! `cargo test -p azul-dll --test speech_builtin_engine -- --ignored`, after
//! `scripts/fetch_whistle.sh target/speech-engine` has put `libcactus.*` and `whistle.cact` there and
//! two recordings, `en.wav` and `de.wav` (mono, 16-bit, any rate: the recognizer resamples), next to
//! them. Ignored by default: the files are 18 MB the default test run does not fetch. What it proves
//! that the unit tests cannot: the library loads through `dlopen`, the ABI's calls take the arguments
//! the header says, the model is accepted, the engine's JSON reads back, and a spoken sentence comes
//! out as words with times.
//!
//! The same test runs on the engine LINKED into the program: `scripts/fetch_whistle.sh --static
//! target/speech-engine` (the archive goes to `target/speech-engine/static`), then `cargo test -p azul-dll
//! --features speech-engine-static --test speech_builtin_engine -- --ignored` (on Linux with the
//! packages `libc++-dev` and `libc++abi-dev`, and `AZUL_CACTUS_CXX_DIR` when libc++ is not on the
//! linker's path). Nothing is opened at run time then, only the model is read.

use std::{collections::VecDeque, path::PathBuf};

use azul::desktop::extra::speech::{
    builtin::{Builtin, Microphone, MicrophoneSource, Tuning},
    recognizer::{EventKind, Listen, Outcome, Recognizer, StreamOptions, SystemClock},
};

fn folder() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../target/speech-engine"))
}

fn engine_file() -> PathBuf {
    let name = if cfg!(target_os = "macos") {
        "libcactus.dylib"
    } else if cfg!(windows) {
        "libcactus.dll"
    } else {
        "libcactus.so"
    };
    folder().join(name)
}

/// The engine the test names: the shared library fetched into `target/speech-engine`, or, in a build with
/// the feature `speech-engine-static`, none: an empty path is the engine the program linked.
fn engine_path() -> String {
    if cfg!(feature = "speech-engine-static") {
        String::new()
    } else {
        engine_file().to_str().unwrap().to_string()
    }
}

/// A mono 16-bit PCM WAV as floats and its rate.
fn read_wav(path: &std::path::Path) -> (Vec<f32>, u32) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert!(&bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE", "{} is not a WAV file", path.display());
    let (mut at, mut rate, mut channels, mut bits) = (12, 0u32, 0u16, 0u16);
    while at + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let body = &bytes[at + 8..(at + 8 + size).min(bytes.len())];
        if &bytes[at..at + 4] == b"fmt " {
            channels = u16::from_le_bytes(body[2..4].try_into().unwrap());
            rate = u32::from_le_bytes(body[4..8].try_into().unwrap());
            bits = u16::from_le_bytes(body[14..16].try_into().unwrap());
        } else if &bytes[at..at + 4] == b"data" {
            assert!(channels == 1 && bits == 16, "{}: mono 16-bit please ({channels} ch, {bits} bit)", path.display());
            let samples = body.chunks_exact(2).map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])) / 32768.0).collect();
            return (samples, rate);
        }
        at += 8 + size + (size & 1);
    }
    panic!("{} has no data", path.display());
}

/// A microphone that plays a recording after a stretch of quiet, in reads of 100 ms.
struct Recording {
    rate: u32,
    chunks: VecDeque<Vec<f32>>,
}

impl Microphone for Recording {
    fn rate(&self) -> u32 {
        self.rate
    }

    fn read(&mut self, out: &mut Vec<f32>, _wait_ms: u32) -> Result<(), String> {
        if let Some(chunk) = self.chunks.pop_front() {
            out.extend(chunk);
            // Real time would make this test as long as the recording; the engine does not care.
        }
        Ok(())
    }
}

struct RecordingSource {
    wav: PathBuf,
}

impl MicrophoneSource for RecordingSource {
    fn open(&mut self) -> Result<Box<dyn Microphone>, (Outcome, String)> {
        let (voice, rate) = read_wav(&self.wav);
        let quiet = |ms: u32| vec![0.0003f32; (u64::from(rate) * u64::from(ms) / 1000) as usize];
        let mut signal = quiet(700);
        signal.extend(voice);
        signal.extend(quiet(1_800));
        let chunk = (rate / 10) as usize;
        Ok(Box::new(Recording {
            rate,
            chunks: signal.chunks(chunk).map(<[f32]>::to_vec).collect(),
        }))
    }
}

fn transcribe(wav: &str, language: &str) -> azul::desktop::extra::speech::recognizer::Answer {
    let mut builtin = Builtin::from_files(
        &engine_path(),
        folder().join("whistle.cact").to_str().unwrap(),
        None,
        Box::new(RecordingSource { wav: folder().join(wav) }),
        Tuning::default(),
    )
    .unwrap_or_else(|failure| panic!("{:?}: {}", failure.status, failure.message));
    let clock = SystemClock::new();
    let never = || false;
    builtin.listen(&Listen {
        language,
        timeout_ms: 30_000,
        clock: &clock,
        cancel: &never,
        flowing: &|| {},
    })
}

/// The same recording as a stream: what the events were, in order, with the text each carried.
fn stream(wav: &str, language: &str) -> Vec<(EventKind, String)> {
    let mut builtin = Builtin::from_files(
        &engine_path(),
        folder().join("whistle.cact").to_str().unwrap(),
        None,
        Box::new(RecordingSource { wav: folder().join(wav) }),
        Tuning::default(),
    )
    .unwrap_or_else(|failure| panic!("{:?}: {}", failure.status, failure.message));
    let clock = SystemClock::new();
    let never = || false;
    let events = std::cell::RefCell::new(Vec::new());
    let ended = builtin.listen_stream(
        &Listen {
            language,
            timeout_ms: 30_000,
            clock: &clock,
            cancel: &never,
            flowing: &|| {},
        },
        &StreamOptions {
            partials: true,
            endpoint_ms: 0,
            max_ms: 0,
            max_utterances: 1,
            pre_roll_ms: 1_000,
        },
        &never,
        &|kind, answer| events.borrow_mut().push((kind, answer.text)),
    );
    assert_eq!(ended.outcome, Outcome::Heard, "{}", ended.message);
    events.into_inner()
}

/// One test for both sentences: the engine owns ONE process-global model and is not thread-safe
/// (two recognizers calling it from two test threads at once crashed it with a SIGSEGV on the
/// runner), so what uses it runs one after the other.
#[test]
#[ignore = "X-EDGE-C-2: a hand-run measurement; it needs scripts/fetch_whistle.sh target/speech-engine and two recordings, en.wav and de.wav, there"]
fn the_real_engine_turns_a_recorded_english_and_german_sentence_into_timed_words() {
    let answer = transcribe("en.wav", "en-US");
    println!("english: {answer:#?}");
    assert_eq!(answer.outcome, Outcome::Heard, "{}", answer.message);
    assert_eq!(answer.language, "en");
    let text = answer.text.to_lowercase();
    assert!(text.contains("label") || text.contains("river"), "the sentence came out as {text:?}");
    assert!(answer.words.len() >= 5, "{:?}", answer.words);
    // The words come in order, each inside the audio, with probabilities.
    let mut last = 0;
    for word in &answer.words {
        assert!(word.start_ms >= last && word.end_ms >= word.start_ms, "{word:?}");
        assert!((0.0..=1.0).contains(&word.probability), "{word:?}");
        last = word.start_ms;
    }
    assert!(answer.started_unix_ms > 1_700_000_000_000, "dated by the wall clock");

    // German, with no language asked for: the engine detects it.
    let answer = transcribe("de.wav", "");
    println!("german: {answer:#?}");
    assert_eq!(answer.outcome, Outcome::Heard, "{}", answer.message);
    assert_eq!(answer.language, "de", "the language is detected when none is asked for");
    assert!(!answer.words.is_empty());

    // The English sentence as a stream: partials while it goes on, then its final.
    let events = stream("en.wav", "en-US");
    println!("stream: {events:#?}");
    let partials = events.iter().filter(|(kind, _)| *kind == EventKind::Partial).count();
    assert!(partials >= 2, "a sentence of several seconds gives partials: {events:?}");
    assert!(events.iter().filter(|(_, text)| !text.is_empty()).count() >= 3);
    let last = events.last().expect("events");
    assert_eq!(last.0, EventKind::Final, "{events:?}");
    let text = last.1.to_lowercase();
    assert!(text.contains("label") || text.contains("river"), "the final came out as {text:?}");
}
