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
//!
//! The same timer keeps the drive index small. In an idle minute on mains power, before the
//! pass, an encrypted drive whose index was not maintained from this computer in the last six
//! hours gets one maintenance round (azul-storage's `MetaIndexProvider::maintain`). The round
//! runs under the bucket's lease, so only one computer runs it at a time. It folds the index's
//! packs into one, writes a checkpoint and deletes what was retired a day ago. One thing runs
//! at a time: the round or the pass.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
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
use azcloud_kit::{TokenError, TokenServer};
use azul_storage::{
    azul_keyring::AzulKeyring,
    azul_transport::AzulTransport,
    crypto::{
        device,
        keys::{RecoveryCode, RecoveryKdf},
        random_bytes, Zeroizing,
    },
    encrypted::{open_encrypted, AutoEncrypted, IndexProvider},
    keyring::KeyringStore,
    meta::{Maintained, Maintenance, MetaIndexProvider},
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
    Some(Arc::new(meta_provider(index_cache_root())))
}

/// Where this computer keeps its copies of the drive indexes: `<cache>/AzDrive/drive-index`.
fn index_cache_root() -> Option<PathBuf> {
    crate::path_of(azul::file::FilePath::get_cache_dir().into_option())
        .map(|dir| dir.join("AzDrive").join("drive-index"))
}

