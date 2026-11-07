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
//! `AZDRIVE_RECOVERY_FINISHED <drive>`, `AZDRIVE_RECOVERY_NOT_FINISHED <drive> <why>`,
//! `AZDRIVE_RECOVERY_PENDING <drive>`,
//! `AZDRIVE_RECOVERY_KEYS <drive> <count>`, `AZDRIVE_RECOVERY_CODE_ADDED <drive>`,
//! `AZDRIVE_RECOVERY_FINDABLE <drive>`, `AZDRIVE_RECOVERY_KEY_REMOVED <drive> <key>`,
//! `AZDRIVE_RECOVERY_KEY_KEPT <drive> <key>` (the token server kept it: the last key),
//! `AZDRIVE_RECOVERY_KEY_REFUSED <drive> <key> <why>` (any other failed removal),
//! `AZDRIVE_RECOVERY_KEY_WRAP_KEPT <drive> <key> <why>` (the kit's wrap could not be deleted: nothing
//! was removed), `AZDRIVE_RECOVERY_COPIES_LEFT <drive> <count>` (after a removal: copies of
//! recovery codes in the storage that no key the token server lists claims).

use std::sync::Arc;

use azcloud_kit::{
    azlin_contract::{IdempotencyKey, IdempotentScope},
    user_errors::{Code, Lang},
    RecoveryKey, RecoveryKeyInfo, SharedKeyring, TokenError, TokenServer, UserError,
};
use azul::{
    callbacks::{ButtonOnClickCallbackType, TextInputOnTextInputCallbackType},
    prelude::*,
    str::String as AzString,
    widgets::{ButtonType, OnTextInputReturn, TextInputState, TextInputValid},
};
use azul_appkit::l10n::{t, t_args, t_label, Arg, Phrase};
use azul_storage::{
    azul_keyring::azlin_keyring,
    azul_transport::AzulTransport,
    config::DriveEntry,
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

/// The label of a second kit's key at the token server (the server keeps it as it is; the
/// window says [`key_label`]).
pub(crate) const SECOND_KIT_LABEL: &str = "another recovery code";

/// A key's label at the token server as the window says it: AzDrive's own labels in the window's
/// language, any other as it is.
pub(crate) fn key_label(label: &str) -> String {
    match label {
        SECOND_KIT_LABEL => t("azdrive-keys-label-second-kit"),
        FINDABLE_LABEL => t("azdrive-keys-label-findable"),
        _ => label.to_string(),
    }
}

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
    let code = RecoveryCode::parse(typed).ok_or_else(|| t("azdrive-drill-not-a-code"))?;
    let drive = crate::encryption::recovery_key_of(&code, drive_id);
    let findable = findable_key_of(&code);
    let state = state.ok_or_else(|| t("azdrive-keys-none-known"))?;
    match state.signer_for(&drive.public_base64(), &findable.public_base64()) {
        Some(CodeKey::Drive) => Ok(drive),
        Some(CodeKey::Findable) => Ok(findable),
        None => Err(t("azdrive-keys-not-this-drives")),
    }
}

