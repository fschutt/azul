//! The engine seam: what the built-in recognizer needs from a speech model runtime, and the one
//! implementation, the Cactus engine (`cactus.h`, 13 functions, the four of them the recognizer
//! calls), reached in one of two ways: loaded at run time from a shared library with `libloading`
//! ([`CactusEngine`]: nothing links it, a program that never registers the library never has it, and
//! a missing or wrong library is a status, not a crash), or linked into the program as the vendor's
//! static library (`LinkedEngine`, the cargo feature `speech-engine-static`, off in every shipped
//! build).
//!
//! The engine owns one process-global model: two recognizers must not use it at the same time (an
//! app has one dictation). Calls on one recognizer are serialised by the session.

// The items are `pub` for the tests and for the parts of the module that are re-exported.
#![allow(unreachable_pub)]

/// What the built-in recognizer asks of an engine.
pub trait Engine: Send {
    /// Loads the model. The engine copies it: the bytes may go once this returns.
    ///
    /// # Errors
    ///
    /// The engine's own message when the bytes are not a model it takes.
    fn load(&mut self, model: &[u8]) -> Result<(), String>;

    /// How many threads the engine may use (0: its own choice, up to four).
    fn set_threads(&mut self, threads: u32);

    /// Transcribes 16 kHz mono samples. `language` is a two-letter code or empty to detect it,
    /// `keywords` the words to favour, one a line, or empty. Answers the engine's JSON.
    ///
    /// # Errors
    ///
    /// The engine's own message.
    fn transcribe(&mut self, pcm: &[f32], language: &str, keywords: &str) -> Result<String, String>;
}

/// Why an engine library could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// The library could not be loaded (nothing there, not a library for this system, a
    /// dependency missing): where it was looked for and what the system said.
    NotLoaded { path: String, why: String },
    /// The library loaded but is not a compatible Cactus engine: `symbol` is missing.
    Incompatible { symbol: &'static str },
}

impl EngineError {
    /// A sentence for a status line.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::NotLoaded { path, why } => format!("the speech engine library {path} could not be loaded: {why}"),
            Self::Incompatible { symbol } => {
                format!("the speech engine library is not a Cactus engine this program can use (no {symbol})")
            }
        }
    }
}

/// The four functions of the engine's C ABI the recognizer calls, wherever they come from (a library
/// opened at run time, or the static library the program links), and what the recognizer does with
/// them: the one place that knows how the arguments are laid out and how an answer is read.
#[cfg(any(
    target_os = "linux",
    target_os = "windows",
    target_os = "android",
    target_os = "macos",
    feature = "speech-engine-static"
))]
mod abi {
    use std::ffi::{c_char, c_int, CStr, CString};

    pub(super) type LoadFn = unsafe extern "C" fn(*const u8, u64) -> c_int;
    pub(super) type LastErrorFn = unsafe extern "C" fn() -> *const c_char;
    pub(super) type ThreadsFn = unsafe extern "C" fn(c_int);
    pub(super) type TranscribeFn =
        unsafe extern "C" fn(*const f32, c_int, *const c_char, *const c_char, c_int, *mut c_char, c_int) -> c_int;

    /// The least room the engine's answer gets, bytes: the vendor's own sizing is 4096 bytes for each
    /// second of audio, 256 KiB at the least.
    pub(super) const MIN_ANSWER_BYTES: usize = 256 * 1024;
    pub(super) const ANSWER_BYTES_PER_SECOND: usize = 4096;

    #[derive(Clone, Copy)]
    pub(super) struct Abi {
        pub(super) load: LoadFn,
        pub(super) last_error: LastErrorFn,
        pub(super) set_threads: ThreadsFn,
        pub(super) transcribe: TranscribeFn,
    }

    impl Abi {
        pub(super) fn last_error(&self) -> String {
            // SAFETY: the engine answers a NUL-terminated string it owns, or null.
            let pointer = unsafe { (self.last_error)() };
            if pointer.is_null() {
                return "the engine gave no reason".to_string();
            }
            let text = unsafe { CStr::from_ptr(pointer) }.to_string_lossy().into_owned();
            if text.is_empty() {
                "the engine gave no reason".to_string()
            } else {
                text
            }
        }

