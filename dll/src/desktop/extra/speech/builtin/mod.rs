//! The built-in recognizer: a small speech model the app provides, run in this process, for when
//! the system's recognizer refuses (a bare binary the system will not let ask) or does not exist
//! (Linux). Nothing of it is in the default build: the engine is a shared library and the model a
//! file the app registers at its start (`SpeechSession::set_builtin_files`); the files are
//! `libcactus.{dylib,so,dll}` (the runtime wheel of Cactus 4.1.4) and `whistle.cact`, fetched and
//! checked by `scripts/fetch_whistle.sh`, and looked for next to the executable, in a `speech`
//! folder there and in the bundle's `Resources` when the app names none.
//!
//! # The choice (measured on an Apple M2, 8 GB, one engine thread, 2026-11-25)
//!
//! | model | size | languages | licence | latency | streaming | links as |
//! |-------|------|-----------|---------|---------|-----------|----------|
//! | **Whistle** (Cactus 4.1.4), the default | 16.9 MB weights + 1.5 to 2.0 MB engine library | en de fr es it nl pl, detected | Apache-2.0 (weights and engine binaries) | 4 s clip 90 to 260 ms, 17.5 s 0.46 s, 21.6 s 0.74 s (real-time factor 0.013 to 0.035), load 6 ms, 53 to 80 MB RSS; vendor, M4 Pro: first token 11 ms at 10 s | windowed at 30 s, word times of 80 ms; its streaming call re-decodes (12 times dearer) | 13-function C ABI, `dlopen` |
//! | Moonshine streaming tiny / small (en, de) | 34 M / 123 M parameters (vendor: tiny v2 41.9 MB) | en, de, es, ar, ja, zh, vi, ... | MIT (older non-English models: non-commercial community licence) | MacBook Pro 18 / 38 ms per response (vendor) | streaming | C++ core on ONNX Runtime, no Rust crate |
//! | whisper.cpp tiny / base | f16 77.7 / 148 MB, q5_1 32.2 / 59.7 MB, 273 / 388 MB RAM | 99 | MIT | tiny 277 ms, small 1940 ms (vendor table, MacBook Pro) | 30 s windows | `whisper-rs` 0.16 builds the C++ with CMake |
//! | Vosk small | en-us 40 MB, de 45 MB, ~300 MB RAM | one model per language | Apache-2.0 | real time, streaming | streaming (Kaldi) | `libvosk`, crate `vosk` 0.3.1 (2024) |
//! | Parakeet TDT 0.6B v3 | 2.4 GB F32, 671 MB int8 ONNX | 25 European | CC-BY-4.0 | GPU class | offline | NeMo, or sherpa-onnx |
//! | sherpa-onnx | a toolkit (Apache-2.0), models by task | per model | per model | per model | per model | crate `sherpa-onnx` 2.0.3, downloads libraries at build time |
//!
//! Word error rates, each in the vendor's own measure and so not comparable: Whistle 4.73 % on
//! LibriSpeech (depth 8, no keywords); Moonshine tiny 12.00 % and small de 7.5 % (average of the
//! open leaderboard's eight sets / 400 German clips); Whisper tiny 12.81 % (same average); Vosk
//! small en 9.85 % (LibriSpeech clean), small de 13.75 % (Tuda); Parakeet v3 1.93 % (LibriSpeech
//! clean), German FLEURS 5.04 %. Measured on this Mac (24 clips of real read speech, no number
//! normalisation, so a spoken "zwei" against a written "2" counts as an error: indicative, not a
//! benchmark): 12 German Multilingual LibriSpeech test clips (185.6 s, 309 words) Whistle 12.6 %,
//! the system's recognizer 7.8 %; 12 English LibriSpeech clips (47.7 s, 120 words) Whistle 5.8 %,
//! the system's 10.0 %. Whistle needed 0.022 of the audio's length on one thread (a 15 s clip
//! 345 ms, 333 ms of CPU); the system's recognizer 0.9 to 1.5 s a clip with its set-up.
//!
//! The alternative is Moonshine streaming (German small: 7.5 %, MIT, streaming) when its C++ core
//! gets a C ABI that does not carry ONNX Runtime; whisper.cpp base (q5_1, 59.7 MB, MIT) is the
//! conservative third. Whistle's engine is binary only: the manifest names source commit
//! a224d36 of a repository that is not public, the LICENSE (11,358 bytes, Apache-2.0 text, the
//! copyright line unfilled) and the model card say Apache-2.0 for engine, binaries and weights.

