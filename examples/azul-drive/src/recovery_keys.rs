//! A drive's several recovery keys in AzDrive (feature `encryption`; the token server's
//! recovery keys and the lookup by one: D51, F12, §18.8).
//!
//! A recovery code has two keys at the token server: the DRIVE KEY (derived with the drive id;
//! it signs lockdowns since the first recovery sheet) and the FINDABLE KEY (the code's alone,
//! `azcloud_kit::RecoveryKey::derive_findable`). The setup registers both. The findable one lets
//! a computer that never had the drive find it from the kit alone - the kit leaves the drive id
//! out on purpose:
//!
//! - Options > Drives > "Recover a drive with its emergency kit...": the code typed, the
//!   findable key signs a lookup challenge, the token server names the drive(s), and the code
//!   signs the same recovery-key lockdown as everywhere else: 48 hours for the owner's other
//!   devices to stop it (D42; the pending family gets nothing meanwhile). The recovery waits in
//!   the list "Recoveries under way" until "Finish" - after the 48 hours its first refresh
//!   hands the drive over, and the drive joins the source list; "Unlock with the recovery code"
//!   then opens it.
//! - "...with trusted contacts": two shares give the code back (`recovery_contacts.rs`), the
//!   lookup the drive.
//!
//! The drive's keys as the token server lists them show under its methods in Options > Drives
//! ("Check" lists them). Every change needs one of the drive's codes typed - the key that signs
//! is whichever of the code's two keys the drive has: "Add another recovery code..." (a second
//! kit: a wrap of its own in the bucket, its findable key at the token server, its own recovery
//! sheet), "Make the kit find this drive..." (the findable key of a code from before) and
//! "Remove" (never the last key: the token server answers `last_recovery_key`). The passkey
//! stays "later".
//!
//! On stdout, for scripts: `AZDRIVE_KIT_LOOKUP <drives found>`, `AZDRIVE_KIT_LOCKDOWN <drive>`,
//! `AZDRIVE_RECOVERY_FINISHED <drive>`, `AZDRIVE_RECOVERY_PENDING <drive>`,
//! `AZDRIVE_RECOVERY_KEYS <drive> <count>`, `AZDRIVE_RECOVERY_CODE_ADDED <drive>`,
//! `AZDRIVE_RECOVERY_FINDABLE <drive>`, `AZDRIVE_RECOVERY_KEY_REMOVED <drive> <key>`,
//! `AZDRIVE_RECOVERY_KEY_KEPT <drive> <key>` (the token server kept it: the last key).

use std::sync::Arc;

use azcloud_kit::{
    user_errors::Code, AzlinSession, RecoveryKey, RecoveryKeyInfo, SharedKeyring, TokenError,
    TokenServer, UserError,
};
use azul::{
    callbacks::{ButtonOnClickCallbackType, TextInputOnTextInputCallbackType},
    prelude::*,
    str::String as AzString,
    widgets::{ButtonType, OnTextInputReturn, TextInputState, TextInputValid},
};
use azul_storage::{
    azul_keyring::AzulKeyring,
    azul_transport::AzulTransport,
    config::{keyring_key, DriveEntry},
    crypto::{
        device,
        keys::{RecoveryCode, RecoveryKdf},
        Zeroizing,
    },
    encrypted::AutoEncrypted,
    time::now_unix,
};

use crate::{
    encryption::{Dialog, EncryptionJob, Sheet},
    ids,
    jobs::Job,
    recovery_health::{
        forget_pending, note_pending, state_mut, state_of, CodeKey, ExtraCode, RecoveryState,
        ServerKey, FINDABLE_LABEL,
    },
    save_settings, spawn,
    ui_dialogs::{button, buttons, label, line, typed_button},
    with_state, DriveState, Popup,
};

/// The name a drive recovered on a computer that never had it gets (the kit has none).
pub(crate) const RECOVERED_NAME: &str = "Recovered drive";
/// The label of a second kit's key at the token server.
pub(crate) const SECOND_KIT_LABEL: &str = "another recovery code";

/// A code's findable key (no drive in it).
pub(crate) fn findable_key_of(code: &RecoveryCode) -> RecoveryKey {
    RecoveryKey::derive_findable(code.as_bytes())
}

/// The key a typed code signs a change of the drive's recovery keys with: whichever of its two
/// keys the drive has (as last listed, else as registered here), or why none.
pub(crate) fn pick_signer(
    state: Option<&RecoveryState>,
    typed: &str,
    drive_id: &str,
) -> Result<RecoveryKey, String> {
    let code = RecoveryCode::parse(typed).ok_or_else(|| {
        String::from("That is not a recovery code: 26 letters and digits, in five groups.")
    })?;
    let drive = crate::encryption::recovery_key_of(&code, drive_id);
    let findable = findable_key_of(&code);
    let state = state.ok_or_else(|| {
        String::from("This computer knows no recovery keys of the drive: Check them first.")
    })?;
    match state.signer_for(&drive.public_base64(), &findable.public_base64()) {
        Some(CodeKey::Drive) => Ok(drive),
        Some(CodeKey::Findable) => Ok(findable),
        None => Err(String::from(
            "That is not a recovery code of this drive (none of its keys at the token server).",
        )),
    }
}

