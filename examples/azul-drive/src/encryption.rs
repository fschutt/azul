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
//! a minute; when the computer has been idle for five minutes on mains power and on a network
//! that costs nothing (azul's `PowerState` and `NetworkState`: not metered, not Low Data
//! Mode), the first open encrypted drive's files are written again, smaller, on a worker
//! thread, and the pass stops at the first input, when the power cord goes or when the network
//! starts to cost. Its state sits beside the migration's, so the next idle minute continues
//! where it stopped.
//!
//! The same timer keeps the drive index small. In such a minute (idle, on mains, on a free
//! network), before the pass, an encrypted drive whose index was not maintained from this
//! computer in the last six hours gets one maintenance round (azul-storage's
//! `MetaIndexProvider::maintain`). The round runs under the bucket's lease, so only one
//! computer runs it at a time. It folds the index's packs into one, writes a checkpoint and
//! deletes what was retired a day ago. One thing runs at a time: the round or the pass.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
};

use azul::{
    callbacks::{TextInputOnTextInputCallbackType, TimerCallbackInfo, TimerCallbackReturn},
    prelude::*,
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    widgets::{ButtonType, OnTextInputReturn, TextInputState, TextInputValid},
    window::{NetworkState, PowerState},
};
use azcloud_kit::{TokenError, TokenServer};
use azul_storage::{
    azul_keyring::AzulKeyring,
    azul_transport::AzulTransport,
    crypto::{
        device,
        keys::{load_recovery_wrap, RecoveryCode, RecoveryKdf},
        random_bytes, CryptoError, Zeroizing,
    },
    encrypted::{open_encrypted, AutoEncrypted, IndexProvider},
    keyring::KeyringStore,
    meta::{merge::keep_both, DriveBucket, Maintained, Maintenance, MetaIndexProvider, MetaRepo},
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
    Some(Arc::new(meta_provider(drive_index_root())))
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

/// The run's cache folder (`--cache-dir`, else `<cache>/AzDrive`; `None` in a `--shot` run
/// without the switch), set once at the start.
static CACHE_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Sets the run's cache folder (the start, once): the drives' index copies live in it.
pub(crate) fn set_cache_dir(dir: Option<PathBuf>) {
    let _ = CACHE_DIR.set(dir);
    // Print's copies of an emergency kit a run before left behind.
    crate::recovery::forget_print_copies();
}

/// The run's cache folder: `--cache-dir`, else `<cache>/AzDrive` (before the start set it too);
/// `None` in a `--shot` run without the switch.
pub(crate) fn run_cache_dir() -> Option<PathBuf> {
    match CACHE_DIR.get() {
        Some(dir) => dir.clone(),
        None => crate::path_of(azul::file::FilePath::get_cache_dir().into_option())
            .map(|dir| dir.join("AzDrive")),
    }
}

/// The folder of this computer's copies of the encrypted drives' indexes: `drive-index/` in the
/// run's cache folder (before the start set it: in `<cache>/AzDrive`).
pub(crate) fn drive_index_root() -> Option<PathBuf> {
    drive_index_root_in(run_cache_dir().as_deref())
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

/// Where the encrypted drive `drive`'s local copies kept encrypted (its AZL1 objects) are: in
/// the drive's own cache folder under `root`, beside its drive index and search index
/// (`<root>/<hash of the drive>/objects`); `None` without a cache folder (none are kept).
pub(crate) fn objects_dir(root: Option<PathBuf>, drive: &str) -> Option<PathBuf> {
    MetaIndexProvider::new("AzDrive")
        .with_cache_root(root)
        .drive_cache_dir(drive)
        .map(|dir| dir.join("objects"))
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

/// Whether the pass may run now ([`recompress_allowed`] of the power and the network now).
fn idle_on_mains() -> bool {
    recompress_allowed(PowerState::query(), NetworkState::query())
}

/// Whether the pass may run on `power` and `network`: idle long enough, on mains power, and on
/// a network that costs the user nothing - the pass rewrites every file of the drive. A
/// platform azul cannot read answers "on battery, just used" ([`PowerState::query`]), so the
/// pass waits there; a network it cannot read counts as free ([`NetworkState::query`]).
fn recompress_allowed(power: PowerState, network: NetworkState) -> bool {
    power.is_idle_on_mains(RECOMPRESS_IDLE_SECS) && network.allows_background_transfer()
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
    // A recovery drill that is due opens (once a minute, when no dialog shows).
    if let Some(mut s) = data.downcast_mut::<DriveState>() {
        if crate::recovery::drill_if_due(&mut s, now_unix()) {
            return TimerCallbackReturn::continue_and_refresh_dom();
        }
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

/// How many groups of the code the sheet asks for (the plan's four, of five).
pub(crate) const SETUP_CHECKS: usize = 4;

/// The recovery sheet: the code, then four of its groups typed back.
pub(crate) struct Sheet {
    pub drive_id: String,
    /// The code as shown (`XXXXX-XXXXX-XXXXX-XXXXX-XXXXXX`). A secret: never printed.
    pub code: Zeroizing<String>,
    /// Which groups (0-based, in order) the user types back: [`SETUP_CHECKS`] of them, chosen
    /// at random.
    pub checks: Vec<usize>,
    /// What the user typed for each of them.
    pub typed: Vec<Zeroizing<String>>,
    pub error: String,
    /// The sheet of a key rotation's new code: re-encryption is offered next (else the
    /// migration of a newly encrypted drive starts).
    pub after_rotation: bool,
    /// What the emergency kit's last button did (printed, saved, why not).
    pub kit_note: String,
    /// The sheet of a further recovery code (a second kit): it only closes when its groups are
    /// typed back - the drive was set up long ago.
    pub extra: bool,
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
    /// The sheet of `code`, asking for [`SETUP_CHECKS`] of its groups chosen at random (a
    /// shuffle by the OS random source; the first groups when it fails).
    pub(crate) fn new(drive_id: &str, code: Zeroizing<String>) -> Sheet {
        let groups = code.split('-').count().max(1);
        let mut order: Vec<usize> = (0..groups).collect();
        let mut random = [0u8; 16];
        if random_bytes(&mut random).is_ok() {
            for i in (1..groups).rev() {
                order.swap(i, usize::from(random[i % random.len()]) % (i + 1));
            }
        }
        let mut checks: Vec<usize> = order.into_iter().take(SETUP_CHECKS.min(groups)).collect();
        checks.sort_unstable();
        let typed = checks.iter().map(|_| Zeroizing::new(String::new())).collect();
        Sheet {
            drive_id: drive_id.to_string(),
            code,
            checks,
            typed,
            error: String::new(),
            after_rotation: false,
            kit_note: String::new(),
            extra: false,
        }
    }

    /// The sheet of a further recovery code (a second kit).
    #[must_use]
    pub(crate) fn extra(mut self) -> Sheet {
        self.extra = true;
        self
    }

    /// The sheet of a key rotation's new code.
    #[must_use]
    pub(crate) fn after_rotation(mut self) -> Sheet {
        self.after_rotation = true;
        self
    }

    /// The groups (0-based) the user types back, in order.
    pub(crate) fn asked(&self) -> Vec<usize> {
        self.checks.clone()
    }

    /// What the user typed into the box of the `slot`-th group asked for.
    pub(crate) fn set_typed(&mut self, slot: usize, text: Zeroizing<String>) {
        if let Some(typed) = self.typed.get_mut(slot) {
            *typed = text;
        }
    }

    /// The groups asked for as people count them: `1, 3, 4 and 5`.
    pub(crate) fn asked_words(&self) -> String {
        let numbers: Vec<String> = self.checks.iter().map(|g| (g + 1).to_string()).collect();
        match numbers.split_last() {
            Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
            Some((last, _)) => last.clone(),
            None => String::new(),
        }
    }

    /// Whether every group typed is the one asked for.
    pub(crate) fn confirmed(&self) -> bool {
        let groups: Vec<&str> = self.code.split('-').collect();
        !self.checks.is_empty()
            && self.checks.iter().zip(&self.typed).all(|(&group, typed)| {
                groups
                    .get(group)
                    .is_some_and(|code| normalized(code) == normalized(typed))
            })
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
    /// "Do you still have your recovery kit?": the code typed for a drill.
    Drill {
        drive_id: String,
        typed: Zeroizing<String>,
        error: String,
    },
    /// Trusted contacts: the owner's shares, a contact's side, the recovery with two shares.
    Contacts(crate::recovery_contacts::Page),
    /// The drive's recovery keys: the kit's lookup, a second kit, a key removed.
    Keys(crate::recovery_keys::Page),
}

impl Dialog {
    /// Whether its close box and Escape take it away: not the recovery sheet, whose code shows
    /// only this once - the setup finishes when its groups are typed back.
    pub(crate) fn may_close(&self) -> bool {
        !matches!(self, Dialog::Sheet(_))
    }
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
            ]);
            // The emergency kit: Print, Save as PDF, Save to a USB stick, and its QR code.
            for piece in crate::recovery::kit_pieces(app, &sheet.code, &sheet.kit_note) {
                body.add_child(piece);
            }
            body.add_child(label(&format!(
                "To check that you have it, type groups {} of the code:",
                sheet.asked_words()
            )));
            let mut boxes = Dom::create_div().with_css("display: flex; flex-direction: row;");
            for (slot, (&group, typed)) in sheet.checks.iter().zip(&sheet.typed).enumerate() {
                boxes.add_child(
                    Dom::create_div()
                        .with_css("display: flex; flex-direction: column; margin-right: 8px;")
                        .with_child(label(&format!("Group {}", group + 1)))
                        .with_child(
                            TextInput::create()
                                .with_text(AzString::from(typed.as_str()))
                                .with_placeholder(AzString::from("XXXXX"))
                                .with_on_text_input(
                                    RefAny::new(GroupRef {
                                        app: app.clone(),
                                        slot,
                                    }),
                                    on_group_typed as TextInputOnTextInputCallbackType,
                                )
                                .dom()
                                .with_id(crate::ids::sheet_group(slot)),
                        ),
                );
            }
            body.add_child(boxes);
            if !sheet.error.is_empty() {
                body.add_child(line(&sheet.error).with_css("color: #C42B1C;"));
            }
            body.add_child(buttons(vec![typed_button(
                "I have written it down",
                ButtonType::Primary,
                app,
                on_sheet_done,
            )
            .with_id(crate::ids::SHEET_DONE)]));
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
                    .dom()
                    .with_id(crate::ids::UNLOCK_CODE),
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
        Dialog::Drill { .. } => crate::recovery::drill_parts(dialog, s, app),
        Dialog::Contacts(page) => crate::recovery_contacts::dialog_parts(page, s, app),
        Dialog::Keys(page) => crate::recovery_keys::dialog_parts(page, s, app),
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
/// [`auto_of`] for the recovery modules: the drive's `AutoEncrypted`, opened first; `None`
/// (with a message) when it cannot be opened yet.
pub(crate) fn auto_for(s: &mut DriveState, drive_id: &str) -> Option<Arc<AutoEncrypted>> {
    auto_of(s, drive_id)
}

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

// ==== "Restore as of..." ====

/// "Restore as of...": the drive's own restore, to run on the job's thread (the drive opened
/// first); `None` (with a message) when it cannot be opened.
pub(crate) fn restore_of(
    s: &mut DriveState,
    drive_id: &str,
) -> Option<crate::restore::EncryptedRestore> {
    let auto = auto_of(s, drive_id)?;
    Some(Box::new(move |as_of| restore_encrypted(&auto, as_of)))
}

/// An encrypted drive back as its metadata repository had it at `as_of` (seconds since 1970),
/// as one new commit ([`azcloud_kit::restore_drive_as_of`]) through a copy of the repository of
/// its own (in memory; the open drive's index sees the commit at its next poll): the files put
/// back or taken away. `None` for a plain drive.
fn restore_encrypted(auto: &AutoEncrypted, as_of: i64) -> Option<Result<usize, String>> {
    if auto.is_encrypted().is_none() {
        // The drive's first call decides whether it is encrypted.
        let _ = auto.head("");
    }
    if auto.is_encrypted() != Some(true) {
        return None;
    }
    let bucket = Arc::clone(auto.bucket());
    let restored = (|| -> Result<usize, String> {
        let key = device::unlock(bucket.as_ref(), &AzulKeyring::new(), auto.drive())
            .map_err(|e| e.to_string())?
            .ok_or_else(|| String::from("This device has no key for the drive."))?;
        let device_id = azul_storage::ids::new_uuid();
        let mut repo = MetaRepo::open(DriveBucket::new(bucket), key, &device_id, "AzDrive")
            .map_err(|e| e.to_string())?;
        let restore = azcloud_kit::restore_drive_as_of(&mut repo, as_of, &mut keep_both)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| String::from("The drive was made after that time."))?;
        Ok(restore.restored + restore.removed)
    })();
    auto.reopen();
    Some(restored)
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

pub(crate) extern "C" fn on_typed(
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
        Some(Popup::Encryption(
            Dialog::Unlock { typed, error, .. }
            | Dialog::RecoveryLockdown { typed, error, .. }
            | Dialog::Drill { typed, error, .. },
        )) => {
            *typed = text;
            error.clear();
        }
        _ => {}
    }
    keep
}

/// What a group's box on the recovery sheet carries: which of the groups asked for it is.
struct GroupRef {
    app: RefAny,
    slot: usize,
}

/// A group typed on the recovery sheet.
extern "C" fn on_group_typed(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let keep = OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    };
    let Some((mut app, slot)) = data
        .downcast_ref::<GroupRef>()
        .map(|group| (group.app.clone(), group.slot))
    else {
        return keep;
    };
    let Some(mut s) = app.downcast_mut::<DriveState>() else {
        return keep;
    };
    if let Some(Popup::Encryption(Dialog::Sheet(sheet))) = s.popup.as_mut() {
        sheet.set_typed(slot, Zeroizing::new(state.get_text().as_str().to_string()));
        sheet.error.clear();
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
                "Groups {} are not all the code's. Look at what you wrote down: the setup \
                 finishes when they are.",
                sheet.asked_words()
            );
            return;
        }
        let drive_id = sheet.drive_id.clone();
        if sheet.extra {
            // A second kit: its key is at the token server already; nothing else to set up.
            crate::recovery::forget_print_copies();
            println!("AZDRIVE_RECOVERY_CODE_VERIFIED {drive_id}");
            s.popup = None;
            s.success("The second recovery code works: keep its kit apart from the first.");
            return;
        }
        let code = RecoveryCode::parse(&sheet.code);
        // The sheet closes: Print's copies of the kit go; the setup's check is kept.
        crate::recovery::forget_print_copies();
        crate::recovery::setup_verified(s, &drive_id);
        crate::save_settings(info, app, s);
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
        /// The code's findable key (its public half) and the drive key that signs its
        /// registration: a computer that never had the drive finds it by it.
        findable: Option<(String, azcloud_kit::RecoveryKey)>,
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
    /// The emergency kit written into the folder picked (a USB stick).
    SaveKit {
        path: PathBuf,
        bytes: Zeroizing<Vec<u8>>,
    },
    /// A drill's code against the bucket's recovery wrap (a drive set up before the drills).
    CheckCode {
        drive_id: String,
        auto: Arc<AutoEncrypted>,
        code: RecoveryCode,
    },
    /// A task of the trusted contacts (keys in the keyring, a recovery's lockdown).
    Contacts(crate::recovery_contacts::ContactsJob),
    /// The drive's other devices counted (member wraps beside this computer's).
    CountDevices {
        drive_id: String,
        auto: Arc<AutoEncrypted>,
    },
    /// A task of the recovery keys (the lookup, a second kit, a removal).
    Keys(crate::recovery_keys::KeysJob),
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
        /// The findable key's registration: its public half, or why not.
        findable: Option<Result<String, String>>,
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
    /// The emergency kit is in the folder picked (its bytes), or why not.
    KitSaved {
        path: PathBuf,
        len: usize,
        result: Result<(), String>,
    },
    /// Whether a drill's code opens the bucket's recovery wrap; the code's public recovery key.
    CodeChecked {
        drive_id: String,
        recovery_key: String,
        result: Result<bool, String>,
    },
    /// What a task of the trusted contacts found.
    Contacts(crate::recovery_contacts::ContactsDone),
    /// How many other devices have the drive's key.
    DevicesCounted {
        drive_id: String,
        result: Result<u32, String>,
    },
    /// What a task of the recovery keys found.
    Keys(crate::recovery_keys::KeysDone),
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
            findable,
        } => {
            let transport = AzulTransport::new(crate::USER_AGENT);
            let server = TokenServer::new(&token_url, &transport).map_err(|e| e.to_string());
            let result = server.as_ref().map_err(Clone::clone).and_then(|server| {
                keyring
                    .with_drive_token(&drive_id, |token| {
                        server.set_recovery_key(&drive_id, token, &public_key)
                    })
                    .map_err(|e| e.to_string())
                    .and_then(|answer| answer.map_err(|e| e.to_string()))
            });
            // Then the code's findable key, signed by the drive key just registered.
            let findable = match (&result, &server, findable) {
                (Ok(()), Ok(server), Some((public, signer))) => Some(
                    keyring
                        .with_drive_token(&drive_id, |token| {
                            server.add_recovery_key(
                                &drive_id,
                                token,
                                &public,
                                crate::recovery_health::FINDABLE_LABEL,
                                |message| Ok(signer.sign_base64(message)),
                            )
                        })
                        .map_err(|e| e.to_string())
                        .and_then(|answer| {
                            answer.map_err(|e| crate::recovery_keys::token_text(&e))
                        })
                        .map(|_| public),
                ),
                _ => None,
            };
            EncryptionOutcome::RecoveryKeyRegistered {
                drive_id,
                result,
                findable,
            }
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
                &meta_provider(drive_index_root()),
                auto.bucket(),
                &keyring,
                auto.drive(),
                &Maintenance::default(),
            );
            EncryptionOutcome::Maintained { drive_id, result }
        }
        EncryptionJob::SaveKit { path, bytes } => EncryptionOutcome::KitSaved {
            result: crate::recovery::save_kit(&path, &bytes),
            len: bytes.len(),
            path,
        },
        EncryptionJob::CheckCode {
            drive_id,
            auto,
            code,
        } => {
            let recovery_key = recovery_key_of(&code, &drive_id).public_base64();
            let result = load_recovery_wrap(auto.bucket().as_ref())
                .map_err(|e| e.to_string())
                .and_then(|wrap| match wrap.open(auto.drive(), &code) {
                    Ok(_) => Ok(true),
                    Err(CryptoError::WrongKey) => Ok(false),
                    Err(e) => Err(e.to_string()),
                });
            EncryptionOutcome::CodeChecked {
                drive_id,
                recovery_key,
                result,
            }
        }
        EncryptionJob::Contacts(job) => {
            EncryptionOutcome::Contacts(crate::recovery_contacts::run(job))
        }
        EncryptionJob::CountDevices { drive_id, auto } => {
            let result = device::other_devices(auto.bucket().as_ref(), &keyring, auto.drive())
                .map_err(|e| e.to_string());
            EncryptionOutcome::DevicesCounted { drive_id, result }
        }
        EncryptionJob::Keys(job) => EncryptionOutcome::Keys(crate::recovery_keys::run(job)),
    }
}