mod audio;
mod engine;

use std::path::{Path, PathBuf};

use self::audio::{Endpointer, Progress, Resampler};
pub use self::{
    audio::{Vad, MODEL_RATE},
    engine::{CactusEngine, Engine, EngineError},
};
use super::{
    keeper::{MicKeeper, PREROLL_MS},
    recognizer::{Answer, Kind, Listen, Outcome, Readiness, Recognizer},
    vocabulary::Vocabulary,
};

/// What the app registered did, as a status the app can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinStatus {
    /// The engine and the model are loaded: the recognizer listens.
    Ready,
    /// There is no engine library where it was looked for.
    EngineMissing,
    /// The library is there but is not a Cactus engine this program can use.
    EngineIncompatible,
    /// There is no model file where it was looked for.
    ModelMissing,
    /// The engine does not take the model (damaged, not a `.cact`, or too large).
    ModelRejected,
}

/// Why a registration did not make a recognizer that is ready: the status and a sentence for the
/// user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub status: BuiltinStatus,
    pub message: String,
}

/// What listens and what limits it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tuning {
    /// Engine threads (0: the engine's choice, up to four). One keeps a window's rendering from
    /// being starved by a transcription.
    pub threads: u32,
    /// The largest model file taken, bytes.
    pub max_model_bytes: u64,
    /// The most keywords, and the longest one, characters.
    pub max_keywords: usize,
    pub max_keyword_chars: usize,
    /// How speech is told from the room.
    pub vad: Vad,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            threads: 1,
            max_model_bytes: 128 * 1024 * 1024,
            max_keywords: 200,
            max_keyword_chars: 64,
            vad: Vad::default(),
        }
    }
}

/// What delivers sound: opened when a call needs a microphone (the session's [`MicKeeper`] may keep
/// it open for the next call: the microphone indicator is on only while listening and for the time
/// the app allows after it).
pub trait MicrophoneSource: Send {
    /// Opens the microphone, or says why not: `Denied` when the user (or an administrator) said
    /// no, `Unavailable` when there is none or it cannot be asked for.
    ///
    /// # Errors
    ///
    /// The outcome and the sentence for the user.
    fn open(&mut self) -> Result<Box<dyn Microphone>, (Outcome, String)>;
}

/// An open microphone.
pub trait Microphone: Send {
    /// The rate the samples come at, Hz.
    fn rate(&self) -> u32;

    /// Appends what was captured since the last call, mono; waits up to `wait_ms` for some. An
    /// empty `out` after it is a stretch without sound.
    ///
    /// # Errors
    ///
    /// The device's message when it failed.
    fn read(&mut self, out: &mut Vec<f32>, wait_ms: u32) -> Result<(), String>;
}

impl From<Box<dyn MicrophoneSource>> for MicKeeper {
    /// A keeper that keeps nothing: the microphone is opened for each call and closed at its end.
    fn from(source: Box<dyn MicrophoneSource>) -> Self {
        Self::new(source)
    }
}

impl<S: MicrophoneSource + 'static> From<Box<S>> for MicKeeper {
    fn from(source: Box<S>) -> Self {
        Self::new(source)
    }
}

/// How long one read waits for sound, ms.
const READ_WAIT_MS: u32 = 100;
/// How long a microphone may deliver nothing before it counts as broken, ms.
const STALL_MS: u64 = 3_000;

/// The languages the model knows.
const MODEL_LANGUAGES: [&str; 7] = ["en", "de", "fr", "es", "it", "nl", "pl"];

