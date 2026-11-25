//! Unified `Speech` namespace (text said aloud, speech heard) and `SpeechSession` (the chain of
//! recognizers: the platform's, then the built-in model). See [`crate::unified`].
//!
//! Off-wasm this is the real one of `desktop::extra::speech`. On wasm there is no speech
//! backend yet: the stubs below have the identical `#[repr(C)]` layout and answer every call
//! `Unavailable`.

#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
use azul_css::{AzString, StringVec};

#[cfg(az_desktop)]
pub use crate::desktop::extra::speech::{
    BuiltinSpeechStatus, HeardStatus, OptionSpeechWord, Speech, SpeechHeard, SpeechLimits, SpeechRecognizer,
    SpeechSession, SpeechStatus, SpeechWord, SpeechWordVec, SpeechWordVecDestructor, SpeechWordVecDestructorType,
    SpeechWordVecSlice,
};

/// wasm stub of the desktop `SpeechStatus`; `#[repr(C)]` layout MUST match.
#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum SpeechStatus {
    Queued,
    Unavailable,
}

/// wasm stub of the desktop `HeardStatus`; `#[repr(C)]` layout MUST match.
#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HeardStatus {
    Heard,
    NothingHeard,
    Denied,
    Unavailable,
    Failed,
}

/// wasm stub of the desktop `SpeechRecognizer`; `#[repr(C)]` layout MUST match.
#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum SpeechRecognizer {
    Native,
    Builtin,
    Scripted,
    None,
}

/// wasm stub of the desktop `BuiltinSpeechStatus`; `#[repr(C)]` layout MUST match.
#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum BuiltinSpeechStatus {
    Ready,
    EngineMissing,
    EngineIncompatible,
    ModelMissing,
    ModelRejected,
}

/// wasm stub of the desktop `SpeechLimits`; `#[repr(C)]` layout MUST match.
#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct SpeechLimits {
    pub min_listen_ms: u32,
    pub max_listen_ms: u32,
    pub max_keywords: u32,
    pub max_model_mb: u32,
}

#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
impl Default for SpeechLimits {
    fn default() -> Self {
        Self {
            min_listen_ms: 500,
            max_listen_ms: 60_000,
            max_keywords: 200,
            max_model_mb: 128,
        }
    }
}

/// wasm stub of the desktop `SpeechWord`; `#[repr(C)]` layout MUST match.
#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct SpeechWord {
    pub text: AzString,
    pub start_ms: u32,
    pub end_ms: u32,
    pub probability: f32,
}

#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
azul_css::impl_option!(SpeechWord, OptionSpeechWord, copy = false, [Debug, Clone, PartialEq]);
#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
azul_css::impl_vec!(
    SpeechWord,
    SpeechWordVec,
    SpeechWordVecDestructor,
    SpeechWordVecDestructorType,
    SpeechWordVecSlice,
    OptionSpeechWord
);
#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
azul_css::impl_vec_debug!(SpeechWord, SpeechWordVec);
#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
azul_css::impl_vec_clone!(SpeechWord, SpeechWordVec, SpeechWordVecDestructor);
#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
azul_css::impl_vec_partialeq!(SpeechWord, SpeechWordVec);

/// wasm stub of the desktop `SpeechHeard`; `#[repr(C)]` layout MUST match: the desktop
/// type's fields in the desktop type's order (the C ABI transmutes between this and
/// `AzSpeechHeard`, which api.json lays out so).
#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct SpeechHeard {
    pub text: AzString,
    pub message: AzString,
    pub language: AzString,
    pub words: SpeechWordVec,
    pub started_unix_ms: u64,
    pub confidence: f32,
    pub status: HeardStatus,
    pub recognizer: SpeechRecognizer,
}

#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
impl SpeechHeard {
    fn unavailable(message: &str) -> Self {
        Self {
            text: AzString::from(String::new()),
            message: AzString::from(message.to_string()),
            language: AzString::from(String::new()),
            words: SpeechWordVec::from_vec(Vec::new()),
            started_unix_ms: 0,
            confidence: 0.0,
            status: HeardStatus::Unavailable,
            recognizer: SpeechRecognizer::None,
        }
    }
}

/// wasm stub of the desktop `Speech` namespace; `#[repr(C)]` layout MUST match.
#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
pub struct Speech {
    pub _reserved: u8,
}

#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
impl Speech {
    pub const fn new() -> Self {
        Self { _reserved: 0 }
    }
    /// No speech on wasm.
    pub fn speak(_text: AzString, _language: AzString) -> SpeechStatus {
        SpeechStatus::Unavailable
    }
    /// No speech on wasm.
    pub fn speak_now(_text: AzString, _language: AzString) -> SpeechStatus {
        SpeechStatus::Unavailable
    }
    /// No speech on wasm.
    pub fn stop() {}
    /// No speech on wasm.
    pub fn is_speaking() -> bool {
        false
    }
    /// No speech on wasm.
    pub fn is_available() -> bool {
        false
    }
    /// No speech on wasm.
    pub fn listen_blocking(_language: AzString, _timeout_ms: u32) -> SpeechHeard {
        SpeechHeard::unavailable("no speech recognition on the web yet")
    }
}

/// wasm stub of the desktop `SpeechSession`; `#[repr(C)]` layout MUST match.
#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
#[repr(C)]
pub struct SpeechSession {
    pub ptr: *mut core::ffi::c_void,
    pub run_destructor: bool,
}

#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
impl Default for SpeechSession {
    fn default() -> Self {
        Self {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
impl Clone for SpeechSession {
    fn clone(&self) -> Self {
        Self::default()
    }
}

#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
impl SpeechSession {
    /// A session that listens to nothing: there is no speech on wasm.
    pub fn new() -> Self {
        Self::default()
    }
    /// No built-in recognizer on wasm.
    pub fn set_builtin_files(&self, _engine_path: AzString, _model_path: AzString) -> BuiltinSpeechStatus {
        BuiltinSpeechStatus::EngineMissing
    }
    /// No built-in recognizer on wasm.
    pub fn set_builtin_bytes(&self, _engine_path: AzString, _model: azul_core::gl::U8VecRef) -> BuiltinSpeechStatus {
        BuiltinSpeechStatus::EngineMissing
    }
    /// No built-in recognizer on wasm.
    pub fn set_vocabulary(&self, _words: StringVec) -> AzString {
        AzString::from(String::new())
    }
    /// Nothing to limit on wasm.
    pub fn set_limits(&self, _limits: SpeechLimits) {}
    /// No speech on wasm.
    pub fn listen_blocking(&self, _language: AzString, _timeout_ms: u32) -> SpeechHeard {
        SpeechHeard::unavailable("no speech recognition on the web yet")
    }
    /// Nothing listens on wasm.
    pub fn cancel(&self) {}
    /// No recognizer on wasm.
    pub fn recognizers(&self) -> StringVec {
        StringVec::from_vec(Vec::new())
    }
    /// Nothing to forget on wasm.
    pub fn reset(&self) {}
}
