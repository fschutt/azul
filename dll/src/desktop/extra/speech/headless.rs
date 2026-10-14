//! The speech of a headless or E2E run: nothing is said aloud and nothing is heard. Every
//! utterance is printed (`AZ_SPEECH <language> <text>`), appended to the file `AZ_SPEECH_LOG`
//! names, and kept for the process' own tests; hearing answers `AZ_SPEECH_HEARD`.

use std::{
    io::Write,
    sync::{Mutex, PoisonError},
};

use super::{HeardStatus, SpeechHeard, SPEECH_HEARD_VAR, SPEECH_LOG_VAR};

/// What this process was asked to say.
static SPOKEN: Mutex<Vec<Utterance>> = Mutex::new(Vec::new());

/// What the process' tests hear instead of `AZ_SPEECH_HEARD`.
#[cfg(test)]
static HEARD: Mutex<Option<String>> = Mutex::new(None);

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
    let line = format!(
        "AZ_SPEECH {} {}",
        if language.is_empty() { "-" } else { language },
        text.replace(['\n', '\r'], " ")
    );
    println!("{line}");
    if let Some(path) = std::env::var_os(SPEECH_LOG_VAR).filter(|p| !p.is_empty()) {
        // A log that cannot be written only loses the record: the run goes on.
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{line}");
        }
    }
    SPOKEN
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Utterance {
            language: language.to_string(),
            text: text.to_string(),
            interrupt,
        });
}

/// What this process was asked to say, oldest first.
pub(crate) fn spoken() -> Vec<Utterance> {
    SPOKEN.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

/// What a headless run hears: the test's answer, else `AZ_SPEECH_HEARD`, else nothing.
pub(super) fn heard() -> SpeechHeard {
    #[cfg(test)]
    let scripted = HEARD.lock().unwrap_or_else(PoisonError::into_inner).clone();
    #[cfg(not(test))]
    let scripted: Option<String> = None;
    let text = scripted.or_else(|| std::env::var(SPEECH_HEARD_VAR).ok()).unwrap_or_default();
    if text.trim().is_empty() {
        SpeechHeard::none(HeardStatus::NothingHeard, "nothing was said")
    } else {
        SpeechHeard::heard(text.trim(), 1.0)
    }
}

/// The answer the process' tests hear (`None`: `AZ_SPEECH_HEARD` again).
#[cfg(test)]
pub(crate) fn set_heard(text: Option<&str>) {
    *HEARD.lock().unwrap_or_else(PoisonError::into_inner) = text.map(str::to_string);
}