/// A token server's refusal as the user reads it (the D33 table), else as it is.
pub(crate) fn token_text(e: &TokenError) -> String {
    UserError::from_token_error(e)
        .filter(|user| user.code != Code::Other)
        .map_or_else(
            || e.to_string(),
            |user| user.message(crate::problems::lang()),
        )
}

// ==== The pages ====

/// What a change of a drive's keys does once a code of it is typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KeyAction {
    /// A second kit: a new code with a wrap of its own, its findable key registered.
    AddCode,
    /// The typed code's findable key registered (a drive set up before findable keys).
    MakeFindable,
    /// One key less (never the last).
    Remove { key_id: String, label: String },
}

/// The dialog's pages.
pub(crate) enum Page {
    /// "Recover a drive with its emergency kit": the code typed.
    Kit {
        typed: Zeroizing<String>,
        error: String,
    },
    /// The drives the kit's findable key belongs to: one to lock down.
    Found {
        code: Zeroizing<String>,
        drives: Vec<String>,
        error: String,
    },
    /// A change of a drive's keys that needs one of its codes.
    Sign {
        drive_id: String,
        action: KeyAction,
        typed: Zeroizing<String>,
        error: String,
    },
}

fn open(s: &mut DriveState, page: Page) {
    if s.popup.is_none() {
        s.popups_opened += 1;
    }
    s.popup = Some(Popup::Encryption(Dialog::Keys(page)));
}

/// "Recover a drive with its emergency kit...".
pub(crate) fn ask_kit(s: &mut DriveState) {
    open(
        s,
        Page::Kit {
            typed: Zeroizing::new(String::new()),
            error: String::new(),
        },
    );
}

fn ask_sign(s: &mut DriveState, drive_id: &str, action: KeyAction) {
    open(
        s,
        Page::Sign {
            drive_id: drive_id.to_string(),
            action,
            typed: Zeroizing::new(String::new()),
            error: String::new(),
        },
    );
}

/// Lists the drive's recovery keys at its token server, in the background.
pub(crate) fn check_keys(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
) {
    let Some(token_url) = crate::encryption::token_url_of(s, drive_id) else {
        s.error("The drive's token server is not known.");
        return;
    };
    let job = KeysJob::List {
        drive_id: drive_id.to_string(),
        token_url,
        keyring: s.keyring.clone(),
    };
    spawn(info, app, s, Job::Encryption(EncryptionJob::Keys(job)));
}