/// A token server's refusal as the user reads it (the D33 table), else as it is - on a worker
/// thread: the table's message as its key (said on the UI thread by `t_label`), in English when
/// it has arguments.
pub(crate) fn token_text(e: &TokenError) -> String {
    UserError::from_token_error(e)
        .filter(|user| user.code != Code::Other)
        .map_or_else(
            || e.to_string(),
            |user| {
                // `detail` is every message's argument; only some rows say it.
                if user.code.row().en.contains("{detail}") {
                    user.message(Lang::En)
                } else {
                    user.message_id()
                }
            },
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
    s.popup.open(Popup::Encryption(Dialog::Keys(page)));
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

/// "Add another recovery code...", the user's presence checked: the page that asks for one of
/// the drive's codes to sign the new one.
pub(crate) fn ask_add_code(s: &mut DriveState, drive_id: &str) {
    ask_sign(s, drive_id, KeyAction::AddCode);
}

/// "Remove" of recovery key `key_id` (`label`), the user's presence checked: the page that asks
/// for one of the drive's codes to sign the removal.
pub(crate) fn ask_remove_code(s: &mut DriveState, drive_id: &str, key_id: String, label: String) {
    ask_sign(s, drive_id, KeyAction::Remove { key_id, label });
}

/// Lists the drive's recovery keys at its token server, in the background.
pub(crate) fn check_keys(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
) {
    let keyring = s.keyring.clone();
    list_keys(info, app, s, drive_id, keyring);
}

/// [`check_keys`] for the window's own work (the start, the daily look): its keyring reads
/// never show a prompt.
pub(crate) fn check_keys_quietly(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
) {
    let keyring = s.quiet_keyring.clone();
    list_keys(info, app, s, drive_id, keyring);
}

fn list_keys(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    keyring: azcloud_kit::SharedKeyring,
) {
    let Some(token_url) = crate::encryption::token_url_of(s, drive_id) else {
        s.error(Phrase::new("azdrive-no-token-server"));
        return;
    };
    let job = KeysJob::List {
        drive_id: drive_id.to_string(),
        token_url,
        keyring,
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
    let red = |text: &str| line(text).with_css("color: system:danger;");
    match page {
        Page::Kit { error, .. } => {
            let mut body = column(vec![
                line("azdrive-keys-kit-what"),
                label("azdrive-drill-code"),
                code_input(app, ids::KIT_RECOVER_CODE),
            ]);
            if !error.is_empty() {
                body.add_child(red(error));
            }
            body.add_child(buttons(vec![
                button("kit-button-cancel", app, crate::ui_dialogs::on_cancel_popup),
                typed_button("azdrive-keys-find", ButtonType::Primary, app, on_kit_find)
                    .with_id(ids::KIT_RECOVER_FIND),
            ]));
            (t("azdrive-keys-kit-title"), body)
        }
        Page::Found { drives, error, .. } => {
            let mut body = column(vec![line(if drives.len() == 1 {
                "azdrive-keys-found-one"
            } else {
                "azdrive-keys-found-several"
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
                            Button::with_type(
                                AzString::from(t("azdrive-enc-lock-down")),
                                ButtonType::Primary,
                            )
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
                "kit-button-cancel",
                app,
                crate::ui_dialogs::on_cancel_popup,
            )]));
            (t("azdrive-keys-found-title"), body)
        }
        Page::Sign {
            drive_id,
            action,
            error,
            ..
        } => {
            let name = s.drive_name(&crate::browse::Place::folder(drive_id, ""));
            let named = |key: &str| t_args(key, &[("name", Arg::from(name.as_str()))]);
            let (title, text, ok) = match action {
                KeyAction::AddCode => (
                    named("azdrive-keys-add-title"),
                    t("azdrive-keys-add-what"),
                    "azdrive-keys-add-button",
                ),
                KeyAction::MakeFindable => (
                    named("azdrive-keys-findable-title"),
                    t("azdrive-keys-findable-what"),
                    "azdrive-keys-findable-button",
                ),
                KeyAction::Remove { label, .. } => (
                    named("azdrive-keys-remove-title"),
                    t_args(
                        "azdrive-keys-remove-what",
                        &[("label", Arg::from(key_label(label)))],
                    ),
                    "azdrive-method-remove",
                ),
            };
            let mut body = column(vec![
                line(&text),
                label("azdrive-keys-a-code"),
                code_input(app, ids::RECOVERY_SIGN_CODE),
            ]);
            if !error.is_empty() {
                body.add_child(red(error));
            }
            body.add_child(buttons(vec![
                button("kit-button-cancel", app, crate::ui_dialogs::on_cancel_popup),
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
    Button::create(azul_appkit::l10n::label(text))
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
                Some(at) => t_args(
                    "azdrive-keys-listed",
                    &[
                        ("count", Arg::from(state.server_keys.len())),
                        ("day", Arg::from(day(at))),
                    ],
                ),
                None => t("azdrive-keys-not-listed"),
            }))
            .with_css("flex-grow: 1;"),
        )
        .with_child(keys_button(
            app,
            drive_id,
            KeysButton::Check,
            if state.keys_checked.is_some() {
                "azdrive-add-check-again"
            } else {
                "azdrive-drill-check"
            },
            ids::recovery_keys_check(drive_id),
        ));
    if state.keys_checked.is_some() {
        head.add_child(keys_button(
            app,
            drive_id,
            KeysButton::AddCode,
            "azdrive-keys-add-another",
            ids::recovery_keys_add(drive_id),
        ));
    }
    block.add_child(head);
    for key in &state.server_keys {
        let added = key.created_at.map_or_else(
            || t("azdrive-keys-from-before"),
            |at| t_args("azdrive-keys-added-on", &[("day", Arg::from(day(at)))]),
        );
        let line_key = if key.verified {
            "azdrive-keys-key-line"
        } else {
            "azdrive-keys-key-line-unverified"
        };
        block.add_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; margin-top: 2px;",
                )
                .with_child(
                    Dom::create_span_with_text(AzString::from(t_args(
                        line_key,
                        &[
                            ("label", Arg::from(key_label(&key.label))),
                            ("added", Arg::from(added)),
                        ],
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
                    "azdrive-keys-remove-button",
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
                    line("azdrive-keys-findable-missing")
                        .with_css(format!("flex-grow: 1; color: #9D5D00; {small}")),
                )
                .with_child(keys_button(
                    app,
                    drive_id,
                    KeysButton::MakeFindable,
                    "azdrive-keys-make-findable",
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
                    Dom::create_span_with_text(azul_appkit::l10n::label("azdrive-keys-never-had"))
                        .with_css("flex-grow: 1;"),
                )
                .with_child(
                    button("azdrive-keys-recover-kit", app, on_kit_open).with_id(ids::KIT_RECOVER),
                )
                .with_child(
                    button("azdrive-keys-recover-contacts", app, on_contacts_open)
                        .with_id(ids::CONTACTS_RECOVER_NEW),
                ),
        );
    for pending in &s.settings.recovery.pending {
        let text = match pending.until {
            Some(at) => t_args(
                "azdrive-keys-recovering-until",
                &[
                    ("drive", Arg::from(pending.drive_id.as_str())),
                    ("until", Arg::from(azul_storage::time::iso8601(at))),
                ],
            ),
            None => t_args(
                "azdrive-keys-recovering",
                &[("drive", Arg::from(pending.drive_id.as_str()))],
            ),
        };
        block.add_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; margin-top: 4px;",
                )
                .with_child(
                    Dom::create_span_with_text(AzString::from(text))
                        .with_css("flex-grow: 1; font-size: 12px;"),
                )
                .with_child(keys_button(
                    app,
                    &pending.drive_id,
                    KeysButton::Finish,
                    "azdrive-keys-finish",
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
        // A new code and a removed one change the drive's keys: only after Touch ID or the
        // password (the gated tier).
        KeysButton::AddCode => {
            crate::presence::ask(info, s, crate::presence::Gated::AddCode { drive_id });
        }
        KeysButton::MakeFindable => ask_sign(s, &drive_id, KeyAction::MakeFindable),
        KeysButton::Remove { key_id, label } => {
            let gated = crate::presence::Gated::RemoveCode {
                drive_id,
                key_id,
                label,
            };
            crate::presence::ask(info, s, gated);
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
                name: t("azdrive-keys-recovered-name"),
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
            set_error(s, t("azdrive-drill-not-a-code"));
            return;
        };
        let Some(token_url) = s.token.url.clone() else {
            set_error(s, t("azdrive-keys-no-azlin-server"));
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
            set_error(s, t("azdrive-keys-no-azlin-server"));
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
            set_error(s, t("azdrive-no-token-server"));
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
                let state = state_of(&s.settings.recovery.drives, &drive_id);
                let kit = kit_wrap_of(state, &key_id);
                let own = state
                    .map(|state| state.extra_codes.clone())
                    .unwrap_or_default();
                let (extra_file, auto) = removal_wrap(s, &drive_id, &kit);
                KeysJob::Remove {
                    drive_id,
                    key_id,
                    signer,
                    extra_file,
                    auto,
                    first_code: kit == KitWrap::FirstCode,
                    own,
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
    /// After the 48 hours: the first refresh hands the drive over; its entry, named `name` (the
    /// kit has no name: "Recovered drive" in the window's language).
    Finish {
        drive_id: String,
        token_url: String,
        keyring: SharedKeyring,
        name: String,
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
        /// The open drive: its storage holds the wrap, and is looked at after the removal.
        auto: Option<Arc<AutoEncrypted>>,
        /// The key is one of the first code's (its copy stays).
        first_code: bool,
        /// The kits this computer made, to tell their copies from the ones nothing claims.
        own: Vec<ExtraCode>,
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
        result: Result<Left, RemoveFailed>,
    },
}

/// Why a recovery key was not removed.
#[derive(Debug)]
pub(crate) enum RemoveFailed {
    /// The kit's wrap in the drive's storage could not be deleted (why): nothing else was touched -
    /// its code still opens the drive key for anyone who reaches the bucket, so the key stays
    /// listed and the removal can be tried again.
    Wrap(String),
    /// The token server's answer, or none (after the wrap went): a removal it did not refuse runs
    /// again, the wrap being gone already.
    Server(TokenError),
    /// The token server REFUSED to remove the key (the last one, a signature it does not take), and
    /// the wrap, taken out of the drive before, could not be put back: the kit is dead and still
    /// listed, and it is said so (X-DELTA-AZDRIVE-KEYS-28).
    WrapLost { refused: TokenError, why: String },
}

/// Which wrap in the drive's storage a recovery key's removal takes with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KitWrap {
    /// The file this computer recorded when it made the kit.
    Known(String),
    /// The file a kit is named by when its key at the token server is known
    /// (`device::recovery_code_file_for`): kits made since the name was the key's, by any
    /// computer. No such file is no failure (a kit made before has a name of its own that no key
    /// maps to: what is left is looked at after the removal, [`Left`]).
    Named(String),
    /// One of the drive's first code's two keys: that code's copy (`recovery.key`) is the
    /// drive's own - the mark that the drive is encrypted - and stays; only a key change
    /// replaces it, and the window says so (X-DELTA-AZDRIVE-KEYS-28).
    FirstCode,
    /// The server's key is not listed here, or listed without its public half: no wrap to name.
    Unknown,
}

impl KitWrap {
    /// The copy in the storage the key's removal takes out with it, if the key has one by name.
    fn file(&self) -> Option<&str> {
        match self {
            KitWrap::Known(file) | KitWrap::Named(file) => Some(file),
            KitWrap::FirstCode | KitWrap::Unknown => None,
        }
    }
}

/// The wrap that goes with recovery key `key_id` of a drive whose recovery state is `state`: this
/// computer's own record of the kit, else the file named by the key's public half as the token
/// server listed it - unless the key is one of the first code's.
fn kit_wrap_of(state: Option<&RecoveryState>, key_id: &str) -> KitWrap {
    let Some(state) = state else {
        return KitWrap::Unknown;
    };
    if let Some(code) = state.extra_code(key_id) {
        return KitWrap::Known(code.file.clone());
    }
    let Some(key) = state
        .server_keys
        .iter()
        .find(|key| key.key_id == key_id && !key.recovery_pubkey.is_empty())
    else {
        return KitWrap::Unknown;
    };
    let public = key.recovery_pubkey.trim();
    let first = [&state.recovery_key, &state.findable_key]
        .into_iter()
        .flatten()
        .any(|first| first.trim() == public);
    if first {
        KitWrap::FirstCode
    } else {
        KitWrap::Named(device::recovery_code_file_for(&key.recovery_pubkey))
    }
}

/// The copy a removal takes out of the storage with the key, and the open drive it is taken from.
/// A key that has a copy by name (this computer's kit, a kit named by its key) needs the drive
/// open - a removal that went on without would leave the copy that still opens the drive, so the
/// job stops with "the drive is not open" when `auto_for` found none. The first code's key and
/// one without a public half have no copy to name; the drive is looked at after the removal only
/// when it is open already (nothing here opens it).
fn removal_wrap(
    s: &mut DriveState,
    drive_id: &str,
    kit: &KitWrap,
) -> (Option<String>, Option<Arc<AutoEncrypted>>) {
    match kit.file() {
        Some(file) => (
            Some(file.to_string()),
            crate::encryption::auto_for(s, drive_id),
        ),
        None => (
            None,
            s.slot_index(drive_id)
                .and_then(|index| s.slots[index].auto.clone()),
        ),
    }
}

/// What a removal that went through leaves in the drive's storage, for the window to say: a
/// key's removal takes its copy with it when the copy can be named, and what it cannot name stays
/// - silently, until this (X-DELTA-AZDRIVE-KEYS-28).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Left {
    /// The removed key was one of the first code's: that code's copy stays and opens the drive.
    pub first_code: bool,
    pub copies: Copies,
}

/// The further recovery codes' copies in the drive's storage after a removal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Copies {
    /// Nothing was looked at: the drive is not open here.
    NotLooked,
    /// The token server's keys or the storage could not be read (why): not known, not "none".
    Unreadable(String),
    /// The copies no key the token server lists now claims (none: every copy has its key).
    Unclaimed(Vec<String>),
}

impl Left {
    /// What to tell the user, `None` when nothing is left that they do not know of.
    fn say(&self) -> Option<String> {
        let mut lines = Vec::new();
        if self.first_code {
            lines.push(t("azdrive-keys-left-first"));
        }
        match &self.copies {
            Copies::NotLooked => {}
            Copies::Unreadable(why) => lines.push(t_args(
                "azdrive-keys-left-unreadable",
                &[("why", Arg::from(t_label(why)))],
            )),
            Copies::Unclaimed(files) if files.is_empty() => {}
            Copies::Unclaimed(files) => lines.push(t_args(
                "azdrive-keys-left-copies",
                &[("count", Arg::from(files.len()))],
            )),
        }
        if lines.is_empty() {
            None
        } else {
            Some(lines.join(" "))
        }
    }
}

/// The copies a key the token server lists claims: the file the key's public half names, and the
/// file this computer recorded for a kit whose key is listed still (a kit this computer made
/// before copies were named by their key has a name of its own).
fn claimed_copies(listed: &[RecoveryKeyInfo], own: &[ExtraCode]) -> Vec<String> {
    let mut claimed: Vec<String> = listed
        .iter()
        .filter(|key| !key.recovery_pubkey.is_empty())
        .map(|key| device::recovery_code_file_for(&key.recovery_pubkey))
        .collect();
    claimed.extend(
        own.iter()
            .filter(|code| listed.iter().any(|key| key.key_id == code.key_id))
            .map(|code| code.file.clone()),
    );
    claimed
}

/// What the drive's storage holds of further recovery codes' copies that no listed key claims,
/// after a removal went through: `bucket` is the open drive's (none: not looked at), `listed` the
/// token server's keys as it lists them now.
fn left_after(
    first_code: bool,
    bucket: Option<&dyn azul_storage::Drive>,
    listed: Result<Vec<RecoveryKeyInfo>, TokenError>,
    own: &[ExtraCode],
) -> Left {
    let copies = match (bucket, listed) {
        (None, _) => Copies::NotLooked,
        (Some(_), Err(e)) => Copies::Unreadable(token_text(&e)),
        (Some(bucket), Ok(listed)) => {
            match device::unclaimed_recovery_codes(bucket, &claimed_copies(&listed, own)) {
                Ok(files) => Copies::Unclaimed(files),
                Err(e) => Copies::Unreadable(e.to_string()),
            }
        }
    };
    Left { first_code, copies }
}

/// Removes a recovery key: its wrap in the drive's storage first (`delete_wrap`: what lets its
/// code open the drive key - a kit that was lost must not keep doing that; it hands back the bytes
/// it took out), then its key at the token server (`remove_at_server`, which checks the signature
/// of the drive's current code and refuses the last key). A wrap that cannot be deleted stops the
/// removal where it is and is said, never ignored; a wrap that is gone already is no failure, so a
/// removal the server never answered runs again. A key the server REFUSES to remove stays - and
/// so does its kit: the wrap goes back (`restore_wrap`), so that the list does not show a kit that
/// can open nothing (X-DELTA-AZDRIVE-KEYS-28).
fn remove_key(
    delete_wrap: impl FnOnce() -> Result<Option<Vec<u8>>, String>,
    remove_at_server: impl FnOnce() -> Result<(), TokenError>,
    restore_wrap: impl FnOnce(Vec<u8>) -> Result<(), String>,
) -> Result<(), RemoveFailed> {
    let taken = delete_wrap().map_err(RemoveFailed::Wrap)?;
    match remove_at_server() {
        Ok(()) => Ok(()),
        Err(refused @ TokenError::Refused { .. }) => match taken.map(restore_wrap) {
            None | Some(Ok(())) => Err(RemoveFailed::Server(refused)),
            Some(Err(why)) => Err(RemoveFailed::WrapLost { refused, why }),
        },
        Err(e) => Err(RemoveFailed::Server(e)),
    }
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

/// [`with_token`] of a change of the recovery keys (`what`: which), named by the key of its open
/// call ([`azcloud_kit::outbox`]): asked again after a lost answer, it is answered like the first.
fn with_token_keyed<T>(
    keyring: &SharedKeyring,
    drive_id: &str,
    what: &str,
    call: impl FnOnce(&str, &IdempotencyKey) -> Result<T, TokenError>,
) -> Result<T, TokenError> {
    let outbox = keyring.outbox();
    with_token(keyring, drive_id, |token| {
        outbox.send(IdempotentScope::RecoveryKey, drive_id, what, |key| call(token, key))
    })
}

/// The drive's entry (named `name`) and session after the hand-over: the pending session's
/// token refreshed.
fn finish(
    drive_id: &str,
    server: &TokenServer<'_>,
    keyring: &SharedKeyring,
    token_url: &str,
    name: &str,
) -> Result<(DriveEntry, String), String> {
    // A drive being recovered holds its drive token alone until this trade.
    let spend = keyring
        .spend(drive_id)
        .map_err(|_| String::from("azdrive-keys-err-no-session"))?;
    let rotation = server
        .refresh(spend, None)
        .rotated()
        .map_err(|e| token_text(&e))?;
    let entry = rotation.bundle().entry_named(name, token_url);
    // Kept before the lock is let go: the token just spent is dead.
    let kept = rotation.keep();
    kept.saved?;
    Ok((entry, kept.session.to_keyring_secret()))
}

/// Runs on a worker thread.
pub(crate) fn run(job: KeysJob) -> KeysDone {
    let transport = AzulTransport::azlin(crate::USER_AGENT);
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
            name,
        } => {
            let result = TokenServer::new(&token_url, &transport)
                .map_err(|e| e.to_string())
                .and_then(|server| finish(&drive_id, &server, &keyring, &token_url, &name));
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
                // The wrap's file is named by the code's key at the token server, so that a removal of
                // the key from another computer finds it (X-DELTA-AZDRIVE-KEYS-28).
                let (file, code) = device::add_recovery_code_for_key(
                    auto.bucket().as_ref(),
                    &azlin_keyring(azul_storage::keyring::Asking::User),
                    auto.drive(),
                    kdf,
                    |code| findable_key_of(code).public_base64(),
                )
                .map_err(|e| e.to_string())?;
                let findable = findable_key_of(&code);
                let public = findable.public_base64();
                let what = format!("add:{public}");
                let added = with_token_keyed(&keyring, &drive_id, &what, |token, key| {
                    server.add_recovery_key(
                        &drive_id,
                        token,
                        &public,
                        SECOND_KIT_LABEL,
                        key,
                        |message| Ok(signer.sign_base64(message)),
                    )
                });
                match added {
                    Ok(key) => Ok((code.to_text(), key.key_id, file)),
                    // No key at the token server: no second kit in the bucket either.
                    Err(e) => Err(rolled_back(auto.bucket().as_ref(), &file, token_text(&e))),
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
                    let what = format!("add:{findable_public}");
                    with_token_keyed(&keyring, &drive_id, &what, |token, key| {
                        server.add_recovery_key(
                            &drive_id,
                            token,
                            &findable_public,
                            FINDABLE_LABEL,
                            key,
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
            first_code,
            own,
            token_url,
            keyring,
        } => {
            let result = remove_key(
                || match (&extra_file, &auto) {
                    (Some(file), Some(auto)) => {
                        let bucket = auto.bucket().as_ref();
                        let taken =
                            device::read_recovery_code(bucket, file).map_err(|e| e.to_string())?;
                        device::remove_recovery_code(bucket, file).map_err(|e| e.to_string())?;
                        Ok(taken)
                    }
                    // The kit's wrap is in the drive's storage, and the drive is not open.
                    (Some(_), None) => Err(t("azdrive-keys-drive-not-open")),
                    // The drive's first code, or a key without a public half: no wrap this
                    // computer can name (what stays is looked at below).
                    (None, _) => Ok(None),
                },
                || {
                    TokenServer::new(&token_url, &transport).and_then(|server| {
                        let what = format!("remove:{key_id}");
                        with_token_keyed(&keyring, &drive_id, &what, |token, key| {
                            server.remove_recovery_key(&drive_id, token, &key_id, key, |message| {
                                Ok(signer.sign_base64(message))
                            })
                        })
                    })
                },
                |bytes| match (&extra_file, &auto) {
                    (Some(file), Some(auto)) => {
                        device::restore_recovery_code(auto.bucket().as_ref(), file, &bytes)
                            .map_err(|e| e.to_string())
                    }
                    // nothing was taken out of a drive that was not open
                    _ => Ok(()),
                },
            )
            // The key is gone: what copies of recovery codes the storage still holds that no key
            // the token server lists now claims (a kit made before copies were named by their
            // key, one whose key went earlier) is read from the storage and the server's list as
            // they are NOW, never from a flag.
            .map(|()| {
                let listed = TokenServer::new(&token_url, &transport).and_then(|server| {
                    with_token(&keyring, &drive_id, |token| {
                        server.recovery_keys(&drive_id, token)
                    })
                });
                left_after(
                    first_code,
                    auto.as_ref().map(|auto| auto.bucket().as_ref()),
                    listed,
                    &own,
                )
            });
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
                    set_error(s, t("azdrive-keys-found-none"));
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
            Err(why) => set_error(s, t_label(&why)),
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
                let text = match until {
                    Some(at) => t_args(
                        "azdrive-keys-locked-down-until",
                        &[("until", Arg::from(azul_storage::time::iso8601(at)))],
                    ),
                    None => t("azdrive-keys-locked-down-what"),
                };
                s.popup.open(Popup::Encryption(Dialog::Message {
                    title: t("azdrive-keys-locked-down-title"),
                    text,
                }));
                save_settings(info, app, s);
            }
            Err(why) => set_error(s, t_label(&why)),
        },
        KeysDone::Finished { drive_id, result } => match result {
            Ok((entry, secret)) => {
                forget_pending(&mut s.settings.recovery.pending, &drive_id);
                crate::add_flow::add_slot(info, app, s, entry, Some(secret), false, true);
                println!("AZDRIVE_RECOVERY_FINISHED {drive_id}");
                s.success(Phrase::new("azdrive-keys-finished").arg("drive", drive_id.as_str()));
                save_settings(info, app, s);
            }
            Err(why) => {
                // The notice says why; scripts read it here, on one line.
                println!("AZDRIVE_RECOVERY_NOT_FINISHED {drive_id} {}", why.replace('\n', " "));
                s.warn(
                    Phrase::new("azdrive-keys-not-finished")
                        .arg("drive", drive_id.as_str())
                        .arg("why", t_label(&why)),
                );
            }
        },
        KeysDone::Listed { drive_id, result } => match result {
            Ok(keys) => {
                println!("AZDRIVE_RECOVERY_KEYS {drive_id} {}", keys.len());
                let state = state_mut(&mut s.settings.recovery.drives, &drive_id);
                // a code whose registration was never recorded: the list says
                let listed: Vec<String> = keys.iter().map(|k| k.recovery_pubkey.clone()).collect();
                state.keys_listed(&listed);
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
            Err(why) => {
                s.warn(Phrase::new("azdrive-keys-not-listed-why").arg("why", t_label(&why)))
            }
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
                s.popup.open(Popup::Encryption(Dialog::Sheet(
                    Sheet::new(&drive_id, code).extra(),
                )));
                save_settings(info, app, s);
                check_keys(info, app, s, &drive_id);
            }
            Err(why) => set_error(s, t_label(&why)),
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
                s.popup.close();
                s.success(Phrase::new("azdrive-keys-findable-now"));
                save_settings(info, app, s);
                check_keys(info, app, s, &drive_id);
            }
            Err(why) => set_error(s, t_label(&why)),
        },
        KeysDone::Removed {
            drive_id,
            key_id,
            result,
        } => match result {
            Ok(left) => {
                println!("AZDRIVE_RECOVERY_KEY_REMOVED {drive_id} {key_id}");
                if let Copies::Unclaimed(files) = &left.copies {
                    if !files.is_empty() {
                        println!("AZDRIVE_RECOVERY_COPIES_LEFT {drive_id} {}", files.len());
                    }
                }
                let state = state_mut(&mut s.settings.recovery.drives, &drive_id);
                state.extra_codes.retain(|code| code.key_id != key_id);
                state.server_keys.retain(|key| key.key_id != key_id);
                // What the removal could not take with it is said, in a window that stays: a copy
                // of a code that still opens the drive is not a toast.
                match left.say() {
                    Some(text) => s.popup.open(Popup::Encryption(Dialog::Message {
                        title: t("azdrive-keys-left-title"),
                        text,
                    })),
                    None => s.popup.close(),
                }
                save_settings(info, app, s);
                check_keys(info, app, s, &drive_id);
            }
            Err(RemoveFailed::Wrap(why)) => {
                // Nothing was removed: the kit's code still opens the drive key.
                println!(
                    "AZDRIVE_RECOVERY_KEY_WRAP_KEPT {drive_id} {key_id} {}",
                    why.replace('\n', " ")
                );
                set_error(
                    s,
                    t_args("azdrive-keys-wrap-not-deleted", &[("why", Arg::from(why))]),
                );
            }
            Err(RemoveFailed::WrapLost { refused, why }) => {
                println!(
                    "AZDRIVE_RECOVERY_KEY_WRAP_LOST {drive_id} {key_id} {}",
                    why.replace('\n', " ")
                );
                set_error(
                    s,
                    t_args(
                        "azdrive-keys-wrap-lost",
                        &[
                            ("refused", Arg::from(t_label(&token_text(&refused)))),
                            ("why", Arg::from(why)),
                        ],
                    ),
                );
            }
            Err(RemoveFailed::Server(e)) => {
                if e.is_last_recovery_key() {
                    println!("AZDRIVE_RECOVERY_KEY_KEPT {drive_id} {key_id}");
                } else {
                    // The dialog says why; scripts read it here, on one line.
                    let why = e.to_string().replace('\n', " ");
                    println!("AZDRIVE_RECOVERY_KEY_REFUSED {drive_id} {key_id} {why}");
                }
                set_error(s, t_label(&token_text(&e)));
            }
        },
    }
}

/// The words for a second kit that could not be added at the token server: its key file goes out
/// of the bucket again (no key at the token server, no second kit in the bucket either). A delete
/// that fails is said - the wrap of a code nobody was shown stays in the drive and counts against
/// the most further codes a drive holds - and is not thrown away (X-AR5b-9).
fn rolled_back(bucket: &dyn azul_storage::Drive, file: &str, why: String) -> String {
    match device::remove_recovery_code(bucket, file) {
        Ok(()) => why,
        Err(left) => format!(
            "{why}; the unused key file {file} could not be removed from the drive either ({left})"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code() -> RecoveryCode {
        RecoveryCode::from_bytes([0x5A; 16])
    }

    fn refused(code: &str) -> TokenError {
        TokenError::Refused {
            status: 409,
            code: code.to_string(),
            message: String::new(),
        }
    }

    /// K11: removing a lost kit deleted its key at the token server first and ignored a failed
    /// delete of its wrap: the list said "removed", and the kit's code kept opening the drive key
    /// for anyone who reaches the bucket. The wrap goes first, a wrap that stays stops the removal
    /// and is said.
    #[test]
    fn a_wrap_that_cannot_be_deleted_stops_the_removal_and_is_said() {
        let asked = std::cell::Cell::new(false);
        let result = remove_key(
            || Err(String::from("no network")),
            || {
                asked.set(true);
                Ok(())
            },
            |_| Ok(()),
        );
        assert!(matches!(result, Err(RemoveFailed::Wrap(why)) if why == "no network"));
        assert!(
            !asked.get(),
            "the token server is not asked while the wrap stays"
        );
    }

    #[test]
    fn the_wrap_goes_first_then_the_key_at_the_token_server() {
        let order = std::cell::RefCell::new(Vec::new());
        let result = remove_key(
            || {
                order.borrow_mut().push("wrap");
                Ok(Some(b"the wrap".to_vec()))
            },
            || {
                order.borrow_mut().push("server");
                Ok(())
            },
            |_| {
                order.borrow_mut().push("restore");
                Ok(())
            },
        );
        assert!(result.is_ok());
        assert_eq!(
            *order.borrow(),
            vec!["wrap", "server"],
            "nothing is put back after it worked"
        );
        // The server's refusal is its own answer, with no wrap to put back.
        let refusal = remove_key(
            || Ok(None),
            || Err(refused("last_recovery_key")),
            |_| Ok(()),
        );
        assert!(matches!(
            refusal,
            Err(RemoveFailed::Server(TokenError::Refused { code, .. })) if code == "last_recovery_key"
        ));
    }

    /// X-DELTA-AZDRIVE-KEYS-28: the wrap went first and the server then refused (the last key, a
    /// signature it does not take): the key stayed listed with a kit that could open nothing. A
    /// refusal puts the wrap back, byte for byte; a network that never answered does not (the key
    /// may be gone at the server, and the removal runs again from there); a wrap that cannot be
    /// put back is said.
    #[test]
    fn x_delta_azdrive_keys_28_a_refusal_of_the_server_puts_the_kits_wrap_back() {
        let put_back = std::cell::RefCell::new(Vec::new());
        let result = remove_key(
            || Ok(Some(b"the wrap".to_vec())),
            || Err(refused("last_recovery_key")),
            |bytes| {
                put_back.borrow_mut().push(bytes);
                Ok(())
            },
        );
        assert!(matches!(result, Err(RemoveFailed::Server(_))), "{result:?}");
        assert_eq!(*put_back.borrow(), vec![b"the wrap".to_vec()]);

        // No answer at all: ambiguous, the wrap stays out and the removal can run again.
        let silent = std::cell::Cell::new(false);
        let result = remove_key(
            || Ok(Some(b"the wrap".to_vec())),
            || Err(TokenError::Connect(String::from("timed out"))),
            |_| {
                silent.set(true);
                Ok(())
            },
        );
        assert!(matches!(
            result,
            Err(RemoveFailed::Server(TokenError::Connect(_)))
        ));
        assert!(
            !silent.get(),
            "a key the server may have removed keeps its wrap out"
        );

        // The wrap cannot be put back: a refusal AND a lost kit, both said.
        let result = remove_key(
            || Ok(Some(b"the wrap".to_vec())),
            || Err(refused("last_recovery_key")),
            |_| Err(String::from("the bucket is read-only")),
        );
        assert!(
            matches!(&result, Err(RemoveFailed::WrapLost { why, .. }) if why == "the bucket is read-only"),
            "{result:?}"
        );
    }

    /// X-DELTA-AZDRIVE-KEYS-28: the wrap that goes with a key is this computer's own record of the
    /// kit, else the file named by the key's public half as the server listed it - so a kit another
    /// computer made (and named so) is found from here.
    #[test]
    fn x_delta_azdrive_keys_28_the_wrap_of_a_kit_another_computer_made_is_found_by_its_key() {
        let mut state = RecoveryState::new("d_1");
        // a kit this computer made: its record
        state.extra_codes.push(crate::recovery_health::ExtraCode {
            key_id: String::from("rk_mine"),
            file: String::from(".azlin/keys/recovery-0123456789abcdef.key"),
            made: 1,
        });
        // a kit another computer made: only the server's list names it
        state.server_keys = vec![crate::recovery_health::ServerKey {
            key_id: String::from("rk_theirs"),
            label: String::from("second kit"),
            recovery_pubkey: String::from("THEIRS="),
            created_at: None,
            verified: true,
        }];
        assert_eq!(
            kit_wrap_of(Some(&state), "rk_mine"),
            KitWrap::Known(String::from(".azlin/keys/recovery-0123456789abcdef.key"))
        );
        assert_eq!(
            kit_wrap_of(Some(&state), "rk_theirs"),
            KitWrap::Named(device::recovery_code_file_for("THEIRS=")),
        );
        assert_eq!(kit_wrap_of(Some(&state), "rk_unlisted"), KitWrap::Unknown);
        assert_eq!(kit_wrap_of(None, "rk_theirs"), KitWrap::Unknown);
        // the name a kit is made with is the one the removal looks for
        assert!(device::recovery_code_file_for("THEIRS=").starts_with(".azlin/keys/recovery-"));
    }

    fn listed(key_id: &str, public: &str) -> RecoveryKeyInfo {
        RecoveryKeyInfo {
            key_id: key_id.to_string(),
            label: String::from(SECOND_KIT_LABEL),
            recovery_pubkey: public.to_string(),
            created_at: None,
            verified: true,
        }
    }

    fn cheap() -> RecoveryKdf {
        RecoveryKdf::with_cost(64, 1, 1).unwrap()
    }

    /// X-DELTA-AZDRIVE-KEYS-28: the first code's two keys (the drive's and the findable one) are
    /// told from a further kit's: the first code's copy is the drive's own (`recovery.key`, the
    /// mark of an encrypted drive) and has no name to look for, a further kit's copy is named by
    /// its key and the removal takes it out - and, with the drive closed, would not have been
    /// looked for at all (the old mapping sent a named kit with `(None, None)` when the drive
    /// was not open, and the key went with the copy staying).
    #[test]
    fn x_delta_azdrive_keys_28_the_first_codes_keys_have_no_copy_to_take_and_a_kits_have_one() {
        let mut state = RecoveryState::new("d_1");
        state.recovery_key = Some(String::from("FIRSTDRIVE="));
        state.findable_key = Some(String::from(" FIRSTFIND= "));
        let key = |id: &str, public: &str| ServerKey {
            key_id: id.to_string(),
            label: String::new(),
            recovery_pubkey: public.to_string(),
            created_at: None,
            verified: true,
        };
        state.server_keys = vec![
            key("rk_drive", "FIRSTDRIVE="),
            key("rk_find", "FIRSTFIND="),
            key("rk_kit", "KIT="),
            key("rk_bare", ""),
        ];
        assert_eq!(kit_wrap_of(Some(&state), "rk_drive"), KitWrap::FirstCode);
        assert_eq!(kit_wrap_of(Some(&state), "rk_find"), KitWrap::FirstCode);
        assert_eq!(kit_wrap_of(Some(&state), "rk_bare"), KitWrap::Unknown);
        let kit = kit_wrap_of(Some(&state), "rk_kit");
        assert_eq!(kit, KitWrap::Named(device::recovery_code_file_for("KIT=")));
        // the copies a removal looks for: a kit's, never the first code's
        assert_eq!(
            kit.file(),
            Some(device::recovery_code_file_for("KIT=").as_str())
        );
        assert_eq!(KitWrap::FirstCode.file(), None);
        assert_eq!(KitWrap::Unknown.file(), None);
        // a code from before the drive kept its public keys: its keys read as a kit's, whose
        // copy is looked for (none by that name) with the drive open
        state.recovery_key = None;
        state.findable_key = None;
        assert!(matches!(
            kit_wrap_of(Some(&state), "rk_drive"),
            KitWrap::Named(_)
        ));
    }

    /// X-DELTA-AZDRIVE-KEYS-28 through the job: a kit whose copy has a name needs the drive open,
    /// and with the drive closed the removal stops before the token server is asked (the key
    /// stays listed with its copy; nothing is half done). The first code's key has no copy to
    /// look for and is removed with the drive closed.
    #[test]
    fn x_delta_azdrive_keys_28_a_named_kit_is_not_removed_while_the_drive_is_closed() {
        use azul_storage::{
            keyring::{KeyringNamespace, MemoryKeyring},
            testing::TempDir,
        };
        crate::l10n::in_english();
        let dir = TempDir::new("keys-28-closed");
        let keyring = SharedKeyring::with_lock_root(
            Arc::new(MemoryKeyring::new()),
            KeyringNamespace::azlin_shared(),
            dir.path(),
        );
        let kit = KitWrap::Named(device::recovery_code_file_for("THEIRS="));
        let done = run(KeysJob::Remove {
            drive_id: String::from("d_1"),
            key_id: String::from("rk_theirs"),
            signer: findable_key_of(&code()),
            extra_file: kit.file().map(str::to_string),
            auto: None,
            first_code: false,
            own: Vec::new(),
            // never reached: the copy is looked for first
            token_url: String::from("https://token.invalid"),
            keyring,
        });
        let KeysDone::Removed { result, .. } = done else {
            panic!("a removal answers Removed");
        };
        match result {
            Err(RemoveFailed::Wrap(why)) => assert!(why.contains("not open"), "{why}"),
            other => panic!("the removal stops at the copy it cannot look for: {other:?}"),
        }
    }

    /// X-DELTA-AZDRIVE-KEYS-28: after a removal the storage is read as it is and every further
    /// recovery code's copy that no listed key claims is counted - the copy of a key that was
    /// removed before, and a kit another computer made before copies were named by their key
    /// (nothing maps it) - while the copies a listed key claims stay out: one named by its key,
    /// one this computer recorded for a kit that is listed still.
    #[test]
    fn x_delta_azdrive_keys_28_what_the_storage_holds_that_no_listed_key_claims_is_counted() {
        use azul_storage::{keyring::MemoryKeyring, meta::MemoryBucket};
        let bucket = MemoryBucket::new();
        let keyring = MemoryKeyring::new();
        device::setup_new_drive(&bucket, &keyring, "d_1", cheap()).unwrap();
        let (kept, _) =
            device::add_recovery_code_for_key(&bucket, &keyring, "d_1", cheap(), |_| {
                String::from("KEPT=")
            })
            .unwrap();
        let (gone, _) =
            device::add_recovery_code_for_key(&bucket, &keyring, "d_1", cheap(), |_| {
                String::from("GONE=")
            })
            .unwrap();
        let (mine_before, _) =
            device::add_recovery_code(&bucket, &keyring, "d_1", cheap()).unwrap();
        let (theirs_before, _) =
            device::add_recovery_code(&bucket, &keyring, "d_1", cheap()).unwrap();
        let own = vec![ExtraCode {
            key_id: String::from("rk_mine"),
            file: mine_before.clone(),
            made: 1,
        }];
        let keys = vec![listed("rk_kept", "KEPT="), listed("rk_mine", "MINE=")];

        let left = left_after(
            false,
            Some(&bucket as &dyn azul_storage::Drive),
            Ok(keys),
            &own,
        );
        let Copies::Unclaimed(files) = left.copies else {
            panic!("the storage was read");
        };
        let mut want = vec![gone, theirs_before];
        want.sort();
        assert_eq!(files, want, "only what no listed key claims");
        assert!(!files.contains(&kept) && !files.contains(&mine_before));
        assert!(
            !files.iter().any(|f| f.ends_with("recovery.key")),
            "the first code's copy is not a further code's"
        );
        // the kit this computer made before the name was the key's is claimed only while its key
        // is listed: once the key is gone, its copy is one nothing claims
        let left = left_after(
            false,
            Some(&bucket as &dyn azul_storage::Drive),
            Ok(vec![listed("rk_kept", "KEPT=")]),
            &own,
        );
        let Copies::Unclaimed(files) = left.copies else {
            panic!("the storage was read");
        };
        assert!(files.contains(&mine_before), "{files:?}");
    }

    /// X-DELTA-AZDRIVE-KEYS-28: what could not be looked at is not "nothing left". A drive that
    /// is not open is not looked at; a token server that does not list, or a storage that does not
    /// list, is `Unreadable` and said.
    #[test]
    fn x_delta_azdrive_keys_28_what_could_not_be_looked_at_is_not_nothing_left() {
        use azul_storage::{meta::MemoryBucket, Drive, DriveError, ListRequest};
        crate::l10n::in_english();

        // a storage whose listing fails
        struct Unlistable(MemoryBucket);
        impl Drive for Unlistable {
            fn list(&self, _: &ListRequest) -> Result<azul_storage::ListPage, DriveError> {
                Err(DriveError::Transport(String::from(
                    "the connection was cut",
                )))
            }
            fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
                self.0.get(key)
            }
            fn get_range(
                &self,
                key: &str,
                range: azul_storage::ByteRange,
            ) -> Result<Vec<u8>, DriveError> {
                self.0.get_range(key, range)
            }
            fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
                self.0.put(key, bytes)
            }
            fn delete(&self, key: &str) -> Result<(), DriveError> {
                self.0.delete(key)
            }
            fn head(&self, key: &str) -> Result<azul_storage::ObjectInfo, DriveError> {
                self.0.head(key)
            }
        }
        let bucket = Unlistable(MemoryBucket::new());

        let storage = left_after(
            false,
            Some(&bucket as &dyn azul_storage::Drive),
            Ok(Vec::new()),
            &[],
        );
        assert!(
            matches!(&storage.copies, Copies::Unreadable(why) if why.contains("cut")),
            "{storage:?}"
        );
        let said = storage.say().expect("an unreadable storage is said");
        assert!(said.contains("could not be checked"), "{said}");

        let server = left_after(
            false,
            Some(&bucket as &dyn azul_storage::Drive),
            Err(TokenError::Connect(String::from("timed out"))),
            &[],
        );
        assert!(matches!(server.copies, Copies::Unreadable(_)), "{server:?}");

        // not open: not looked at, and nothing is said about it
        let closed = left_after(false, None, Ok(Vec::new()), &[]);
        assert_eq!(closed.copies, Copies::NotLooked);
        assert_eq!(closed.say(), None);
    }

    /// X-DELTA-AZDRIVE-KEYS-28: the words. Nothing left is no window; the first code's copy and
    /// the copies nothing claims are each said, with their number.
    #[test]
    fn x_delta_azdrive_keys_28_the_window_says_what_is_left_and_only_that() {
        crate::l10n::in_english();
        let none = Left {
            first_code: false,
            copies: Copies::Unclaimed(Vec::new()),
        };
        assert_eq!(none.say(), None, "every copy has its key: nothing to say");

        let first = Left {
            first_code: true,
            copies: Copies::Unclaimed(Vec::new()),
        };
        let said = first.say().expect("the first code's copy stays");
        assert!(
            said.contains("first recovery code still opens the drive"),
            "{said}"
        );

        let one = Left {
            first_code: false,
            copies: Copies::Unclaimed(vec![String::from("a")]),
        };
        let said = one.say().expect("one copy is left");
        assert!(said.starts_with("One copy of a recovery code"), "{said}");
        assert!(said.contains("A key change removes them"), "{said}");

        let both = Left {
            first_code: true,
            copies: Copies::Unclaimed(vec![String::from("a"), String::from("b")]),
        };
        let said = both.say().expect("both are said");
        assert!(said.contains("first recovery code"), "{said}");
        assert!(said.contains("2 copies of recovery codes"), "{said}");
    }

    #[test]
    fn a_typed_code_picks_the_key_the_drive_has_or_says_why_not() {
        crate::l10n::in_english();
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
        crate::l10n::in_english();
        let refused = |code: &str| TokenError::Refused {
            status: 409,
            code: code.to_string(),
            message: String::from("raw"),
        };
        // The table's message as its key on a worker; the window says it.
        assert_eq!(
            token_text(&refused("last_recovery_key")),
            "azlin-error-last-recovery-key"
        );
        assert!(t_label(&token_text(&refused("last_recovery_key"))).contains("last recovery key"));
        assert!(
            token_text(&refused("frobnicated")).contains("raw"),
            "as it is otherwise"
        );
    }

    /// X-AR5b-9: a second kit that could not be added at the token server is taken out of the
    /// bucket again, and a delete that fails is part of the answer.
    #[test]
    fn x_ar5b_9_a_second_kit_that_could_not_be_added_is_removed_again_or_the_leftover_is_said() {
        use azul_storage::{
            crypto::device::{add_recovery_code, EXTRA_RECOVERY_PREFIX},
            keyring::MemoryKeyring,
            meta::MemoryBucket,
            Drive,
        };
        let bucket = MemoryBucket::new();
        let keyring = MemoryKeyring::new();
        let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
        device::setup_new_drive(&bucket, &keyring, "d_1", kdf).unwrap();
        let (file, _code) = add_recovery_code(
            &bucket,
            &keyring,
            "d_1",
            RecoveryKdf::with_cost(64, 1, 1).unwrap(),
        )
        .unwrap();
        assert!(file.starts_with(EXTRA_RECOVERY_PREFIX));
        assert!(
            bucket.head(&file).is_ok(),
            "the kit's wrap is in the bucket"
        );

        let said = rolled_back(&bucket, &file, String::from("no answer"));
        assert_eq!(said, "no answer", "removed: only the reason is said");
        assert!(bucket.head(&file).is_err(), "the unused wrap is gone");

        // A file the removal refuses (it is no further code's) is a leftover, and is said.
        let said = rolled_back(
            &bucket,
            ".azlin/keys/recovery.key",
            String::from("no answer"),
        );
        assert!(
            said.starts_with(
                "no answer; the unused key file .azlin/keys/recovery.key could not be removed"
            ),
            "{said}"
        );
    }
}