        pub(super) fn load(&self, model: &[u8]) -> Result<(), String> {
            // SAFETY: the engine reads `len` bytes and copies them.
            let code = unsafe { (self.load)(model.as_ptr(), model.len() as u64) };
            if code < 0 {
                Err(self.last_error())
            } else {
                Ok(())
            }
        }

        pub(super) fn set_threads(&self, threads: u32) {
            let threads = c_int::try_from(threads).unwrap_or(c_int::MAX);
            // SAFETY: a plain integer.
            unsafe { (self.set_threads)(threads) };
        }

        pub(super) fn transcribe(&self, pcm: &[f32], language: &str, keywords: &str) -> Result<String, String> {
            let count = c_int::try_from(pcm.len()).map_err(|_| "the audio is too long for the engine".to_string())?;
            let language = CString::new(language.replace('\0', "")).map_err(|e| e.to_string())?;
            let keywords = CString::new(keywords.replace('\0', "")).map_err(|e| e.to_string())?;
            let seconds = pcm.len() / 16_000 + 1;
            let capacity = (seconds * ANSWER_BYTES_PER_SECOND).max(MIN_ANSWER_BYTES);
            let mut out = vec![0u8; capacity];
            // SAFETY: the samples, the two strings and the buffer outlive the call; the engine writes
            // at most `capacity` bytes, with a NUL.
            let code = unsafe {
                (self.transcribe)(
                    pcm.as_ptr(),
                    count,
                    if language.as_bytes().is_empty() { std::ptr::null() } else { language.as_ptr() },
                    if keywords.as_bytes().is_empty() { std::ptr::null() } else { keywords.as_ptr() },
                    1,
                    out.as_mut_ptr().cast::<c_char>(),
                    c_int::try_from(capacity).unwrap_or(c_int::MAX),
                )
            };
            if code < 0 {
                return Err(self.last_error());
            }
            let end = out.iter().position(|b| *b == 0).unwrap_or(out.len());
            String::from_utf8(out[..end].to_vec()).map_err(|_| "the engine's answer is not UTF-8".to_string())
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "android", target_os = "macos"))]
mod cactus {
    use std::path::Path;

    use super::{
        abi::{Abi, LastErrorFn, LoadFn, ThreadsFn, TranscribeFn},
        Engine, EngineError,
    };

    /// The Cactus engine behind its C ABI, in a library opened at run time.
    pub struct CactusEngine {
        /// Keeps the library mapped while the function pointers below are used.
        _library: libloading::Library,
        abi: Abi,
    }

    impl std::fmt::Debug for CactusEngine {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("CactusEngine").finish_non_exhaustive()
        }
    }

    /// A function of the engine's ABI.
    fn symbol<T: Copy>(library: &libloading::Library, name: &'static str) -> Result<T, EngineError> {
        // SAFETY: `T` is the function type the engine's header declares for `name`.
        unsafe { library.get::<T>(name.as_bytes()) }
            .map(|found| *found)
            .map_err(|_| EngineError::Incompatible { symbol: name })
    }

    impl CactusEngine {
        /// Loads the engine library at `path`.
        ///
        /// # Errors
        ///
        /// [`EngineError`] when there is no such library, or it is not a compatible engine.
        pub fn open(path: &Path) -> Result<Self, EngineError> {
            // SAFETY: loading a library runs its initialisers; the caller names the file.
            let library = unsafe { libloading::Library::new(path) }.map_err(|e| EngineError::NotLoaded {
                path: path.display().to_string(),
                why: e.to_string(),
            })?;
            Self::from_library(library)
        }

        /// The engine whose functions are already in this process (the app linked `libcactus.a`).
        ///
        /// # Errors
        ///
        /// [`EngineError`] when the process has no such functions.
        pub fn this() -> Result<Self, EngineError> {
            #[cfg(unix)]
            let library: libloading::Library = libloading::os::unix::Library::this().into();
            #[cfg(windows)]
            let library: libloading::Library = libloading::os::windows::Library::this()
                .map_err(|e| EngineError::NotLoaded {
                    path: "this program".to_string(),
                    why: e.to_string(),
                })?
                .into();
            Self::from_library(library)
        }

        fn from_library(library: libloading::Library) -> Result<Self, EngineError> {
            let load = symbol::<LoadFn>(&library, "cactus_load")?;
            let last_error = symbol::<LastErrorFn>(&library, "cactus_last_error")?;
            let set_threads = symbol::<ThreadsFn>(&library, "cactus_set_threads")?;
            let transcribe = symbol::<TranscribeFn>(&library, "cactus_transcribe")?;
            Ok(Self {
                _library: library,
                abi: Abi {
                    load,
                    last_error,
                    set_threads,
                    transcribe,
                },
            })
        }
    }

    impl Engine for CactusEngine {
        fn load(&mut self, model: &[u8]) -> Result<(), String> {
            self.abi.load(model)
        }

        fn set_threads(&mut self, threads: u32) {
            self.abi.set_threads(threads);
        }

        fn transcribe(&mut self, pcm: &[f32], language: &str, keywords: &str) -> Result<String, String> {
            self.abi.transcribe(pcm, language, keywords)
        }
    }
}

