//! Windows: SAPI 5's `ISpVoice` says the text, asynchronously, so a call answers at once; WinRT's
//! `SpeechRecognizer` hears. A COM voice belongs to the thread that made it, so it lives on a
//! thread of its own and is fed through a channel; that thread also polls whether it talks.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Mutex, OnceLock, PoisonError,
    },
    time::Duration,
};

use windows::{
    core::HSTRING,
    Win32::{
        Media::Speech::{
            ISpVoice, SpVoice, SPF_ASYNC, SPF_IS_XML, SPF_PURGEBEFORESPEAK, SPRS_IS_SPEAKING,
            SPVOICESTATUS,
        },
        System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED},
    },
};

use super::{sapi_text, HeardStatus, SpeechHeard, SpeechStatus};

/// What the voice's thread is asked to do.
enum Command {
    Say {
        text: String,
        language: String,
        interrupt: bool,
    },
    Stop,
}

/// Whether the voice talks, as its thread last saw it.
static SPEAKING: AtomicBool = AtomicBool::new(false);

/// The channel to the voice's thread; `None` when SAPI could not make a voice.
static VOICE: OnceLock<Option<Mutex<mpsc::Sender<Command>>>> = OnceLock::new();

/// The voice's thread, started on first use.
fn voice() -> Option<&'static Mutex<mpsc::Sender<Command>>> {
    VOICE
        .get_or_init(|| {
            let (commands, inbox) = mpsc::channel::<Command>();
            let (ready, started) = mpsc::channel::<bool>();
            std::thread::Builder::new()
                .name("azul-speech".into())
                .spawn(move || run_voice(inbox, ready))
                .ok()?;
            started
                .recv_timeout(Duration::from_secs(5))
                .ok()
                .filter(|made| *made)?;
            Some(Mutex::new(commands))
        })
        .as_ref()
}

