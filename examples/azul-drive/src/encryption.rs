//! Encrypted drives in AzDrive (feature `encryption`, off by default).
//!
//! An Azlin drive opens through azul-storage's `AutoEncrypted`: its first call, on a worker
//! thread, finds whether the drive is encrypted (this computer keeps its key, or the bucket
//! holds key files) and then goes through the encryption, the index from
//! [`index_provider`] - the bucket's encrypted metadata repository, browsed without listing
//! the bucket; a plain drive is used as it is.
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
//! - "I was hacked: new keys...": a question, then the drive's lockdown (every other computer,
//!   key and link loses access at once) and a new drive key (azul-storage's `rotation`), then
//!   the recovery sheet of the NEW code, then "Re-encrypt every file?" - recommended after a
//!   compromise: every file into a new object with a new key, in the background, resumably.
//!
//! In the background: the RECOMPRESSION PASS (azul-storage's `recompress`). A timer looks once
//! a minute; when the computer has been idle for five minutes on mains power (azul's
//! `PowerState`), the first open encrypted drive's files are written again, smaller, on a
//! worker thread, and the pass stops at the first input or when the power cord goes. Its state
//! sits beside the migration's, so the next idle minute continues where it stopped.

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
};

use azul::{
    callbacks::{TextInputOnTextInputCallbackType, TimerCallbackInfo, TimerCallbackReturn},
    prelude::*,
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    widgets::{ButtonType, OnTextInputReturn, TextInputState, TextInputValid},
    window::PowerState,
};
use azul_storage::{
    azul_keyring::AzulKeyring,
    crypto::{
        device,
        keys::{RecoveryCode, RecoveryKdf},
        random_bytes, Zeroizing,
    },
    encrypted::{open_encrypted, AutoEncrypted, IndexProvider},
    meta::MetaIndexProvider,
    migrate::{migrate, MigrationState},
    recompress::{run_pass, RecompressPolicy, RecompressState},
    rotation::{self, reencrypt_pass, ReencryptState},
    time::now_unix,
    Drive,
};

use crate::{
    browse,
    jobs::Job,
    spawn,
    ui_dialogs::{button, buttons, label, line, on_cancel_popup, typed_button},
    with_state, DriveState, Popup,
};

/// The provider of encrypted drives' indexes: the drive's encrypted metadata repository
/// (azul-storage's `meta` module), this computer's copy of it kept in the user's cache folder
/// between runs (`<cache>/AzDrive/drive-index`), so a drive opens with one conditional read and
/// browses without listing the bucket.
pub(crate) fn index_provider() -> Option<Arc<dyn IndexProvider>> {
    Some(Arc::new(
        MetaIndexProvider::new("AzDrive").with_cache_root(drive_index_root()),
    ))
}

/// The run's cache folder (`--cache-dir`, else `<cache>/AzDrive`; `None` in a `--shot` run
/// without the switch), set once at the start.
static CACHE_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Sets the run's cache folder (the start, once): the drives' index copies live in it.
pub(crate) fn set_cache_dir(dir: Option<PathBuf>) {
    let _ = CACHE_DIR.set(dir);
}

/// The folder of this computer's copies of the encrypted drives' indexes: `drive-index/` in the
/// run's cache folder (before the start set it: in `<cache>/AzDrive`).
pub(crate) fn drive_index_root() -> Option<PathBuf> {
    match CACHE_DIR.get() {
        Some(dir) => drive_index_root_in(dir.as_deref()),
        None => {
            let default = crate::path_of(azul::file::FilePath::get_cache_dir().into_option())
                .map(|dir| dir.join("AzDrive"));
            drive_index_root_in(default.as_deref())
        }
    }
}

/// `drive-index/` in the cache folder `cache_dir`; `None` without one (the copies in memory).
pub(crate) fn drive_index_root_in(cache_dir: Option<&Path>) -> Option<PathBuf> {
    cache_dir.map(|dir| dir.join("drive-index"))
}

