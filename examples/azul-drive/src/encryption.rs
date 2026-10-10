//! Encrypted drives in AzDrive (feature `encryption`; off until the drive index - the bucket's
//! encrypted metadata repository - is in).
//!
//! An Azlin drive opens through azul-storage's `AutoEncrypted`: its first call, on a worker
//! thread, finds whether the drive is encrypted (this computer keeps its key, or the bucket
//! holds key files) and then goes through the encryption, the index from
//! [`index_provider`]; a plain drive is used as it is. A build without an index provider opens
//! plain drives only, says so for encrypted ones, and offers neither flow below.
//!
//! The flows, from a cloud drive's menu in the source list:
//! - "Encrypt this drive...": a question, then the drive's keys (its key and this computer's
//!   member key in the keyring, the recovery wrap and this computer's wrap in the bucket), then
//!   the RECOVERY SHEET: the recovery code in groups, to write down and confirm by typing one
//!   group back (it is stored nowhere: this is the one time it shows). Then the drive's
//!   plaintext files move into the encryption on a worker thread, resumably: the migration's
//!   state file sits beside the drives file and the next "Encrypt" continues from it.
//! - "Unlock with the recovery code...": a computer without the drive's key types the code;
//!   the key is kept in its keyring and the computer gets a wrap of its own.

use std::{path::PathBuf, sync::Arc};

use azul::{
    callbacks::TextInputOnTextInputCallbackType,
    prelude::*,
    str::String as AzString,
    widgets::{ButtonType, OnTextInputReturn, TextInputState, TextInputValid},
};
use azul_storage::{
    azul_keyring::AzulKeyring,
    crypto::{
        device,
        keys::{RecoveryCode, RecoveryKdf},
        random_bytes, Zeroizing,
    },
    encrypted::{open_encrypted, AutoEncrypted, IndexProvider},
    migrate::{migrate, MigrationState},
    Drive,
};

use crate::{
    browse,
    jobs::Job,
    spawn,
    ui_dialogs::{button, buttons, label, line, on_cancel_popup, typed_button},
    with_state, DriveState, Popup,
};

/// The provider of encrypted drives' indexes: the drive's encrypted metadata repository once
/// it is in (azul-storage's metadata module); `None` until then - this build opens plain
/// drives only.
pub(crate) fn index_provider() -> Option<Arc<dyn IndexProvider>> {
    None
}

/// An Azlin drive's bucket as the drive the app uses: decided plain or encrypted on its first
/// call.
pub(crate) fn wrap(drive_id: &str, bucket: Arc<dyn Drive>) -> Arc<AutoEncrypted> {
    Arc::new(AutoEncrypted::new(
        bucket,
        drive_id,
        Arc::new(AzulKeyring::new()),
        index_provider(),
    ))
}

/// Whether the encryption flows are offered (an index provider is in this build).
pub(crate) fn offered() -> bool {
    index_provider().is_some()
}

