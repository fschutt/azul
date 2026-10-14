//! macOS and iOS's speech engines (see the module's table in `mod.rs`).

use super::{HeardStatus, SpeechHeard, SpeechStatus};

/// Hands `text` to the engine.
pub fn say(text: &str, language: &str, interrupt: bool) -> SpeechStatus {
    let _ = (text, language, interrupt);
    SpeechStatus::Unavailable
}

/// Stops the engine and drops what waits.
pub fn stop() {}

/// Whether the engine is talking.
pub fn is_speaking() -> bool {
    false
}

/// Whether there is an engine.
pub fn is_available() -> bool {
    false
}

/// Listens for up to `timeout_ms`.
pub fn listen(language: &str, timeout_ms: u32) -> SpeechHeard {
    let _ = (language, timeout_ms);
    SpeechHeard::none(HeardStatus::Unavailable, "not built")
}