/// Where the encrypted drive `drive`'s search index (the plain text of its files) is kept: in
/// the drive's own cache folder under `root`, beside its drive index (`<root>/<hash of the
/// drive>/search`, gone with it when the drive's key is rotated) - never in the cache the other
/// drives share; `None` without a cache folder.
pub(crate) fn search_index_dir(root: Option<PathBuf>, drive: &str) -> Option<PathBuf> {
    MetaIndexProvider::new("AzDrive")
        .with_cache_root(root)
        .drive_cache_dir(drive)
        .map(|dir| dir.join("search"))
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
    state_dir(s).join(format!("{drive_id}.migration.json"))
}

/// The folder of the encryption's local state files: beside the drives file.
fn state_dir(s: &DriveState) -> PathBuf {
    s.drives_file
        .as_ref()
        .and_then(|file| file.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(std::env::temp_dir)
        .join("encryption")
}

// ==== The recompression pass ====

/// Seconds without input before the pass starts (it stops at the first input).
const RECOMPRESS_IDLE_SECS: u64 = 300;
/// How often the timer looks.
const RECOMPRESS_CHECK_MS: u64 = 60_000;
/// One pass at a time.
static RECOMPRESSING: AtomicBool = AtomicBool::new(false);

/// Whether the pass may run: idle long enough, on mains power. A platform azul cannot read
/// answers "on battery, just used" ([`PowerState::query`]), so the pass waits there.
fn idle_on_mains() -> bool {
    PowerState::query().is_idle_on_mains(RECOMPRESS_IDLE_SECS)
}

/// Starts the timer that starts the pass (from the window's start).
pub(crate) fn start_recompression(info: &mut CallbackInfo, app: &RefAny) {
    if !offered() {
        return;
    }
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(app.clone(), on_recompress_timer, get_time).with_interval(
            Duration::System(SystemTimeDiff::from_millis(RECOMPRESS_CHECK_MS)),
        ),
    );
}