/// The model's code of a BCP 47 tag (`de-DE` is `de`), `None` for a language it does not know or
/// for the system's (empty): the engine then detects it.
#[must_use]
pub fn model_language(tag: &str) -> Option<&'static str> {
    let primary = tag.trim().split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
    MODEL_LANGUAGES.iter().copied().find(|code| *code == primary)
}

/// The file name of the engine library on `os` (`macos`, `linux`, `windows`, ...).
#[must_use]
pub fn engine_file_name(os: &str) -> &'static str {
    match os {
        "macos" | "ios" => "libcactus.dylib",
        "windows" => "libcactus.dll",
        _ => "libcactus.so",
    }
}

/// The model's file name.
pub const MODEL_FILE_NAME: &str = "whistle.cact";

/// Where the two files are looked for when the app names none, folders in order: next to the
/// executable, in a `speech` folder there, and in a bundle's `Resources` (`Contents/MacOS/..`).
#[must_use]
pub fn default_places(exe_dir: &Path) -> Vec<PathBuf> {
    vec![
        exe_dir.to_path_buf(),
        exe_dir.join("speech"),
        exe_dir.join("../Resources"),
        exe_dir.join("../Resources/speech"),
    ]
}

/// The first of `places` that holds `name`.
fn find_in(places: &[PathBuf], name: &str) -> Option<PathBuf> {
    places.iter().map(|folder| folder.join(name)).find(|path| path.is_file())
}

/// The built-in recognizer.
pub struct Builtin {
    state: State,
    /// The microphone, opened for a call and kept for the next one for as long as the session says.
    keeper: MicKeeper,
    tuning: Tuning,
    /// What the model is asked to favour, kept to the limits; its words are marked in an answer.
    vocabulary: Vocabulary,
    /// The quietest the room was in the last call: the next call starts from it.
    floor: f32,
}

enum State {
    /// Registered and loaded.
    Ready(Box<dyn Engine>),
    /// Not usable, and why (a sentence for the user).
    Refused(String),
    /// Nothing was registered: the files are looked for next to the executable (in `exe_dir`) the
    /// first time they are needed, so a program that never falls back never reads them.
    Discover(Option<PathBuf>),
}

impl Builtin {
    /// A recognizer that has nothing registered: it refuses with how to set it up.
    #[must_use]
    pub fn not_set_up(microphone: impl Into<MicKeeper>) -> Self {
        Self::refused(
            microphone,
            format!(
                "the built-in recognizer has no model: register {MODEL_FILE_NAME} and the speech engine \
                 library ({}) with set_builtin_files at the start (scripts/fetch_whistle.sh fetches them)",
                engine_file_name(std::env::consts::OS)
            ),
        )
    }

    /// A recognizer that looks for the files next to the executable (`exe_dir`) when it is first
    /// needed and refuses, saying where it looked, when they are not there.
    #[must_use]
    pub fn discover(exe_dir: Option<PathBuf>, microphone: impl Into<MicKeeper>, tuning: Tuning) -> Self {
        Self {
            state: State::Discover(exe_dir),
            keeper: microphone.into(),
            tuning,
            vocabulary: Vocabulary::default(),
            floor: 0.0004,
        }
    }

    /// A recognizer that cannot listen, for `message`.
    #[must_use]
    pub fn refused(microphone: impl Into<MicKeeper>, message: String) -> Self {
        Self {
            state: State::Refused(message),
            keeper: microphone.into(),
            tuning: Tuning::default(),
            vocabulary: Vocabulary::default(),
            floor: 0.0004,
        }
    }