/// The token server's recovery key of `drive_id` from its recovery `code`
/// ([`azcloud_kit::RecoveryKey`]).
pub(crate) fn recovery_key_of(code: &RecoveryCode, drive_id: &str) -> azcloud_kit::RecoveryKey {
    azcloud_kit::RecoveryKey::derive(code.as_bytes(), drive_id)
}

/// The drive's token server, from its entry (else this run's).
pub(crate) fn token_url_of(s: &DriveState, drive_id: &str) -> Option<String> {
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
        findable: Some((
            crate::recovery_keys::findable_key_of(code).public_base64(),
            recovery_key_of(code, drive_id),
        )),
    };
    spawn(info, app, s, Job::Encryption(job));
}

/// A lockdown signed with the drive's recovery key (a fresh nonce, no drive token); the
/// pending family's token kept as this computer's session (its credentials come with the
/// first refresh, once the lockdown takes effect).
pub(crate) fn recovery_lockdown(
    drive_id: &str,
    code: &RecoveryCode,
    token_url: &str,
    keyring: &azcloud_kit::SharedKeyring,
) -> Result<Option<u64>, String> {
    let transport = AzulTransport::new(crate::USER_AGENT);
    let server = TokenServer::new(token_url, &transport).map_err(|e| e.to_string())?;
    let key = recovery_key_of(code, drive_id);
    let findable = crate::recovery_keys::findable_key_of(code);
    let pending = server
        .recovery_lockdown(drive_id, |message| Ok(key.sign_base64(message)))
        .or_else(|e| {
            // Not the drive key: the code's findable key (a second kit registers only that).
            if matches!(e, TokenError::Refused { status: 401, .. }) {
                server.recovery_lockdown(drive_id, |message| Ok(findable.sign_base64(message)))
            } else {
                Err(e)
            }
        })
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
                crate::recovery::code_made(s, &drive_id, &code);
                crate::save_settings(info, app, s);
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
                crate::recovery::code_made(s, &drive_id, &done.code);
                crate::save_settings(info, app, s);
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
        EncryptionOutcome::RecoveryKeyRegistered {
            drive_id,
            result,
            findable,
        } => match result {
            Ok(()) => {
                println!("AZDRIVE_RECOVERY_KEY {drive_id}");
                match findable {
                    Some(Ok(public)) => {
                        println!("AZDRIVE_RECOVERY_FINDABLE {drive_id}");
                        crate::recovery_health::state_mut(
                            &mut s.settings.recovery.drives,
                            &drive_id,
                        )
                        .findable_key = Some(public);
                        crate::save_settings(info, app, s);
                    }
                    Some(Err(why)) => {
                        let name = drive_name(s, &drive_id);
                        s.warn(format!(
                            "A computer that never had \"{name}\" cannot find it from its kit \
                             yet ({why}): Options > Drives > Make the kit find it."
                        ));
                    }
                    None => {}
                }
            }
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
        EncryptionOutcome::KitSaved { path, len, result } => {
            crate::recovery::kit_saved(s, &path, len, result);
        }
        EncryptionOutcome::CodeChecked {
            drive_id,
            recovery_key,
            result,
        } => crate::recovery::bucket_answered(info, app, s, &drive_id, recovery_key, result),
        EncryptionOutcome::Contacts(done) => crate::recovery_contacts::on_done(info, app, s, done),
        EncryptionOutcome::DevicesCounted { drive_id, result } => {
            crate::recovery::devices_counted(info, app, s, &drive_id, result);
        }
        EncryptionOutcome::Keys(done) => crate::recovery_keys::on_done(info, app, s, done),
    }
}