/// The drive index's provider with this computer's copies under `cache_root`. The copies
/// are lazy (C6): opening reads the pack indexes only, and a folder's objects arrive in
/// ranged chunk reads when it is browsed, so a computer new to a drive browses it before
/// every pack has downloaded.
fn meta_provider(cache_root: Option<PathBuf>) -> MetaIndexProvider {
    MetaIndexProvider::new("AzDrive")
        .with_cache_root(cache_root)
        .with_lazy(true)
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

/// Starts the one timer that starts the pass and the drive index's maintenance rounds (from
/// the window's start).
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
    let encrypted: Vec<(String, Arc<AutoEncrypted>)> = s
        .slots
        .iter()
        .filter_map(|slot| {
            let auto = slot.auto.clone()?;
            (auto.is_encrypted() == Some(true)).then(|| (slot.entry.id.clone(), auto))
        })
        .collect();
    // The drive index's upkeep first: a short round, once every six hours per drive.
    let now = now_unix();
    let running = MAINTAINING.load(Ordering::SeqCst);
    let due = encrypted
        .iter()
        .find(|(drive_id, _)| maintenance_due(now, maintained_at(drive_id), running));
    if let Some((drive_id, auto)) = due.cloned() {
        MAINTAINING.store(true, Ordering::SeqCst);
        MAINTAINED_AT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(drive_id.clone(), now);
        spawn(
            &mut callback_info,
            &app,
            &mut *s,
            Job::Encryption(EncryptionJob::Maintain { drive_id, auto }),
        );
        return TimerCallbackReturn::continue_unchanged();
    }
    if running {
        return TimerCallbackReturn::continue_unchanged();
    }
    let Some((drive_id, auto)) = encrypted.into_iter().next() else {
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

// ==== The drive index's upkeep ====

/// How long after a drive's index was maintained from this computer the next round is due.
const MAINTAIN_EVERY_SECS: u64 = 6 * 3_600;

/// One round at a time (beside the recompression pass, never during it).
static MAINTAINING: AtomicBool = AtomicBool::new(false);
/// When a round of each drive's index last started from this computer (seconds since 1970),
/// by drive id. Kept in memory only, so the first idle minute after a start runs one round.
static MAINTAINED_AT: Mutex<BTreeMap<String, u64>> = Mutex::new(BTreeMap::new());

/// Whether a drive's index is due for a maintenance round at `now`: none running, and none
/// started from this computer in the last [`MAINTAIN_EVERY_SECS`] (`last`).
fn maintenance_due(now: u64, last: Option<u64>, running: bool) -> bool {
    !running && last.map_or(true, |last| now.saturating_sub(last) >= MAINTAIN_EVERY_SECS)
}

/// When the round of the drive `drive_id`'s index last started from this computer.
fn maintained_at(drive_id: &str) -> Option<u64> {
    MAINTAINED_AT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(drive_id)
        .copied()
}

/// One maintenance round of the drive `drive`'s index in `bucket`
/// (`MetaIndexProvider::maintain`), with this computer's key from `keyring`. `None`: there
/// is no index yet, or another computer's round holds the lease.
fn run_maintenance(
    provider: &MetaIndexProvider,
    bucket: &Arc<dyn Drive>,
    keyring: &dyn KeyringStore,
    drive: &str,
    rules: &Maintenance,
) -> Result<Option<Maintained>, String> {
    let drive_key = device::unlock(bucket.as_ref(), keyring, drive)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("this computer has no key for \"{drive}\""))?;
    provider
        .maintain(Arc::clone(bucket), &drive_key, rules)
        .map_err(|e| e.to_string())
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
    /// "Lock down with the recovery code...": the code typed, signing a lockdown without a
    /// drive token.
    RecoveryLockdown {
        drive_id: String,
        typed: Zeroizing<String>,
        error: String,
    },
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
        Dialog::RecoveryLockdown {
            drive_id, error, ..
        } => {
            let mut body = column(vec![
                line(
                    "From a computer that lost the drive (its devices were taken over), the \
                     recovery code locks it down: it takes no writes for 48 hours, during which \
                     a device of the owner may cancel; then every other device and key loses \
                     the drive and this computer keeps it.",
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
                typed_button("Lock down", ButtonType::Primary, app, on_recovery_lockdown),
            ]));
            (
                format!("Lock \"{}\" down with the recovery code", drive_name(s, drive_id)),
                body,
            )
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
        Some(Popup::Encryption(
            Dialog::Unlock { typed, error, .. } | Dialog::RecoveryLockdown { typed, error, .. },
        )) => {
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
        let code = RecoveryCode::parse(&sheet.code);
        // The token server's recovery key from this code: what a lockdown without a drive token
        // is signed with ("Lock down with the recovery code...").
        if let Some(code) = code {
            register_recovery_key(info, app, s, &drive_id, &code);
        }
        let Some(Popup::Encryption(Dialog::Sheet(sheet))) = s.popup.as_ref() else {
            return;
        };
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
    /// The drive's recovery key (from its recovery code) registered at its token server - a
    /// grant: under the drive's keyring lock with its newest drive token.
    RegisterRecoveryKey {
        drive_id: String,
        public_key: String,
        token_url: String,
        keyring: azcloud_kit::SharedKeyring,
    },
    /// "Lock down with the recovery code": the lockdown signed with the drive's recovery key,
    /// no drive token sent; the pending family's token kept as this computer's session.
    RecoveryLockdown {
        drive_id: String,
        code: RecoveryCode,
        token_url: String,
        keyring: azcloud_kit::SharedKeyring,
    },
    /// A maintenance round of the drive's index, in idle time on mains power.
    Maintain {
        drive_id: String,
        auto: Arc<AutoEncrypted>,
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
    RecoveryKeyRegistered {
        drive_id: String,
        result: Result<(), String>,
    },
    /// The recovery-key lockdown pending until then (seconds since 1970), or why not.
    LockedDown {
        drive_id: String,
        result: Result<Option<u64>, String>,
    },
    /// What the round did (`None`: no index yet, or another computer's round was running).
    Maintained {
        drive_id: String,
        result: Result<Option<Maintained>, String>,
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
        EncryptionJob::RegisterRecoveryKey {
            drive_id,
            public_key,
            token_url,
            keyring,
        } => {
            let transport = AzulTransport::new(crate::USER_AGENT);
            let result = TokenServer::new(&token_url, &transport)
                .map_err(|e| e.to_string())
                .and_then(|server| {
                    keyring
                        .with_drive_token(&drive_id, |token| {
                            server.set_recovery_key(&drive_id, token, &public_key)
                        })
                        .map_err(|e| e.to_string())
                        .and_then(|answer| answer.map_err(|e| e.to_string()))
                });
            EncryptionOutcome::RecoveryKeyRegistered { drive_id, result }
        }
        EncryptionJob::RecoveryLockdown {
            drive_id,
            code,
            token_url,
            keyring,
        } => {
            let result = recovery_lockdown(&drive_id, &code, &token_url, &keyring);
            EncryptionOutcome::LockedDown { drive_id, result }
        }
        EncryptionJob::Maintain { drive_id, auto } => {
            let result = run_maintenance(
                &meta_provider(index_cache_root()),
                auto.bucket(),
                &keyring,
                auto.drive(),
                &Maintenance::default(),
            );
            EncryptionOutcome::Maintained { drive_id, result }
        }
    }
}

/// The token server's recovery key of `drive_id` from its recovery `code`
/// ([`azcloud_kit::RecoveryKey`]).
pub(crate) fn recovery_key_of(code: &RecoveryCode, drive_id: &str) -> azcloud_kit::RecoveryKey {
    azcloud_kit::RecoveryKey::derive(code.as_bytes(), drive_id)
}

/// The drive's token server, from its entry (else this run's).
fn token_url_of(s: &DriveState, drive_id: &str) -> Option<String> {
    let fallback = s.token.url.clone();
    s.slot_index(drive_id)
        .and_then(|index| crate::periods::azlin_drive(&s.slots[index].entry, fallback.as_deref()))
        .map(|(_, url)| url)
}

/// Registers the recovery key of `code` for `drive_id` at its token server, in the background.
fn register_recovery_key(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    code: &RecoveryCode,
) {
    let Some(token_url) = token_url_of(s, drive_id) else {
        return;
    };
    let job = EncryptionJob::RegisterRecoveryKey {
        drive_id: drive_id.to_string(),
        public_key: recovery_key_of(code, drive_id).public_base64(),
        token_url,
        keyring: s.keyring.clone(),
    };
    spawn(info, app, s, Job::Encryption(job));
}

/// A lockdown signed with the drive's recovery key (a fresh nonce, no drive token); the
/// pending family's token kept as this computer's session (its credentials come with the
/// first refresh, once the lockdown takes effect).
fn recovery_lockdown(
    drive_id: &str,
    code: &RecoveryCode,
    token_url: &str,
    keyring: &azcloud_kit::SharedKeyring,
) -> Result<Option<u64>, String> {
    let transport = AzulTransport::new(crate::USER_AGENT);
    let server = TokenServer::new(token_url, &transport).map_err(|e| e.to_string())?;
    let key = recovery_key_of(code, drive_id);
    let pending = server
        .recovery_lockdown(drive_id, |message| Ok(key.sign_base64(message)))
        .map_err(|e| match &e {
            TokenError::Refused { status: 401, .. } => String::from(
                "That recovery code does not match the drive's recovery key.",
            ),
            TokenError::Refused { code, .. } if code == "no_recovery_key" => String::from(
                "The drive has no recovery key at its token server (its recovery sheet was \
                 made before AzDrive registered one).",
            ),
            _ => e.to_string(),
        })?;
    let entry = azul_storage::config::keyring_key(drive_id);
    let _lock = keyring.lock(&entry).map_err(|e| e.to_string())?;
    let session = azcloud_kit::AzlinSession {
        drive_id: drive_id.to_string(),
        drive_token: pending.drive_token.clone(),
        access_key_id: String::new(),
        secret_access_key: String::new(),
        session_token: None,
        expires_at: Some(0),
    };
    keyring
        .set(&entry, &session.to_keyring_secret())
        .map_err(|e| e.to_string())?;
    Ok(pending.pending_until)
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
        EncryptionOutcome::RecoveryKeyRegistered { drive_id, result } => match result {
            Ok(()) => println!("AZDRIVE_RECOVERY_KEY {drive_id}"),
            Err(why) => {
                let name = drive_name(s, &drive_id);
                s.warn(format!(
                    "The recovery code of \"{name}\" could not be registered for a lockdown \
                     ({why}): it still unlocks the drive's files."
                ));
            }
        },
        EncryptionOutcome::LockedDown { drive_id, result } => {
            let name = drive_name(s, &drive_id);
            s.popup = Some(Popup::Encryption(match result {
                Ok(until) => {
                    let until = until.map_or_else(String::new, |at| {
                        format!(" until {}", azul_storage::time::iso8601(at))
                    });
                    println!("AZDRIVE_RECOVERY_LOCKDOWN {drive_id}");
                    Dialog::Message {
                        title: format!("\"{name}\" is locked down"),
                        text: format!(
                            "The lockdown with the recovery code is pending{until}: the drive \
                             takes no writes, and a device of the owner may still cancel it. \
                             Then every other device and key loses the drive, and this \
                             computer keeps it."
                        ),
                    }
                }
                Err(why) => Dialog::Message {
                    title: String::from("The drive was not locked down"),
                    text: why,
                },
            }));
        }
        // Quiet, like the recompression pass: a round that stopped runs again six hours on.
        EncryptionOutcome::Maintained { drive_id, result } => {
            MAINTAINING.store(false, Ordering::SeqCst);
            if let Err(why) = result {
                eprintln!("AZDRIVE_MAINTAIN_STOPPED {drive_id}: {why}");
            }
        }
    }
}

/// "Lock down with the recovery code...": the dialog.
pub(crate) fn ask_recovery_lockdown(s: &mut DriveState, drive_id: &str) {
    if s.popup.is_none() {
        s.popups_opened += 1;
        s.popup = Some(Popup::Encryption(Dialog::RecoveryLockdown {
            drive_id: drive_id.to_string(),
            typed: Zeroizing::new(String::new()),
            error: String::new(),
        }));
    }
}

extern "C" fn on_recovery_lockdown(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::Encryption(Dialog::RecoveryLockdown {
            drive_id, typed, ..
        })) = s.popup.as_ref()
        else {
            return;
        };
        let drive_id = drive_id.clone();
        let code = RecoveryCode::parse(typed);
        let token_url = token_url_of(s, &drive_id);
        let (Some(code), Some(token_url)) = (code, token_url) else {
            if let Some(Popup::Encryption(Dialog::RecoveryLockdown { error, .. })) =
                s.popup.as_mut()
            {
                *error = String::from(
                    "That is not a recovery code (26 letters and digits, in five groups), or \
                     the drive's token server is not known.",
                );
            }
            return;
        };
        s.popup = Some(Popup::Encryption(Dialog::Busy {
            title: String::from("Locking the drive down"),
            text: String::from("Signing the lockdown with the recovery code..."),
        }));
        let job = EncryptionJob::RecoveryLockdown {
            drive_id,
            code,
            token_url,
            keyring: s.keyring.clone(),
        };
        spawn(info, app, s, Job::Encryption(job));
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drives_recovery_key_at_the_token_server_comes_from_its_recovery_code() {
        // The key a lockdown without a drive token is signed with: derived from the code on
        // the sheet (azcloud-kit's RecoveryKey), the same wherever the code is typed.
        let code = RecoveryCode::from_bytes([0x5A; 16]);
        assert_eq!(
            recovery_key_of(&code, "d_1").public_base64(),
            "TZrvbj1nF30/6IIsz2wc36FCNEID9Pu4HUE9+wjQ7Wo="
        );
        let typed = RecoveryCode::parse(&code.to_text()).unwrap();
        assert_eq!(
            recovery_key_of(&typed, "d_1").public_base64(),
            recovery_key_of(&code, "d_1").public_base64(),
            "the code as typed back"
        );
        assert_ne!(
            recovery_key_of(&code, "d_2").public_base64(),
            recovery_key_of(&code, "d_1").public_base64()
        );
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

    /// C6 in AzDrive: a computer new to an encrypted drive lists a folder from the index's
    /// pack indexes and the chunks it needs (ranged reads), before any pack is read whole.
    #[test]
    fn a_new_computer_lists_an_encrypted_drive_before_reading_a_whole_pack() {
        use azul_storage::{
            crypto::DriveKey, encrypted::EncryptedDrive, meta::MemoryBucket, ListRequest,
        };
        let bucket = Arc::new(MemoryBucket::new());
        let key = DriveKey::generate().unwrap();
        // Another computer wrote the drive: five files, five packs of the index.
        let index = meta_provider(None)
            .open_index("d_lazy", bucket.clone(), &key)
            .unwrap();
        let writer = EncryptedDrive::new(bucket.clone() as Arc<dyn Drive>, key.clone(), index);
        for i in 0..5 {
            writer.put(&format!("docs/{i}.txt"), b"x").unwrap();
        }

        let before = bucket.whole_reads().len();
        let index = meta_provider(None)
            .open_index("d_lazy", bucket.clone(), &key)
            .unwrap();
        let drive = EncryptedDrive::new(bucket.clone() as Arc<dyn Drive>, key, index);
        let page = drive.list(&ListRequest::folder("docs/")).unwrap();
        assert_eq!(page.objects.len(), 6, "the folder's marker and five files");
        let packs_read = bucket.whole_reads()[before..]
            .iter()
            .filter(|key| key.ends_with(".pack"))
            .count();
        assert_eq!(packs_read, 0, "no pack read whole before the first listing");
        assert!(bucket.counts().range_reads > 0);
    }

    #[test]
    fn a_drive_index_is_maintained_once_every_six_hours_and_one_round_at_a_time() {
        let now = 1_760_000_000;
        assert!(maintenance_due(now, None, false), "never maintained from here");
        assert!(!maintenance_due(now, Some(now - 60), false), "a minute ago");
        assert!(!maintenance_due(now, Some(now - 6 * 3_600 + 1), false));
        assert!(maintenance_due(now, Some(now - 6 * 3_600), false), "six hours ago");
        assert!(!maintenance_due(now, None, true), "a round is running");
    }

    #[test]
    fn an_idle_rounds_maintenance_folds_the_drive_indexs_packs_into_one() {
        use azul_storage::{
            crypto::DriveKey, encrypted::EncryptedDrive, keyring::MemoryKeyring,
            meta::MemoryBucket, ListRequest,
        };
        let bucket: Arc<dyn Drive> = Arc::new(MemoryBucket::new());
        let keyring = MemoryKeyring::new();
        let key = DriveKey::generate().unwrap();
        device::store_drive_key(&keyring, "d_upkeep", &key).unwrap();
        let index = meta_provider(None)
            .open_index("d_upkeep", Arc::clone(&bucket), &key)
            .unwrap();
        let drive = EncryptedDrive::new(Arc::clone(&bucket), key, index);
        for i in 0..3 {
            drive.put(&format!("docs/{i}.txt"), b"x").unwrap();
        }
        let rules = Maintenance {
            compact_at_packs: 2,
            ..Maintenance::default()
        };
        let done = run_maintenance(&meta_provider(None), &bucket, &keyring, "d_upkeep", &rules)
            .unwrap()
            .expect("no other computer holds the lease");
        assert!(done.compacted);
        let page = drive.list(&ListRequest::folder("docs/")).unwrap();
        assert_eq!(page.objects.len(), 4, "the folder's marker and three files");

        // A computer without the drive's key maintains nothing.
        let stranger = MemoryKeyring::new();
        assert!(run_maintenance(&meta_provider(None), &bucket, &stranger, "d_upkeep", &rules).is_err());
    }
}
