//! Speech on the platform's own engines: saying text aloud ([`Speech::speak`]) and hearing what
//! the user says ([`Speech::listen_blocking`]).
//!
//! | Platform | Saying | Hearing |
//! |----------|--------|---------|
//! | macOS / iOS | `AVSpeechSynthesizer` (AVFoundation, dlopen'd) | `SFSpeechRecognizer` on the microphone through `AVAudioEngine` (Speech, dlopen'd); the app's Info.plist must carry `NSSpeechRecognitionUsageDescription` and `NSMicrophoneUsageDescription`, else hearing answers `Unavailable` instead of the process being stopped by the OS |
//! | Windows | SAPI 5 `ISpVoice` | WinRT `Windows.Media.SpeechRecognition.SpeechRecognizer` |
//! | Linux | speech-dispatcher (`libspeechd.so.2`, dlopen'd) | not yet: `Unavailable` |
//! | Android | `TextToSpeech` through the Java helper `AzulSpeech` | `SpeechRecognizer` through the same helper, on the main thread; the app declares `RECORD_AUDIO` (`AZ_ANDROID_PERMISSIONS`) and asks for it through the permission manager, else hearing answers `Denied` |
//!
//! Saying never blocks: the text is handed to the engine, which speaks it after what it is
//! saying ([`Speech::speak`]) or instead of it ([`Speech::speak_now`], for a navigation prompt
//! that must not wait). Hearing blocks until the user stopped talking or the time is up, so it
//! is for a worker thread.
//!
//! A headless or E2E run (`AZ_BACKEND=headless`, `AZ_E2E_TEST`) neither speaks nor listens:
//! every utterance is printed as `AZ_SPEECH <language> <text>` (and appended to the file
//! `AZ_SPEECH_LOG` names), and hearing answers what `AZ_SPEECH_HEARD` holds, or nothing.

use azul_css::AzString;

#[cfg(target_os = "android")]
pub mod android;
#[cfg(any(target_os = "ios", target_os = "macos"))]
pub mod apple;
mod headless;
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(test)]
mod tests;

#[cfg(target_os = "android")]
use self::android as platform;
#[cfg(any(target_os = "ios", target_os = "macos"))]
use self::apple as platform;
#[cfg(target_os = "linux")]
use self::linux as platform;
#[cfg(target_os = "windows")]
use self::windows as platform;

/// The platforms without a speech backend yet (the BSDs): nothing to say with.
#[cfg(not(any(
    target_os = "android",
    target_os = "ios",
    target_os = "macos",
    target_os = "linux",
    target_os = "windows"
)))]
mod platform {
    use super::{HeardStatus, SpeechHeard, SpeechStatus};

    pub fn say(_text: &str, _language: &str, _interrupt: bool) -> SpeechStatus {
        SpeechStatus::Unavailable
    }

    pub fn stop() {}

    pub fn is_speaking() -> bool {
        false
    }

    pub fn is_available() -> bool {
        false
    }

    pub fn listen(_language: &str, _timeout_ms: u32) -> SpeechHeard {
        SpeechHeard::none(
            HeardStatus::Unavailable,
            "there is no speech recognition on this platform yet",
        )
    }
}

/// The variable naming the file a headless run appends its utterances to.
pub const SPEECH_LOG_VAR: &str = "AZ_SPEECH_LOG";

/// The variable holding what a headless run hears.
pub const SPEECH_HEARD_VAR: &str = "AZ_SPEECH_HEARD";

/// What became of a text to say.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum SpeechStatus {
    /// The engine has it: it is said (or, in a headless run, logged).
    Queued,
    /// There is no speech engine here (no backend for the platform, or the engine is missing).
    Unavailable,
}

/// What became of listening.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HeardStatus {
    /// Something was understood: the text.
    Heard,
    /// The time was up and nothing was said.
    NothingHeard,
    /// The user refused the microphone or speech recognition.
    Denied,
    /// There is no recognizer here, or the app is not set up for one: see the message.
    Unavailable,
    /// The recognizer failed: see the message.
    Failed,
}