/// A drill's code checked against the bucket's recovery wrap, on a worker thread (a drive set
/// up before the drills kept no recovery key).
pub(crate) fn check_code_in_bucket(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    code: RecoveryCode,
) {
    let Some(auto) = auto_of(s, drive_id) else {
        return;
    };
    s.popup = Some(Popup::Encryption(Dialog::Busy {
        title: String::from("Checking the recovery code"),
        text: String::from("Opening the drive's recovery key with the code (a few seconds)..."),
    }));
    let job = EncryptionJob::CheckCode {
        drive_id: drive_id.to_string(),
        auto,
        code,
    };
    spawn(info, app, s, Job::Encryption(job));
}

/// Counts the drive's other devices on a worker thread (Options > Drives' Count again).
pub(crate) fn count_devices(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
) {
    let Some(auto) = auto_of(s, drive_id) else {
        return;
    };
    let job = EncryptionJob::CountDevices {
        drive_id: drive_id.to_string(),
        auto,
    };
    spawn(info, app, s, Job::Encryption(job));
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
    use azul::window::NetworkKind;

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

    /// An encrypted drive's local copies kept encrypted (its AZL1 objects) live in the drive's
    /// own cache folder too, beside its drive index - in the run's cache folder.
    #[test]
    fn an_encrypted_drives_kept_objects_live_in_the_drives_own_cache_folder() {
        let cache = PathBuf::from("/run/cache/drive-index");
        let a = objects_dir(Some(cache.clone()), "drive-a").expect("a folder");
        assert!(a.starts_with(&cache) && a.ends_with("objects"), "{}", a.display());
        assert_eq!(
            a.parent(),
            search_index_dir(Some(cache), "drive-a")
                .as_deref()
                .and_then(std::path::Path::parent)
        );
        assert_eq!(objects_dir(None, "drive-a"), None, "no cache folder, nothing kept");
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
    fn the_recompression_pass_waits_for_a_network_that_costs_nothing() {
        let idle = PowerState {
            on_mains: true,
            idle_secs: RECOMPRESS_IDLE_SECS,
        };
        let on = |kind, metered, constrained| {
            recompress_allowed(
                idle,
                NetworkState {
                    kind,
                    connected: true,
                    metered,
                    constrained,
                },
            )
        };
        assert!(on(NetworkKind::Wired, false, false));
        assert!(!on(NetworkKind::Cellular, true, false), "a phone's hotspot");
        assert!(!on(NetworkKind::WiFi, false, true), "Low Data Mode");
        let busy = PowerState {
            on_mains: true,
            idle_secs: 0,
        };
        assert!(!recompress_allowed(busy, NetworkState::headless()), "used a moment ago");
    }

    #[test]
    fn the_recompression_pass_waits_for_mains_power_and_five_idle_minutes() {
        let at = |on_mains, idle_secs| PowerState { on_mains, idle_secs }.is_idle_on_mains(RECOMPRESS_IDLE_SECS);
        assert!(at(true, 300));
        assert!(!at(true, 299), "used a moment ago");
        assert!(!at(false, 3_600), "on battery");
    }

    /// The groups of the code a sheet asks for, as the user types them in.
    fn type_groups(sheet: &mut Sheet, change: impl Fn(&str) -> String) {
        let groups: Vec<String> = sheet.code.split('-').map(str::to_string).collect();
        for (slot, group) in sheet.asked().into_iter().enumerate() {
            sheet.set_typed(slot, Zeroizing::new(change(&groups[group])));
        }
    }

    #[test]
    fn the_recovery_sheet_asks_for_four_different_groups_of_the_code() {
        let code = RecoveryCode::from_bytes([0x5A; 16]);
        for _ in 0..20 {
            let sheet = Sheet::new("d_1", code.to_text());
            let asked = sheet.asked();
            assert_eq!(asked.len(), SETUP_CHECKS, "four groups: {asked:?}");
            assert_eq!(SETUP_CHECKS, 4);
            assert!(asked.windows(2).all(|w| w[0] < w[1]), "different, in order: {asked:?}");
            assert!(asked.iter().all(|&group| group < 5), "groups of the code: {asked:?}");
        }
        let left_out: std::collections::HashSet<Vec<usize>> = (0..60)
            .map(|_| Sheet::new("d_1", code.to_text()).asked())
            .collect();
        assert!(left_out.len() > 1, "chosen at random: {left_out:?}");
    }

    #[test]
    fn the_recovery_sheet_takes_the_four_groups_as_people_type_them() {
        let code = RecoveryCode::from_bytes([0x5A; 16]);
        let mut sheet = Sheet::new("d_1", code.to_text());
        assert!(!sheet.confirmed(), "nothing typed");
        type_groups(&mut sheet, str::to_lowercase);
        assert!(sheet.confirmed());
        type_groups(&mut sheet, |g| format!(" {} ", g.replace('0', "O").replace('1', "l")));
        assert!(sheet.confirmed(), "O for 0, l for 1, spaces around");
    }

    #[test]
    fn the_signup_does_not_finish_until_all_four_groups_are_right() {
        let code = RecoveryCode::from_bytes([0x5A; 16]);
        let mut sheet = Sheet::new("d_1", code.to_text());
        type_groups(&mut sheet, str::to_string);
        assert!(sheet.confirmed());
        for slot in 0..SETUP_CHECKS {
            let mut wrong = Sheet::new("d_1", code.to_text());
            type_groups(&mut wrong, str::to_string);
            wrong.set_typed(slot, Zeroizing::new(String::from("WRONG")));
            assert!(!wrong.confirmed(), "group {slot} wrong");
            wrong.set_typed(slot, Zeroizing::new(String::new()));
            assert!(!wrong.confirmed(), "group {slot} empty");
        }
        // Neither its close box nor Escape takes the sheet away: the code shows only now.
        assert!(!Dialog::Sheet(Sheet::new("d_1", code.to_text())).may_close());
        assert!(Dialog::Message {
            title: String::new(),
            text: String::new()
        }
        .may_close());
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
