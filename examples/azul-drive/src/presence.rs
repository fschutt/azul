//! The presence check before an action that reveals or changes a drive's keys: Touch ID, or
//! the computer's password when there is no finger to give (azul's biometric prompt with the
//! passcode fallback). It is the GATED tier of AzDrive's secrets (`unlock.rs` says both tiers):
//! only these actions ask - a drive removed, new keys after a hack, another device added,
//! another recovery code made, a recovery code removed - never a drive's sync or its opening.
//!
//! The action's own question comes first (its dialog stays while the system asks); Touch ID or
//! the password then runs the action, a cancel or a failure leaves it undone and says why. A
//! system with no way to check (azul answers Unavailable: no sensor and no password policy,
//! and every headless or E2E run, whose azul never shows a real prompt) runs it after its own
//! question, as before.
//!
//! On stdout, for scripts: `AZDRIVE_PRESENCE <action> run|stopped`.

use azul::{
    biometric::{BiometricPrompt, BiometricResult},
    prelude::*,
    str::String as AzString,
};
use azul_appkit::l10n::{t, Text};

use crate::{with_state, DriveState};

/// An action that waits for the user's presence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Gated {
    /// "Remove drive" answered yes (its question still open).
    Forget,
    /// "I was hacked: new keys..." confirmed (its dialog still open).
    Rotate,
    /// Another device of drive `drive_id`: its join code made.
    AddDevice { drive_id: String },
    /// Another recovery code for drive `drive_id`.
    AddCode { drive_id: String },
    /// Recovery key `key_id` (`label`) of drive `drive_id` removed.
    RemoveCode {
        drive_id: String,
        key_id: String,
        label: String,
    },
}

impl Gated {
    /// Why the system's prompt asks, as a key of the resources.
    pub fn reason(&self) -> &'static str {
        match self {
            Gated::Forget => "azdrive-presence-forget",
            Gated::Rotate => "azdrive-presence-rotate",
            Gated::AddDevice { .. } => "azdrive-presence-add-device",
            Gated::AddCode { .. } => "azdrive-presence-add-code",
            Gated::RemoveCode { .. } => "azdrive-presence-remove-code",
        }
    }

    /// Its name on stdout.
    fn word(&self) -> &'static str {
        match self {
            Gated::Forget => "forget",
            Gated::Rotate => "rotate",
            Gated::AddDevice { .. } => "add-device",
            Gated::AddCode { .. } => "add-code",
            Gated::RemoveCode { .. } => "remove-code",
        }
    }
}

/// What the prompt's answer does to the action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decision {
    /// The action runs.
    Run,
    /// The action stays undone; why, as a key of the resources.
    Stop(&'static str),
}

/// What azul's biometric prompt answered decides: the user's finger or password runs the
/// action, as does a system that cannot check; a cancel or a failure stops it.
pub(crate) fn decide(result: BiometricResult) -> Decision {
    match result {
        BiometricResult::Authenticated
        | BiometricResult::FellBackToPasscode
        | BiometricResult::Unavailable => Decision::Run,
        BiometricResult::Cancelled => Decision::Stop("azdrive-presence-cancelled"),
        _ => Decision::Stop("azdrive-presence-failed"),
    }
}

/// Asks for the user's presence before `action` (one at a time: a second action while the
/// system asks is not started).
pub(crate) fn ask(info: &mut CallbackInfo, s: &mut DriveState, action: Gated) {
    if s.presence.is_some() {
        return;
    }
    let reason = t(action.reason());
    s.presence = Some(action);
    info.request_biometric_auth(BiometricPrompt {
        reason: AzString::from(reason),
        cancel_label: AzString::from(""),
        allow_device_credential: true,
    });
}

/// azul's biometric prompt answered: the action waiting for it runs, or stays undone.
pub(crate) extern "C" fn on_biometric_result(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(result) = info.get_biometric_result().into_option() else {
        return Update::DoNothing;
    };
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(action) = s.presence.take() else {
            return;
        };
        match decide(result) {
            Decision::Run => {
                println!("AZDRIVE_PRESENCE {} run", action.word());
                run(info, app, s, action);
            }
            Decision::Stop(why) => {
                println!("AZDRIVE_PRESENCE {} stopped", action.word());
                s.warn(Text::key(why));
            }
        }
    })
}

/// The action, its presence checked.
fn run(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, action: Gated) {
    match action {
        Gated::Forget => crate::actions::forget_drive(info, app, s),
        #[cfg(feature = "encryption")]
        Gated::Rotate => crate::encryption::rotate_confirmed(info, app, s),
        #[cfg(feature = "encryption")]
        Gated::AddDevice { drive_id } => crate::encryption::invite_device(info, app, s, &drive_id),
        #[cfg(feature = "encryption")]
        Gated::AddCode { drive_id } => crate::recovery_keys::ask_add_code(s, &drive_id),
        #[cfg(feature = "encryption")]
        Gated::RemoveCode {
            drive_id,
            key_id,
            label,
        } => crate::recovery_keys::ask_remove_code(s, &drive_id, key_id, label),
        // The encryption flows are not in this build: nothing asks for them.
        #[cfg(not(feature = "encryption"))]
        Gated::Rotate | Gated::AddDevice { .. } | Gated::AddCode { .. } | Gated::RemoveCode { .. } => {}
    }
}