/// Where a drive's migration keeps its state: beside the drives file.
fn state_file(s: &DriveState, drive_id: &str) -> PathBuf {
    let dir = s
        .drives_file
        .as_ref()
        .and_then(|file| file.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(std::env::temp_dir);
    dir.join("encryption").join(format!("{drive_id}.migration.json"))
}

// ==== The dialog ====

/// The recovery sheet: the code, then one of its groups typed back.
pub(crate) struct Sheet {
    pub drive_id: String,
    /// The code as shown (`XXXXX-XXXXX-XXXXX-XXXXX-XXXXXX`). A secret: never printed.
    pub code: Zeroizing<String>,
    /// Which group (0-based) the user types back.
    pub check: usize,
    pub typed: Zeroizing<String>,
    pub error: String,
}

/// A group of a recovery code as people type it: no spaces or dashes, upper case, `O` for 0,
/// `I` and `L` for 1.
fn normalized(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .collect()
}

impl Sheet {
    /// The sheet of `code`, asking for a random group.
    pub(crate) fn new(drive_id: &str, code: Zeroizing<String>) -> Sheet {
        let groups = code.split('-').count().max(1);
        let mut pick = [0u8; 1];
        let check = match random_bytes(&mut pick) {
            Ok(()) => usize::from(pick[0]) % groups,
            Err(_) => groups - 1,
        };
        Sheet {
            drive_id: drive_id.to_string(),
            code,
            check,
            typed: Zeroizing::new(String::new()),
            error: String::new(),
        }
    }

    /// Whether the typed group is the one asked for.
    pub(crate) fn confirmed(&self) -> bool {
        self.code
            .split('-')
            .nth(self.check)
            .is_some_and(|group| normalized(group) == normalized(&self.typed))
    }
}

/// The encryption dialog's pages.
pub(crate) enum Dialog {
    /// "Encrypt this drive?"
    Confirm { drive_id: String },
    /// A job runs: what it does.
    Busy { title: String, text: String },
    /// The recovery code, shown once.
    Sheet(Sheet),
    /// The recovery code typed in to unlock the drive on this computer.
    Unlock {
        drive_id: String,
        typed: Zeroizing<String>,
        error: String,
    },
    /// The end: a title and a sentence.
    Message { title: String, text: String },
}

fn drive_name(s: &DriveState, drive_id: &str) -> String {
    s.drive_name(&browse::Place::folder(drive_id, ""))
}

/// The dialog's title and content.
pub(crate) fn dialog_parts(dialog: &Dialog, s: &DriveState, app: &RefAny) -> (String, Dom) {
    let column = |children: Vec<Dom>| {
        Dom::create_div()
            .with_css("display: flex; flex-direction: column; min-width: 420px; max-width: 520px;")
            .with_children(DomVec::from(children))
    };
    match dialog {
        Dialog::Confirm { drive_id } => (
            format!("Encrypt \"{}\"?", drive_name(s, drive_id)),
            column(vec![
                line(
                    "Its files are encrypted on this computer before they leave it: Azlin stores \
                     ciphertext under random names, without the files' names or folders.",
                ),
                line(
                    "This computer keeps the drive's key in its keyring. Other computers get it \
                     with a join code from one that has it.",
                ),
                line(
                    "You get a RECOVERY CODE: the only way back in when every computer with the \
                     key is lost. Azlin cannot reset it.",
                ),
                line(
                    "Then the drive's files move into the encryption. It runs in the background \
                     and continues where it stopped if AzDrive closes.",
                ),
                buttons(vec![
                    button("Cancel", app, on_cancel_popup),
                    typed_button("Encrypt", ButtonType::Primary, app, on_encrypt),
                ]),
            ]),
        ),
        Dialog::Busy { title, text } => (
            title.clone(),
            column(vec![
                line(text),
                buttons(vec![button("Hide", app, on_cancel_popup)]),
            ]),
        ),
        Dialog::Sheet(sheet) => {
            let mut body = column(vec![
                line(
                    "Write it down, or print it, and keep it apart from this computer. It is \
                     stored nowhere: this is the one time it shows.",
                ),
                Dom::create_span_with_text(AzString::from(sheet.code.as_str())).with_css(
                    "font-family: monospace; font-size: 20px; margin-top: 14px; \
                     margin-bottom: 14px; letter-spacing: 1px;",
                ),
                label(&format!(
                    "To check that you have it, type group {} of the code:",
                    sheet.check + 1
                )),
                TextInput::create()
                    .with_text(AzString::from(sheet.typed.as_str()))
                    .with_on_text_input(app.clone(), on_typed as TextInputOnTextInputCallbackType)
                    .dom(),
            ]);
            if !sheet.error.is_empty() {
                body.add_child(line(&sheet.error).with_css("color: #C42B1C;"));
            }
            body.add_child(buttons(vec![typed_button(
                "I have written it down",
                ButtonType::Primary,
                app,
                on_sheet_done,
            )]));
            (String::from("Your recovery code"), body)
        }
        Dialog::Unlock {
            drive_id, error, ..
        } => {
            let mut body = column(vec![
                line(
                    "This computer has no key for this drive. Type its recovery code (any \
                     case, with or without dashes) to keep the key here.",
                ),
                label("The recovery code"),
                TextInput::create()
                    .with_placeholder(AzString::from("XXXXX-XXXXX-XXXXX-XXXXX-XXXXXX"))
                    .with_on_text_input(app.clone(), on_typed as TextInputOnTextInputCallbackType)
                    .dom(),
            ]);
            if !error.is_empty() {
                body.add_child(line(error).with_css("color: #C42B1C;"));
            }
            body.add_child(buttons(vec![
                button("Cancel", app, on_cancel_popup),
                typed_button("Unlock", ButtonType::Primary, app, on_unlock),
            ]));
            (format!("Unlock \"{}\"", drive_name(s, drive_id)), body)
        }
        Dialog::Message { title, text } => (
            title.clone(),
            column(vec![
                line(text),
                buttons(vec![typed_button(
                    "Close",
                    ButtonType::Primary,
                    app,
                    on_cancel_popup,
                )]),
            ]),
        ),
    }
}

// ==== Opening it ====

/// The drive's `AutoEncrypted`, opening the drive first; `None` (with a message) when it cannot
/// be opened yet.
fn auto_of(s: &mut DriveState, drive_id: &str) -> Option<Arc<AutoEncrypted>> {
    let index = s.slot_index(drive_id)?;
    if s.slots[index].locked() {
        s.error("Open the drive first: its session is read from the keyring.");
        return None;
    }
    crate::open_slot(s, index)?;
    let auto = s.slots[index].auto.clone();
    if auto.is_none() {
        s.error("Only an Azlin drive can be encrypted.");
    }
    auto
}

/// "Encrypt this drive...": the question.
pub(crate) fn ask_encrypt(s: &mut DriveState, drive_id: &str) {
    if s.popup.is_none() {
        s.popups_opened += 1;
        s.popup = Some(Popup::Encryption(Dialog::Confirm {
            drive_id: drive_id.to_string(),
        }));
    }
}

/// "Unlock with the recovery code...".
pub(crate) fn ask_unlock(s: &mut DriveState, drive_id: &str) {
    if s.popup.is_none() {
        s.popups_opened += 1;
        s.popup = Some(Popup::Encryption(Dialog::Unlock {
            drive_id: drive_id.to_string(),
            typed: Zeroizing::new(String::new()),
            error: String::new(),
        }));
    }
}

extern "C" fn on_typed(
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
    let text = Zeroizing::new(state.get_text().as_str().to_string());
    match s.popup.as_mut() {
        Some(Popup::Encryption(Dialog::Sheet(sheet))) => {
            sheet.typed = text;
            sheet.error.clear();
        }
        Some(Popup::Encryption(Dialog::Unlock { typed, error, .. })) => {
            *typed = text;
            error.clear();
        }
        _ => {}
    }
    keep
}

extern "C" fn on_encrypt(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::Encryption(Dialog::Confirm { drive_id })) = s.popup.take() else {
            return;
        };
        let Some(auto) = auto_of(s, &drive_id) else {
            return;
        };
        s.popup = Some(Popup::Encryption(Dialog::Busy {
            title: String::from("Encrypting the drive"),
            text: String::from(
                "Making the drive's keys (the recovery code's protection takes a few seconds)...",
            ),
        }));
        spawn(
            info,
            app,
            s,
            Job::Encryption(EncryptionJob::SetUp { drive_id, auto }),
        );
    })
}