/// The page's error line.
fn set_error(s: &mut DriveState, why: String) {
    if let Some(Popup::Encryption(Dialog::Keys(
        Page::Kit { error, .. } | Page::Found { error, .. } | Page::Sign { error, .. },
    ))) = s.popup.as_mut()
    {
        *error = why;
    } else {
        s.error(why);
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
    if let Some(Popup::Encryption(Dialog::Keys(
        Page::Kit { typed, error } | Page::Sign { typed, error, .. },
    ))) = s.popup.as_mut()
    {
        *typed = text;
        error.clear();
    }
    keep
}

fn code_input(app: &RefAny, id: AzString) -> Dom {
    TextInput::create()
        .with_placeholder(AzString::from("XXXXX-XXXXX-XXXXX-XXXXX-XXXXXX"))
        .with_on_text_input(app.clone(), on_typed as TextInputOnTextInputCallbackType)
        .dom()
        .with_id(id)
}

fn column(children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; min-width: 440px; max-width: 540px;")
        .with_children(DomVec::from(children))
}

/// What a button of a found drive carries.
struct FoundRef {
    app: RefAny,
    index: usize,
}

/// The dialog's title and content.
pub(crate) fn dialog_parts(page: &Page, s: &DriveState, app: &RefAny) -> (String, Dom) {
    let red = |text: &str| line(text).with_css("color: #C42B1C;");
    match page {
        Page::Kit { error, .. } => {
            let mut body = column(vec![
                line(
                    "Type the recovery code from the drive's emergency kit. AzDrive finds the \
                     drive at its token server by the code alone, then locks it down for this \
                     computer: the drive's other devices are told and have 48 hours to stop \
                     it.",
                ),
                label("The recovery code"),
                code_input(app, ids::KIT_RECOVER_CODE),
            ]);
            if !error.is_empty() {
                body.add_child(red(error));
            }
            body.add_child(buttons(vec![
                button("Cancel", app, crate::ui_dialogs::on_cancel_popup),
                typed_button("Find the drive", ButtonType::Primary, app, on_kit_find)
                    .with_id(ids::KIT_RECOVER_FIND),
            ]));
            (String::from("Recover a drive with its emergency kit"), body)
        }
        Page::Found { drives, error, .. } => {
            let mut body = column(vec![line(if drives.len() == 1 {
                "The kit's code belongs to this drive. Lock it down for this computer? Its \
                 other devices are told and may stop it within 48 hours; then the drive is \
                 this computer's."
            } else {
                "The kit's code belongs to these drives. Lock one down for this computer?"
            })]);
            for (index, drive_id) in drives.iter().enumerate() {
                body.add_child(
                    Dom::create_div()
                        .with_css(
                            "display: flex; flex-direction: row; align-items: center; \
                             margin-top: 6px;",
                        )
                        .with_child(
                            Dom::create_span_with_text(AzString::from(drive_id.as_str()))
                                .with_css("font-family: monospace; flex-grow: 1;"),
                        )
                        .with_child(
                            Button::with_type(AzString::from("Lock down"), ButtonType::Primary)
                                .with_on_click(
                                    RefAny::new(FoundRef {
                                        app: app.clone(),
                                        index,
                                    }),
                                    on_found_lockdown as ButtonOnClickCallbackType,
                                )
                                .dom()
                                .with_id(ids::kit_recover_lockdown(index)),
                        ),
                );
            }
            if !error.is_empty() {
                body.add_child(red(error));
            }
            body.add_child(buttons(vec![button(
                "Cancel",
                app,
                crate::ui_dialogs::on_cancel_popup,
            )]));
            (String::from("The kit's drive"), body)
        }
        Page::Sign {
            drive_id,
            action,
            error,
            ..
        } => {
            let name = s.drive_name(&crate::browse::Place::folder(drive_id, ""));
            let (title, text, ok) = match action {
                KeyAction::AddCode => (
                    format!("Another recovery code for \"{name}\""),
                    String::from(
                        "A second emergency kit: a new recovery code that opens the drive as \
                         the first does - for a kit kept in another place. Type a recovery \
                         code the drive has now; then the new one shows once, to write down.",
                    ),
                    "Make the code",
                ),
                KeyAction::MakeFindable => (
                    format!("Let the kit find \"{name}\""),
                    String::from(
                        "On a computer that never had the drive, its kit finds it only by a \
                         findable key. Type the recovery code to register its findable key.",
                    ),
                    "Make findable",
                ),
                KeyAction::Remove { label, .. } => (
                    format!("Remove a recovery key of \"{name}\""),
                    format!(
                        "\"{label}\" stops locking the drive down. Type a recovery code the \
                         drive has to confirm. The drive's last key always stays."
                    ),
                    "Remove",
                ),
            };
            let mut body = column(vec![
                line(&text),
                label("A recovery code of the drive"),
                code_input(app, ids::RECOVERY_SIGN_CODE),
            ]);
            if !error.is_empty() {
                body.add_child(red(error));
            }
            body.add_child(buttons(vec![
                button("Cancel", app, crate::ui_dialogs::on_cancel_popup),
                typed_button(ok, ButtonType::Primary, app, on_sign_ok)
                    .with_id(ids::RECOVERY_SIGN_OK),
            ]));
            (title, body)
        }
    }
}

// ==== Options > Drives ====

/// What a button of the Options carries: the drive and what it does.
struct KeysRef {
    app: RefAny,
    drive_id: String,
    what: KeysButton,
}

#[derive(Clone)]
enum KeysButton {
    Check,
    AddCode,
    MakeFindable,
    Remove { key_id: String, label: String },
    Finish,
}

fn keys_button(app: &RefAny, drive_id: &str, what: KeysButton, text: &str, id: AzString) -> Dom {
    Button::create(AzString::from(text))
        .with_on_click(
            RefAny::new(KeysRef {
                app: app.clone(),
                drive_id: drive_id.to_string(),
                what,
            }),
            on_keys_button as ButtonOnClickCallbackType,
        )
        .dom()
        .with_id(id)
        .with_css("margin-left: 6px;")
}

fn day(at: u64) -> String {
    let text = azul_storage::time::iso8601(at);
    text.get(..10).unwrap_or(&text).to_string()
}

/// A drive's recovery keys at the token server, under its methods in Options > Drives.
pub(crate) fn keys_block(state: &RecoveryState, app: &RefAny) -> Dom {
    let drive_id = state.drive_id.as_str();
    let small = "font-size: 12px; opacity: 0.75;";
    let mut block = Dom::create_div()
        .with_css("display: flex; flex-direction: column; margin-top: 6px;")
        .with_id(ids::recovery_keys(drive_id));
    let mut head = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center;")
        .with_child(
            Dom::create_span_with_text(AzString::from(match state.keys_checked {
                Some(at) => format!(
                    "Recovery keys at the token server ({}, listed on {})",
                    state.server_keys.len(),
                    day(at)
                ),
                None => String::from("Recovery keys at the token server: not listed yet"),
            }))
            .with_css("flex-grow: 1;"),
        )
        .with_child(keys_button(
            app,
            drive_id,
            KeysButton::Check,
            if state.keys_checked.is_some() {
                "Check again"
            } else {
                "Check"
            },
            ids::recovery_keys_check(drive_id),
        ));
    if state.keys_checked.is_some() {
        head.add_child(keys_button(
            app,
            drive_id,
            KeysButton::AddCode,
            "Add another recovery code\u{2026}",
            ids::recovery_keys_add(drive_id),
        ));
    }
    block.add_child(head);
    for key in &state.server_keys {
        let added = key.created_at.map_or_else(
            || String::from("from before"),
            |at| format!("added on {}", day(at)),
        );
        let verified = if key.verified {
            ""
        } else {
            ", not verified yet"
        };
        block.add_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; margin-top: 2px;",
                )
                .with_child(
                    Dom::create_span_with_text(AzString::from(format!(
                        "{} - {added}{verified}",
                        key.label
                    )))
                    .with_css(format!("flex-grow: 1; {small}")),
                )
                .with_child(keys_button(
                    app,
                    drive_id,
                    KeysButton::Remove {
                        key_id: key.key_id.clone(),
                        label: key.label.clone(),
                    },
                    "Remove\u{2026}",
                    ids::recovery_key_remove(drive_id, &key.key_id),
                )),
        );
    }
    if state.findable_missing() {
        block.add_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; margin-top: 4px;",
                )
                .with_child(
                    line(
                        "A computer that never had this drive cannot find it from its kit: no \
                         key finds it.",
                    )
                    .with_css(format!("flex-grow: 1; color: #9D5D00; {small}")),
                )
                .with_child(keys_button(
                    app,
                    drive_id,
                    KeysButton::MakeFindable,
                    "Make the kit find it\u{2026}",
                    ids::recovery_keys_findable(drive_id),
                )),
        );
    }
    block
}

