//! The navigation's prompts said aloud, in the window's language, through azul's speech
//! (`Speech`, api.json's `speech`: AVSpeechSynthesizer, SAPI, speech-dispatcher). A headless
//! run says nothing and azul logs each prompt (`AZ_SPEECH <language> <text>`); AzMaps prints
//! `AZMAPS_SAY <text>` for scripts either way.

use azul::speech::{Speech, SpeechStatus};

/// Says `text`; `interrupt` cuts off what is being said. Whether the system took it.
pub fn say(text: &str, interrupt: bool) -> bool {
    println!("AZMAPS_SAY {text}");
    let language = azul_appkit::l10n::locale();
    let status = if interrupt {
        Speech::speak_now(text, language.as_str())
    } else {
        Speech::speak(text, language.as_str())
    };
    matches!(status, SpeechStatus::Queued)
}

/// Stops saying anything (the navigation ended, the voice was turned off).
pub fn stop() {
    Speech::stop();
}

/// Whether this system can speak at all (the voice switch shows only then).
#[must_use]
pub fn available() -> bool {
    Speech::is_available()
}