extern "C" fn on_sheet_done(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::Encryption(Dialog::Sheet(sheet))) = s.popup.as_mut() else {
            return;
        };
        if !sheet.confirmed() {
            sheet.error = format!(
                "That is not group {} of the code. Look at what you wrote down.",
                sheet.check + 1
            );
            return;
        }
        let drive_id = sheet.drive_id.clone();
        start_migration(info, app, s, &drive_id);
    })
}

/// The drive's plaintext files into the encryption, on a worker thread.
pub(crate) fn start_migration(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
) {
    let Some(auto) = auto_of(s, drive_id) else {
        return;
    };
    let state_file = state_file(s, drive_id);
    s.popup = Some(Popup::Encryption(Dialog::Busy {
        title: String::from("Moving the files into the encryption"),
        text: format!(
            "The files of \"{}\" are being encrypted. You can hide this: it continues in the \
             background, and where it stopped if AzDrive closes.",
            drive_name(s, drive_id)
        ),
    }));
    spawn(
        info,
        app,
        s,
        Job::Encryption(EncryptionJob::Migrate {
            drive_id: drive_id.to_string(),
            auto,
            state_file,
        }),
    );
}

extern "C" fn on_unlock(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::Encryption(Dialog::Unlock {
            drive_id,
            typed,
            error,
        })) = s.popup.as_mut()
        else {
            return;
        };
        let Some(code) = RecoveryCode::parse(typed) else {
            *error = String::from(
                "That is not a recovery code: 26 letters and digits, in five groups.",
            );
            return;
        };
        let drive_id = drive_id.clone();
        let Some(auto) = auto_of(s, &drive_id) else {
            return;
        };
        s.popup = Some(Popup::Encryption(Dialog::Busy {
            title: String::from("Unlocking the drive"),
            text: String::from("Opening the drive's key with the recovery code..."),
        }));
        spawn(
            info,
            app,
            s,
            Job::Encryption(EncryptionJob::Recover {
                drive_id,
                auto,
                code,
            }),
        );
    })
}