/// Options > Drives' doors for a drive this computer never had, and the recoveries under way.
pub(crate) fn recover_block(s: &DriveState, app: &RefAny) -> Dom {
    let mut block = Dom::create_div()
        .with_css("display: flex; flex-direction: column; padding: 6px 0px;")
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center;")
                .with_child(
                    Dom::create_span_with_text(AzString::from("A drive this computer never had:"))
                        .with_css("flex-grow: 1;"),
                )
                .with_child(
                    button("Recover with its emergency kit\u{2026}", app, on_kit_open)
                        .with_id(ids::KIT_RECOVER),
                )
                .with_child(
                    button(
                        "Recover with trusted contacts\u{2026}",
                        app,
                        on_contacts_open,
                    )
                    .with_id(ids::CONTACTS_RECOVER_NEW),
                ),
        );
    for pending in &s.settings.recovery.pending {
        let until = pending.until.map_or_else(String::new, |at| {
            format!(" until {}", azul_storage::time::iso8601(at))
        });
        block.add_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; margin-top: 4px;",
                )
                .with_child(
                    Dom::create_span_with_text(AzString::from(format!(
                        "Recovering {}: its lockdown is pending{until}. Then Finish adds the \
                         drive here.",
                        pending.drive_id
                    )))
                    .with_css("flex-grow: 1; font-size: 12px;"),
                )
                .with_child(keys_button(
                    app,
                    &pending.drive_id,
                    KeysButton::Finish,
                    "Finish",
                    ids::recovery_finish(&pending.drive_id),
                )),
        );
    }
    block
}

extern "C" fn on_kit_open(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| ask_kit(s))
}

extern "C" fn on_contacts_open(mut data: RefAny, mut info: CallbackInfo) -> Update {
    // The drive is found by the code the shares give back.
    with_state(&mut data, &mut info, |info, app, s| {
        crate::recovery_contacts::ask_recover(info, app, s, "", false);
    })
}

extern "C" fn on_keys_button(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, drive_id, what)) = data
        .downcast_ref::<KeysRef>()
        .map(|k| (k.app.clone(), k.drive_id.clone(), k.what.clone()))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| match what {
        KeysButton::Check => check_keys(info, app, s, &drive_id),
        KeysButton::AddCode => ask_sign(s, &drive_id, KeyAction::AddCode),
        KeysButton::MakeFindable => ask_sign(s, &drive_id, KeyAction::MakeFindable),
        KeysButton::Remove { key_id, label } => {
            ask_sign(s, &drive_id, KeyAction::Remove { key_id, label });
        }
        KeysButton::Finish => {
            let Some(pending) = s
                .settings
                .recovery
                .pending
                .iter()
                .find(|p| p.drive_id == drive_id)
                .cloned()
            else {
                return;
            };
            let job = KeysJob::Finish {
                drive_id,
                token_url: pending.token_url,
                keyring: s.keyring.clone(),
            };
            spawn(info, app, s, Job::Encryption(EncryptionJob::Keys(job)));
        }
    })
}

