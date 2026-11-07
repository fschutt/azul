//! The presence check before an action that reveals or changes a drive's keys: Touch ID, or
//! the computer's password when there is no finger to give (azul's biometric prompt with the
//! passcode fallback). It is the GATED tier of AzDrive's secrets (`unlock.rs` says both tiers):
//! only these actions ask - a drive removed, new keys after a hack, another device added,
//! another recovery code made, a recovery code removed - never a drive's sync or its opening.
//!
//! The action's own question comes first (its dialog stays while the system asks); Touch ID or
//! the password then runs the action, a cancel or a failure leaves it undone and says why.
//!
//! A system with no way to check (azul answers Unavailable: no fingerprint reader and no password
//! that locks the computer - Windows without Hello, Linux without a fingerprint) does NOT run the
//! action: a gate that opens when the sensor is missing is no gate (AZ-W9, X-AR2-O5). It asks
//! for the name of the drive, typed in ([`NameCheck`]): the one thing a person at the screen
//! knows and a stray click does not. That keeps "I was hacked: new keys" possible on every
//! computer, and a script that clicks through it knows the name too. Only a TEST run - a build
//! with the cargo feature `test-hooks` in a headless or E2E run, whose libazul never shows a
//! real prompt and answers Unavailable for it - runs the action at once ([`decide`]).
//!
//! On stdout, for scripts: `AZDRIVE_PRESENCE <action> run|stopped|unchecked` (`unchecked`: the
//! name is asked; `run` follows when it was typed right).

use azul::{
    biometric::{BiometricPrompt, BiometricResult},
    callbacks::TextInputOnTextInputCallbackType,
    prelude::*,
    str::String as AzString,
    widgets::{ButtonType, OnTextInputReturn, TextInputState, TextInputValid},
};
use azul_appkit::l10n::{t, t_args, Arg, Text};

use crate::{
    ids,
    popup_slot::PopupSlot,
    ui_dialogs::{button, buttons, label, line, on_cancel_popup, text_line, typed_button},
    with_state, DriveState, Popup,
};

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

    /// The drive the action is about: the one it carries, or the one its own question (`shown`,
    /// the dialog showing) is about. `None` when neither says.
    fn drive_id(&self, shown: Option<&Popup>) -> Option<String> {
        match self {
            Gated::AddDevice { drive_id }
            | Gated::AddCode { drive_id }
            | Gated::RemoveCode { drive_id, .. } => Some(drive_id.clone()),
            Gated::Forget => match shown {
                Some(Popup::ConfirmForget { drive_id }) => Some(drive_id.clone()),
                _ => None,
            },
            #[cfg(feature = "encryption")]
            Gated::Rotate => match shown {
                Some(Popup::Encryption(crate::encryption::Dialog::ConfirmRotate {
                    drive_id,
                    ..
                })) => Some(drive_id.clone()),
                _ => None,
            },
            #[cfg(not(feature = "encryption"))]
            Gated::Rotate => None,
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
    /// This computer has no way to check the user's presence: the action waits for the name of
    /// the drive, typed in ([`NameCheck`]).
    Unchecked,
}

/// What azul's biometric prompt answered decides: the user's finger or password runs the
/// action; a cancel or a failure stops it; a system that cannot check (Unavailable) asks for the
/// drive's name instead - it never lets the action through unchecked. `test_stand_in`: this is a
/// TEST run whose libazul answers Unavailable for every prompt without showing one
/// ([`test_stand_in`]); only then does Unavailable run the action.
pub(crate) fn decide(result: BiometricResult, test_stand_in: bool) -> Decision {
    match result {
        BiometricResult::Authenticated | BiometricResult::FellBackToPasscode => Decision::Run,
        BiometricResult::Unavailable if test_stand_in => Decision::Run,
        BiometricResult::Unavailable => Decision::Unchecked,
        BiometricResult::Cancelled => Decision::Stop("azdrive-presence-cancelled"),
        _ => Decision::Stop("azdrive-presence-failed"),
    }
}

/// Whether this run is a TEST run whose libazul stands in for the sensor: a build with the cargo
/// feature `test-hooks` (the one libazul of the E2E builds is built with too) in a headless run
/// (`AZ_BACKEND=headless`) or a scripted one (`AZ_E2E_TEST`). A shipped AzDrive is never one,
/// whatever its environment says - and its libazul does not stand in either (RF38).
fn test_stand_in() -> bool {
    test_stand_in_in(cfg!(feature = "test-hooks"), &|name| {
        std::env::var(name).ok()
    })
}

/// [`test_stand_in`] for a build that has the test hooks (`hooks`) or not, in the environment
/// `var` reads. The same predicate as libazul's `azul_core::run_mode`, which this crate cannot
/// ask: it holds no dependency on azul-core.
pub(crate) fn test_stand_in_in(hooks: bool, var: &dyn Fn(&str) -> Option<String>) -> bool {
    hooks && (var("AZ_BACKEND").as_deref() == Some("headless") || var("AZ_E2E_TEST").is_some())
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
        match decide(result, test_stand_in()) {
            Decision::Run => {
                println!("AZDRIVE_PRESENCE {} run", action.word());
                run(info, app, s, action);
            }
            Decision::Stop(why) => {
                println!("AZDRIVE_PRESENCE {} stopped", action.word());
                s.warn(Text::key(why));
            }
            Decision::Unchecked => {
                if open_name_check(&mut s.popup, action.clone()) {
                    println!("AZDRIVE_PRESENCE {} unchecked", action.word());
                } else {
                    println!("AZDRIVE_PRESENCE {} stopped", action.word());
                    s.warn(Text::key("azdrive-presence-unavailable"));
                }
            }
        }
    })
}

