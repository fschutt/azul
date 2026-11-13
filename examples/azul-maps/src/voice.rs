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

/// What the listening worker hears into.
pub struct ListenRequest {
    /// The language the user speaks (`de`, `en`).
    pub language: String,
    /// The write-back that hands what was heard to the app.
    pub on_done: azul::callbacks::WriteBackCallbackType,
}

/// What was heard: the text, or why nothing was.
#[derive(Debug, Clone)]
pub struct Heard {
    pub text: Option<String>,
    /// Why nothing was heard, readable.
    pub why: String,
}

/// How long the user may speak a destination, ms.
const LISTEN_MS: u32 = 8_000;

/// The listening worker, an azul `Thread`: listens on the microphone (azul's speech
/// recognition, which blocks) and hands the text back.
pub extern "C" fn listen_worker(
    mut init: azul::prelude::RefAny,
    mut sender: azul::task::ThreadSender,
    _receiver: azul::task::ThreadReceiver,
) {
    use azul::speech::HeardStatus;
    let Some((language, on_done)) = init
        .downcast_ref::<ListenRequest>()
        .map(|r| (r.language.clone(), r.on_done))
    else {
        return;
    };
    let heard = Speech::listen_blocking(language.as_str(), LISTEN_MS);
    let reply = if matches!(heard.status, HeardStatus::Heard) {
        Heard {
            text: Some(heard.text.as_str().to_string()),
            why: String::new(),
        }
    } else {
        Heard {
            text: None,
            why: heard.message.as_str().to_string(),
        }
    };
    let _sent = sender.send(azul::task::ThreadReceiveMsg::WriteBack(
        azul::task::ThreadWriteBackMsg::create(on_done, azul::prelude::RefAny::new(reply)),
    ));
}