// ==== The pages' buttons ====

extern "C" fn on_kit_find(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::Encryption(Dialog::Keys(Page::Kit { typed, .. }))) = s.popup.as_ref()
        else {
            return;
        };
        let Some(code) = RecoveryCode::parse(typed) else {
            set_error(
                s,
                String::from("That is not a recovery code: 26 letters and digits, in five groups."),
            );
            return;
        };
        let Some(token_url) = s.token.url.clone() else {
            set_error(s, String::from("This AzDrive knows no Azlin token server."));
            return;
        };
        let job = KeysJob::Lookup { code, token_url };
        spawn(info, app, s, Job::Encryption(EncryptionJob::Keys(job)));
    })
}

extern "C" fn on_found_lockdown(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data
        .downcast_ref::<FoundRef>()
        .map(|f| (f.app.clone(), f.index))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| {
        let Some(Popup::Encryption(Dialog::Keys(Page::Found { code, drives, .. }))) =
            s.popup.as_ref()
        else {
            return;
        };
        let (Some(drive_id), Some(code)) = (drives.get(index).cloned(), RecoveryCode::parse(code))
        else {
            return;
        };
        let Some(token_url) = s.token.url.clone() else {
            set_error(s, String::from("This AzDrive knows no Azlin token server."));
            return;
        };
        let job = KeysJob::Lockdown {
            drive_id,
            code,
            token_url,
            keyring: s.keyring.clone(),
        };
        spawn(info, app, s, Job::Encryption(EncryptionJob::Keys(job)));
    })
}

extern "C" fn on_sign_ok(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::Encryption(Dialog::Keys(Page::Sign {
            drive_id,
            action,
            typed,
            ..
        }))) = s.popup.as_ref()
        else {
            return;
        };
        let (drive_id, action) = (drive_id.clone(), action.clone());
        let typed = Zeroizing::new(typed.as_str().to_string());
        let signer = match pick_signer(
            state_of(&s.settings.recovery.drives, &drive_id),
            &typed,
            &drive_id,
        ) {
            Ok(signer) => signer,
            Err(why) => {
                set_error(s, why);
                return;
            }
        };
        let Some(token_url) = crate::encryption::token_url_of(s, &drive_id) else {
            set_error(s, String::from("The drive's token server is not known."));
            return;
        };
        let keyring = s.keyring.clone();
        let job = match action {
            KeyAction::AddCode => {
                let Some(auto) = crate::encryption::auto_for(s, &drive_id) else {
                    return;
                };
                KeysJob::AddCode {
                    drive_id,
                    signer,
                    auto,
                    token_url,
                    keyring,
                }
            }
            KeyAction::MakeFindable => {
                let Some(code) = RecoveryCode::parse(&typed) else {
                    return;
                };
                KeysJob::MakeFindable {
                    drive_id,
                    signer,
                    findable_public: findable_key_of(&code).public_base64(),
                    token_url,
                    keyring,
                }
            }
            KeyAction::Remove { key_id, .. } => {
                let extra_file = state_of(&s.settings.recovery.drives, &drive_id)
                    .and_then(|state| state.extra_code(&key_id))
                    .map(|code| code.file.clone());
                let auto = if extra_file.is_some() {
                    crate::encryption::auto_for(s, &drive_id)
                } else {
                    None
                };
                KeysJob::Remove {
                    drive_id,
                    key_id,
                    signer,
                    extra_file,
                    auto,
                    token_url,
                    keyring,
                }
            }
        };
        spawn(info, app, s, Job::Encryption(EncryptionJob::Keys(job)));
    })
}

// ==== The jobs ====

/// One blocking task of the recovery keys (the token server, the bucket, the keyring).
pub(crate) enum KeysJob {
    /// The drives the code's findable key belongs to.
    Lookup {
        code: RecoveryCode,
        token_url: String,
    },
    /// The recovery-key lockdown of a drive the lookup named.
    Lockdown {
        drive_id: String,
        code: RecoveryCode,
        token_url: String,
        keyring: SharedKeyring,
    },
    /// After the 48 hours: the first refresh hands the drive over; its entry.
    Finish {
        drive_id: String,
        token_url: String,
        keyring: SharedKeyring,
    },
    /// The drive's keys as the token server lists them.
    List {
        drive_id: String,
        token_url: String,
        keyring: SharedKeyring,
    },
    /// A second kit: a wrap of its own, its findable key registered (signed by `signer`).
    AddCode {
        drive_id: String,
        signer: RecoveryKey,
        auto: Arc<AutoEncrypted>,
        token_url: String,
        keyring: SharedKeyring,
    },
    /// A code's findable key registered.
    MakeFindable {
        drive_id: String,
        signer: RecoveryKey,
        findable_public: String,
        token_url: String,
        keyring: SharedKeyring,
    },
    /// One key less; a second kit's wrap goes with its key.
    Remove {
        drive_id: String,
        key_id: String,
        signer: RecoveryKey,
        extra_file: Option<String>,
        auto: Option<Arc<AutoEncrypted>>,
        token_url: String,
        keyring: SharedKeyring,
    },
}