extern "C" fn on_recompress_timer(mut data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
    if !offered() {
        return TimerCallbackReturn::terminate_unchanged();
    }
    if RECOMPRESSING.load(Ordering::SeqCst) || !idle_on_mains() {
        return TimerCallbackReturn::continue_unchanged();
    }
    let mut callback_info = info.callback_info;
    let app = data.clone();
    let Some(mut s) = data.downcast_mut::<DriveState>() else {
        return TimerCallbackReturn::continue_unchanged();
    };
    let found = s.slots.iter().find_map(|slot| {
        let auto = slot.auto.clone()?;
        (auto.is_encrypted() == Some(true)).then(|| (slot.entry.id.clone(), auto))
    });
    let Some((drive_id, auto)) = found else {
        return TimerCallbackReturn::continue_unchanged();
    };
    let state_file = state_dir(&*s).join(format!("{drive_id}.recompress.json"));
    RECOMPRESSING.store(true, Ordering::SeqCst);
    spawn(
        &mut callback_info,
        &app,
        &mut *s,
        Job::Encryption(EncryptionJob::Recompress {
            drive_id,
            auto,
            state_file,
        }),
    );
    TimerCallbackReturn::continue_unchanged()
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
    /// The sheet of a key rotation's new code: re-encryption is offered next (else the
    /// migration of a newly encrypted drive starts).
    pub after_rotation: bool,
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
            after_rotation: false,
        }
    }

    /// The sheet of a key rotation's new code.
    #[must_use]
    pub(crate) fn after_rotation(mut self) -> Sheet {
        self.after_rotation = true;
        self
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
    /// "I was hacked: new keys for this drive?"
    ConfirmRotate { drive_id: String },
    /// After a rotation: "Re-encrypt every file?"
    OfferReencrypt { drive_id: String },
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
        Dialog::ConfirmRotate { drive_id } => (
            format!("New keys for \"{}\"?", drive_name(s, drive_id)),
            column(vec![
                line(
                    "Use this when a computer, a phone or a key of this drive may be in someone \
                     else's hands.",
                ),
                line(
                    "1. The drive is locked down: every other computer, every key and every \
                     shared link loses access at once.",
                ),
                line(
                    "2. The drive gets a new key, and you get a NEW RECOVERY CODE. The old code \
                     stops working.",
                ),
                line(
                    "3. Your other computers join again with a new join code from this one; \
                     links are shared again; incoming mail gets a new drop key.",
                ),
                line(
                    "Then re-encrypting every file is recommended: afterwards nothing in the \
                     drive opens with the old key.",
                ),
                buttons(vec![
                    button("Cancel", app, on_cancel_popup),
                    typed_button(
                        "Lock down and change the keys",
                        ButtonType::Primary,
                        app,
                        on_rotate,
                    ),
                ]),
            ]),
        ),
        Dialog::OfferReencrypt { drive_id } => (
            String::from("Re-encrypt every file?"),
            column(vec![
                line(&format!(
                    "Recommended after a compromise. The files of \"{}\" are under the new key \
                     now, but each file still has its own old file key: whoever copied the \
                     drive's data and its old key before the lockdown could read those copies.",
                    drive_name(s, drive_id)
                )),
                line(
                    "Re-encrypting writes every file anew with new keys. It runs in the \
                     background and continues where it stopped if AzDrive closes.",
                ),
                buttons(vec![
                    button("Later", app, on_cancel_popup),
                    typed_button(
                        "Re-encrypt everything",
                        ButtonType::Primary,
                        app,
                        on_reencrypt,
                    ),
                ]),
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

/// "I was hacked: new keys...": the question.
pub(crate) fn ask_rotate(s: &mut DriveState, drive_id: &str) {
    if s.popup.is_none() {
        s.popups_opened += 1;
        s.popup = Some(Popup::Encryption(Dialog::ConfirmRotate {
            drive_id: drive_id.to_string(),
        }));
    }
}

extern "C" fn on_rotate(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::Encryption(Dialog::ConfirmRotate { drive_id })) = s.popup.take() else {
            return;
        };
        let Some(auto) = auto_of(s, &drive_id) else {
            return;
        };
        let Some(azlin) = s
            .slot_index(&drive_id)
            .and_then(|index| s.slots[index].azlin.clone())
        else {
            s.error("Only an Azlin drive can be locked down.");
            return;
        };
        s.popup = Some(Popup::Encryption(Dialog::Busy {
            title: String::from("Changing the drive's keys"),
            text: String::from(
                "Locking the drive down, then making its new key and recovery code...",
            ),
        }));
        spawn(
            info,
            app,
            s,
            Job::Encryption(EncryptionJob::Rotate {
                drive_id,
                auto,
                azlin,
            }),
        );
    })
}

extern "C" fn on_reencrypt(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::Encryption(Dialog::OfferReencrypt { drive_id })) = s.popup.take() else {
            return;
        };
        let Some(auto) = auto_of(s, &drive_id) else {
            return;
        };
        let state_file = state_dir(s).join(format!("{drive_id}.reencrypt.json"));
        let name = drive_name(s, &drive_id);
        s.info(format!(
            "The files of \"{name}\" are being re-encrypted in the background."
        ));
        spawn(
            info,
            app,
            s,
            Job::Encryption(EncryptionJob::Reencrypt {
                drive_id,
                auto,
                state_file,
            }),
        );
    })
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
        if sheet.after_rotation {
            s.popup = Some(Popup::Encryption(Dialog::OfferReencrypt { drive_id }));
            return;
        }
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
    /// The recompression pass, while the computer stays idle on mains; the state in
    /// `state_file`.
    Recompress {
        drive_id: String,
        auto: Arc<AutoEncrypted>,
        state_file: PathBuf,
    },
    /// "I was hacked": the lockdown (through the Azlin drive, which keeps its new grant), then
    /// the key rotation (or the rest of one that stopped).
    Rotate {
        drive_id: String,
        auto: Arc<AutoEncrypted>,
        azlin: Arc<azcloud_kit::AzlinDrive>,
    },
    /// Every file into a new object, the state in `state_file`.
    Reencrypt {
        drive_id: String,
        auto: Arc<AutoEncrypted>,
        state_file: PathBuf,
    },
}