/// The platforms that cannot load a library at run time (iOS, the BSDs): the engine is
/// not there.
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "android", target_os = "macos")))]
mod cactus {
    use std::path::Path;

    use super::{Engine, EngineError};

    #[derive(Debug, Clone, Copy)]
    pub struct CactusEngine;

    impl CactusEngine {
        pub fn open(path: &Path) -> Result<Self, EngineError> {
            Err(EngineError::NotLoaded {
                path: path.display().to_string(),
                why: "this platform cannot load a speech engine library at run time".to_string(),
            })
        }

        pub fn this() -> Result<Self, EngineError> {
            Err(EngineError::NotLoaded {
                path: "this program".to_string(),
                why: "this platform has no speech engine".to_string(),
            })
        }
    }

    impl Engine for CactusEngine {
        fn load(&mut self, _model: &[u8]) -> Result<(), String> {
            Err("no speech engine".to_string())
        }

        fn set_threads(&mut self, _threads: u32) {}

        fn transcribe(&mut self, _pcm: &[f32], _language: &str, _keywords: &str) -> Result<String, String> {
            Err("no speech engine".to_string())
        }
    }
}

/// The engine linked into the program: the vendor's static library (`libcactus.a`, linked by the
/// build script of this crate when the cargo feature `speech-engine-static` is on, which no shipped
/// build turns on). Nothing is looked up at run time, so it works where no library can be opened (iOS).
#[cfg(feature = "speech-engine-static")]
mod linked {
    use std::ffi::{c_char, c_int};

    use super::{abi::Abi, Engine};

    extern "C" {
        fn cactus_load(cact: *const u8, n: u64) -> c_int;
        fn cactus_last_error() -> *const c_char;
        fn cactus_set_threads(threads: c_int);
        fn cactus_transcribe(
            pcm: *const f32,
            samples: c_int,
            language: *const c_char,
            keywords: *const c_char,
            word_timestamps: c_int,
            out: *mut c_char,
            out_capacity: c_int,
        ) -> c_int;
    }

    /// The Cactus engine of the static library. Like the library's, it has one process-global model.
    pub struct LinkedEngine {
        abi: Abi,
    }

    impl LinkedEngine {
        /// The engine the program links.
        #[must_use]
        pub fn new() -> Self {
            Self {
                abi: Abi {
                    load: cactus_load,
                    last_error: cactus_last_error,
                    set_threads: cactus_set_threads,
                    transcribe: cactus_transcribe,
                },
            }
        }
    }

    impl Default for LinkedEngine {
        fn default() -> Self {
            Self::new()
        }
    }

    impl std::fmt::Debug for LinkedEngine {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("LinkedEngine").finish_non_exhaustive()
        }
    }

    impl Engine for LinkedEngine {
        fn load(&mut self, model: &[u8]) -> Result<(), String> {
            self.abi.load(model)
        }

        fn set_threads(&mut self, threads: u32) {
            self.abi.set_threads(threads);
        }

        fn transcribe(&mut self, pcm: &[f32], language: &str, keywords: &str) -> Result<String, String> {
            self.abi.transcribe(pcm, language, keywords)
        }
    }
}

pub use cactus::CactusEngine;
#[cfg(feature = "speech-engine-static")]
pub use linked::LinkedEngine;

#[cfg(test)]
#[path = "engine_test.rs"]
mod engine_test;