// ==== The jobs ====

/// One blocking encryption task.
pub(crate) enum EncryptionJob {
    /// The drive's keys and the recovery code.
    SetUp {
        drive_id: String,
        auto: Arc<AutoEncrypted>,
    },
    /// The recovery code opens the drive's key on this computer.
    Recover {
        drive_id: String,
        auto: Arc<AutoEncrypted>,
        code: RecoveryCode,
    },
    /// The plaintext files moved into the encryption, the state in `state_file`.
    Migrate {
        drive_id: String,
        auto: Arc<AutoEncrypted>,
        state_file: PathBuf,
    },
}

/// What an encryption task found.
pub(crate) enum EncryptionOutcome {
    /// The recovery code to show (`Err`: why there is none).
    SetUp {
        drive_id: String,
        result: Result<Zeroizing<String>, String>,
    },
    Recovered {
        drive_id: String,
        result: Result<(), String>,
    },
    Migrated {
        drive_id: String,
        result: Result<MigrationState, String>,
    },
}

/// Runs on a worker thread.
pub(crate) fn run(job: EncryptionJob) -> EncryptionOutcome {
    let keyring = AzulKeyring::new();
    match job {
        EncryptionJob::SetUp { drive_id, auto } => {
            let result = (|| -> Result<Zeroizing<String>, String> {
                if index_provider().is_none() {
                    return Err(String::from(
                        "this build of AzDrive has no drive index to open encrypted drives with",
                    ));
                }
                let kdf = RecoveryKdf::fresh().map_err(|e| e.to_string())?;
                let (_, code) =
                    device::setup_new_drive(auto.bucket().as_ref(), &keyring, auto.drive(), kdf)
                        .map_err(|e| e.to_string())?;
                auto.reopen();
                Ok(code.to_text())
            })();
            EncryptionOutcome::SetUp { drive_id, result }
        }
        EncryptionJob::Recover {
            drive_id,
            auto,
            code,
        } => {
            let result = device::recover(auto.bucket().as_ref(), &keyring, auto.drive(), &code)
                .map(|_| auto.reopen())
                .map_err(|e| e.to_string());
            EncryptionOutcome::Recovered { drive_id, result }
        }
        EncryptionJob::Migrate {
            drive_id,
            auto,
            state_file,
        } => {
            let result = (|| -> Result<MigrationState, String> {
                let provider = index_provider().ok_or_else(|| {
                    String::from("this build of AzDrive has no drive index to move files into")
                })?;
                let drive = open_encrypted(
                    Arc::clone(auto.bucket()),
                    &keyring,
                    auto.drive(),
                    provider.as_ref(),
                )
                .map_err(|e| e.to_string())?;
                let mut state = std::fs::read_to_string(&state_file)
                    .ok()
                    .and_then(|text| MigrationState::from_json(&text).ok())
                    .unwrap_or_default();
                if let Some(dir) = state_file.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                let mut save = |state: &MigrationState| {
                    std::fs::write(&state_file, state.to_json())
                        .map_err(|e| azul_storage::DriveError::Io(e.to_string()))
                };
                migrate(&drive, &mut state, &mut save, &|| false).map_err(|e| e.to_string())?;
                Ok(state)
            })();
            EncryptionOutcome::Migrated { drive_id, result }
        }
    }
}