    /// A recognizer on `engine` with `model` loaded.
    ///
    /// # Errors
    ///
    /// [`Failure`] with `ModelRejected` when the model is too large or the engine does not take it.
    pub fn with_engine(
        mut engine: Box<dyn Engine>,
        model: &[u8],
        microphone: impl Into<MicKeeper>,
        tuning: Tuning,
    ) -> Result<Self, Failure> {
        if model.len() as u64 > tuning.max_model_bytes {
            return Err(Failure {
                status: BuiltinStatus::ModelRejected,
                message: format!(
                    "the model is {} bytes, more than the {} bytes taken",
                    model.len(),
                    tuning.max_model_bytes
                ),
            });
        }
        engine.set_threads(tuning.threads);
        engine.load(model).map_err(|why| Failure {
            status: BuiltinStatus::ModelRejected,
            message: format!("the speech engine does not take the model: {why}"),
        })?;
        Ok(Self {
            state: State::Ready(engine),
            keeper: microphone.into(),
            tuning,
            vocabulary: Vocabulary::default(),
            floor: 0.0004,
        })
    }

    /// The engine library at `engine_path` (empty: looked for in the default places of `exe_dir`,
    /// and in the process itself when the app linked it).
    fn open_engine(engine_path: &str, exe_dir: Option<&Path>) -> Result<Box<dyn Engine>, Failure> {
        if !engine_path.trim().is_empty() {
            return Ok(Box::new(
                CactusEngine::open(Path::new(engine_path.trim())).map_err(engine_failure)?,
            ));
        }
        let places = exe_dir.map(default_places).unwrap_or_default();
        let name = engine_file_name(std::env::consts::OS);
        match find_in(&places, name) {
            Some(path) => Ok(Box::new(CactusEngine::open(&path).map_err(engine_failure)?)),
            None => match CactusEngine::this() {
                Ok(engine) => Ok(Box::new(engine)),
                Err(_) => Err(Failure {
                    status: BuiltinStatus::EngineMissing,
                    message: format!(
                        "no speech engine library: {name} was not found in {} and the program has not linked it",
                        describe_places(&places)
                    ),
                }),
            },
        }
    }

    /// The model file at `model_path` (empty: looked for in the default places of `exe_dir`),
    /// read, when it is not larger than the limit.
    fn read_model(model_path: &str, exe_dir: Option<&Path>, tuning: &Tuning) -> Result<Vec<u8>, Failure> {
        let places = exe_dir.map(default_places).unwrap_or_default();
        let file = if model_path.trim().is_empty() {
            find_in(&places, MODEL_FILE_NAME).ok_or_else(|| Failure {
                status: BuiltinStatus::ModelMissing,
                message: format!("no speech model: {MODEL_FILE_NAME} was not found in {}", describe_places(&places)),
            })?
        } else {
            PathBuf::from(model_path.trim())
        };
        let size = std::fs::metadata(&file).map_err(|e| Failure {
            status: BuiltinStatus::ModelMissing,
            message: format!("the speech model {} cannot be read: {e}", file.display()),
        })?;
        if size.len() > tuning.max_model_bytes {
            return Err(Failure {
                status: BuiltinStatus::ModelRejected,
                message: format!(
                    "the speech model {} is {} bytes, more than the {} bytes taken",
                    file.display(),
                    size.len(),
                    tuning.max_model_bytes
                ),
            });
        }
        std::fs::read(&file).map_err(|e| Failure {
            status: BuiltinStatus::ModelMissing,
            message: format!("the speech model {} cannot be read: {e}", file.display()),
        })
    }

    /// A recognizer on the engine library and the model file at the given paths (an empty path
    /// is looked for in the default places of `exe_dir`; the engine library may also be in the
    /// process already, when the app linked it).
    ///
    /// # Errors
    ///
    /// [`Failure`] naming what is missing or wrong and where it was looked for.
    pub fn from_files(
        engine_path: &str,
        model_path: &str,
        exe_dir: Option<&Path>,
        microphone: impl Into<MicKeeper>,
        tuning: Tuning,
    ) -> Result<Self, Failure> {
        let engine = Self::open_engine(engine_path, exe_dir)?;
        let bytes = Self::read_model(model_path, exe_dir, &tuning)?;
        Self::with_engine(engine, &bytes, microphone, tuning)
    }

