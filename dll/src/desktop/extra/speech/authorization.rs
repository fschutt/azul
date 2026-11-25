//! What the system says about the two permissions listening needs (speech recognition and the
//! microphone) before anything is asked, and what to do about it. Pure: the platform code reads
//! the statuses and the Info.plist, this decides.
//!
//! macOS attributes a request to the app that is RESPONSIBLE for the process. For an app bundle
//! that is the app itself: its Info.plist must carry `NSSpeechRecognitionUsageDescription` and
//! `NSMicrophoneUsageDescription`, else the system stops the process at the first request. For a
//! bare binary started from a terminal it is the terminal, which either already holds the grant or
//! shows its own prompt. Measured on macOS 15.5 (2026-11-25): Terminal held both, a binary without
//! any Info.plist read `Authorized` for both, and recognition and microphone capture worked. So the
//! statuses are read FIRST and the Info.plist only decides whether asking is safe:
//!
//! | speech | microphone | usage texts | decision |
//! |--------|------------|-------------|----------|
//! | authorized | authorized | any | listen |
//! | denied or restricted | any | any | refuse (`Denied`), say where to change it |
//! | any | denied or restricted | any | refuse (`Denied`), say where to change it |
//! | undecided | undecided or authorized | the undecided ones have their text | ask, then listen |
//! | undecided | undecided or authorized | one is missing | refuse (`Unavailable`) WITHOUT asking: the system would stop the process |

// The items are `pub` for the tests and for the parts of the module that are re-exported.
#![allow(unreachable_pub)]

/// One permission's answer, the platforms' different scales on one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Access {
    /// Nobody was asked yet (also what an unknown status value reads as).
    #[default]
    NotDetermined,
    Authorized,
    Denied,
    /// A profile or Screen Time forbids it: the user cannot change it.
    Restricted,
}

impl Access {
    /// `SFSpeechRecognizerAuthorizationStatus`: 0 not determined, 1 DENIED, 2 RESTRICTED,
    /// 3 authorized.
    #[must_use]
    pub const fn from_speech(raw: isize) -> Self {
        match raw {
            1 => Self::Denied,
            2 => Self::Restricted,
            3 => Self::Authorized,
            _ => Self::NotDetermined,
        }
    }

    /// `AVAuthorizationStatus`: 0 not determined, 1 RESTRICTED, 2 DENIED, 3 authorized. The two
    /// enums order 1 and 2 the other way round.
    #[must_use]
    pub const fn from_capture(raw: isize) -> Self {
        match raw {
            1 => Self::Restricted,
            2 => Self::Denied,
            3 => Self::Authorized,
            _ => Self::NotDetermined,
        }
    }
}

/// What the system and the program's Info.plist say.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Gates {
    pub speech: Access,
    pub microphone: Access,
    /// The Info.plist of this program has `NSSpeechRecognitionUsageDescription`.
    pub speech_key: bool,
    /// The Info.plist of this program has `NSMicrophoneUsageDescription`.
    pub microphone_key: bool,
    /// The app this program was started from (Terminal, iTerm, ...), when the caller found it:
    /// for the messages only.
    pub starter: Option<String>,
}

/// Why listening does not go on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The user (or an administrator) said no.
    Denied,
    /// Nothing was said yet, and asking is not safe or not possible.
    Unavailable,
}

/// What to do before listening.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Everything is allowed: listen.
    Listen,
    /// Something is undecided and this program carries the text for the system's prompt: ask for
    /// what is marked, then listen.
    Ask { speech: bool, microphone: bool },
    /// Do not listen, and do not ask: the message says why and what to change.
    Refuse { why: Refusal, message: String },
}

/// The key of the speech recognition usage text.
pub const SPEECH_KEY: &str = "NSSpeechRecognitionUsageDescription";
/// The key of the microphone usage text.
pub const MICROPHONE_KEY: &str = "NSMicrophoneUsageDescription";

/// Who the system will hold responsible for the request, in words: this app when its Info.plist
/// has both texts, else the app that started the program.
fn who(gates: &Gates) -> String {
    if gates.speech_key && gates.microphone_key {
        return "this app".to_string();
    }
    match gates.starter.as_deref().filter(|name| !name.trim().is_empty()) {
        Some(name) => format!("{name}, the app this program was started from"),
        None => "the app this program was started from (usually your terminal)".to_string(),
    }
}