/// What listening heard.
#[repr(C)]
#[derive(Debug, Clone, PartialEq)]
pub struct SpeechHeard {
    /// What was understood (empty unless `Heard`).
    pub text: AzString,
    /// Why nothing was heard, readable enough to show (empty for `Heard`).
    pub message: AzString,
    pub status: HeardStatus,
    /// How sure the recognizer is, 0 to 1 (0 where it does not say).
    pub confidence: f32,
}

impl SpeechHeard {
    /// An answer without text.
    #[must_use]
    pub fn none(status: HeardStatus, message: &str) -> Self {
        Self {
            status,
            text: AzString::from(String::new()),
            confidence: 0.0,
            message: AzString::from(message.to_string()),
        }
    }

    /// An answer with what was understood.
    #[must_use]
    pub fn heard(text: &str, confidence: f32) -> Self {
        Self {
            status: HeardStatus::Heard,
            text: AzString::from(text.to_string()),
            confidence,
            message: AzString::from(String::new()),
        }
    }
}

/// The system's speech, as an app uses it: every call answers at once except
/// [`Speech::listen_blocking`].
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(clippy::pub_underscore_fields)] // _reserved: FFI/api.json static-namespace placeholder
pub struct Speech {
    pub _reserved: u8,
}

impl Default for Speech {
    fn default() -> Self {
        Self::new()
    }
}

impl Speech {
    /// A zero-initialised namespace handle: the calls are static.
    #[must_use]
    pub const fn new() -> Self {
        Self { _reserved: 0 }
    }

    /// Says `text` in `language` (a BCP 47 tag such as `de-DE`, or just `de`; empty for the
    /// system's voice) after what is being said.
    #[must_use]
    pub fn speak(text: AzString, language: AzString) -> SpeechStatus {
        say_with(backend_of_run(), text.as_str(), language.as_str(), false)
    }

    /// Says `text` in `language` at once, cutting off what is being said and dropping what
    /// waits: for a prompt that is wrong a second later.
    #[must_use]
    pub fn speak_now(text: AzString, language: AzString) -> SpeechStatus {
        say_with(backend_of_run(), text.as_str(), language.as_str(), true)
    }

    /// Stops saying anything and drops what waits.
    pub fn stop() {
        stop_with(backend_of_run());
    }

    /// Whether something is being said.
    #[must_use]
    pub fn is_speaking() -> bool {
        speaking_with(backend_of_run())
    }

    /// Whether this platform can say anything at all.
    #[must_use]
    pub fn is_available() -> bool {
        available_with(backend_of_run())
    }

    /// Listens on the microphone for up to `timeout_ms` and answers what was said, in
    /// `language` (empty for the system's). Blocks: call it from a worker thread. The first
    /// call may show the system's permission prompt.
    #[must_use]
    pub fn listen_blocking(language: AzString, timeout_ms: u32) -> SpeechHeard {
        listen_with(backend_of_run(), language.as_str(), timeout_ms)
    }
}

/// Where a run's speech goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Backend {
    /// The platform's engines.
    Platform,
    /// Nothing is said or heard; utterances are logged.
    Headless,
}

/// The speech of this run: the headless stand-in in a headless / E2E run (a test on a dev
/// machine must not talk), else the platform's.
fn backend_of_run() -> Backend {
    let headless = std::env::var("AZ_BACKEND").as_deref() == Ok("headless")
        || std::env::var("AZ_E2E_TEST").is_ok();
    if headless {
        Backend::Headless
    } else {
        Backend::Platform
    }
}

/// A language as the engines want it: `de` is `de-DE`, `en` is `en-US`, `pt_BR` is `pt-BR`;
/// empty stays empty (the system's voice).
#[must_use]
pub fn language_tag(language: &str) -> String {
    let tag = language.trim().replace('_', "-");
    let mut parts = tag.split('-').filter(|p| !p.is_empty());
    let Some(primary) = parts.next().map(str::to_ascii_lowercase) else {
        return String::new();
    };
    if let Some(region) = parts.next() {
        return format!("{primary}-{}", region.to_ascii_uppercase());
    }
    // The region whose voice a bare language most likely means.
    let region = match primary.as_str() {
        "en" => "US",
        "de" => "DE",
        "fr" => "FR",
        "es" => "ES",
        "it" => "IT",
        "nl" => "NL",
        "pt" => "PT",
        "pl" => "PL",
        "cs" => "CZ",
        "da" => "DK",
        "sv" => "SE",
        "nb" | "no" => "NO",
        "fi" => "FI",
        "ja" => "JP",
        "zh" => "CN",
        "ko" => "KR",
        "ru" => "RU",
        "tr" => "TR",
        "el" => "GR",
        "uk" => "UA",
        _ => return primary,
    };
    format!("{primary}-{region}")
}

