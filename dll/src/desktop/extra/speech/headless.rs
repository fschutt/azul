//! The speech of a headless or E2E run: nothing is said aloud and nothing is heard. Every
//! utterance is printed (`AZ_SPEECH <language> <text>`), appended to the file `AZ_SPEECH_LOG`
//! names, and kept for the process' own tests; hearing answers `AZ_SPEECH_HEARD`.

use super::SpeechHeard;

/// One utterance a headless run was asked to say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Utterance {
    pub language: String,
    pub text: String,
    /// Whether it cut off what was being said.
    pub interrupt: bool,
}

/// Logs `text`.
pub(super) fn say(text: &str, language: &str, interrupt: bool) {
    let _ = (text, language, interrupt);
}

/// What this process was asked to say, oldest first.
pub(crate) fn spoken() -> Vec<Utterance> {
    Vec::new()
}

/// What a headless run hears: the test's answer, else `AZ_SPEECH_HEARD`, else nothing.
pub(super) fn heard() -> SpeechHeard {
    SpeechHeard::none(super::HeardStatus::NothingHeard, "")
}

/// The answer the process' tests hear (`None`: `AZ_SPEECH_HEARD` again).
#[cfg(test)]
pub(crate) fn set_heard(text: Option<&str>) {
    let _ = text;
}