// ==== The name check: the stand-in for a computer with no sensor ====

/// The dialog that asks for the name of a drive before an action whose presence check could not
/// be made. The action's own question (the dialog it came from) waits behind it and comes back
/// when the name was typed right, so the action finds it as it left it.
pub(crate) struct NameCheck {
    /// The action that waits.
    pub(crate) action: Gated,
    /// The drive whose name is asked.
    pub(crate) drive_id: String,
    /// What was typed.
    pub(crate) typed: String,
    /// Why the last Confirm did nothing.
    pub(crate) error: Text,
    /// The dialog the action's own question had open.
    pub(crate) behind: Option<Box<Popup>>,
}

/// What Confirm of the name check did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Named {
    /// The name is right: the dialog is gone, the action's own question is back, the action
    /// runs.
    Right(Gated),
    /// The name is wrong (the dialog says so and stays), or no name check is open.
    Wrong,
}

/// Opens the name check of `action` in `slot`, over the dialog showing (its own question), which
/// waits behind it. `false` when it cannot be asked - the drive the action is about is unknown, or
/// the dialog showing must be seen first - and nothing changed.
pub(crate) fn open_name_check(slot: &mut PopupSlot, action: Gated) -> bool {
    let Some(drive_id) = action.drive_id(slot.as_ref()) else {
        return false;
    };
    let behind = if slot.is_none() {
        None
    } else if let Some(shown) = slot.take() {
        Some(Box::new(shown))
    } else {
        return false;
    };
    slot.open(Popup::Presence(NameCheck {
        action,
        drive_id,
        typed: String::new(),
        error: Text::default(),
        behind,
    }));
    true
}

/// Whether `typed` is the name `expected`: the same words, spaces around them aside. A drive
/// with no name is confirmed by nothing - a blank field is never the name.
pub(crate) fn names_match(typed: &str, expected: &str) -> bool {
    let expected = expected.trim();
    !expected.is_empty() && typed.trim() == expected
}

/// Confirm of the name check in `slot`: `drive_name` says the name of a drive by its id.
pub(crate) fn confirm_name(slot: &mut PopupSlot, drive_name: &dyn Fn(&str) -> String) -> Named {
    let Some(Popup::Presence(check)) = slot.as_ref() else {
        return Named::Wrong;
    };
    if !names_match(&check.typed, &drive_name(&check.drive_id)) {
        if let Some(Popup::Presence(check)) = slot.as_mut() {
            check.error = Text::key("azdrive-presence-name-wrong");
        }
        return Named::Wrong;
    }
    let Some(Popup::Presence(check)) = slot.take() else {
        return Named::Wrong;
    };
    if let Some(behind) = check.behind {
        slot.open(*behind);
    }
    Named::Right(check.action)
}

/// The name of the drive `drive_id` as the window shows it.
fn name_of(s: &DriveState, drive_id: &str) -> String {
    s.drive_name(&crate::browse::Place::folder(drive_id, ""))
}

/// The name check's title and content.
pub(crate) fn dialog_parts(check: &NameCheck, s: &DriveState, app: &RefAny) -> (String, Dom) {
    let name = name_of(s, &check.drive_id);
    let mut body = Dom::create_div()
        .with_id(ids::PRESENCE)
        .with_css("display: flex; flex-direction: column; min-width: 420px;")
        .with_child(line(&t_args(
            "azdrive-presence-name-what",
            &[("action", Arg::from(t(check.action.reason())))],
        )))
        .with_child(label(&t_args(
            "azdrive-presence-name-label",
            &[("name", Arg::from(name.as_str()))],
        )))
        .with_child(
            TextInput::create()
                .with_text(AzString::from(check.typed.as_str()))
                .with_on_text_input(
                    app.clone(),
                    on_name_typed as TextInputOnTextInputCallbackType,
                )
                .dom()
                .with_id(ids::PRESENCE_NAME),
        );
    if !check.error.is_empty() {
        body.add_child(text_line(&check.error).with_css("color: system:danger;"));
    }
    body.add_child(buttons(vec![
        button("kit-button-cancel", app, on_cancel_popup),
        typed_button(
            "azdrive-presence-name-confirm",
            ButtonType::Primary,
            app,
            on_name_confirmed,
        )
        .with_id(ids::PRESENCE_CONFIRM),
    ]));
    (
        t_args("azdrive-presence-name-title", &[("name", Arg::from(name))]),
        body,
    )
}

/// The field of the name check changed.
extern "C" fn on_name_typed(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let keep = OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    };
    let Some(mut s) = data.downcast_mut::<DriveState>() else {
        return keep;
    };
    if let Some(Popup::Presence(check)) = s.popup.as_mut() {
        check.typed = state.get_text().as_str().to_string();
        check.error = Text::default();
    }
    keep
}

/// The name check's Confirm: the right name runs the action.
extern "C" fn on_name_confirmed(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let names: Vec<(String, String)> = s
            .slots
            .iter()
            .map(|slot| (slot.entry.id.clone(), slot.name()))
            .collect();
        let name = |id: &str| {
            names
                .iter()
                .find(|(slot_id, _)| slot_id == id)
                .map_or_else(|| id.to_string(), |(_, name)| name.clone())
        };
        if let Named::Right(action) = confirm_name(&mut s.popup, &name) {
            println!("AZDRIVE_PRESENCE {} run", action.word());
            run(info, app, s, action);
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