/// SAPI's id of a language (a Windows LCID), for the ones it ships voices for.
fn sapi_language_id(tag: &str) -> Option<u16> {
    Some(match tag {
        "en-US" => 0x409,
        "en-GB" => 0x809,
        "de-DE" => 0x407,
        "de-AT" => 0xC07,
        "de-CH" => 0x807,
        "fr-FR" => 0x40C,
        "es-ES" => 0xC0A,
        "it-IT" => 0x410,
        "nl-NL" => 0x413,
        "pt-PT" => 0x816,
        "pt-BR" => 0x416,
        "pl-PL" => 0x415,
        "ja-JP" => 0x411,
        "zh-CN" => 0x804,
        _ => return None,
    })
}

/// What SAPI is given for `text` in `language`: the text in a `<lang>` element naming the
/// language's id, escaped, and that it is markup; the text as it is when the language is empty
/// or one SAPI has no id for.
#[must_use]
pub fn sapi_text(text: &str, language: &str) -> (String, bool) {
    let Some(id) = sapi_language_id(&language_tag(language)) else {
        return (text.to_string(), false);
    };
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    (format!("<lang langid=\"{id:X}\">{escaped}</lang>"), true)
}

/// What Android's recognizer answered, as the Java helper reports it (`AzulSpeech`'s
/// `nativeOnHeard`): its status code (this enum's order: heard, nothing heard, denied,
/// unavailable, failed), the text, why not, and how sure. Blank text is nothing heard, an
/// unknown code a failure, the confidence kept to 0..1.
#[must_use]
pub(crate) fn heard_from_android(status: i32, text: &str, message: &str, confidence: f32) -> SpeechHeard {
    let status = match status {
        0 if text.trim().is_empty() => HeardStatus::NothingHeard,
        0 => HeardStatus::Heard,
        1 => HeardStatus::NothingHeard,
        2 => HeardStatus::Denied,
        3 => HeardStatus::Unavailable,
        _ => HeardStatus::Failed,
    };
    if status == HeardStatus::Heard {
        let confidence = if confidence.is_finite() {
            confidence.clamp(0.0, 1.0)
        } else {
            0.0
        };
        return SpeechHeard::heard(text.trim(), confidence);
    }
    let message = if message.trim().is_empty() && status == HeardStatus::Failed {
        "the speech recognizer failed"
    } else {
        message
    };
    SpeechHeard::none(status, message)
}

/// Says `text` on `backend`.
pub(crate) fn say_with(backend: Backend, text: &str, language: &str, interrupt: bool) -> SpeechStatus {
    let text = text.trim();
    if text.is_empty() {
        return SpeechStatus::Queued;
    }
    let language = language_tag(language);
    match backend {
        Backend::Headless => {
            headless::say(text, &language, interrupt);
            SpeechStatus::Queued
        }
        Backend::Platform => platform::say(text, &language, interrupt),
    }
}

fn stop_with(backend: Backend) {
    if backend == Backend::Platform {
        platform::stop();
    }
}

fn speaking_with(backend: Backend) -> bool {
    backend == Backend::Platform && platform::is_speaking()
}

fn available_with(backend: Backend) -> bool {
    match backend {
        Backend::Headless => true,
        Backend::Platform => platform::is_available(),
    }
}

/// Listens on `backend`.
pub(crate) fn listen_with(backend: Backend, language: &str, timeout_ms: u32) -> SpeechHeard {
    match backend {
        Backend::Headless => headless::heard(),
        Backend::Platform => platform::listen(&language_tag(language), timeout_ms.max(1)),
    }
}