/// The UI thread takes a task's answer.
pub(crate) fn on_outcome(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    outcome: EncryptionOutcome,
) {
    match outcome {
        EncryptionOutcome::SetUp { drive_id, result } => match result {
            Ok(code) => {
                s.popup = Some(Popup::Encryption(Dialog::Sheet(Sheet::new(&drive_id, code))));
            }
            Err(why) => {
                s.popup = Some(Popup::Encryption(Dialog::Message {
                    title: String::from("The drive was not encrypted"),
                    text: why,
                }));
            }
        },
        EncryptionOutcome::Recovered { drive_id, result } => match result {
            Ok(()) => {
                if matches!(s.popup, Some(Popup::Encryption(Dialog::Busy { .. }))) {
                    s.popup = None;
                }
                let name = drive_name(s, &drive_id);
                s.info(format!("\"{name}\" is unlocked on this computer."));
                crate::refresh(info, app, s);
            }
            Err(why) => {
                s.popup = Some(Popup::Encryption(Dialog::Message {
                    title: String::from("The drive was not unlocked"),
                    text: why,
                }));
            }
        },
        EncryptionOutcome::Migrated { drive_id, result } => {
            if matches!(s.popup, Some(Popup::Encryption(Dialog::Busy { .. }))) {
                s.popup = None;
            }
            let name = drive_name(s, &drive_id);
            match result {
                Ok(state) => {
                    let skipped = if state.skipped.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "; {} left as they were (other contents under their names, or \
                             names an encrypted drive cannot take)",
                            state.skipped.len()
                        )
                    };
                    let rest = if state.done {
                        ""
                    } else {
                        " Some changed meanwhile: \"Encrypt\" again moves them."
                    };
                    s.info(format!(
                        "{} files of \"{name}\" are encrypted{skipped}.{rest}",
                        state.moved
                    ));
                    crate::refresh(info, app, s);
                }
                Err(why) => s.error(format!(
                    "Moving the files of \"{name}\" into the encryption stopped: {why}. It \
                     continues where it stopped the next time."
                )),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_recovery_sheet_takes_the_group_it_asks_for_as_people_type_it() {
        let code = RecoveryCode::from_bytes([0x5A; 16]);
        let mut sheet = Sheet::new("d_1", code.to_text());
        assert!(sheet.check < 5, "one of five groups");
        let group = sheet
            .code
            .split('-')
            .nth(sheet.check)
            .unwrap()
            .to_string();
        sheet.typed = Zeroizing::new(group.to_lowercase());
        assert!(sheet.confirmed());
        sheet.typed = Zeroizing::new(format!(
            " {} ",
            group.replace('0', "O").replace('1', "l")
        ));
        assert!(sheet.confirmed(), "O for 0, l for 1, spaces around");
        sheet.typed = Zeroizing::new(String::from("WRONG"));
        assert!(!sheet.confirmed());
        sheet.typed = Zeroizing::new(String::new());
        assert!(!sheet.confirmed());
    }

    #[test]
    fn without_a_drive_index_the_flows_are_not_offered() {
        assert!(!offered());
    }
}