/// What a task of the recovery keys found.
pub(crate) enum KeysDone {
    Found {
        code: Zeroizing<String>,
        result: Result<Vec<String>, String>,
    },
    LockedDown {
        drive_id: String,
        token_url: String,
        result: Result<Option<u64>, String>,
    },
    Finished {
        drive_id: String,
        result: Result<(DriveEntry, String), String>,
    },
    Listed {
        drive_id: String,
        result: Result<Vec<RecoveryKeyInfo>, String>,
    },
    /// A second kit: its code (for its sheet), its key id, its wrap's file.
    CodeAdded {
        drive_id: String,
        result: Result<(Zeroizing<String>, String, String), String>,
    },
    MadeFindable {
        drive_id: String,
        findable_public: String,
        result: Result<(), String>,
    },
    Removed {
        drive_id: String,
        key_id: String,
        result: Result<(), TokenError>,
    },
}

/// A grant of the drive (its newest drive token, under its keyring lock).
fn with_token<T>(
    keyring: &SharedKeyring,
    drive_id: &str,
    call: impl FnOnce(&str) -> Result<T, TokenError>,
) -> Result<T, TokenError> {
    keyring
        .with_drive_token(drive_id, call)
        .map_err(|e| TokenError::Config(e.to_string()))?
}

/// The drive's entry and session after the hand-over: the pending session's token refreshed.
fn finish(
    drive_id: &str,
    server: &TokenServer<'_>,
    keyring: &SharedKeyring,
    token_url: &str,
) -> Result<(DriveEntry, String), String> {
    let entry_key = keyring_key(drive_id);
    let _lock = keyring.lock(&entry_key).map_err(|e| e.to_string())?;
    let session = keyring
        .get(&entry_key)
        .map_err(|e| e.to_string())?
        .map(|text| AzlinSession::from_keyring_secret(&text))
        .transpose()
        .map_err(|e| e.to_string())?
        .filter(|session| session.drive_id == drive_id && !session.drive_token.is_empty())
        .ok_or_else(|| String::from("this computer keeps no session of the recovery"))?;
    let bundle = server
        .refresh(drive_id, &session.drive_token)
        .map_err(|e| token_text(&e))?;
    let text = bundle.session().to_keyring_secret();
    keyring.set(&entry_key, &text).map_err(|e| e.to_string())?;
    Ok((bundle.entry_named(RECOVERED_NAME, token_url), text))
}