/// What the status of the permissions allows. `Decision::Refuse` never asks the system: a refusal
/// is a message, not a prompt.
#[must_use]
pub fn decide(gates: &Gates) -> Decision {
    let who = who(gates);
    // A "no" ends it: asking again changes nothing, the user has to change the setting.
    let mut said_no = Vec::new();
    match gates.speech {
        Access::Denied => said_no.push(format!(
            "speech recognition is turned off for {who}: allow it in System Settings > Privacy & \
             Security > Speech Recognition"
        )),
        Access::Restricted => said_no.push(
            "speech recognition is restricted on this Mac (a profile or Screen Time): only an \
             administrator can allow it"
                .to_string(),
        ),
        Access::NotDetermined | Access::Authorized => {}
    }
    match gates.microphone {
        Access::Denied => said_no.push(format!(
            "the microphone is turned off for {who}: allow it in System Settings > Privacy & \
             Security > Microphone"
        )),
        Access::Restricted => said_no.push(
            "the microphone is restricted on this Mac (a profile or Screen Time): only an \
             administrator can allow it"
                .to_string(),
        ),
        Access::NotDetermined | Access::Authorized => {}
    }
    if !said_no.is_empty() {
        return Decision::Refuse {
            why: Refusal::Denied,
            message: said_no.join("; "),
        };
    }

    let ask_speech = gates.speech == Access::NotDetermined;
    let ask_microphone = gates.microphone == Access::NotDetermined;
    if !ask_speech && !ask_microphone {
        return Decision::Listen;
    }
    // Asking without the usage text stops the process: only what is undecided needs its text.
    let mut missing = Vec::new();
    let mut undecided = Vec::new();
    if ask_speech {
        undecided.push("speech recognition");
        if !gates.speech_key {
            missing.push(SPEECH_KEY);
        }
    }
    if ask_microphone {
        undecided.push("the microphone");
        if !gates.microphone_key {
            missing.push(MICROPHONE_KEY);
        }
    }
    if missing.is_empty() {
        return Decision::Ask {
            speech: ask_speech,
            microphone: ask_microphone,
        };
    }
    Decision::Refuse {
        why: Refusal::Unavailable,
        message: format!(
            "{} has not been allowed for {who} yet, and this program has no Info.plist that says \
             why it asks ({}): macOS would stop it at the first request. Start it from an app \
             bundle that declares them (azul-doc bundle), or put an Info.plist into the binary \
             (linker argument -sectcreate __TEXT __info_plist <file>)",
            undecided.join(" and "),
            missing.join(", ")
        ),
    }
}

/// What the permissions allow for a recognizer that listens to the microphone and needs no
/// speech recognition of the system (the built-in model): only the microphone counts.
#[must_use]
pub fn decide_microphone(gates: &Gates) -> Decision {
    decide(&Gates {
        speech: Access::Authorized,
        speech_key: true,
        ..gates.clone()
    })
}

/// Which of the two permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    Speech,
    Microphone,
}

/// What the user is told after the system's prompt for `gate` came back without a yes:
/// `answered` is whether the user answered at all.
#[must_use]
pub fn declined(gate: Gate, answered: bool, gates: &Gates) -> String {
    let (what, pane) = match gate {
        Gate::Speech => ("speech recognition", "Speech Recognition"),
        Gate::Microphone => ("the microphone", "Microphone"),
    };
    if answered {
        format!(
            "{what} was not allowed for {}: allow it in System Settings > Privacy & Security > {pane}",
            who(gates)
        )
    } else {
        format!("{what} was not allowed (no answer to the system's prompt)")
    }
}

/// The name of the app bundle a process runs from: the first path component that ends in `.app`
/// (so a helper inside an app is named by the app), without the suffix. `None` for a path outside
/// any bundle.
#[must_use]
pub fn app_name_from_path(path: &str) -> Option<String> {
    path.split('/')
        .find_map(|part| part.strip_suffix(".app"))
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
#[path = "authorization_test.rs"]
mod authorization_test;