    /// The same with the model's bytes in memory.
    ///
    /// # Errors
    ///
    /// [`Failure`] naming what is missing or wrong.
    pub fn from_engine_and_bytes(
        engine_path: &str,
        model: &[u8],
        exe_dir: Option<&Path>,
        microphone: impl Into<MicKeeper>,
        tuning: Tuning,
    ) -> Result<Self, Failure> {
        let engine = Self::open_engine(engine_path, exe_dir)?;
        Self::with_engine(engine, model, microphone, tuning)
    }

    /// Loads what [`Self::discover`] looked for, once: the state is `Ready` or `Refused` after it.
    fn load_discovered(&mut self) {
        let State::Discover(exe_dir) = &self.state else {
            return;
        };
        let loaded = Self::open_engine("", exe_dir.as_deref()).and_then(|mut engine| {
            let bytes = Self::read_model("", exe_dir.as_deref(), &self.tuning)?;
            if bytes.len() as u64 > self.tuning.max_model_bytes {
                return Err(Failure {
                    status: BuiltinStatus::ModelRejected,
                    message: "the speech model is larger than the limit".to_string(),
                });
            }
            engine.set_threads(self.tuning.threads);
            engine.load(&bytes).map_err(|why| Failure {
                status: BuiltinStatus::ModelRejected,
                message: format!("the speech engine does not take the model: {why}"),
            })?;
            Ok(engine)
        });
        self.state = match loaded {
            Ok(engine) => State::Ready(engine),
            Err(failure) => State::Refused(failure.message),
        };
    }

    /// The words the model should favour (names, jargon): at most the limit, each kept to its
    /// length. Answers the word about what was cut. The words of an answer that are among them
    /// are marked `biased`.
    pub fn set_keywords(&mut self, words: &[String]) -> Option<String> {
        let (vocabulary, note) = Vocabulary::from_words(words, self.tuning.max_keywords, self.tuning.max_keyword_chars);
        self.vocabulary = vocabulary;
        note.map(|note| format!("{}: {note}", Kind::Builtin.describe()))
    }
}

fn engine_failure(error: EngineError) -> Failure {
    Failure {
        status: match error {
            EngineError::NotLoaded { .. } => BuiltinStatus::EngineMissing,
            EngineError::Incompatible { .. } => BuiltinStatus::EngineIncompatible,
        },
        message: error.describe(),
    }
}

fn describe_places(places: &[PathBuf]) -> String {
    if places.is_empty() {
        "no folder (the executable's folder is unknown)".to_string()
    } else {
        places.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
    }
}

impl Recognizer for Builtin {
    fn kind(&self) -> Kind {
        Kind::Builtin
    }

    fn label(&self) -> String {
        "Whistle (built-in)".to_string()
    }

    fn readiness(&mut self) -> Readiness {
        match &self.state {
            State::Ready(_) => Readiness::Ready,
            State::Refused(message) => Readiness::Refused {
                outcome: Outcome::Unavailable,
                message: message.clone(),
            },
            // Without loading anything: are the files there?
            State::Discover(exe_dir) => {
                let places = exe_dir.as_deref().map(default_places).unwrap_or_default();
                let engine = find_in(&places, engine_file_name(std::env::consts::OS)).is_some() || CactusEngine::this().is_ok();
                let model = find_in(&places, MODEL_FILE_NAME).is_some();
                if engine && model {
                    Readiness::Ready
                } else {
                    Readiness::Refused {
                        outcome: Outcome::Unavailable,
                        message: format!(
                            "the built-in recognizer is not set up: {} was not found in {} (register the files with \
                             set_builtin_files; scripts/fetch_whistle.sh fetches them)",
                            if !engine { engine_file_name(std::env::consts::OS) } else { MODEL_FILE_NAME },
                            describe_places(&places)
                        ),
                    }
                }
            }
        }
    }

    fn set_vocabulary(&mut self, words: &[String]) -> Option<String> {
        self.set_keywords(words)
    }

