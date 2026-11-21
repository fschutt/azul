//! Unified `Speech` namespace (text said aloud, speech heard). See [`crate::unified`].
//!
//! Off-wasm this is the real one of `desktop::extra::speech`. On wasm there is no speech
//! backend yet: the stubs below have the identical `#[repr(C)]` layout and answer every call
//! `Unavailable`.

#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
use azul_css::AzString;

#[cfg(az_desktop)]
pub use crate::desktop::extra::speech::{HeardStatus, Speech, SpeechHeard, SpeechStatus};

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

/// wasm stub of the desktop `SpeechHeard`; `#[repr(C)]` layout MUST match: the desktop
/// type's fields in the desktop type's order (the C ABI transmutes between this and
/// `AzSpeechHeard`, which api.json lays out so).
#[cfg(any(target_arch = "wasm32", az_engine_stubs))]
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct SpeechHeard {
    pub text: AzString,
    pub message: AzString,
    pub status: HeardStatus,
    pub confidence: f32,
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
        SpeechHeard {
            status: HeardStatus::Unavailable,
            text: AzString::from(String::new()),
            confidence: 0.0,
            message: AzString::from("no speech recognition on the web yet".to_string()),
        }
    }
}