/// Runs on a worker thread.
pub(crate) fn run(job: KeysJob) -> KeysDone {
    let transport = AzulTransport::new(crate::USER_AGENT);
    match job {
        KeysJob::Lookup { code, token_url } => {
            let key = findable_key_of(&code);
            let result = TokenServer::new(&token_url, &transport)
                .and_then(|server| {
                    server.recovery_lookup(&key.public_base64(), |message| {
                        Ok(key.sign_base64(message))
                    })
                })
                .map(|found| found.into_iter().map(|d| d.drive_id).collect())
                .map_err(|e| token_text(&e));
            KeysDone::Found {
                code: code.to_text(),
                result,
            }
        }
        KeysJob::Lockdown {
            drive_id,
            code,
            token_url,
            keyring,
        } => {
            let result =
                crate::encryption::recovery_lockdown(&drive_id, &code, &token_url, &keyring);
            KeysDone::LockedDown {
                drive_id,
                token_url,
                result,
            }
        }
        KeysJob::Finish {
            drive_id,
            token_url,
            keyring,
        } => {
            let result = TokenServer::new(&token_url, &transport)
                .map_err(|e| e.to_string())
                .and_then(|server| finish(&drive_id, &server, &keyring, &token_url));
            KeysDone::Finished { drive_id, result }
        }
        KeysJob::List {
            drive_id,
            token_url,
            keyring,
        } => {
            let result = TokenServer::new(&token_url, &transport)
                .and_then(|server| {
                    with_token(&keyring, &drive_id, |token| {
                        server.recovery_keys(&drive_id, token)
                    })
                })
                .map_err(|e| token_text(&e));
            KeysDone::Listed { drive_id, result }
        }
        KeysJob::AddCode {
            drive_id,
            signer,
            auto,
            token_url,
            keyring,
        } => {
            let result = (|| -> Result<(Zeroizing<String>, String, String), String> {
                let server = TokenServer::new(&token_url, &transport).map_err(|e| e.to_string())?;
                let kdf = RecoveryKdf::fresh().map_err(|e| e.to_string())?;
                let (file, code) = device::add_recovery_code(
                    auto.bucket().as_ref(),
                    &AzulKeyring::new(),
                    auto.drive(),
                    kdf,
                )
                .map_err(|e| e.to_string())?;
                let findable = findable_key_of(&code);
                let added = with_token(&keyring, &drive_id, |token| {
                    server.add_recovery_key(
                        &drive_id,
                        token,
                        &findable.public_base64(),
                        SECOND_KIT_LABEL,
                        |message| Ok(signer.sign_base64(message)),
                    )
                });
                match added {
                    Ok(key) => Ok((code.to_text(), key.key_id, file)),
                    Err(e) => {
                        // No key at the token server: no second kit in the bucket either.
                        let _ = device::remove_recovery_code(auto.bucket().as_ref(), &file);
                        Err(token_text(&e))
                    }
                }
            })();
            KeysDone::CodeAdded { drive_id, result }
        }
        KeysJob::MakeFindable {
            drive_id,
            signer,
            findable_public,
            token_url,
            keyring,
        } => {
            let result = TokenServer::new(&token_url, &transport)
                .and_then(|server| {
                    with_token(&keyring, &drive_id, |token| {
                        server.add_recovery_key(
                            &drive_id,
                            token,
                            &findable_public,
                            FINDABLE_LABEL,
                            |message| Ok(signer.sign_base64(message)),
                        )
                    })
                })
                .map(|_| ())
                .map_err(|e| token_text(&e));
            KeysDone::MadeFindable {
                drive_id,
                findable_public,
                result,
            }
        }
        KeysJob::Remove {
            drive_id,
            key_id,
            signer,
            extra_file,
            auto,
            token_url,
            keyring,
        } => {
            let result = TokenServer::new(&token_url, &transport).and_then(|server| {
                with_token(&keyring, &drive_id, |token| {
                    server.remove_recovery_key(&drive_id, token, &key_id, |message| {
                        Ok(signer.sign_base64(message))
                    })
                })
            });
            if result.is_ok() {
                if let (Some(file), Some(auto)) = (extra_file, auto) {
                    let _ = device::remove_recovery_code(auto.bucket().as_ref(), &file);
                }
            }
            KeysDone::Removed {
                drive_id,
                key_id,
                result,
            }
        }
    }
}