    fn listen(&mut self, call: &Listen<'_>) -> Answer {
        self.load_discovered();
        if let State::Refused(message) = &self.state {
            return Answer::none(Outcome::Unavailable, message);
        }
        let began_unix_ms = call.clock.unix_ms();
        let mut lease = match self.keeper.acquire() {
            Ok(lease) => lease,
            Err((outcome, message)) => return Answer::none(outcome, &message),
        };
        let rate = lease.rate();
        let mut resampler = Resampler::new(rate);
        let mut endpointer = Endpointer::with_floor(self.tuning.vad.clone(), self.floor);
        // What came in while nobody listened (a kept microphone) is heard first, and does not use up
        // the time the caller asked for.
        let preroll = std::mem::take(&mut lease.preroll);
        let preroll_ms = preroll.len() as u64 * 1000 / u64::from(rate.max(1));
        let timeout_ms = u64::from(call.timeout_ms) + preroll_ms;
        let mut raw = Vec::new();
        let mut last_sound_at = call.clock.now_ms();
        let mut flowed = false;
        if lease.warm {
            // The microphone was open before the call: its sound is flowing already.
            flowed = true;
            (call.flowing)();
        }
        let mut progress = if preroll.is_empty() {
            Progress::Waiting
        } else {
            endpointer.push(&resampler.process(&preroll))
        };
        while progress != Progress::Done && endpointer.elapsed_ms() < timeout_ms && !(call.cancel)() {
            raw.clear();
            if let Err(why) = lease.microphone().read(&mut raw, READ_WAIT_MS) {
                return Answer::none(Outcome::Failed, &format!("the microphone failed: {why}"));
            }
            if raw.is_empty() {
                call.clock.sleep_ms(10);
                if call.clock.now_ms().saturating_sub(last_sound_at) > STALL_MS {
                    return Answer::none(Outcome::Failed, "the microphone delivers no sound");
                }
                continue;
            }
            last_sound_at = call.clock.now_ms();
            if !flowed {
                // The first sound of this call: a UI may say "speak now".
                flowed = true;
                (call.flowing)();
            }
            progress = endpointer.push(&resampler.process(&raw));
        }
        // The microphone is given back before the (heavier) transcription starts: kept for the next
        // call, unless this one was cancelled (the user is done: the indicator goes out).
        lease.finish(!(call.cancel)());
        self.floor = endpointer.noise_floor();
        let segment = if progress == Progress::Done {
            endpointer.take()
        } else {
            endpointer.finish()
        };
        let Some(segment) = segment else {
            return Answer::none(Outcome::NothingHeard, "nothing was said");
        };

        let (code, note) = match (model_language(call.language), call.language.trim().is_empty()) {
            (Some(code), _) => (code, None),
            (None, true) => ("", None),
            (None, false) => (
                "",
                Some(format!(
                    "the built-in model does not know {}: the language was detected",
                    call.language.trim()
                )),
            ),
        };
        let State::Ready(engine) = &mut self.state else {
            return Answer::none(Outcome::Unavailable, "the built-in recognizer is not set up");
        };
        let reply = match engine.transcribe(&segment.samples, code, &self.vocabulary.joined()) {
            Ok(reply) => reply,
            Err(why) => return Answer::none(Outcome::Failed, &format!("the speech engine failed: {why}")),
        };
        let transcript = match super::json::transcript(&reply) {
            Ok(transcript) => transcript,
            Err(why) => {
                return Answer::none(Outcome::Failed, &format!("the speech engine's answer could not be read: {why}"))
            }
        };
        if transcript.text.is_empty() {
            return Answer::none(Outcome::NothingHeard, "nothing was said");
        }
        let mut words = transcript.words;
        self.vocabulary.mark(&mut words);
        // The audio began `preroll_ms` before the call did (what a kept microphone heard in between).
        let audio_began_unix_ms = began_unix_ms.saturating_sub(preroll_ms);
        let mut answer = Answer::heard_words(words, &transcript.language, audio_began_unix_ms + segment.start_ms);
        answer.text = transcript.text;
        if let Some(note) = note {
            answer.message = note;
        }
        answer
    }
}

#[cfg(test)]
#[path = "builtin_test.rs"]
mod builtin_test;