/// The voice's thread: makes the voice, says what it is sent, and notes every 100 ms whether it
/// talks.
fn run_voice(inbox: mpsc::Receiver<Command>, ready: mpsc::Sender<bool>) {
    unsafe {
        // A COM call needs an initialised apartment on its thread.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let made: windows::core::Result<ISpVoice> = unsafe { CoCreateInstance(&SpVoice, None, CLSCTX_ALL) };
    let Ok(voice) = made else {
        let _ = ready.send(false);
        return;
    };
    let _ = ready.send(true);
    #[allow(clippy::cast_sign_loss)] // SAPI's flags are small positive numbers
    let flag = |f: i32| f as u32;
    loop {
        match inbox.recv_timeout(Duration::from_millis(100)) {
            Ok(Command::Say {
                text,
                language,
                interrupt,
            }) => {
                let (body, is_xml) = sapi_text(&text, &language);
                let mut flags = flag(SPF_ASYNC.0);
                if is_xml {
                    flags |= flag(SPF_IS_XML.0);
                }
                if interrupt {
                    flags |= flag(SPF_PURGEBEFORESPEAK.0);
                }
                if unsafe { voice.Speak(&HSTRING::from(body), flags, None) }.is_ok() {
                    SPEAKING.store(true, Ordering::Relaxed);
                }
            }
            Ok(Command::Stop) => {
                // An empty text with the purge flag drops what is said and what waits.
                let flags = flag(SPF_ASYNC.0) | flag(SPF_PURGEBEFORESPEAK.0);
                let _ = unsafe { voice.Speak(&HSTRING::new(), flags, None) };
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        let mut status = SPVOICESTATUS::default();
        if unsafe { voice.GetStatus(&mut status, core::ptr::null_mut()) }.is_ok() {
            SPEAKING.store(
                status.dwRunningState == flag(SPRS_IS_SPEAKING.0),
                Ordering::Relaxed,
            );
        }
    }
}

/// Hands `text` to the voice.
pub fn say(text: &str, language: &str, interrupt: bool) -> SpeechStatus {
    let Some(voice) = voice() else {
        return SpeechStatus::Unavailable;
    };
    let sent = voice
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .send(Command::Say {
            text: text.to_string(),
            language: language.to_string(),
            interrupt,
        });
    if sent.is_ok() {
        SpeechStatus::Queued
    } else {
        SpeechStatus::Unavailable
    }
}

/// Stops the voice and drops what waits.
pub fn stop() {
    if let Some(Some(voice)) = VOICE.get() {
        let _ = voice
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .send(Command::Stop);
    }
}

/// Whether the voice talks.
pub fn is_speaking() -> bool {
    SPEAKING.load(Ordering::Relaxed)
}

/// Whether SAPI made a voice.
pub fn is_available() -> bool {
    voice().is_some()
}

/// Listens with WinRT's recognizer for up to `timeout_ms` in `language`.
pub fn listen(language: &str, timeout_ms: u32) -> SpeechHeard {
    use std::future::IntoFuture;

    use windows::{
        Foundation::TimeSpan,
        Globalization::Language,
        Media::SpeechRecognition::{SpeechRecognitionResultStatus, SpeechRecognizer},
    };

    unsafe {
        // A WinRT async call needs an initialised apartment on its thread.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let made = if language.is_empty() {
        SpeechRecognizer::new()
    } else {
        Language::CreateLanguage(&HSTRING::from(language)).and_then(|l| SpeechRecognizer::Create(&l))
    };
    let recognizer = match made {
        Ok(recognizer) => recognizer,
        Err(e) => {
            return SpeechHeard::none(
                HeardStatus::Unavailable,
                &format!("no speech recognizer for {language}: {}", e.message()),
            )
        }
    };
    if let Ok(timeouts) = recognizer.Timeouts() {
        // TimeSpan counts in 100 ns.
        let _ = timeouts.SetInitialSilenceTimeout(TimeSpan {
            Duration: i64::from(timeout_ms) * 10_000,
        });
        let _ = timeouts.SetEndSilenceTimeout(TimeSpan {
            Duration: 15_000_000,
        });
    }
    let compiled = recognizer
        .CompileConstraintsAsync()
        .and_then(|op| pollster::block_on(op.into_future()));
    match compiled.and_then(|c| c.Status()) {
        Ok(status) if status == SpeechRecognitionResultStatus::Success => {}
        Ok(_) | Err(_) => {
            return SpeechHeard::none(
                HeardStatus::Unavailable,
                "the speech recognizer could not be prepared (is speech recognition allowed in \
                 Settings > Privacy > Speech?)",
            )
        }
    }
    let result = match recognizer
        .RecognizeAsync()
        .and_then(|op| pollster::block_on(op.into_future()))
    {
        Ok(result) => result,
        // SPERR_SPEECH_PRIVACY_POLICY_NOT_ACCEPTED: the user has not allowed online speech.
        Err(e) if e.code().0 == 0x8004_5509_u32 as i32 => {
            return SpeechHeard::none(HeardStatus::Denied, "speech recognition is not allowed in Settings > Privacy > Speech")
        }
        Err(e) => return SpeechHeard::none(HeardStatus::Failed, &e.message().to_string()),
    };
    let status = result.Status().unwrap_or(SpeechRecognitionResultStatus::Unknown);
    let text = result.Text().map(|t| t.to_string()).unwrap_or_default();
    #[allow(clippy::cast_possible_truncation)] // a confidence of 0..1
    let confidence = result.RawConfidence().unwrap_or(0.0) as f32;
    if status == SpeechRecognitionResultStatus::Success && !text.trim().is_empty() {
        SpeechHeard::heard(text.trim(), confidence)
    } else if status == SpeechRecognitionResultStatus::Success
        || status == SpeechRecognitionResultStatus::TimeoutExceeded
        || status == SpeechRecognitionResultStatus::UserCanceled
    {
        SpeechHeard::none(HeardStatus::NothingHeard, "nothing was said")
    } else if status == SpeechRecognitionResultStatus::MicrophoneUnavailable {
        SpeechHeard::none(HeardStatus::Unavailable, "there is no microphone")
    } else {
        SpeechHeard::none(HeardStatus::Failed, &format!("the recognizer failed ({})", status.0))
    }
}