/// What a rotation brings back to the UI thread.
pub(crate) struct RotationDone {
    /// The new recovery code, for the sheet. A secret: never printed.
    pub code: Zeroizing<String>,
    /// The new drop public key (hex) when incoming mail is on.
    pub drop_key: Option<String>,
    pub members_removed: usize,
    pub shares_revoked: usize,
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
    /// The pass stopped (`Ok(true)`: it finished).
    Recompressed {
        drive_id: String,
        result: Result<bool, String>,
    },
    Rotated {
        drive_id: String,
        result: Result<RotationDone, String>,
    },
    Reencrypted {
        drive_id: String,
        result: Result<ReencryptState, String>,
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
        EncryptionJob::Recompress {
            drive_id,
            auto,
            state_file,
        } => {
            let result = (|| -> Result<bool, String> {
                let provider = index_provider()
                    .ok_or_else(|| String::from("this build of AzDrive has no drive index"))?;
                let drive = open_encrypted(
                    Arc::clone(auto.bucket()),
                    &keyring,
                    auto.drive(),
                    provider.as_ref(),
                )
                .map_err(|e| e.to_string())?;
                let mut state = std::fs::read_to_string(&state_file)
                    .ok()
                    .and_then(|text| RecompressState::from_json(&text).ok())
                    .unwrap_or_default();
                if let Some(dir) = state_file.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                let mut save = |state: &RecompressState| {
                    std::fs::write(&state_file, state.to_json())
                        .map_err(|e| azul_storage::DriveError::Io(e.to_string()))
                };
                let stop = || !idle_on_mains();
                run_pass(
                    &drive,
                    &mut state,
                    &RecompressPolicy::default(),
                    now_unix(),
                    &mut save,
                    &stop,
                )
                .map_err(|e| e.to_string())
            })();
            EncryptionOutcome::Recompressed { drive_id, result }
        }
        EncryptionJob::Rotate {
            drive_id,
            auto,
            azlin,
        } => {
            let result = (|| -> Result<RotationDone, String> {
                let provider = index_provider()
                    .ok_or_else(|| String::from("this build of AzDrive has no drive index"))?;
                let resuming = rotation::pending(auto.bucket().as_ref())
                    .map_err(|e| e.to_string())?
                    .is_some();
                if !resuming {
                    azlin.lockdown().map_err(|e| e.to_string())?;
                }
                let kdf = RecoveryKdf::fresh().map_err(|e| e.to_string())?;
                let rotated = rotation::rotate(
                    Arc::clone(auto.bucket()),
                    &keyring,
                    auto.drive(),
                    provider.as_ref(),
                    kdf,
                )
                .map_err(|e| e.to_string())?;
                auto.reopen();
                Ok(RotationDone {
                    code: rotated.recovery_code.to_text(),
                    drop_key: rotated.drop_key.map(|key| key.to_hex()),
                    members_removed: rotated.members_removed,
                    shares_revoked: rotated.shares_revoked,
                })
            })();
            EncryptionOutcome::Rotated { drive_id, result }
        }
        EncryptionJob::Reencrypt {
            drive_id,
            auto,
            state_file,
        } => {
            let result = (|| -> Result<ReencryptState, String> {
                let provider = index_provider()
                    .ok_or_else(|| String::from("this build of AzDrive has no drive index"))?;
                let drive = open_encrypted(
                    Arc::clone(auto.bucket()),
                    &keyring,
                    auto.drive(),
                    provider.as_ref(),
                )
                .map_err(|e| e.to_string())?;
                let mut state = std::fs::read_to_string(&state_file)
                    .ok()
                    .and_then(|text| ReencryptState::from_json(&text).ok())
                    .unwrap_or_else(|| ReencryptState::new(now_unix()));
                if let Some(dir) = state_file.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                let mut save = |state: &ReencryptState| {
                    std::fs::write(&state_file, state.to_json())
                        .map_err(|e| azul_storage::DriveError::Io(e.to_string()))
                };
                reencrypt_pass(&drive, &mut state, &mut save, &|| false)
                    .map_err(|e| e.to_string())?;
                // Done: the next "Re-encrypt" starts a new pass.
                let _ = std::fs::remove_file(&state_file);
                Ok(state)
            })();
            EncryptionOutcome::Reencrypted { drive_id, result }
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
        // Quiet either way: the pass runs while nobody looks, and an error (no answer from the
        // bucket) is tried again at the next idle minute, from where it stopped.
        EncryptionOutcome::Recompressed { drive_id, result } => {
            RECOMPRESSING.store(false, Ordering::SeqCst);
            if let Err(why) = result {
                eprintln!("AZDRIVE_RECOMPRESS_STOPPED {drive_id}: {why}");
            }
        }
        EncryptionOutcome::Rotated { drive_id, result } => match result {
            Ok(done) => {
                let name = drive_name(s, &drive_id);
                let mail = if done.drop_key.is_some() {
                    " Incoming mail has a new drop key: give it to your mail Worker (AzMail, or \
                     azcloud mail-drop)."
                } else {
                    ""
                };
                s.info(format!(
                    "\"{name}\" is locked down and has new keys: {} other computers and invites \
                     removed, {} shared links revoked.{mail}",
                    done.members_removed, done.shares_revoked
                ));
                s.popup = Some(Popup::Encryption(Dialog::Sheet(
                    Sheet::new(&drive_id, done.code).after_rotation(),
                )));
                crate::refresh(info, app, s);
            }
            Err(why) => {
                s.popup = Some(Popup::Encryption(Dialog::Message {
                    title: String::from("The keys were not changed"),
                    text: format!(
                        "{why}. A rotation that stopped continues where it stopped when you \
                         try again on this computer."
                    ),
                }));
            }
        },
        EncryptionOutcome::Reencrypted { drive_id, result } => {
            let name = drive_name(s, &drive_id);
            match result {
                Ok(state) => {
                    let failed = if state.failed > 0 {
                        format!(
                            " {} damaged files were left as they were.",
                            state.failed
                        )
                    } else {
                        String::new()
                    };
                    s.info(format!(
                        "{} files of \"{name}\" have new keys: nothing in the drive opens with \
                         the old key any more.{failed}",
                        state.done
                    ));
                }
                Err(why) => s.error(format!(
                    "Re-encrypting \"{name}\" stopped: {why}. It continues where it stopped the \
                     next time."
                )),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An encrypted drive's search index (the plain text of its files) lives in the drive's own
    /// cache folder, beside its drive index - one folder per drive, gone with it - never in the
    /// cache the other drives share.
    #[test]
    fn an_encrypted_drives_search_index_lives_in_the_drives_own_cache_folder() {
        let cache = PathBuf::from("/cache/AzDrive/drive-index");
        let a = search_index_dir(Some(cache.clone()), "drive-a").expect("a folder");
        let b = search_index_dir(Some(cache.clone()), "drive-b").expect("a folder");
        assert!(a.starts_with(&cache) && a.ends_with("search"), "{}", a.display());
        assert_ne!(a, b);
        assert_eq!(a.parent().and_then(std::path::Path::parent), Some(cache.as_path()));
        assert_eq!(search_index_dir(None, "drive-a"), None, "no cache folder, no index");
    }

    /// This computer's copies of the drives' indexes (and the search indexes beside them) live
    /// in the run's cache folder - `--cache-dir`, else `<cache>/AzDrive` - so a test run keeps
    /// them in its own folder; a run without one (`--shot`) keeps them in memory.
    #[test]
    fn the_drive_index_copies_live_in_the_runs_cache_folder() {
        let cache = PathBuf::from("/run/cache");
        assert_eq!(drive_index_root_in(Some(&cache)), Some(cache.join("drive-index")));
        assert_eq!(drive_index_root_in(None), None);
    }

    #[test]
    fn a_rotations_sheet_offers_re_encryption_next() {
        let code = RecoveryCode::from_bytes([0x5A; 16]);
        assert!(!Sheet::new("d_1", code.to_text()).after_rotation);
        assert!(Sheet::new("d_1", code.to_text()).after_rotation().after_rotation);
    }

    #[test]
    fn the_recompression_pass_waits_for_mains_power_and_five_idle_minutes() {
        let at = |on_mains, idle_secs| PowerState { on_mains, idle_secs }.is_idle_on_mains(RECOMPRESS_IDLE_SECS);
        assert!(at(true, 300));
        assert!(!at(true, 299), "used a moment ago");
        assert!(!at(false, 3_600), "on battery");
    }

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
    fn with_the_drive_index_the_flows_are_offered() {
        assert!(offered());
    }
}