/// The UI thread takes a task's answer.
pub(crate) fn on_done(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, done: KeysDone) {
    let now = now_unix();
    match done {
        KeysDone::Found { code, result } => match result {
            Ok(drives) => {
                println!("AZDRIVE_KIT_LOOKUP {}", drives.len());
                if drives.is_empty() {
                    set_error(
                        s,
                        String::from(
                            "No drive has this code as a key that finds it. A kit made before \
                             AzDrive registered findable keys finds its drive only after a \
                             computer that has the drive lets it (Options > Drives > Make the \
                             kit find it).",
                        ),
                    );
                } else {
                    open(
                        s,
                        Page::Found {
                            code,
                            drives,
                            error: String::new(),
                        },
                    );
                }
            }
            Err(why) => set_error(s, why),
        },
        KeysDone::LockedDown {
            drive_id,
            token_url,
            result,
        } => match result {
            Ok(until) => {
                println!("AZDRIVE_KIT_LOCKDOWN {drive_id}");
                println!("AZDRIVE_RECOVERY_LOCKDOWN {drive_id}");
                if s.slot_index(&drive_id).is_none() {
                    note_pending(
                        &mut s.settings.recovery.pending,
                        &drive_id,
                        &token_url,
                        until,
                        now,
                    );
                    println!("AZDRIVE_RECOVERY_PENDING {drive_id}");
                }
                let until = until.map_or_else(String::new, |at| {
                    format!(" until {}", azul_storage::time::iso8601(at))
                });
                s.popup = Some(Popup::Encryption(Dialog::Message {
                    title: String::from("The drive is locked down for this computer"),
                    text: format!(
                        "The lockdown with the kit's code is pending{until}: the drive's other \
                         devices are told and may stop it. Then Options > Drives > Finish adds \
                         the drive here, and \"Unlock with the recovery code\" opens it."
                    ),
                }));
                save_settings(info, app, s);
            }
            Err(why) => set_error(s, why),
        },
        KeysDone::Finished { drive_id, result } => match result {
            Ok((entry, secret)) => {
                forget_pending(&mut s.settings.recovery.pending, &drive_id);
                crate::add_flow::add_slot(info, app, s, entry, Some(secret), false, true);
                println!("AZDRIVE_RECOVERY_FINISHED {drive_id}");
                s.success(format!(
                    "{drive_id} is this computer's now. Open it with the kit's code: its menu > \
                     Unlock with the recovery code."
                ));
                save_settings(info, app, s);
            }
            Err(why) => s.warn(format!("The recovery of {drive_id} is not finished: {why}")),
        },
        KeysDone::Listed { drive_id, result } => match result {
            Ok(keys) => {
                println!("AZDRIVE_RECOVERY_KEYS {drive_id} {}", keys.len());
                let state = state_mut(&mut s.settings.recovery.drives, &drive_id);
                state.server_keys = keys
                    .into_iter()
                    .map(|key| ServerKey {
                        key_id: key.key_id,
                        label: key.label,
                        recovery_pubkey: key.recovery_pubkey,
                        created_at: key.created_at,
                        verified: key.verified,
                    })
                    .collect();
                state.keys_checked = Some(now);
                save_settings(info, app, s);
            }
            Err(why) => s.warn(format!("The recovery keys were not listed: {why}")),
        },
        KeysDone::CodeAdded { drive_id, result } => match result {
            Ok((code, key_id, file)) => {
                println!("AZDRIVE_RECOVERY_CODE_ADDED {drive_id}");
                state_mut(&mut s.settings.recovery.drives, &drive_id)
                    .extra_codes
                    .push(ExtraCode {
                        key_id,
                        file,
                        made: now,
                    });
                s.popup = Some(Popup::Encryption(Dialog::Sheet(
                    Sheet::new(&drive_id, code).extra(),
                )));
                save_settings(info, app, s);
                check_keys(info, app, s, &drive_id);
            }
            Err(why) => set_error(s, why),
        },
        KeysDone::MadeFindable {
            drive_id,
            findable_public,
            result,
        } => match result {
            Ok(()) => {
                println!("AZDRIVE_RECOVERY_FINDABLE {drive_id}");
                state_mut(&mut s.settings.recovery.drives, &drive_id).findable_key =
                    Some(findable_public);
                s.popup = None;
                s.success("Its kit finds the drive now, on any computer.");
                save_settings(info, app, s);
                check_keys(info, app, s, &drive_id);
            }
            Err(why) => set_error(s, why),
        },
        KeysDone::Removed {
            drive_id,
            key_id,
            result,
        } => match result {
            Ok(()) => {
                println!("AZDRIVE_RECOVERY_KEY_REMOVED {drive_id} {key_id}");
                let state = state_mut(&mut s.settings.recovery.drives, &drive_id);
                state.extra_codes.retain(|code| code.key_id != key_id);
                state.server_keys.retain(|key| key.key_id != key_id);
                s.popup = None;
                save_settings(info, app, s);
                check_keys(info, app, s, &drive_id);
            }
            Err(e) => {
                if matches!(&e, TokenError::Refused { code, .. } if code == "last_recovery_key") {
                    println!("AZDRIVE_RECOVERY_KEY_KEPT {drive_id} {key_id}");
                }
                set_error(s, token_text(&e));
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code() -> RecoveryCode {
        RecoveryCode::from_bytes([0x5A; 16])
    }

    #[test]
    fn a_typed_code_picks_the_key_the_drive_has_or_says_why_not() {
        let mut state = RecoveryState::new("d_1");
        let drive = crate::encryption::recovery_key_of(&code(), "d_1").public_base64();
        let findable = findable_key_of(&code()).public_base64();
        assert_ne!(drive, findable);
        state.recovery_key = Some(drive.clone());
        let typed = code().to_text().to_lowercase();
        let signer = pick_signer(Some(&state), &typed, "d_1").unwrap();
        assert_eq!(signer.public_base64(), drive);
        state.server_keys = vec![ServerKey {
            key_id: String::from("rk_2"),
            label: String::from(FINDABLE_LABEL),
            recovery_pubkey: findable.clone(),
            created_at: None,
            verified: true,
        }];
        state.keys_checked = Some(1);
        let signer = pick_signer(Some(&state), &typed, "d_1").unwrap();
        assert_eq!(
            signer.public_base64(),
            findable,
            "only the findable key is listed"
        );
        let other = RecoveryCode::from_bytes([0x11; 16]).to_text();
        assert!(pick_signer(Some(&state), &other, "d_1")
            .unwrap_err()
            .contains("not a recovery code of this drive"));
        assert!(pick_signer(Some(&state), "nonsense", "d_1")
            .unwrap_err()
            .contains("not a recovery code"));
        assert!(pick_signer(None, &typed, "d_1").is_err());
    }

    #[test]
    fn the_token_servers_recovery_refusals_read_as_the_table_says() {
        let refused = |code: &str| TokenError::Refused {
            status: 409,
            code: code.to_string(),
            message: String::from("raw"),
        };
        // The table's row, in the language of this computer's locale (English or German).
        let last = token_text(&refused("last_recovery_key"));
        assert!(
            last.contains("last recovery key") || last.contains("letzte Wiederherstellungsschl"),
            "{last}"
        );
        assert!(
            token_text(&refused("frobnicated")).contains("raw"),
            "as it is otherwise"
        );
    }
}
