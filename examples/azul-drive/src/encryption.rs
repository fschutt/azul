//! Encrypted drives in AzDrive (feature `encryption`, off by default).
//!
//! An Azlin drive opens through azul-storage's `AutoEncrypted`: its first call, on a worker
//! thread, finds whether the drive is encrypted (this computer keeps its key, or the bucket
//! holds key files) and then goes through the encryption, the index from
//! [`restoring_provider`] - the bucket's encrypted metadata repository, browsed without listing
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
//! Mode), and it is healthy (its client health 85 or more: not in Low Power Mode, not hot -
//! `crate::health`), the first open encrypted drive's files are written again, smaller, on a
//! worker thread, and the pass stops at the first input, when the power cord goes, when the
//! network starts to cost or when the computer weakens. Its state sits beside the migration's,
//! so the next idle minute continues where it stopped.
//!
//! The same timer keeps the drive index small. In such a minute (idle, on mains, on a free
//! network), before the pass, an encrypted drive whose index was not maintained from this
//! computer in the last six hours gets one maintenance round (azul-storage's
//! `MetaIndexProvider::maintain_drive`). The round runs under the bucket's lease, so only one
//! computer runs it at a time. It folds the index's packs (into packs of at most 48 MiB, which a
//! drive the owner has not paid still takes), writes a checkpoint and deletes what was retired a
//! day ago; once a day per drive - from whichever computer is first - the log then says what the
//! reclaimer would delete of the drive's data objects (`AZDRIVE_RECLAIM`: the digest of the set, the
//! commits that released it, whether the head listed again agrees, the clean rounds so far and what
//! holds the switch to deleting back; a drive too big to judge says so). One thing runs at a time:
//! the round or the pass.

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
    sensor::{BatteryState, NetworkState, PowerState},
    widgets::{ButtonType, OnTextInputReturn, TextInputState, TextInputValid},
};
use azcloud_kit::{
    azlin_contract::IdempotentScope,
    token::{Presigned, RecoveryRefusal},
    TokenServer,
};
use azul_appkit::{
    client_health::{client_health, Device},
    l10n::{t, t_args, t_label, t_text, Arg, Phrase, Text},
};
use azul_storage::{
    azul_keyring::azlin_keyring,
    azul_transport::AzulTransport,
    durable,
    keyring::Asking,
    crypto::{
        device,
        keys::{load_recovery_wrap, RecoveryCode, RecoveryKdf, RecoveryWrap},
        random_bytes, CryptoError, KeyId, Zeroizing,
    },
    config::EncryptionRecord,
    encrypted::{open_encrypted, AutoEncrypted, IndexProvider, Recorded},
    packing::{pack_reads, PackUpkeep},
    keyring::KeyringStore,
    scratch::Scratch,
    meta::{
        merge::keep_both, DriveBucket, IndexProfile, Maintained, Maintenance, MetaIndexProvider,
        MetaRepo, ReclaimOptions, RestoreSource,
    },
    migrate::{migrate, MigrationState},
    recompress::{run_pass, RecompressPolicy, RecompressState},
    rotation::{self, reencrypt_pass, ReencryptState},
    time::now_unix,
    trust::{EncState, RestoreKind},
    Drive, LocalDrive,
};

use crate::{
    browse,
    jobs::Job,
    recovery_health::{
        registrations_to_retry, state_mut, state_of, FindableSigned, PendingRegistration,
        Registration, RegistrationAnswer, Signed,
    },
    spawn,
    ui_dialogs::{button, buttons, label, line, on_cancel_popup, typed_button},
    with_state, DriveState, Popup,
};

/// AzDrive's profile of the provider of encrypted drives' indexes: the drive's encrypted metadata
/// repository (azul-storage's `meta` module), this computer's copy of it kept in the user's cache
/// folder between runs (`<cache>/AzDrive/drive-index`), so a drive opens with one conditional read
/// and browses without listing the bucket. The copies are lazy (C6): opening reads the pack
/// indexes only, and a folder's objects arrive in ranged chunk reads when it is browsed, so a
/// computer new to a drive browses it before every pack has downloaded. Small files are packed
/// together (one ranged read each, one index commit per pack), their journal beside the drive's
/// index copy. `restore` is where the drive's restore announcement comes from.
pub(crate) fn index_profile(cache_root: Option<PathBuf>, restore: RestoreSource) -> IndexProfile {
    IndexProfile {
        device_name: String::from("AzDrive"),
        cache_root,
        lazy: true,
        packing: true,
        restore,
        reclaim: ReclaimOptions::shadow(),
    }
}

/// The drive index's provider with this computer's copies under `cache_root`, for a provider that
/// opens no copy of an index: the maintenance round reads the copy's floor and runs under the
/// bucket's lease, and the slot's own provider ([`restoring_provider`]) takes the announcement.
/// What opens an index goes through [`restoring_provider`] or the slot's provider
/// (`AutoEncrypted::provider`).
pub(crate) fn meta_provider(cache_root: Option<PathBuf>) -> MetaIndexProvider {
    MetaIndexProvider::new(index_profile(
        cache_root,
        // gate:restore-none allow: a maintenance round opens no copy of an index, and the slot's provider takes the announcement [X-DELTA-AZDRIVE-SYNC-3]
        RestoreSource::none("a maintenance round opens no copy of an index"),
    ))
}

/// The provider of one drive's slot: its indexes ask `restore` (the drive's restore announcement
/// as the token server gave it) at every poll and every commit. The jobs that open the same
/// drive's index on their own (migrate, rotate, re-encrypt, totals) take this provider from the
/// slot ([`AutoEncrypted::provider`]) and not one of their own.
pub(crate) fn restoring_provider(
    cache_root: Option<PathBuf>,
    restore: RestoreSource,
) -> Arc<dyn IndexProvider> {
    Arc::new(MetaIndexProvider::new(index_profile(cache_root, restore)))
}

/// The run's cache folder (`--cache-dir`, else `<cache>/AzDrive`; `None` in a `--shot` run
/// without the switch), set once at the start.
static CACHE_DIR: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Sets the run's cache folder (the start, once): the drives' index copies live in it.
pub(crate) fn set_cache_dir(dir: Option<PathBuf>) {
    // The poison pill of the analysis of a file's deflate streams and JPEG images, and of the
    // decode of a stored transform: the counted marker of one under way is kept here, so a file
    // that ended the program three times is not analysed (nor a stream decoded) again.
    azul_storage::crypto::codec::set_analysis_marker_dir(dir.as_ref().map(|d| d.join("analysis")));
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
    crate::footprint::drive_cache_dir(root, drive).map(|dir| dir.join("search"))
}

/// Where the encrypted drive `drive`'s local copies kept encrypted (its AZL1 objects) are: in
/// the drive's own cache folder under `root`, beside its drive index and search index
/// (`<root>/<hash of the drive>/objects`); `None` without a cache folder (none are kept).
pub(crate) fn objects_dir(root: Option<PathBuf>, drive: &str) -> Option<PathBuf> {
    crate::footprint::drive_cache_dir(root, drive).map(|dir| dir.join("objects"))
}

/// An Azlin drive's bucket as the drive the app uses: encrypted, always - opened on its first
/// call with this computer's key, locked without it, and (while it has no key and no file) set
/// up at once; never a plain drive that takes files, whatever its bucket lists. `record` is what
/// the drives file says of it (`slot` is the drive's id there): a drive recorded as encrypted is
/// locked without its key, and every time the drive opens encrypted, the id of its key goes to
/// `records`, for the UI thread to write into the drives file ([`crate::take_key_records`]).
/// `restore` is the drive's restore announcement (the token server's word, as the drive's last
/// refresh answered it): the drive's index copy asks it, so a drive the operator restored from
/// its off-site copy is read anew once instead of being refused as a rollback for good.
pub(crate) fn wrap(
    drive_id: &str,
    bucket: Arc<dyn Drive>,
    record: EncryptionRecord,
    slot: &str,
    records: &crate::KeyRecords,
    restore: RestoreSource,
) -> Arc<AutoEncrypted> {
    wrap_with(
        Arc::new(azlin_keyring(Asking::User)),
        Some(restoring_provider(drive_index_root(), restore)),
        drive_id,
        bucket,
        record,
        slot,
        records,
    )
}

/// [`wrap`] over `keyring` and `provider`: what the glue is tested with.
pub(crate) fn wrap_with(
    keyring: Arc<dyn KeyringStore>,
    provider: Option<Arc<dyn IndexProvider>>,
    drive_id: &str,
    bucket: Arc<dyn Drive>,
    record: EncryptionRecord,
    slot: &str,
    records: &crate::KeyRecords,
) -> Arc<AutoEncrypted> {
    let queue = records.clone();
    let slot = slot.to_string();
    Arc::new(
        AutoEncrypted::new(bucket, drive_id, keyring, provider)
        .with_record(recorded(&record))
        .on_encrypted(Arc::new(move |key: &KeyId| {
            if let Ok(mut queue) = queue.lock() {
                queue.push(crate::KeyRecord {
                    drive_id: slot.clone(),
                    key_id: key.to_hex(),
                });
            }
        })),
    )
}

/// What the drives file says of a drive's encryption, as azul-storage's `AutoEncrypted` takes it.
fn recorded(record: &EncryptionRecord) -> Recorded {
    // A key id that cannot be read is a damaged record, not "no id" (X-DELTA-AZDRIVE-KEYS-21).
    Recorded::from_entry(record.encrypted, record.key_id.as_deref())
}

/// "Remove drive" confirmed: the drive leaves this computer. A worker thread takes it off - its
/// session and keys, the copies and indexes (with the plain text of its files), the vault of the
/// paid months, the open calls, the states of its jobs, its sync pairing's state - and the
/// drives-file entry goes LAST, only when the rest is clean (`footprint::forget`), so a delete the
/// keyring refused leaves the drive on the list for another try. The keys, the paid months and the
/// calls that guard them go only once the drive's recovery code was typed back right (once): the
/// keys here may be the only way into the drive, and a removal must not make one nobody can open.
/// Without a checked code they stay, and that is said. The window keeps the drive until the
/// answer ([`EncryptionOutcome::Forgotten`]).
pub(crate) fn start_forget(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
) {
    let Some(index) = s.slot_index(drive_id) else {
        return;
    };
    let name = s.slots[index].entry.name.clone();
    let uses_keyring =
        s.slots[index].entry.needs_keyring() || s.slots[index].entry.azlin().is_some();
    let grant = crate::sign_in::grant_of(
        &s.slots[index].entry,
        s.slots.iter().map(|slot| &slot.entry),
    );
    let proof = crate::recovery_health::removal_proof(
        &s.settings.recovery.drives,
        drive_id,
        crate::actions::now_secs(),
    );
    // An Azlin drive's period vault on this computer is compared with the drive's own when the
    // job runs: months that are only here are not thrown away with the drive.
    let vault = s.slots[index].entry.azlin().map(|_| VaultCheck {
        store: crate::periods::store_through(&s.period_tokens, &s.keyring),
        azlin: s.slots[index].azlin.clone(),
    });
    // A synced drive's pairing goes with it: its passes stop now; its state folder (the base, the
    // states and the logs, which name its files) is taken off with the rest; the files stay where
    // they are.
    let pairings: Vec<azcloud_kit::sync::session::SyncSetup> = s
        .settings
        .synced
        .iter()
        .filter(|pairing| pairing.drive_id == drive_id)
        .cloned()
        .collect();
    if !pairings.is_empty() {
        crate::sync_jobs::stop(info, app, s, drive_id);
    }
    // What else runs for the drive holds its credentials and writes what the removal takes away:
    // told to stop before the removal starts (X-DELTA-AZDRIVE-KEYS-32).
    crate::stop_jobs_of(s, drive_id);
    let computer = crate::footprint::Computer {
        drives_file: s.drives_file.clone(),
        cache_dir: s.cache_dir.clone(),
        drive_index_root: drive_index_root(),
        state_dir: state_dir_path(s),
        sync_root: crate::sync_jobs::state_root(s),
        nodes_root: crate::nodes_root(),
        period_tokens: s.period_tokens.clone(),
        outbox: s.keyring.outbox(),
        picked_scratch: crate::picker::picked_scratch(),
        scratch_root: None,
    };
    let places = crate::footprint::Places::of(&computer, drive_id, &pairings);
    s.popup.open(Popup::Encryption(Dialog::Busy {
        title: t("azdrive-forget-busy-title"),
        text: t_args("azdrive-forget-busy", &[("name", Arg::from(name.clone()))]),
    }));
    spawn(
        info,
        app,
        s,
        Job::Encryption(EncryptionJob::Forget {
            drive_id: drive_id.to_string(),
            name,
            places,
            uses_keyring,
            proof,
            grant,
            vault,
        }),
    );
}

/// An Azlin drive's period vault on this computer, to be compared with the drive's own before it
/// is taken off with the drive.
#[derive(Clone)]
pub(crate) struct VaultCheck {
    store: azcloud_kit::PeriodTokenStore,
    azlin: Option<Arc<azcloud_kit::AzlinDrive>>,
}

impl VaultCheck {
    /// Whether the drive's own vault holds everything this computer's holds (what it lacks is
    /// written into it once more). `Err`: it could not be told - the drive is not open, its key is
    /// not here, a vault could not be read - and not telling is not "yes".
    fn redundant(&self, drive_id: &str) -> Result<bool, String> {
        // A drive with no vault of its own (its key is not here) is held by this computer's alone:
        // not redundant unless it holds nothing.
        let tokens = crate::jobs::drive_tokens(&self.store, drive_id, self.azlin.as_ref())?;
        tokens.local_is_redundant().map_err(|e| e.to_string())
    }
}

/// Why a removal's proof was taken back when the removal ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProofTaken {
    /// The keyring holds the new key of a key change that is not the drive's key yet.
    KeyChangeUnfinished,
    /// The keyring could not be asked whether a key change is unfinished.
    KeyringUnreadable,
    /// The months paid for the drive are only in this computer's vault.
    PaidMonthsOnlyHere,
    /// Whether the drive's vault holds the paid months could not be told.
    PaidMonthsUnknown,
}

/// The removal as the worker thread runs it: the proof made from the settings when the button was
/// pressed ([`start_forget`]) is taken back where the keyring or the drive say otherwise NOW - a key
/// change that is unfinished (the new key is in one keyring entry only, the index may be sealed
/// under it), the months paid that are only in this computer's vault - and then every store of the
/// drive is taken off ([`crate::footprint::forget`]).
pub(crate) fn forget_job(
    keyring: &dyn KeyringStore,
    drive_id: &str,
    places: &crate::footprint::Places,
    uses_keyring: bool,
    proof: Option<azul_storage::footprint::RemovalProof>,
    paid_months_elsewhere: Option<Result<bool, String>>,
) -> azul_storage::footprint::ForgetReport {
    let (proof, taken) = proof_for_removal(
        proof,
        rotation::unfinished(keyring, drive_id).map_err(|e| e.to_string()),
        paid_months_elsewhere,
    );
    if let Some(taken) = taken {
        println!("AZDRIVE_FORGET_PROOF_TAKEN {drive_id}: {taken:?}");
    }
    crate::footprint::forget(
        drive_id,
        places,
        uses_keyring.then_some(keyring),
        proof.as_ref(),
    )
}

/// The proof a removal runs with: the one made from the settings (`proof`), kept only while the
/// keyring says no key change is unfinished (`unfinished`) and the drive's own vault holds the
/// months paid (`paid_months_elsewhere`: `None` for a drive that has none). A question that could
/// not be answered takes the proof back too: the keys here may be the only way into the drive, and
/// "unknown" is not "fine" (X-DELTA-AZDRIVE-KEYS-2, -8). Pure: the job asks, this decides.
pub(crate) fn proof_for_removal(
    proof: Option<azul_storage::footprint::RemovalProof>,
    unfinished: Result<bool, String>,
    paid_months_elsewhere: Option<Result<bool, String>>,
) -> (
    Option<azul_storage::footprint::RemovalProof>,
    Option<ProofTaken>,
) {
    let Some(proof) = proof else {
        return (None, None);
    };
    match unfinished {
        Ok(false) => {}
        Ok(true) => return (None, Some(ProofTaken::KeyChangeUnfinished)),
        Err(_) => return (None, Some(ProofTaken::KeyringUnreadable)),
    }
    match paid_months_elsewhere {
        None | Some(Ok(true)) => {}
        Some(Ok(false)) => return (None, Some(ProofTaken::PaidMonthsOnlyHere)),
        Some(Err(_)) => return (None, Some(ProofTaken::PaidMonthsUnknown)),
    }
    (Some(proof), None)
}

/// What the user must know of the sign-in of a removed drive: it stays valid at its provider
/// (another drive may share it, the provider cannot end it from here, or it could not be ended),
/// and where it is ended by hand. Nothing is said of a sign-in that was ended or never was.
fn tell_grant(s: &mut DriveState, end: &crate::sign_in::GrantEnd) {
    use crate::sign_in::GrantEnd;
    match end {
        GrantEnd::Ended | GrantEnd::NothingToEnd => {}
        GrantEnd::Kept { provider } => {
            s.warn(Phrase::new("azdrive-forget-grant-kept").arg("provider", *provider));
        }
        GrantEnd::InAccount { provider, page } => {
            s.warn(
                Phrase::new("azdrive-forget-grant-in-account")
                    .arg("provider", *provider)
                    .arg("page", *page),
            );
        }
        GrantEnd::NotEnded { provider, why } => {
            s.warn(
                Phrase::new("azdrive-forget-grant-not-ended")
                    .arg("provider", *provider)
                    .arg("why", why.as_str()),
            );
        }
    }
}

/// Whether the encryption flows are offered: always - the drive index is in every build.
pub(crate) fn offered() -> bool {
    true
}

/// Where a drive's migration keeps its state: beside the drives file.
fn state_file(s: &DriveState, drive_id: &str) -> Result<PathBuf, String> {
    Ok(crate::footprint::migration_state(&state_dir(s)?, drive_id))
}

/// The folder of the encryption's local state files: beside the drives file, or - when the
/// system names no configuration folder, so there is no drives file - in scratch, the private
/// folder of this user's (never the system's shared temporary folder). Made by [`state_dir`].
fn state_dir_path(s: &DriveState) -> PathBuf {
    match s.drives_file.as_ref().and_then(|file| file.parent()) {
        Some(parent) => parent.join("encryption"),
        None => Scratch::new("encryption").dir().to_path_buf(),
    }
}

/// [`state_dir_path`], with a scratch folder made and checked (a folder beside the drives file is
/// made by the first state saved in it).
fn state_dir(s: &DriveState) -> Result<PathBuf, String> {
    if s.drives_file
        .as_ref()
        .and_then(|file| file.parent())
        .is_none()
    {
        let scratch = Scratch::new("encryption");
        scratch.ensure().map_err(|e| {
            format!(
                "the folder for the encryption's state ({}): {e}",
                scratch.dir().display()
            )
        })?;
    }
    Ok(state_dir_path(s))
}

/// The state of a "re-encrypt everything" pass as the NEXT pass takes it up: the one a pass of the
/// current keys left, not one of the keys before. A pass that began before the keys were made
/// (`rotated_at`) has a cut-off and a cursor of other files: loaded after a second rotation it
/// skipped the files at or before its cursor and the ones newer than its cut-off, which are in
/// objects of the keys before, and ended - and the reminder to re-encrypt stopped
/// (X-DELTA-AZDRIVE-KEYS-29). A state of unknown age (`rotated_at` 0) is taken as it is.
fn current_pass(state: Option<ReencryptState>, rotated_at: u64) -> Option<ReencryptState> {
    state.filter(|state| state.before >= rotated_at)
}

/// The state a pass saved and did not finish, if any. A state that is there and cannot be read
/// or does not parse is an error and not "no state": a pass that started over would redo - and
/// send again - what it had done, and nobody would know. A damaged file is set aside whole
/// (`<name>.damaged`) so the next run starts anew, and the error says where it went.
fn load_state<T>(file: &Path, parse: impl FnOnce(&str) -> Option<T>) -> Result<Option<T>, String> {
    let bytes = match std::fs::read(file) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(format!(
                "the saved state of this pass ({}) cannot be read: {e}",
                file.display()
            ))
        }
    };
    if let Some(state) = std::str::from_utf8(&bytes).ok().and_then(parse) {
        return Ok(Some(state));
    }
    let aside = crate::footprint::damaged_aside(file);
    let set_aside = durable::replace(&aside, &bytes, durable::Mode::Private)
        .and_then(|()| std::fs::remove_file(file));
    Err(match set_aside {
        Ok(()) => format!(
            "the saved state of this pass ({}) is damaged, so the pass was not started over it; \
             it was set aside as {}, and the next run starts anew",
            file.display(),
            aside.display()
        ),
        Err(e) => format!(
            "the saved state of this pass ({}) is damaged, so the pass was not started over it, \
             and it could not be set aside ({e}): delete it to start anew",
            file.display()
        ),
    })
}

/// Saves a pass's state where it can be found again after a cut: written whole beside the file,
/// synced, renamed over it (`durable::replace`), readable by this user only. Never half a file.
fn save_state(file: &Path, json: &str) -> Result<(), azul_storage::DriveError> {
    durable::replace(file, json.as_bytes(), durable::Mode::Private)
        .map_err(|e| azul_storage::DriveError::Io(format!("{}: {e}", file.display())))
}

// ==== The recompression pass ====

/// Seconds without input before the pass starts (it stops at the first input).
const RECOMPRESS_IDLE_SECS: u64 = 300;
/// How often the timer looks.
const RECOMPRESS_CHECK_MS: u64 = 60_000;
/// One pass at a time.
static RECOMPRESSING: AtomicBool = AtomicBool::new(false);

/// Whether the pass (and the drive index's upkeep) may run now: [`recompress_allowed`] of the
/// power, the network and the computer's client health now.
fn recompress_may_run() -> bool {
    let (power, battery, network) = (
        PowerState::query(),
        BatteryState::query(),
        NetworkState::query(),
    );
    let health = client_health(&Device::of_azul(&power, &battery, &network));
    recompress_allowed(power, network, health)
}

/// Whether the pass may run on `power` and `network` at the computer's client `health`: idle
/// long enough, on mains power, on a network that costs the user nothing - the pass rewrites
/// every file of the drive - and healthy ([`crate::health::recompress_allowed`]: not in Low
/// Power Mode, not hot). A platform azul cannot read answers "on battery, just used"
/// ([`PowerState::query`]), so the pass waits there; a network it cannot read counts as free
/// ([`NetworkState::query`]).
fn recompress_allowed(power: PowerState, network: NetworkState, health: u8) -> bool {
    power.is_idle_on_mains(RECOMPRESS_IDLE_SECS)
        && network.allows_background_transfer()
        && crate::health::recompress_allowed(health)
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
    let mut callback_info = info.callback_info;
    let app = data.clone();
    // A recovery drill that is due opens (once a minute, when no dialog shows), and so does the
    // question of re-encrypting the files a rotation left under their old file keys.
    if let Some(mut s) = data.downcast_mut::<DriveState>() {
        let now = now_unix();
        if crate::recovery::drill_if_due(&mut s, now) {
            return TimerCallbackReturn::continue_and_refresh_dom();
        }
        if reencrypt_if_due(&mut s, now) {
            crate::save_settings(&mut callback_info, &app, &mut s);
            return TimerCallbackReturn::continue_and_refresh_dom();
        }
        if resume_migration(&mut callback_info, &app, &mut s) {
            return TimerCallbackReturn::continue_and_refresh_dom();
        }
    }
    if RECOMPRESSING.load(Ordering::SeqCst) || !recompress_may_run() {
        return TimerCallbackReturn::continue_unchanged();
    }
    let Some(mut s) = data.downcast_mut::<DriveState>() else {
        return TimerCallbackReturn::continue_unchanged();
    };
    let encrypted: Vec<(String, Arc<AutoEncrypted>)> = s
        .slots
        .iter()
        .filter_map(|slot| {
            let auto = slot.auto.clone()?;
            auto.state().has_drive_index().then(|| (slot.entry.id.clone(), auto))
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
    let state_file = match state_dir(&s) {
        Ok(dir) => crate::footprint::recompress_state(&dir, &drive_id),
        Err(why) => {
            eprintln!("AZDRIVE_RECOMPRESS_SKIPPED {drive_id}: {why}");
            return TimerCallbackReturn::continue_unchanged();
        }
    };
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

/// The line that says a maintenance round of the drive `drive_id` ended, with what its repack
/// did to the packs (the E2E proof waits for it and reads the counts).
fn maintained_marker(drive_id: &str, packs: &PackUpkeep) -> String {
    let repack = &packs.repack;
    format!(
        "AZDRIVE_MAINTAINED {drive_id} packs_read={} packs_written={} packs_emptied={} \
         files_moved={}",
        repack.packs_read, repack.packs_written, repack.packs_emptied, repack.files_moved
    )
}

/// The `AZDRIVE_PACK_READS` lines of a transfer's drives (`source_id`, `target_id`) that
/// opened encrypted in this process: what their reads of packs cost so far, ranged GETs, packs
/// fetched whole and bytes (the E2E proof counts what reading one small file cost).
pub(crate) fn pack_reads_lines(source_id: &str, target_id: &str) -> Vec<String> {
    let mut drives = vec![source_id];
    if target_id != source_id {
        drives.push(target_id);
    }
    drives
        .into_iter()
        .filter_map(|drive| {
            pack_reads(drive).map(|reads| {
                format!(
                    "AZDRIVE_PACK_READS {drive} ranged={} whole={} bytes={}",
                    reads.ranged, reads.whole, reads.bytes
                )
            })
        })
        .collect()
}

/// The log line of the reclaimer's round at the end of a maintenance round
/// ([`MetaIndexProvider::last_reclaim`]): in shadow mode what it would delete - the evidence the
/// switch to deleting waits for - or why the round could not read the drive. `None`: no round ran.
fn reclaim_marker(
    drive: &str,
    outcome: Option<Result<azul_storage::meta::ReclaimReport, String>>,
) -> Option<String> {
    Some(match outcome? {
        Ok(report) => format!("AZDRIVE_RECLAIM {drive} {}", report.line()),
        Err(why) => format!("AZDRIVE_RECLAIM_STOPPED {drive}: {why}"),
    })
}

/// One maintenance round of the drive `drive`'s index in `bucket`
/// (`MetaIndexProvider::maintain`), with this computer's key from `keyring`: what it did to the
/// index, and to the packs of the drive's small files. `None`: there is no index yet, or
/// another computer's round holds the lease.
fn run_maintenance(
    provider: &MetaIndexProvider,
    bucket: &Arc<dyn Drive>,
    keyring: &dyn KeyringStore,
    drive: &str,
    rules: &Maintenance,
) -> Result<Option<(Maintained, PackUpkeep)>, String> {
    let drive_key = device::unlock(bucket.as_ref(), keyring, drive)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| String::from("azdrive-enc-err-no-key"))?;
    // The round knows the drive: it refuses a manifest older than this computer's copy has seen.
    let maintained = provider
        .maintain_drive(drive, Arc::clone(bucket), &drive_key, rules)
        .map_err(|e| e.to_string())?;
    // What the reclaimer's round at its end came to (in shadow mode: what it would delete).
    if let Some(line) = reclaim_marker(drive, provider.last_reclaim()) {
        println!("{line}");
    }
    // The packs of small files, when the index's round ran (there is an index): uploads a
    // crash left half done cleared up, sparse packs repacked, retired packs deleted after
    // their grace period.
    let Some(index) = maintained else {
        return Ok(None);
    };
    let packs = open_encrypted(Arc::clone(bucket), keyring, drive, provider)
        .and_then(|encrypted| encrypted.maintain_packs(now_unix()))
        .map_err(|e| e.to_string())?;
    Ok(Some((index, packs)))
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
    /// The sheet of a NEW drive's code (encrypted as it was made): nothing to move into the
    /// encryption afterwards.
    pub new_drive: bool,
    /// "Copy" put the code on the clipboard (cleared again after
    /// [`azul_appkit::secret_clipboard::CLEAR_SECRET_MS`]). The sheet takes the code off the
    /// clipboard when it leaves a step whether or not this is set: the text on the page is
    /// selectable, and a Cmd-C of it is a copy this flag never heard of.
    pub copied: bool,
    /// The code this one replaces, as typed under "I was hacked: new keys": it signs the new
    /// code's registration at the token server (F12 C). A secret: never printed.
    pub previous_code: Option<Zeroizing<String>>,
    /// The sheet of a further recovery code (a second kit): it only closes when its groups are
    /// typed back - the drive was set up long ago.
    pub extra: bool,
    /// The second step: the code is hidden and its groups are asked for. The groups are typed
    /// from the user's own copy - a code that stays on the screen while it is checked is checked
    /// by copying it (AZ-K12).
    pub checking: bool,
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
            new_drive: false,
            copied: false,
            previous_code: None,
            extra: false,
            checking: false,
        }
    }

    /// The code is hidden and its groups are asked for ([`Sheet::checking`]); what was typed
    /// before is gone.
    pub(crate) fn hide_code(&mut self) {
        self.checking = true;
        self.clear_typed();
    }

    /// The code shows again; what was typed is gone.
    pub(crate) fn show_code(&mut self) {
        self.checking = false;
        self.clear_typed();
    }

    fn clear_typed(&mut self) {
        self.error.clear();
        for typed in &mut self.typed {
            *typed = Zeroizing::new(String::new());
        }
    }

    /// The sheet of a further recovery code (a second kit).
    #[must_use]
    pub(crate) fn extra(mut self) -> Sheet {
        self.extra = true;
        self
    }

    /// The sheet's title: a new drive's (its purchase), else the code's.
    pub(crate) fn title(&self) -> String {
        t(if self.new_drive {
            "azdrive-enc-sheet-title-new"
        } else {
            "azdrive-enc-sheet-title"
        })
    }

    /// What the sheet says after "Copy".
    pub(crate) fn copied_note(&self) -> String {
        if self.copied {
            t("azdrive-enc-code-copied")
        } else {
            String::new()
        }
    }

    /// The sheet of a new drive's code.
    #[must_use]
    pub(crate) fn for_new_drive(mut self) -> Sheet {
        self.new_drive = true;
        self
    }

    /// The sheet of a key rotation's new code.
    #[must_use]
    pub(crate) fn after_rotation(mut self) -> Sheet {
        self.after_rotation = true;
        self
    }

    /// The new code's registration is signed with `previous` (the code it replaces).
    #[must_use]
    pub(crate) fn replacing(mut self, previous: Option<Zeroizing<String>>) -> Sheet {
        self.previous_code = previous;
        self
    }

    /// The groups (0-based) the user types back, in order.
    #[cfg(test)]
    pub(crate) fn asked(&self) -> Vec<usize> {
        self.checks.clone()
    }

    /// What the user typed into the box of the `slot`-th group asked for.
    pub(crate) fn set_typed(&mut self, slot: usize, text: Zeroizing<String>) {
        if let Some(typed) = self.typed.get_mut(slot) {
            *typed = text;
        }
    }

    /// The groups asked for as people count them: `1, 3, 4 and 5` (in the window's language).
    pub(crate) fn asked_words(&self) -> String {
        let numbers: Vec<String> = self.checks.iter().map(|g| (g + 1).to_string()).collect();
        match numbers.split_last() {
            Some((last, rest)) if !rest.is_empty() => t_args(
                "azdrive-enc-groups-and",
                &[
                    ("rest", Arg::from(rest.join(", "))),
                    ("last", Arg::from(last.as_str())),
                ],
            ),
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
    /// "I was hacked: new keys for this drive?", with the current recovery code if the user
    /// has it (it signs the new code's registration).
    ConfirmRotate {
        drive_id: String,
        typed: Zeroizing<String>,
        error: String,
    },
    /// After a rotation: "Re-encrypt every file?"
    OfferReencrypt { drive_id: String },
    /// A device was removed from the drive: it cannot sign in again but keeps the drive's key and
    /// what it copied; "I was hacked: new keys" is offered (AZ-K4).
    DeviceRemoved { drive_id: String, device: String },
    /// "Lock down with the recovery code...": the code typed, signing a lockdown without a
    /// drive token.
    RecoveryLockdown {
        drive_id: String,
        typed: Zeroizing<String>,
        error: String,
    },
    /// "Cancel lockdown": the recovery code typed, signing the cancel (F12, "the recovery code
    /// always wins" - a device alone cancels nothing).
    CancelLockdown {
        drive_id: String,
        typed: Zeroizing<String>,
        error: String,
    },
    /// "Get my drive back" on a computer without the drive's device key: the recovery code
    /// typed, signing the token server's challenge for the locked drive.
    GetDriveBack {
        drive_id: String,
        /// The paid checkout the proof unlocks, when the payment came first.
        checkout: Option<String>,
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
    /// "Add a device": the join code made for it (a secret: never printed), as text, as a QR
    /// code and as a file.
    Invite {
        drive_id: String,
        code: Zeroizing<String>,
        note: String,
    },
}

impl Dialog {
    /// Whether its close box and Escape take it away: not the recovery sheet, whose code shows
    /// only this once - the setup finishes when its groups are typed back.
    pub(crate) fn may_close(&self) -> bool {
        !matches!(self, Dialog::Sheet(_))
    }

    /// Whether it shows something only this once: a recovery code (its sheet), the join code
    /// made for another computer, the shares made for trusted contacts or the code two shares
    /// gave back. Only its user closes it.
    pub(crate) const fn must_be_seen(&self) -> bool {
        match self {
            Dialog::Sheet(_) | Dialog::Invite { .. } => true,
            Dialog::Contacts(page) => page.shows_a_secret_once(),
            _ => false,
        }
    }

    /// Whether it only reports work under way: the work's answer takes its place.
    pub(crate) const fn reports_progress(&self) -> bool {
        matches!(self, Dialog::Busy { .. })
    }
}

fn drive_name(s: &DriveState, drive_id: &str) -> String {
    s.drive_name(&browse::Place::folder(drive_id, ""))
}

/// A dialog's column of `children`.
fn dialog_column(children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; min-width: 420px; max-width: 520px;")
        .with_children(DomVec::from(children))
}

/// The second step of the recovery sheet: the code is NOT on this page. The groups asked for are
/// typed from the user's own copy; a sheet that keeps the code on the screen while it is checked
/// is checked by copying it (AZ-K12).
fn check_page(sheet: &Sheet, app: &RefAny) -> Dom {
    let mut body = dialog_column(Vec::new());
    body.add_child(label(&t_args(
        "azdrive-enc-sheet-check",
        &[("groups", Arg::from(sheet.asked_words()))],
    )));
    let mut boxes = Dom::create_div().with_css("display: flex; flex-direction: row;");
    for (slot, (&group, typed)) in sheet.checks.iter().zip(&sheet.typed).enumerate() {
        boxes.add_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; margin-right: 8px;")
                .with_child(label(&t_args(
                    "azdrive-enc-group",
                    &[("n", Arg::from(group + 1))],
                )))
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
        body.add_child(line(&sheet.error).with_css("color: system:danger;"));
    }
    body.add_child(buttons(vec![
        button("azdrive-enc-sheet-show", app, on_sheet_show).with_id(crate::ids::SHEET_BACK),
        typed_button(
            "azdrive-enc-written-down",
            ButtonType::Primary,
            app,
            on_sheet_done,
        )
        .with_id(crate::ids::SHEET_DONE),
    ]));
    body
}

/// The lines of the "Encrypt this drive" confirm, as resource keys: what encrypting does, and -
/// first - for a drive that holds files and shows no key file, that this computer cannot tell it
/// from an encrypted drive whose storage hides its key files (X-DELTA-AZDRIVE-KEYS-19: encrypting
/// moves what the storage shows into a new key).
fn confirm_lines(state: Option<EncState>) -> Vec<&'static str> {
    let mut lines = Vec::with_capacity(5);
    if state.is_some_and(EncState::encrypt_needs_warning) {
        lines.push("azdrive-enc-confirm-cannot-tell");
    }
    lines.extend([
        "azdrive-enc-confirm-1",
        "azdrive-enc-confirm-2",
        "azdrive-enc-confirm-3",
        "azdrive-enc-confirm-4",
    ]);
    lines
}

/// The dialog's title and content.
pub(crate) fn dialog_parts(dialog: &Dialog, s: &DriveState, app: &RefAny) -> (String, Dom) {
    let column = dialog_column;
    match dialog {
        Dialog::Confirm { drive_id } => {
            let state = s
                .slot_index(drive_id)
                .and_then(|index| s.slots[index].auto.as_ref())
                .map(|auto| auto.state());
            let mut children: Vec<Dom> = confirm_lines(state).into_iter().map(line).collect();
            children.push(buttons(vec![
                button("kit-button-cancel", app, on_cancel_popup),
                typed_button("azdrive-enc-encrypt", ButtonType::Primary, app, on_encrypt),
            ]));
            (
                t_args(
                    "azdrive-enc-confirm-title",
                    &[("name", Arg::from(drive_name(s, drive_id)))],
                ),
                column(children),
            )
        }
        Dialog::Busy { title, text } => (
            title.clone(),
            column(vec![
                line(text),
                buttons(vec![button("azdrive-enc-hide", app, on_cancel_popup)]),
            ]),
        ),
        Dialog::Sheet(sheet) if sheet.checking => (sheet.title(), check_page(sheet, app)),
        Dialog::Sheet(sheet) => {
            let mut body = column(Vec::new());
            if sheet.new_drive {
                body.add_child(line(
                    "azdrive-enc-your-new-drive-encrypted",
                ));
            }
            body.add_child(line(
                "azdrive-enc-write-down-print-save",
            ));
            // The code as text: selectable, and a "Copy" whose copy leaves the clipboard again.
            body.add_child(
                Dom::create_div()
                    .with_css(
                        "display: flex; flex-direction: row; align-items: center; \
                         margin-top: 14px; margin-bottom: 6px;",
                    )
                    .with_child(
                        Dom::create_span_with_text(AzString::from(sheet.code.as_str()))
                            .with_css(
                                "font-family: monospace; font-size: 20px; letter-spacing: 1px; \
                                 user-select: text; cursor: text; margin-right: 12px;",
                            )
                            .with_id(crate::ids::SHEET_CODE),
                    )
                    .with_child(
                        button("azdrive-bridge-copy", app, on_copy_code)
                            .with_id(crate::ids::SHEET_COPY),
                    ),
            );
            if sheet.copied {
                body.add_child(line(&sheet.copied_note()).with_css("font-size: 12px;"));
            }
            // The emergency kit: Print, Save as PDF, Save to a USB stick, and its QR code - once
            // the token server has the code (until then it locks nothing down).
            let state = crate::recovery_health::state_of(&s.settings.recovery.drives, &sheet.drive_id);
            if crate::recovery::RegisteredCode::of(state, sheet).is_some() {
                for piece in crate::recovery::kit_pieces(app, &sheet.code, &sheet.kit_note) {
                    body.add_child(piece);
                }
            } else {
                body.add_child(
                    line(&t("azdrive-kit-not-active-yet"))
                        .with_css("font-size: 12px;")
                        .with_id(crate::ids::KIT_NOT_ACTIVE),
                );
            }
            // The check is the next step, with the code hidden.
            body.add_child(line(&t_args(
                "azdrive-enc-sheet-next",
                &[("groups", Arg::from(sheet.asked_words()))],
            )));
            body.add_child(buttons(vec![typed_button(
                "azdrive-enc-sheet-hide",
                ButtonType::Primary,
                app,
                on_sheet_hide,
            )
            .with_id(crate::ids::SHEET_NEXT)]));
            (sheet.title(), body)
        }
        Dialog::Unlock {
            drive_id, error, ..
        } => {
            let mut body = column(vec![
                line("azdrive-enc-unlock-what"),
                label("azdrive-drill-code"),
                TextInput::create()
                    .with_placeholder(AzString::from("XXXXX-XXXXX-XXXXX-XXXXX-XXXXXX"))
                    .with_on_text_input(app.clone(), on_typed as TextInputOnTextInputCallbackType)
                    .dom()
                    .with_id(crate::ids::UNLOCK_CODE),
            ]);
            if !error.is_empty() {
                body.add_child(line(error).with_css("color: system:danger;"));
            }
            body.add_child(buttons(vec![
                button("kit-button-cancel", app, on_cancel_popup),
                typed_button("azdrive-enc-unlock", ButtonType::Primary, app, on_unlock),
            ]));
            (
                t_args("azdrive-enc-unlock-title", &[("name", Arg::from(drive_name(s, drive_id)))]),
                body,
            )
        }
        Dialog::RecoveryLockdown {
            drive_id, error, ..
        } => {
            let mut body = column(vec![
                line("azdrive-enc-from-computer-lost-drive"),
                label("azdrive-drill-code"),
                TextInput::create()
                    .with_placeholder(AzString::from("XXXXX-XXXXX-XXXXX-XXXXX-XXXXXX"))
                    .with_on_text_input(app.clone(), on_typed as TextInputOnTextInputCallbackType)
                    .dom(),
            ]);
            if !error.is_empty() {
                body.add_child(line(error).with_css("color: system:danger;"));
            }
            body.add_child(buttons(vec![
                button("kit-button-cancel", app, on_cancel_popup),
                typed_button(
                    "azdrive-enc-lock-down",
                    ButtonType::Primary,
                    app,
                    on_recovery_lockdown,
                ),
            ]));
            (
                t_args(
                    "azdrive-enc-lockdown-title",
                    &[("name", Arg::from(drive_name(s, drive_id)))],
                ),
                body,
            )
        }
        Dialog::Invite { drive_id, code, note } => {
            let mut body = column(vec![
                line(&t_args(
                    "azdrive-invite-what",
                    &[("name", Arg::from(drive_name(s, drive_id)))],
                )),
                Dom::create_span_with_text(AzString::from(code.as_str()))
                    .with_css(
                        "font-family: monospace; font-size: 11px; user-select: text; \
                         cursor: text; margin-top: 8px; word-break: break-all;",
                    )
                    .with_id(crate::ids::INVITE_CODE),
            ]);
            if let Ok(symbol) = azul_appkit::qr::QrCode::encode(code.as_bytes()) {
                body.add_child(
                    azul_appkit::ui::qr_dom(&symbol, INVITE_MODULE_PX)
                        .with_css("margin-top: 8px;"),
                );
            }
            if !note.is_empty() {
                body.add_child(line(note).with_css("font-size: 12px;"));
            }
            body.add_child(buttons(vec![
                button("azdrive-invite-save", app, on_invite_save)
                    .with_id(crate::ids::INVITE_SAVE),
                typed_button(
                    "azdrive-button-close",
                    ButtonType::Primary,
                    app,
                    on_cancel_popup,
                ),
            ]));
            (t("azdrive-invite-title"), body)
        }
        Dialog::Message { title, text } => (
            title.clone(),
            column(vec![
                line(text),
                buttons(vec![typed_button(
                    "azdrive-button-close",
                    ButtonType::Primary,
                    app,
                    on_cancel_popup,
                )]),
            ]),
        ),
        Dialog::CancelLockdown {
            drive_id, error, ..
        } => {
            let mut body = column(vec![
                line("azdrive-enc-lockdown-started-recovery-code"),
                label("azdrive-drill-code"),
                TextInput::create()
                    .with_placeholder(AzString::from("XXXXX-XXXXX-XXXXX-XXXXX-XXXXXX"))
                    .with_on_text_input(app.clone(), on_typed as TextInputOnTextInputCallbackType)
                    .dom()
                    .with_id(crate::ids::LOCKDOWN_CANCEL_CODE),
            ]);
            if !error.is_empty() {
                body.add_child(line(error).with_css("color: system:danger;"));
            }
            body.add_child(buttons(vec![
                button("azdrive-button-close", app, on_cancel_popup),
                typed_button(
                    "azdrive-enc-cancel-lockdown",
                    ButtonType::Primary,
                    app,
                    on_cancel_lockdown_signed,
                )
                .with_id(crate::ids::LOCKDOWN_CANCEL_CONFIRM),
            ]));
            (
                t_args("azdrive-enc-cancel-title", &[("name", Arg::from(drive_name(s, drive_id)))]),
                body,
            )
        }
        Dialog::GetDriveBack {
            drive_id, error, ..
        } => {
            let mut body = column(vec![
                line("azdrive-archive-code-why"),
                label("azdrive-drill-code"),
                TextInput::create()
                    .with_placeholder(AzString::from("XXXXX-XXXXX-XXXXX-XXXXX-XXXXXX"))
                    .with_on_text_input(app.clone(), on_typed as TextInputOnTextInputCallbackType)
                    .dom()
                    .with_id(crate::ids::ARCHIVE_CODE),
            ]);
            if !error.is_empty() {
                body.add_child(line(error).with_css("color: system:danger;"));
            }
            body.add_child(buttons(vec![
                button("azdrive-button-close", app, on_cancel_popup),
                typed_button(
                    "azdrive-archive-get-back",
                    ButtonType::Primary,
                    app,
                    on_get_back_signed,
                )
                .with_id(crate::ids::ARCHIVE_CODE_CONFIRM),
            ]));
            (
                t_args("azdrive-archive-code-title", &[("name", Arg::from(drive_name(s, drive_id)))]),
                body,
            )
        }
        Dialog::ConfirmRotate {
            drive_id, error, ..
        } => (
            t_args("azdrive-enc-rotate-title", &[("name", Arg::from(drive_name(s, drive_id)))]),
            column(vec![
                line("azdrive-enc-rotate-when"),
                line("azdrive-enc-rotate-1"),
                line("azdrive-enc-rotate-2"),
                line("azdrive-enc-rotate-3"),
                line("azdrive-enc-rotate-then"),
                line("azdrive-enc-rotate-history"),
                label("azdrive-enc-your-current-recovery-code"),
                line("azdrive-enc-token-server-takes-new").with_css("font-size: 12px;"),
                TextInput::create()
                    .with_placeholder(AzString::from("XXXXX-XXXXX-XXXXX-XXXXX-XXXXXX"))
                    .with_on_text_input(app.clone(), on_typed as TextInputOnTextInputCallbackType)
                    .dom()
                    .with_id(crate::ids::ROTATE_CODE),
                line(error).with_css("color: system:danger;"),
                buttons(vec![
                    button("kit-button-cancel", app, on_cancel_popup),
                    typed_button(
                        "azdrive-enc-rotate-button",
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
        Dialog::DeviceRemoved { drive_id, device } => (
            t("azdrive-device-removed-title"),
            column(vec![
                line(&t_text(&Text::from(crate::devices::removed_text(
                    &drive_name(s, drive_id),
                    device,
                )))),
                buttons(vec![
                    button("azdrive-drill-later", app, on_cancel_popup),
                    typed_button(
                        "azdrive-side-rotate",
                        ButtonType::Primary,
                        app,
                        on_new_keys_after_removal,
                    ),
                ]),
            ]),
        ),
        Dialog::OfferReencrypt { drive_id } => (
            t("azdrive-enc-reencrypt-title"),
            column(vec![
                line(&t_args(
                    "azdrive-enc-reencrypt-why",
                    &[("name", Arg::from(drive_name(s, drive_id)))],
                )),
                line("azdrive-enc-reencrypt-how"),
                buttons(vec![
                    button("azdrive-drill-later", app, on_cancel_popup),
                    typed_button(
                        "azdrive-enc-reencrypt-button",
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

/// Whether a drive's encryption was set up and its recovery code never typed back (AzDrive
/// closed on the sheet): the drive is not used until a new code's sheet passed.
pub(crate) fn setup_unfinished(state: Option<&crate::recovery_health::RecoveryState>) -> bool {
    state.is_some_and(|state| state.code_made.is_some() && state.code_checked.is_none())
}

/// The drives a new code is being made for (one job each).
static NEW_CODE_ASKED: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn new_code_asked() -> std::sync::MutexGuard<'static, Vec<String>> {
    NEW_CODE_ASKED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Opening a drive whose setup never passed its sheet ([`setup_unfinished`]: AzDrive closed on
/// it; the code was stored nowhere and nobody confirmed it): the drive is not used yet - a NEW
/// recovery code is made (its wrap in the old one's place) and its sheet shows, as at the
/// drive's making.
pub(crate) fn guard_unfinished_setup(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    place: &browse::Place,
) {
    let browse::Place::Folder { drive, .. } = place else {
        return;
    };
    let drive = drive.clone();
    let state = s.settings.recovery.drives.iter().find(|r| r.drive_id == drive);
    if !offered() || !setup_unfinished(state) {
        return;
    }
    let showing = match s.popup.as_ref() {
        Some(Popup::Encryption(Dialog::Sheet(sheet))) => sheet.drive_id == drive,
        Some(Popup::Encryption(Dialog::Busy { .. })) => true,
        _ => false,
    };
    let waiting = s.popup.waiting().any(|popup| {
        matches!(popup, Popup::Encryption(Dialog::Sheet(sheet)) if sheet.drive_id == drive)
    });
    if showing || waiting || new_code_asked().contains(&drive) {
        return;
    }
    let Some(auto) = auto_of(s, &drive) else {
        return;
    };
    println!("AZDRIVE_RECOVERY_SETUP_UNFINISHED {drive}");
    new_code_asked().push(drive.clone());
    if s.popup.is_none() {
        s.popups_opened += 1;
        s.popup.open(Popup::Encryption(Dialog::Busy {
            title: t("azdrive-enc-finishing-title"),
            text: t("azdrive-enc-finishing"),
        }));
    }
    spawn(
        info,
        app,
        s,
        Job::Encryption(EncryptionJob::NewCode {
            drive_id: drive,
            auto,
        }),
    );
}

extern "C" fn on_copy_code(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::Encryption(Dialog::Sheet(sheet))) = s.popup.as_mut() else {
            return;
        };
        // A recovery code is a secret: clipboard managers and history leave it alone, and it
        // leaves the clipboard again unless something else was copied meanwhile.
        crate::ui_bridge::clipboard_secret(
            info,
            sheet.code.as_str(),
            "AZDRIVE_CODE_CLIPBOARD_CLEARED",
        );
        sheet.copied = true;
        println!("AZDRIVE_CODE_COPIED {}", sheet.drive_id);
        let _ = app;
    })
}

/// Whether a drive that was just made is encrypted as part of its making: every new Azlin
/// drive (bought, a test drive, a voucher's, a claimed checkout's) - "we always encrypt".
pub(crate) fn sets_up_encryption_at_creation(entry: &azul_storage::config::DriveEntry) -> bool {
    entry.azlin().is_some()
}

/// A drive that was just made ([`sets_up_encryption_at_creation`]) gets its keys now: the
/// drive key and this computer's member key in the keyring, the recovery wrap and this
/// computer's wrap in the bucket - then the recovery sheet (four groups typed back, the
/// emergency kit), which finishes the drive's making. Nothing is moved afterwards: the drive is
/// empty. A build without a drive index cannot open encrypted drives, so it leaves the drive
/// plain. Called by the flows that MAKE a drive (bought, claimed): the drive is remembered as
/// made in this run, the only new drive a scan's end sets up by itself ([`resume_wanted`]).
pub(crate) fn encrypt_new_drive(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
) {
    {
        let mut made = made_this_run();
        if !made.iter().any(|d| d == drive_id) {
            made.push(drive_id.to_string());
        }
    }
    begin_new_drive_setup(info, app, s, drive_id);
}

/// The setup of a new drive, once per drive and run: the job and its busy dialog.
fn begin_new_drive_setup(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
) {
    let Some(index) = s.slot_index(drive_id) else {
        return;
    };
    if !offered() || !sets_up_encryption_at_creation(&s.slots[index].entry) {
        return;
    }
    let Some(auto) = auto_of(s, drive_id) else {
        return;
    };
    {
        let mut started = new_drive_set_up();
        if started.iter().any(|d| d == drive_id) {
            return;
        }
        started.push(drive_id.to_string());
    }
    println!("AZDRIVE_ENCRYPTING_NEW_DRIVE {drive_id}");
    // Another dialog stays (a paid drive that arrived while the Add drive dialog is open): the
    // sheet waits for it to close.
    if s.popup.is_none() {
        s.popups_opened += 1;
        s.popup.open(Popup::Encryption(Dialog::Busy {
            title: t("azdrive-enc-setting-up-title"),
            text: t("azdrive-enc-setting-up"),
        }));
    }
    spawn(
        info,
        app,
        s,
        Job::Encryption(EncryptionJob::SetUp {
            drive_id: drive_id.to_string(),
            auto,
            new_drive: true,
            before: None,
        }),
    );
}

/// The new drives whose setup started in this run (at their making, or when a scan ended on
/// them, [`resume_wanted`]): each is set up once per run.
static NEW_DRIVE_SET_UP: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn new_drive_set_up() -> std::sync::MutexGuard<'static, Vec<String>> {
    NEW_DRIVE_SET_UP
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The drives this run made (bought or claimed, [`encrypt_new_drive`]).
static MADE_THIS_RUN: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn made_this_run() -> std::sync::MutexGuard<'static, Vec<String>> {
    MADE_THIS_RUN
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Whether the end of a scan sets the drive up by itself, from the drive's state (`None`: the
/// drive is not open), whether this run made the drive and whether its setup started in this run.
/// Only a drive this run made, whose first try could not start (it was not open yet), and a setup
/// of THIS computer that stopped halfway in an earlier run (its key is in the keyring): a new
/// drive that comes up from the storage with no key, no note and an empty listing is a server's
/// word, and a key made on it would be a second key family beside the drive's real one - it waits
/// for "Encrypt this drive" (X-DELTA-AZDRIVE-KEYS-19).
fn resume_wanted(state: Option<EncState>, made_this_run: bool, started: bool) -> bool {
    !started
        && state.is_some_and(|state| {
            state.resumes_setup() || (made_this_run && state.needs_encryption())
        })
}

/// A new Azlin drive takes no file until it is encrypted (azul-storage refuses the writes): when
/// a scan ends on one that THIS run made and could not set up at once, or on a setup of this
/// computer that stopped halfway ([`resume_wanted`]), its setup runs now. Not while this run's
/// setup of it is under way or done; a failure leaves "Encrypt this drive" in its menu.
pub(crate) fn resume_new_drive_setup(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(drive_id) = s.current_drive_id() else {
        return;
    };
    let state = s
        .slot_index(&drive_id)
        .and_then(|index| s.slots[index].auto.as_ref())
        .map(|auto| auto.state());
    let made_here = made_this_run().contains(&drive_id);
    let started = new_drive_set_up().contains(&drive_id);
    if !resume_wanted(state, made_here, started) {
        return;
    }
    println!("AZDRIVE_NEW_DRIVE_NOT_ENCRYPTED {drive_id}");
    begin_new_drive_setup(info, app, s, &drive_id);
}

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
        s.error(Text::key("azdrive-restore-open-first"));
        return None;
    }
    crate::open_slot(s, index)?;
    let auto = s.slots[index].auto.clone();
    if auto.is_none() {
        s.error(Text::key("azdrive-enc-only-azlin"));
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
    Some(restore_with(auto))
}

/// The closure the restore job is given for the open drive `auto`: the one place it is made, so
/// that a test of the job ([`crate::restore::run`]) enters with the very closure production
/// does and not one made by hand (X-DELTA-AZDRIVE-KEYS-40).
pub(crate) fn restore_with(auto: Arc<AutoEncrypted>) -> crate::restore::EncryptedRestore {
    Box::new(move |as_of| restore_encrypted(&auto, as_of))
}

/// An encrypted drive back as its metadata repository had it at `as_of` (seconds since 1970),
/// as one new commit ([`azcloud_kit::restore_drive_as_of`]) through a copy of the repository of
/// its own (in memory; the open drive's index sees the commit at its next poll): the files put
/// back or taken away.
///
/// The drive is decided first, on the job's thread, and what "could not tell" (a keyring that
/// would not open, no answer, a locked drive) and "not encrypted" do is the state's policy
/// ([`EncState::restore_kind`]): an error for the user. NEVER the bucket's restore at the token
/// server, which rolls every object of the bucket back, an encrypted drive's included
/// (X-AR5b-2).
fn restore_encrypted(auto: &AutoEncrypted, as_of: i64) -> Result<usize, String> {
    let state = auto.decide().map_err(|e| e.to_string())?;
    match state.restore_kind() {
        RestoreKind::Files => {}
        RestoreKind::OpenFirst => return Err(String::from("azdrive-restore-open-first")),
        RestoreKind::EncryptFirst => return Err(String::from("azdrive-restore-encrypt-first")),
    }
    let bucket = Arc::clone(auto.bucket());
    let restored = (|| -> Result<usize, String> {
        let key = device::unlock(bucket.as_ref(), &azlin_keyring(Asking::User), auto.drive())
            .map_err(|e| e.to_string())?
            .ok_or_else(|| String::from("azdrive-enc-err-device-no-key"))?;
        let device_id = azul_storage::ids::new_uuid();
        let mut repo = MetaRepo::open(DriveBucket::new(bucket), key, &device_id, "AzDrive")
            .map_err(|e| e.to_string())?;
        let restore = azcloud_kit::restore_drive_as_of(&mut repo, as_of, &mut keep_both)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| String::from("azdrive-enc-err-made-later"))?;
        Ok(restore.restored + restore.removed)
    })();
    // The files changed and no key did: the key id this computer recorded stays
    // (X-DELTA-AZDRIVE-KEYS-18; `reopen` is for an act that brings a key).
    auto.reload();
    restored
}

/// The bucket below an encrypted drive (decided first: why it could not be is the error); an
/// error for one that is not, whose index names only each file's current version.
fn versioned_bucket(auto: &AutoEncrypted) -> Result<Arc<dyn Drive>, String> {
    if !auto.decide().map_err(|e| e.to_string())?.has_drive_index() {
        return Err(String::from(
            "a plain drive keeps no versions of its files: its index names each file's current \
             version only",
        ));
    }
    Ok(Arc::clone(auto.bucket()))
}

/// Every stored version of the file `key` of an encrypted drive, oldest first, from its
/// metadata repository (a copy of its own, in memory), the drive key from `keyring`.
pub(crate) fn file_versions(
    auto: &AutoEncrypted,
    keyring: &dyn KeyringStore,
    key: &str,
) -> Result<Vec<azcloud_kit::restore::FileVersion>, String> {
    let bucket = versioned_bucket(auto)?;
    let drive_key = device::unlock(bucket.as_ref(), keyring, auto.drive())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| String::from("azdrive-enc-err-device-no-key"))?;
    let device_id = azul_storage::ids::new_uuid();
    let mut repo = MetaRepo::open(DriveBucket::new(bucket), drive_key, &device_id, "AzDrive")
        .map_err(|e| e.to_string())?;
    azcloud_kit::restore::file_versions(&mut repo, key).map_err(|e| e.to_string())
}

/// The version `id` of the file `key` of an encrypted drive put back as the drive's (an older
/// version, or a file deleted since), as one commit; the open drive reads its index again.
pub(crate) fn restore_file_version(
    auto: &AutoEncrypted,
    keyring: &dyn KeyringStore,
    key: &str,
    id: azul_storage::meta::ObjectId,
) -> Result<(), String> {
    let bucket = versioned_bucket(auto)?;
    let drive_key = device::unlock(bucket.as_ref(), keyring, auto.drive())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| String::from("azdrive-enc-err-device-no-key"))?;
    let device_id = azul_storage::ids::new_uuid();
    let mut repo = MetaRepo::open(DriveBucket::new(bucket), drive_key, &device_id, "AzDrive")
        .map_err(|e| e.to_string())?;
    let restored = azcloud_kit::restore::restore_file_version(&mut repo, key, id, &mut keep_both)
        .map(drop)
        .map_err(|e| e.to_string());
    auto.reload();
    restored
}

/// "Encrypt this drive...": the question.
pub(crate) fn ask_encrypt(s: &mut DriveState, drive_id: &str) {
    if s.popup.is_none() {
        s.popups_opened += 1;
        s.popup.open(Popup::Encryption(Dialog::Confirm {
            drive_id: drive_id.to_string(),
        }));
    }
}

/// Asks the drive index of every open encrypted Azlin drive whose totals are due for them (the
/// files' size before compression, their stored bytes): the usage line's extra information.
pub(crate) fn request_totals(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let now = azul_storage::time::now_unix();
    let due: Vec<(String, Arc<AutoEncrypted>)> = s
        .slots
        .iter()
        .filter_map(|slot| {
            let (azlin_id, _) = slot.entry.azlin()?;
            let auto = slot.auto.clone()?;
            (auto.state().has_drive_index()
                && crate::usage_view::totals_due(s.usage.get(azlin_id), now))
            .then(|| (azlin_id.to_string(), auto))
        })
        .collect();
    for (azlin_id, auto) in due {
        // Asked once: a second look before the answer does not ask again.
        let previous = s.usage.get(&azlin_id).copied().unwrap_or_default();
        s.usage.insert(
            azlin_id.clone(),
            crate::usage_view::DriveUsage {
                totals_at: Some(now),
                ..previous
            },
        );
        spawn(
            info,
            app,
            s,
            Job::Encryption(EncryptionJob::Totals { azlin_id, auto }),
        );
    }
}

/// Whether a recovery sheet may take the window's dialog now: no dialog is open, or one that
/// only reports progress is (the encryption's own "working" page: the sheet is what it waits
/// for). The popup slot's [`open_when_free`](crate::popup_slot::PopupSlot::open_when_free) goes
/// by the same rule.
pub(crate) fn sheet_shows_now(popup: Option<&Popup>) -> bool {
    popup.is_none_or(Popup::reports_progress)
}

/// The set-up's answer for a new drive that holds key files already: another computer made its
/// keys first (a paid drive picked up with its claim code after the computer it was bought on
/// set it up). It is not set up again; it opens here with an invite from that computer or with
/// its recovery code. Also the l10n key of what the status line says then.
pub(crate) const ENCRYPTED_ELSEWHERE: &str = "azdrive-enc-new-drive-encrypted-elsewhere";

/// Whether a failed set-up is told in a dialog of its own: not for a drive another computer
/// encrypted first (the status line says it), and never in place of another open dialog (the
/// Add drive dialog the drive was picked up in stays; the status line says why).
pub(crate) fn set_up_failure_shows_as_dialog(why: &str, popup: Option<&Popup>) -> bool {
    why != ENCRYPTED_ELSEWHERE && sheet_shows_now(popup)
}

/// The drives whose move of plaintext files into the encryption was looked at in this run.
static MIGRATION_LOOKED_AT: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The drives whose move runs now: one at a time per drive, however it was started (the sheet's
/// end, the resume).
static MIGRATING: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn migrating() -> std::sync::MutexGuard<'static, Vec<String>> {
    MIGRATING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Whether the encryption of the drive `drive_id` has plaintext files left to move: its mark in
/// the settings says so, or the move's journal does (`done` false). A journal that cannot be read
/// is an error, said by `Err`.
fn files_left_to_move(s: &DriveState, drive_id: &str) -> Result<bool, String> {
    let marked =
        state_of(&s.settings.recovery.drives, drive_id).is_some_and(|state| state.moves_files);
    let file = state_file(s, drive_id)?;
    let journal = load_state(&file, |text| MigrationState::from_json(text).ok())?;
    Ok(move_is_due(marked, journal.as_ref()))
}

/// Whether a drive's encryption has plaintext files left to move: the settings' mark says so
/// (until the move's end clears it), or the move's journal was left unfinished.
fn move_is_due(marked: bool, journal: Option<&MigrationState>) -> bool {
    marked || journal.is_some_and(|state| !state.done)
}

/// An encryption that was stopped goes on (AZ-K9): a drive open encrypted on this computer whose
/// plaintext files were not all moved in - a kill during the sheet or the move - starts its move
/// again, once per run and drive, when no dialog shows. The move continues from its journal; it
/// deletes an original only after the encrypted copy is verified. Whether a move started.
fn resume_migration(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) -> bool {
    if s.popup.is_some() {
        return false;
    }
    let candidates: Vec<String> = s
        .slots
        .iter()
        .filter(|slot| {
            slot.auto
                .as_ref()
                .is_some_and(|auto| auto.state().has_drive_index())
        })
        .map(|slot| slot.entry.id.clone())
        .collect();
    for drive_id in candidates {
        if migrating().contains(&drive_id) {
            continue;
        }
        {
            let mut looked = MIGRATION_LOOKED_AT
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if looked.contains(&drive_id) {
                continue;
            }
            looked.push(drive_id.clone());
        }
        match files_left_to_move(s, &drive_id) {
            Ok(true) => {
                println!("AZDRIVE_MIGRATION_RESUMED {drive_id}");
                start_migration(info, app, s, &drive_id);
                return true;
            }
            Ok(false) => {}
            Err(why) => {
                let name = drive_name(s, &drive_id);
                s.warn(
                    Phrase::new("azdrive-enc-migrate-stopped")
                        .arg("name", name.as_str())
                        .arg("why", t_label(&why)),
                );
            }
        }
    }
    false
}

/// The question of re-encrypting the files a rotation left under their old file keys, asked
/// again (AZ-K18): a drive whose files wait ([`RecoveryState::reencrypt_due`]) that is open
/// encrypted on this computer and was not asked about in the last week, when no dialog shows.
/// What was asked is stamped (the caller saves the settings). Whether a dialog opened.
fn reencrypt_if_due(s: &mut DriveState, now: u64) -> bool {
    if s.popup.is_some() {
        return false;
    }
    let due = s
        .settings
        .recovery
        .drives
        .iter()
        .find(|state| {
            state.reencrypt_ask_due(now)
                && s.slot_index(&state.drive_id)
                    .and_then(|index| s.slots[index].auto.as_ref())
                    .is_some_and(|auto| auto.state().has_drive_index())
        })
        .map(|state| state.drive_id.clone());
    let Some(drive_id) = due else {
        return false;
    };
    state_mut(&mut s.settings.recovery.drives, &drive_id).reencrypt_questioned(now);
    println!("AZDRIVE_REENCRYPT_DUE {drive_id}");
    s.popups_opened += 1;
    s.popup
        .open(Popup::Encryption(Dialog::OfferReencrypt { drive_id }));
    true
}

/// "I was hacked: new keys...": the question.
pub(crate) fn ask_rotate(s: &mut DriveState, drive_id: &str) {
    if s.popup.is_none() {
        s.popups_opened += 1;
        s.popup.open(Popup::Encryption(Dialog::ConfirmRotate {
            drive_id: drive_id.to_string(),
            typed: Zeroizing::new(String::new()),
            error: String::new(),
        }));
    }
}

/// "I was hacked: new keys…" from the answer to a device's removal: the question of the new keys
/// for the drive the device was removed from.
extern "C" fn on_new_keys_after_removal(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| {
        let Some(Popup::Encryption(Dialog::DeviceRemoved { drive_id, .. })) = s.popup.take() else {
            return;
        };
        ask_rotate(s, &drive_id);
    })
}

extern "C" fn on_rotate(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, _app, s| {
        if matches!(s.popup.as_ref(), Some(Popup::Encryption(Dialog::ConfirmRotate { .. }))) {
            // New keys are made only after Touch ID or the password (the gated tier).
            crate::presence::ask(info, s, crate::presence::Gated::Rotate);
        }
    })
}

/// "I was hacked: new keys..." confirmed and the user's presence checked: the typed code is
/// checked, the dialog says it works, the rotation runs on a worker thread.
pub(crate) fn rotate_confirmed(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    let Some(Popup::Encryption(Dialog::ConfirmRotate {
        drive_id, typed, ..
    })) = s.popup.as_ref()
    else {
        return;
    };
    let drive_id = drive_id.clone();
    // The current code, when typed, signs the new one's registration (F12 C).
    let current = if typed.trim().is_empty() {
        None
    } else {
        let known = crate::recovery_health::state_of(&s.settings.recovery.drives, &drive_id)
            .and_then(|state| state.recovery_key.clone());
        match current_code(known.as_deref(), &drive_id, typed) {
            Ok(code) => Some(code.to_text()),
            Err(why) => {
                if let Some(Popup::Encryption(Dialog::ConfirmRotate { error, .. })) =
                    s.popup.as_mut()
                {
                    *error = why;
                }
                return;
            }
        }
    };
    s.popup.close();
    let Some(auto) = auto_of(s, &drive_id) else {
        return;
    };
    let Some(azlin) = s
        .slot_index(&drive_id)
        .and_then(|index| s.slots[index].azlin.clone())
    else {
        s.error(Text::key("azdrive-enc-only-azlin-lockdown"));
        return;
    };
    s.popup.open(Popup::Encryption(Dialog::Busy {
        title: t("azdrive-enc-rotating-title"),
        text: t("azdrive-enc-rotating"),
    }));
    // The rotation replaces the recovery wrap before the new code is shown: the code typed back
    // right until now is no witness from here on, whatever becomes of the job (it may stop with
    // the wrap replaced and the new code never seen). Saved before the job starts.
    state_mut(&mut s.settings.recovery.drives, &drive_id).wrap_may_change();
    crate::save_settings(info, app, s);
    spawn(
        info,
        app,
        s,
        Job::Encryption(EncryptionJob::Rotate {
            reencrypt_state: state_dir(s)
                .ok()
                .map(|dir| crate::footprint::reencrypt_state(&dir, &drive_id)),
            drive_id,
            auto,
            azlin,
            current,
        }),
    );
}

extern "C" fn on_reencrypt(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::Encryption(Dialog::OfferReencrypt { drive_id })) = s.popup.take() else {
            return;
        };
        let Some(auto) = auto_of(s, &drive_id) else {
            return;
        };
        let name = drive_name(s, &drive_id);
        let state_file = match state_dir(s) {
            Ok(dir) => crate::footprint::reencrypt_state(&dir, &drive_id),
            Err(why) => {
                s.error(
                    Phrase::new("azdrive-enc-reencrypt-stopped")
                        .arg("name", name.as_str())
                        .arg("why", t_label(&why)),
                );
                return;
            }
        };
        s.info(Phrase::new("azdrive-enc-reencrypting").arg("name", name));
        spawn(
            info,
            app,
            s,
            Job::Encryption(EncryptionJob::Reencrypt {
                rotated_at: state_of(&s.settings.recovery.drives, &drive_id)
                    .and_then(|state| state.reencrypt_due)
                    .unwrap_or(0),
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
        s.popup.open(Popup::Encryption(Dialog::Unlock {
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
            | Dialog::CancelLockdown { typed, error, .. }
            | Dialog::GetDriveBack { typed, error, .. }
            | Dialog::ConfirmRotate { typed, error, .. }
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
        s.popup.open(Popup::Encryption(Dialog::Busy {
            title: t("azdrive-enc-encrypting-title"),
            text: t("azdrive-enc-encrypting"),
        }));
        // A drive that holds files: the mark that they still have to move is saved by the job
        // itself before it makes the key.
        let before = s.settings_drive.clone().map(|drive| {
            mark_files_will_move_first(
                drive,
                s.settings_base.clone(),
                s.settings.clone(),
                &drive_id,
            )
        });
        spawn(
            info,
            app,
            s,
            Job::Encryption(EncryptionJob::SetUp {
                drive_id,
                auto,
                new_drive: false,
                before,
            }),
        );
    })
}

/// "Hide the code and check it": the second step of the sheet. The code leaves the clipboard now
/// (when it is still the code), so that the check cannot be pasted - also when it got there by
/// a Cmd-C of the selectable text and not by the Copy button, which is the only thing that sets
/// `copied` (X-DELTA-AZDRIVE-KEYS-24).
extern "C" fn on_sheet_hide(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, _app, s| {
        let Some(Popup::Encryption(Dialog::Sheet(sheet))) = s.popup.as_mut() else {
            return;
        };
        crate::ui_bridge::clipboard_forget(info, sheet.code.as_str());
        sheet.copied = false;
        sheet.hide_code();
    })
}

/// "Show the code again": back to the first step.
extern "C" fn on_sheet_show(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| {
        if let Some(Popup::Encryption(Dialog::Sheet(sheet))) = s.popup.as_mut() {
            sheet.show_code();
        }
    })
}

extern "C" fn on_sheet_done(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::Encryption(Dialog::Sheet(sheet))) = s.popup.as_mut() else {
            return;
        };
        if !sheet.confirmed() {
            sheet.error = t_args(
                "azdrive-enc-groups-wrong",
                &[("groups", Arg::from(sheet.asked_words()))],
            );
            return;
        }
        let drive_id = sheet.drive_id.clone();
        let (extra, after_rotation, new_drive) =
            (sheet.extra, sheet.after_rotation, sheet.new_drive);
        // The sheet closes: the code leaves the clipboard if it is still there, however it got
        // there (X-DELTA-AZDRIVE-KEYS-24).
        crate::ui_bridge::clipboard_forget(info, sheet.code.as_str());
        // The groups typed back are the user's own close of the sheet: it goes now, and what
        // follows shows after it.
        s.popup.user_closed();
        if extra {
            // A second kit: its key is at the token server already; nothing else to set up.
            crate::recovery::forget_print_copies();
            println!("AZDRIVE_RECOVERY_CODE_VERIFIED {drive_id}");
            s.success(Phrase::new("azdrive-keys-second-works"));
            return;
        }
        // The sheet closes: Print's copies of the kit go; the setup's check is kept.
        crate::recovery::forget_print_copies();
        crate::recovery::setup_verified(s, &drive_id);
        crate::save_settings(info, app, s);
        if after_rotation {
            s.popup.open(Popup::Encryption(Dialog::OfferReencrypt { drive_id }));
            return;
        }
        if new_drive {
            // A new drive is empty: nothing to move into the encryption.
            let name = drive_name(s, &drive_id);
            println!("AZDRIVE_ENCRYPTED_NEW_DRIVE {drive_id}");
            s.success(Phrase::new("azdrive-enc-new-drive-ready").arg("name", name));
            crate::refresh(info, app, s);
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
    let state_file = match state_file(s, drive_id) {
        Ok(file) => file,
        Err(why) => {
            let name = drive_name(s, drive_id);
            s.error(
                Phrase::new("azdrive-enc-migrate-stopped")
                    .arg("name", name.as_str())
                    .arg("why", t_label(&why)),
            );
            return;
        }
    };
    {
        // One move at a time per drive: a second start while it runs would move the same files.
        let mut running = migrating();
        if running.iter().any(|other| other == drive_id) {
            return;
        }
        running.push(drive_id.to_string());
    }
    s.popup.open(Popup::Encryption(Dialog::Busy {
        title: t("azdrive-enc-migrating-title"),
        text: t_args(
            "azdrive-enc-migrating",
            &[("name", Arg::from(drive_name(s, drive_id)))],
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
            *error = t("azdrive-drill-not-a-code");
            return;
        };
        let drive_id = drive_id.clone();
        let Some(auto) = auto_of(s, &drive_id) else {
            return;
        };
        s.popup.open(Popup::Encryption(Dialog::Busy {
            title: t("azdrive-enc-unlocking-title"),
            text: t("azdrive-enc-unlocking"),
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

/// Work a job does on its own thread BEFORE its first irreversible step: a write of the
/// settings that the step's effect depends on, so that it is on disk before the effect exists
/// (X-DELTA-AZDRIVE-KEYS-20). An error stops the job with nothing done.
pub(crate) type Before = Box<dyn FnOnce() -> Result<(), String> + Send>;

/// The write that puts the mark "this drive's plaintext files still have to move into the
/// encryption" on disk: `mine` (the window's settings) with the mark set, saved by a merge
/// against `start` as every save of the settings is. The mark used to be set by the outcome
/// handler of the setup, after the key was made: a kill in between left an encrypted drive with no
/// mark, no journal and its plaintext files in the bucket, invisible in the encrypted view. A
/// mark that turns out stale is harmless: the resume looks at encrypted drives only and the
/// move's end clears it.
fn mark_files_will_move_first(
    drive: LocalDrive,
    start: crate::model::Settings,
    mut mine: crate::model::Settings,
    drive_id: &str,
) -> Before {
    state_mut(&mut mine.recovery.drives, drive_id).files_will_move();
    Box::new(move || crate::save_settings_merged(&drive, &start, &mine).map_err(|e| e.to_string()))
}

/// One blocking encryption task.
pub(crate) enum EncryptionJob {
    /// The drive's keys and the recovery code (`new_drive`: as part of the drive's making).
    SetUp {
        drive_id: String,
        auto: Arc<AutoEncrypted>,
        new_drive: bool,
        /// Done after the checks and before the key is made (see [`Before`]).
        before: Option<Before>,
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
        /// The current recovery code as typed (it signs the new code's registration).
        current: Option<Zeroizing<String>>,
        /// The state of the "re-encrypt everything" pass of the keys before: a pass that stopped
        /// has its cursor and its cut-off time there, and a new rotation makes both wrong
        /// (X-DELTA-AZDRIVE-KEYS-29). `None`: no state folder.
        reencrypt_state: Option<PathBuf>,
    },
    /// A new recovery code for a drive whose setup never passed its sheet.
    NewCode {
        drive_id: String,
        auto: Arc<AutoEncrypted>,
    },
    /// The drive index's totals (the usage line's original size).
    Totals {
        azlin_id: String,
        auto: Arc<AutoEncrypted>,
    },
    /// Every file into a new object, the state in `state_file`.
    Reencrypt {
        drive_id: String,
        auto: Arc<AutoEncrypted>,
        state_file: PathBuf,
        /// When the keys the pass is for were made (the last rotation's end; 0: not known): a
        /// state of a pass that began before is of the keys before.
        rotated_at: u64,
    },
    /// The drive's recovery key (from its recovery code) registered at its token server, as
    /// signed while the code was shown ([`PendingRegistration`]) - a grant: under the drive's
    /// keyring lock with its newest drive token. Sent again at every start and daily look
    /// until the token server has it.
    RegisterRecoveryKey {
        drive_id: String,
        token_url: String,
        keyring: azcloud_kit::SharedKeyring,
        pending: PendingRegistration,
    },
    /// "Cancel lockdown": the cancel signed with the recovery code (F12; no drive token).
    CancelLockdown {
        drive_id: String,
        code: RecoveryCode,
        token_url: String,
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
    /// "Add a device": a join code of a new member family, the drive key sealed into it.
    Invite {
        drive_id: String,
        auto: Arc<AutoEncrypted>,
        token_url: String,
        shared: azcloud_kit::SharedKeyring,
        name: String,
    },
    /// "Remove drive": everything of the drive taken off this computer, the drives-file entry last
    /// ([`start_forget`]).
    Forget {
        drive_id: String,
        name: String,
        places: crate::footprint::Places,
        /// The drive keeps a session or keys in the keyring.
        uses_keyring: bool,
        /// The evidence that its recovery code was checked: the keys may go. The job takes it back
        /// where the keyring or the drive's vault say otherwise ([`proof_for_removal`]).
        proof: Option<azul_storage::footprint::RemovalProof>,
        /// The sign-in at a provider (a Google Drive, a Dropbox) to end before its refresh token
        /// goes with the keyring entry (AZ-CN12).
        grant: Option<crate::sign_in::Grant>,
        /// An Azlin drive's period vault on this computer, to be compared with the drive's own
        /// before it goes (`None`: the drive has none).
        vault: Option<VaultCheck>,
    },
    /// A joined drive's key from its join code's seal: this device enrolled with it.
    AdoptJoinKey {
        drive_id: String,
        auto: Arc<AutoEncrypted>,
        seal: Zeroizing<String>,
        /// The id of the drive key the join code names (codes from before name none).
        key_id: Option<String>,
    },
}

/// What a rotation brings back to the UI thread.
pub(crate) struct RotationDone {
    /// The new recovery code, for the sheet. A secret: never printed.
    pub code: Zeroizing<String>,
    /// The code it replaces, as typed (it signs the new one's registration). A secret.
    pub previous_code: Option<Zeroizing<String>>,
    /// The new drop public key (hex) when incoming mail is on.
    pub drop_key: Option<String>,
    pub members_removed: usize,
    /// The further recovery codes (kits) whose wraps went: their paper opens nothing now.
    pub codes_removed: usize,
    pub shares_revoked: usize,
    /// What the rotation's last step could not clear away (the keys ARE changed).
    pub left_over: Vec<String>,
}

/// What an encryption task found.
pub(crate) enum EncryptionOutcome {
    /// The recovery code to show (`Err`: why there is none).
    SetUp {
        drive_id: String,
        new_drive: bool,
        result: Result<Zeroizing<String>, String>,
    },
    Recovered {
        drive_id: String,
        result: Result<(), String>,
    },
    /// A joined drive's key from its join code's seal ([`EncryptionJob::AdoptJoinKey`]): the
    /// answer of an unlock, and - when this device has the key - whether the code's one-time key
    /// file is still in the drive (`invite_left`: why it could not be deleted).
    JoinKeyAdopted {
        drive_id: String,
        result: Result<(), String>,
        invite_left: Option<String>,
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
    /// (the files' size before compression, their stored bytes)
    Totals {
        azlin_id: String,
        result: Result<(u64, u64), String>,
    },
    RecoveryKeyRegistered {
        drive_id: String,
        /// The code's public recovery key the request registers.
        public_key: String,
        answer: RegistrationAnswer,
        /// Why not all of it, when not.
        why: Option<String>,
    },
    /// The pending recovery-key lockdown called off (or why not).
    LockdownCancelled {
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
        result: Result<Option<(Maintained, PackUpkeep)>, String>,
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
    /// The join code for another device (a secret), or why there is none.
    Invited {
        drive_id: String,
        result: Result<Zeroizing<String>, String>,
    },
    /// What taking the drive off this computer did, store by store.
    Forgotten {
        drive_id: String,
        name: String,
        report: azul_storage::footprint::ForgetReport,
        /// What ending the drive's sign-in at its provider came to (`None`: it has none).
        grant: Option<crate::sign_in::GrantEnd>,
    },
}

/// Runs on a worker thread.
pub(crate) fn run(job: EncryptionJob) -> EncryptionOutcome {
    let keyring = azlin_keyring(Asking::User);
    match job {
        EncryptionJob::SetUp {
            drive_id,
            auto,
            new_drive,
            before,
        } => {
            let result = (|| -> Result<Zeroizing<String>, String> {
                if auto.provider().is_none() {
                    return Err(String::from("azdrive-enc-err-no-index"));
                }
                // A drive that arrived with key files was set up by the computer it was
                // bought on: this one joins it (an invite, the recovery code), it does not
                // make new keys.
                // (A setup of this computer that stopped halfway has the key files of its try in
                // the bucket and its key in the keyring: it is finished, not refused.)
                if new_drive
                    && device::is_encrypted(auto.bucket().as_ref()).map_err(|e| e.to_string())?
                    && !device::setup_pending(&keyring, auto.drive()).map_err(|e| e.to_string())?
                {
                    return Err(String::from(ENCRYPTED_ELSEWHERE));
                }
                // Recorded before it is done: the mark that the files still have to move is on
                // disk before the key exists (X-DELTA-AZDRIVE-KEYS-20).
                if let Some(before) = before {
                    before()?;
                }
                let kdf = RecoveryKdf::fresh().map_err(|e| e.to_string())?;
                let (_, code) =
                    device::setup_new_drive(auto.bucket().as_ref(), &keyring, auto.drive(), kdf)
                        .map_err(|e| e.to_string())?;
                auto.reopen();
                Ok(code.to_text())
            })();
            EncryptionOutcome::SetUp {
                drive_id,
                new_drive,
                result,
            }
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
                // The saved state first: one that cannot be read stops the pass before it touches
                // the drive, and is not read as "nothing saved" (X-AR5b-12).
                let mut state =
                    load_state(&state_file, |text| MigrationState::from_json(text).ok())?
                        .unwrap_or_default();
                let provider =
                    auto.provider().ok_or_else(|| String::from("azdrive-enc-err-no-index"))?;
                let drive = open_encrypted(
                    Arc::clone(auto.bucket()),
                    &keyring,
                    auto.drive(),
                    provider.as_ref(),
                )
                .map_err(|e| e.to_string())?;
                let mut save =
                    |state: &MigrationState| save_state(&state_file, &state.to_json());
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
                let mut state =
                    load_state(&state_file, |text| RecompressState::from_json(text).ok())?
                        .unwrap_or_default();
                let provider = auto
                    .provider()
                    .ok_or_else(|| String::from("azdrive-enc-err-no-index"))?;
                let drive = open_encrypted(
                    Arc::clone(auto.bucket()),
                    &keyring,
                    auto.drive(),
                    provider.as_ref(),
                )
                .map_err(|e| e.to_string())?;
                let mut save =
                    |state: &RecompressState| save_state(&state_file, &state.to_json());
                // The pass stops at the first input, when the power cord goes, when the network
                // starts to cost, and when the computer weakens (Low Power Mode, heat).
                let stop = || !recompress_may_run();
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
            current,
            reencrypt_state,
        } => {
            let result = (|| -> Result<RotationDone, String> {
                let provider = auto
                    .provider()
                    .ok_or_else(|| String::from("azdrive-enc-err-no-index"))?;
                // The rotation of THIS device that stopped has had its lockdown. A journal anyone
                // planted in the bucket is none: it must not skip the lockdown. A rotation that
                // cannot be finished is refused here, before the lockdown.
                if rotation::lockdown_needed(auto.bucket().as_ref(), &keyring, auto.drive())
                    .map_err(|e| e.to_string())?
                {
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
                // The keys changed: a "re-encrypt everything" pass that stopped under the keys
                // before has its cursor and its cut-off in a file that is wrong now, and a new pass
                // loaded it, skipped the files at or before the cursor and ended at once
                // (X-DELTA-AZDRIVE-KEYS-29). The pass asks for its own age as well
                // ([`current_pass`]), for a file that could not be removed here.
                if let Some(file) = &reencrypt_state {
                    if let Err(e) = std::fs::remove_file(file) {
                        if e.kind() != std::io::ErrorKind::NotFound {
                            eprintln!("AZDRIVE_REENCRYPT_STATE_KEPT {}: {e}", file.display());
                        }
                    }
                }
                Ok(RotationDone {
                    code: rotated.recovery_code.to_text(),
                    previous_code: current,
                    drop_key: rotated.drop_key.map(|key| key.to_hex()),
                    members_removed: rotated.members_removed,
                    codes_removed: rotated.codes_removed,
                    shares_revoked: rotated.shares_revoked,
                    left_over: rotated.left_over,
                })
            })();
            EncryptionOutcome::Rotated { drive_id, result }
        }
        EncryptionJob::NewCode { drive_id, auto } => {
            let result = (|| -> Result<Zeroizing<String>, String> {
                let bucket = Arc::clone(auto.bucket());
                // The key is in the keyring after `unlock` (it was, or the device's wrap opened it
                // and it was kept): read from there, it is the evidence the wrap may be published.
                device::unlock(bucket.as_ref(), &keyring, auto.drive())
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| String::from("azdrive-enc-err-no-key"))?;
                let (key, stashed) =
                    device::held(&keyring, &device::drive_key_entry(auto.drive()))
                        .map_err(|e| e.to_string())?
                        .ok_or_else(|| String::from("azdrive-enc-err-no-key"))?;
                let code = RecoveryCode::generate().map_err(|e| e.to_string())?;
                let kdf = RecoveryKdf::fresh().map_err(|e| e.to_string())?;
                let wrap = RecoveryWrap::seal(&key, auto.drive(), &code, kdf)
                    .map_err(|e| e.to_string())?;
                device::publish_recovery_wrap(
                    bucket.as_ref(),
                    &stashed,
                    &wrap,
                    device::Publish::FirstReplace,
                )
                .map_err(|e| e.to_string())?;
                Ok(code.to_text())
            })();
            // The sheet a new drive's making shows (the four groups, the kit, no migration).
            EncryptionOutcome::SetUp {
                drive_id,
                new_drive: true,
                result,
            }
        }
        EncryptionJob::Totals { azlin_id, auto } => {
            let result = (|| -> Result<(u64, u64), String> {
                let provider = auto
                    .provider()
                    .ok_or_else(|| String::from("azdrive-enc-err-no-index"))?;
                let drive = open_encrypted(
                    Arc::clone(auto.bucket()),
                    &keyring,
                    auto.drive(),
                    provider.as_ref(),
                )
                .map_err(|e| e.to_string())?;
                let totals = drive.totals().map_err(|e| e.to_string())?;
                Ok((totals.original_bytes, totals.stored_bytes))
            })();
            EncryptionOutcome::Totals { azlin_id, result }
        }
        EncryptionJob::Reencrypt {
            drive_id,
            auto,
            state_file,
            rotated_at,
        } => {
            let result = (|| -> Result<ReencryptState, String> {
                let mut state = current_pass(
                    load_state(&state_file, |text| ReencryptState::from_json(text).ok())?,
                    rotated_at,
                )
                .unwrap_or_else(|| ReencryptState::new(now_unix()));
                let provider = auto
                    .provider()
                    .ok_or_else(|| String::from("azdrive-enc-err-no-index"))?;
                let drive = open_encrypted(
                    Arc::clone(auto.bucket()),
                    &keyring,
                    auto.drive(),
                    provider.as_ref(),
                )
                .map_err(|e| e.to_string())?;
                let mut save =
                    |state: &ReencryptState| save_state(&state_file, &state.to_json());
                reencrypt_pass(&drive, &mut state, &mut save, &|| false)
                    .map_err(|e| e.to_string())?;
                // Done: the next "Re-encrypt" starts a new pass. A state that cannot be removed
                // is said (the next pass would find a finished one).
                if let Err(e) = std::fs::remove_file(&state_file) {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        eprintln!(
                            "AZDRIVE_REENCRYPT_STATE_KEPT {}: {e}",
                            state_file.display()
                        );
                    }
                }
                Ok(state)
            })();
            EncryptionOutcome::Reencrypted { drive_id, result }
        }
        EncryptionJob::RegisterRecoveryKey {
            drive_id,
            token_url,
            keyring,
            pending,
        } => {
            let (answer, why) = register(&drive_id, &token_url, &keyring, &pending);
            EncryptionOutcome::RecoveryKeyRegistered {
                drive_id,
                public_key: pending.public_key,
                answer,
                why,
            }
        }
        EncryptionJob::CancelLockdown {
            drive_id,
            code,
            token_url,
        } => {
            let result = cancel_lockdown(&drive_id, &code, &token_url);
            EncryptionOutcome::LockdownCancelled { drive_id, result }
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
        EncryptionJob::Invite {
            drive_id,
            auto,
            token_url,
            shared,
            name,
        } => {
            let result = (|| -> Result<Zeroizing<String>, String> {
                let transport = AzulTransport::azlin(crate::USER_AGENT);
                let server = TokenServer::new(&token_url, &transport).map_err(|e| e.to_string())?;
                let mut code = shared
                    .invite(&server, &drive_id, &name, None)
                    .map_err(|e| e.to_string())?;
                // the drive key rides along, sealed to a one-time key whose secret is in the code
                if let Some(drive_key) =
                    device::load_drive_key(&keyring, auto.drive()).map_err(|e| e.to_string())?
                {
                    let invite = device::seal_invite(auto.bucket().as_ref(), auto.drive(), &drive_key)
                        .map_err(|e| e.to_string())?;
                    code.key_seal = Some(device::invite_text(&invite).to_string());
                    // the joining device takes no other key than this one
                    code.key_id = Some(drive_key.id().to_hex());
                }
                Ok(Zeroizing::new(code.encode()))
            })();
            EncryptionOutcome::Invited { drive_id, result }
        }
        EncryptionJob::Forget {
            drive_id,
            name,
            places,
            uses_keyring,
            proof,
            grant,
            vault,
        } => {
            // The sign-in at a provider is ended first: it needs the refresh token the keyring is
            // about to lose, and a copy of that token must open nothing afterwards (AZ-CN12).
            let grant = grant.map(|grant| {
                crate::sign_in::end_grant(
                    &grant,
                    &keyring,
                    &AzulTransport::foreign(crate::USER_AGENT),
                )
            });
            // The vault is compared with the drive's only when a proof would let it go: without
            // one it stays, and nothing is asked of the drive for it.
            let paid_months_elsewhere = proof
                .as_ref()
                .and_then(|_| vault.map(|vault| vault.redundant(&drive_id)));
            // The user asked for this: the keyring may ask (the unlock prompt, Touch ID).
            let report = forget_job(
                &keyring,
                &drive_id,
                &places,
                uses_keyring,
                proof,
                paid_months_elsewhere,
            );
            EncryptionOutcome::Forgotten {
                drive_id,
                name,
                report,
                grant,
            }
        }
        EncryptionJob::AdoptJoinKey {
            drive_id,
            auto,
            seal,
            key_id,
        } => {
            let (result, invite_left) =
                adopt_join_key_with(&keyring, &auto, &seal, key_id.as_deref());
            EncryptionOutcome::JoinKeyAdopted {
                drive_id,
                result,
                invite_left,
            }
        }
    }
}

/// How big a join code's QR code draws its modules (it holds more than a recovery code).
const INVITE_MODULE_PX: usize = 2;

/// "Add a device" (the other devices' row of the recovery methods): a join code for it, made in
/// the background.
pub(crate) fn invite_device(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
) {
    if s.popup.is_some() {
        return;
    }
    let Some(token_url) = token_url_of(s, drive_id) else {
        s.error(Phrase::new("azdrive-no-token-server"));
        return;
    };
    let Some(auto) = auto_of(s, drive_id) else {
        return;
    };
    s.popups_opened += 1;
    s.popup.open(Popup::Encryption(Dialog::Busy {
        title: t("azdrive-invite-title"),
        text: t("azdrive-invite-making"),
    }));
    let job = EncryptionJob::Invite {
        drive_id: drive_id.to_string(),
        auto,
        token_url,
        shared: s.keyring.clone(),
        name: drive_name(s, drive_id),
    };
    spawn(info, app, s, Job::Encryption(job));
}

/// A joined drive's join code carried its key (`seal`): this device takes it in the
/// background.
pub(crate) fn adopt_join_key(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    seal: String,
    key_id: Option<String>,
) {
    let seal = Zeroizing::new(seal);
    let Some(auto) = auto_of(s, drive_id) else {
        return;
    };
    let job = EncryptionJob::AdoptJoinKey {
        drive_id: drive_id.to_string(),
        auto,
        seal,
        key_id,
    };
    spawn(info, app, s, Job::Encryption(job));
}

/// "Save to a file…" of the join code: the system's save dialog asks for the place, and the file is
/// written HERE, readable by this user only (`paper::save_private`) - `FileDialog::save_bytes`
/// writes it with the default mode (0644 on Unix), and a join code is the drive token and the key
/// seal of a computer that joins: whoever reads the file joins (X-DELTA-AZDRIVE-KEYS-24).
extern "C" fn on_invite_save(mut data: RefAny, _info: CallbackInfo) -> Update {
    let code = {
        let Some(s) = data.downcast_ref::<DriveState>() else {
            return Update::DoNothing;
        };
        match s.popup.as_ref() {
            Some(Popup::Encryption(Dialog::Invite { code, .. })) => code.clone(),
            _ => return Update::DoNothing,
        }
    };
    crate::paper::save_private(
        "azdrive-invite-save",
        &t("azdrive-invite-file-name"),
        Zeroizing::new(code.as_bytes().to_vec()),
        RefAny::new(InviteSave { app: data.clone() }),
        on_invite_saved,
    );
    Update::DoNothing
}

/// The join code's save dialog: the app.
struct InviteSave {
    app: RefAny,
}

/// The save dialog of the join code answered: what became of it is said in the dialog.
fn on_invite_saved(
    info: &mut CallbackInfo,
    mut then: RefAny,
    saved: crate::paper::Saved,
) -> Update {
    let Some(mut app) = then.downcast_ref::<InviteSave>().map(|d| d.app.clone()) else {
        return Update::DoNothing;
    };
    with_state(&mut app, info, |_info, _app, s| {
        if let Some(Popup::Encryption(Dialog::Invite { note, .. })) = s.popup.as_mut() {
            *note = if saved.ok {
                println!("AZDRIVE_INVITE_SAVED");
                t_args(
                    "azdrive-kit-saved",
                    &[("name", Arg::from(saved.name.as_str()))],
                )
            } else {
                t("azdrive-kit-not-saved")
            };
        }
    })
}

/// The token server's recovery key of `drive_id` from its recovery `code`
/// ([`azcloud_kit::RecoveryKey`]).
pub(crate) fn recovery_key_of(code: &RecoveryCode, drive_id: &str) -> azcloud_kit::RecoveryKey {
    azcloud_kit::RecoveryKey::derive(code.as_bytes(), drive_id)
}

/// The recovery code typed where the token server takes only the drive's CURRENT code's
/// signature (F12, "the recovery code always wins": cancelling a lockdown, a new code's
/// registration after "I was hacked"): parsed, and checked offline against the code's public
/// key when this computer knows it ([`crate::recovery::drill_answer`]).
pub(crate) fn current_code(
    known_key: Option<&str>,
    drive_id: &str,
    typed: &str,
) -> Result<RecoveryCode, String> {
    let Some(code) = RecoveryCode::parse(typed) else {
        return Err(t("azdrive-drill-not-a-code"));
    };
    match crate::recovery::drill_answer(known_key, drive_id, typed) {
        crate::recovery::DrillAnswer::NotTheCode => Err(t("azdrive-enc-not-current-code")),
        _ => Ok(code),
    }
}

/// The drive's token server, from its entry (else this run's).
pub(crate) fn token_url_of(s: &DriveState, drive_id: &str) -> Option<String> {
    let fallback = s.token.url.clone();
    s.slot_index(drive_id)
        .and_then(|index| crate::periods::azlin_drive(&s.slots[index].entry, fallback.as_deref()))
        .map(|(_, url)| url)
}

/// The registration of the new recovery `code` of `drive_id` at its token server: signed now,
/// while the code is at hand (the replacement by `previous`, the code it replaces, F12 C; the
/// findable key by the code's drive key), kept in the drive's recovery state and sent in the
/// background - and again at every start and daily look until the token server has it. The
/// emergency kit is offered only then. A drive without a token server has nothing to register.
pub(crate) fn begin_registration(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    code: &str,
    previous: Option<&str>,
) {
    let Some(code) = RecoveryCode::parse(code) else {
        return;
    };
    let Some(token_url) = token_url_of(s, drive_id) else {
        state_mut(&mut s.settings.recovery.drives, drive_id).nothing_to_register();
        crate::save_settings(info, app, s);
        return;
    };
    let pending = match sign_registration(drive_id, &code, previous.and_then(RecoveryCode::parse)) {
        Ok(pending) => pending,
        Err(why) => {
            // nothing to send: the state says the code is missing at the token server
            let state = state_mut(&mut s.settings.recovery.drives, drive_id);
            state.registration = Registration::Missing;
            crate::save_settings(info, app, s);
            let name = drive_name(s, drive_id);
            s.warn(
                Phrase::new("azdrive-enc-key-refused")
                    .arg("name", name.as_str())
                    .arg("why", t_label(&why)),
            );
            return;
        }
    };
    state_mut(&mut s.settings.recovery.drives, drive_id).registration_started(pending.clone());
    crate::save_settings(info, app, s);
    if !registering(drive_id, &pending.public_key) {
        return;
    }
    let job = EncryptionJob::RegisterRecoveryKey {
        drive_id: drive_id.to_string(),
        token_url,
        keyring: s.keyring.clone(),
        pending,
    };
    spawn(info, app, s, Job::Encryption(job));
}

/// The signed registration of `code` for `drive_id` (`previous`: the code it replaces).
fn sign_registration(
    drive_id: &str,
    code: &RecoveryCode,
    previous: Option<RecoveryCode>,
) -> Result<PendingRegistration, String> {
    let public_key = recovery_key_of(code, drive_id).public_base64();
    let signed = |p: Presigned| Signed {
        nonce: p.nonce,
        signature: p.signature,
    };
    let replace = match previous {
        Some(previous) => {
            let key = recovery_key_of(&previous, drive_id);
            let p = Presigned::replace(drive_id, &public_key, |m| Ok(key.sign_base64(m)))
                .map_err(|e| e.to_string())?;
            Some(signed(p))
        }
        None => None,
    };
    let findable = crate::recovery_keys::findable_key_of(code).public_base64();
    let signer = recovery_key_of(code, drive_id);
    let p = Presigned::add(drive_id, &findable, |m| Ok(signer.sign_base64(m)))
        .map_err(|e| e.to_string())?;
    Ok(PendingRegistration {
        public_key,
        replace,
        findable: Some(FindableSigned {
            public: findable,
            signed: signed(p),
        }),
        key_set: false,
    })
}

/// Sends a kept registration (on a worker thread): the key (unless set already), then the
/// findable key; what the token server answered, and why not all of it.
fn register(
    drive_id: &str,
    token_url: &str,
    keyring: &azcloud_kit::SharedKeyring,
    pending: &PendingRegistration,
) -> (RegistrationAnswer, Option<String>) {
    let transport = AzulTransport::azlin(crate::USER_AGENT);
    let server = match TokenServer::new(token_url, &transport) {
        Ok(server) => server,
        Err(e) => return (RegistrationAnswer::NotSet { last: true }, Some(e.to_string())),
    };
    // one key across the retries of each change (azcloud_kit::outbox): one whose answer was
    // lost is answered like the first when asked again
    let outbox = keyring.outbox();
    let public = pending.public_key.as_str();
    let presigned = |signed: &Signed| Presigned {
        nonce: signed.nonce.clone(),
        signature: signed.signature.clone(),
    };
    if !pending.key_set {
        let set = format!("set:{public}");
        let sent = keyring.with_drive_token(drive_id, |token| {
            outbox.send(IdempotentScope::RecoveryKey, drive_id, &set, |call| {
                match &pending.replace {
                    // F12 C: the code it replaces signed the change
                    Some(signed) => server.replace_recovery_key_presigned(
                        drive_id,
                        token,
                        public,
                        call,
                        &presigned(signed),
                    ),
                    None => server.set_recovery_key(drive_id, token, public, call),
                }
            })
        });
        match sent {
            Ok(Ok(_)) => {}
            // a refusal of a request whose first answer was lost (its nonce counted once): the
            // server's list says whether the key is there
            Ok(Err(e))
                if azcloud_kit::outbox::is_final_error(&e)
                    && listed(&server, keyring, drive_id, public) => {}
            Ok(Err(e)) => {
                let last = azcloud_kit::outbox::is_final_error(&e);
                // The code that replaces another is signed by the one it replaces: a signature no
                // key of the drive made is a mistyped or an older code, and the drive token is
                // good (the server answers it as its own refusal, not as a 401).
                let why = if e.recovery_refusal() == Some(RecoveryRefusal::BadSignature) {
                    String::from("azdrive-enc-err-keeps-previous-code")
                } else {
                    e.to_string()
                };
                return (RegistrationAnswer::NotSet { last }, Some(why));
            }
            // the lock or the drive's session: asked again later
            Err(e) => return (RegistrationAnswer::NotSet { last: false }, Some(e.to_string())),
        }
    }
    let Some(findable) = &pending.findable else {
        return (RegistrationAnswer::Done { findable: None }, None);
    };
    let add = format!("add:{}", findable.public);
    let sent = keyring.with_drive_token(drive_id, |token| {
        outbox.send(IdempotentScope::RecoveryKey, drive_id, &add, |call| {
            server.add_recovery_key_presigned(
                drive_id,
                token,
                &findable.public,
                crate::recovery_health::FINDABLE_LABEL,
                call,
                &presigned(&findable.signed),
            )
        })
    });
    let done = RegistrationAnswer::Done {
        findable: Some(findable.public.clone()),
    };
    match sent {
        Ok(Ok(_)) => (done, None),
        // there from an earlier try whose answer was lost
        Ok(Err(e)) if e.recovery_refusal() == Some(RecoveryRefusal::KeyExists) => (done, None),
        Ok(Err(e)) => (
            RegistrationAnswer::KeySetOnly {
                last: azcloud_kit::outbox::is_final_error(&e),
            },
            Some(crate::recovery_keys::token_text(&e)),
        ),
        Err(e) => (RegistrationAnswer::KeySetOnly { last: false }, Some(e.to_string())),
    }
}

/// Whether the drive's recovery keys at the token server include `public`.
fn listed(
    server: &TokenServer<'_>,
    keyring: &azcloud_kit::SharedKeyring,
    drive_id: &str,
    public: &str,
) -> bool {
    keyring
        .with_drive_token(drive_id, |token| server.recovery_keys(drive_id, token))
        .ok()
        .and_then(Result::ok)
        .is_some_and(|keys| keys.iter().any(|key| key.recovery_pubkey == public))
}

/// The registrations being sent in this process (drive, public key): one request per code at
/// a time (a second with the same signed nonce would be refused).
static REGISTERING: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// Takes the registration of `public_key` for `drive_id` (false: it is out already).
fn registering(drive_id: &str, public_key: &str) -> bool {
    let mut out = REGISTERING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if out.iter().any(|(d, k)| d == drive_id && k == public_key) {
        return false;
    }
    out.push((drive_id.to_string(), public_key.to_string()));
    true
}

/// The registration of `public_key` for `drive_id` answered.
fn registered(drive_id: &str, public_key: &str) {
    REGISTERING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .retain(|(d, k)| !(d == drive_id && k == public_key));
}

/// Sends the kept registrations again (`only`: of these drives): at the start, with the daily
/// look. A code from before AzDrive kept its registration is looked up in the token server's
/// list of the drive's keys instead.
pub(crate) fn retry_registrations(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    only: Option<&[String]>,
) {
    let wanted = |drive_id: &str| only.is_none_or(|only| only.iter().any(|d| d == drive_id));
    for (drive_id, pending) in registrations_to_retry(&s.settings.recovery.drives) {
        if !wanted(&drive_id) {
            continue;
        }
        let Some(token_url) = token_url_of(s, &drive_id) else {
            continue;
        };
        if !registering(&drive_id, &pending.public_key) {
            continue;
        }
        println!("AZDRIVE_RECOVERY_KEY_RETRY {drive_id}");
        // At the start and with the daily look: quietly (a refused read is sent next time).
        let job = EncryptionJob::RegisterRecoveryKey {
            drive_id,
            token_url,
            keyring: s.quiet_keyring.clone(),
            pending,
        };
        spawn(info, app, s, Job::Encryption(job));
    }
    let unknown: Vec<String> = s
        .settings
        .recovery
        .drives
        .iter()
        .filter(|state| {
            state.registration == Registration::Unknown
                && state.code_made.is_some()
                && state.recovery_key.is_some()
                && wanted(&state.drive_id)
        })
        .map(|state| state.drive_id.clone())
        .collect();
    for drive_id in unknown {
        if token_url_of(s, &drive_id).is_some() {
            crate::recovery_keys::check_keys_quietly(info, app, s, &drive_id);
        }
    }
}

/// A pending recovery-key lockdown called off with the recovery code (F12: its key signs a
/// fresh nonce; no drive token). "None is pending" (409) is NOT "called off": the token server
/// answers it both when the lockdown was called off and when its notice ended and the drive was
/// handed over, so a cancel that lost that race must not be shown as done (T1-N2) - it says what
/// is known and sends the user to the drive's devices.
fn cancel_lockdown(drive_id: &str, code: &RecoveryCode, token_url: &str) -> Result<(), String> {
    let transport = AzulTransport::azlin(crate::USER_AGENT);
    let server = TokenServer::new(token_url, &transport).map_err(|e| e.to_string())?;
    let key = recovery_key_of(code, drive_id);
    let findable = crate::recovery_keys::findable_key_of(code);
    let cancelled = server
        .lockdown_cancel_signed(drive_id, |message| Ok(key.sign_base64(message)))
        .or_else(|e| {
            // Not the drive key: the code's findable key (a second kit registers only that).
            if e.is_unauthorized() {
                server.lockdown_cancel_signed(drive_id, |message| {
                    Ok(findable.sign_base64(message))
                })
            } else {
                Err(e)
            }
        });
    match cancelled {
        Ok(_) => Ok(()),
        Err(e) if e.recovery_refusal() == Some(RecoveryRefusal::NoPendingLockdown) => {
            Err(String::from("azdrive-enc-err-no-lockdown-pending"))
        }
        Err(e) if e.is_unauthorized() => Err(String::from("azdrive-enc-err-code-mismatch")),
        Err(e) => Err(e.to_string()),
    }
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
    let transport = AzulTransport::azlin(crate::USER_AGENT);
    let server = TokenServer::new(token_url, &transport).map_err(|e| e.to_string())?;
    let key = recovery_key_of(code, drive_id);
    let findable = crate::recovery_keys::findable_key_of(code);
    // one key across the retries of this lockdown (azcloud_kit::outbox): one whose answer was
    // lost, signed anew, gets the same pending family
    let pending = keyring
        .outbox()
        .send(IdempotentScope::Lockdown, drive_id, "recovery", |call| {
            server
                .recovery_lockdown(drive_id, call, |message| Ok(key.sign_base64(message)))
                .or_else(|e| {
                    // Not the drive key: the code's findable key (a second kit registers only
                    // that).
                    if e.is_unauthorized() {
                        server.recovery_lockdown(drive_id, call, |message| {
                            Ok(findable.sign_base64(message))
                        })
                    } else {
                        Err(e)
                    }
                })
        })
        .map_err(|e| {
            if e.is_unauthorized() {
                String::from("azdrive-enc-err-code-mismatch")
            } else if e.recovery_refusal() == Some(RecoveryRefusal::NoRecoveryKey) {
                String::from("azdrive-enc-err-no-recovery-key")
            } else {
                e.to_string()
            }
        })?;
    let entry = azul_storage::config::keyring_key(drive_id);
    let _lock = keyring.lock(&entry).map_err(|e| e.to_string())?;
    let session = azcloud_kit::AzlinSession {
        drive_id: drive_id.to_string(),
        drive_token: azcloud_kit::DriveToken::new(&pending.drive_token),
        access_key_id: String::new(),
        secret_access_key: String::new(),
        session_token: None,
        expires_at: Some(0),
        grant: azcloud_kit::Grant::ReadWrite,
    };
    keyring
        .set(&entry, &session.to_keyring_secret())
        .map_err(|e| e.to_string())?;
    Ok(pending.pending_until)
}

/// A joined drive's key from its join code's `seal` (`key_id`: the id of the key the code names),
/// taken on `keyring`: this device is enrolled with it and the drive opens again. This device has
/// the key once that works. An invite that could not be deleted from the drive is not the join's
/// failure, and not the user's to miss: its reason comes back beside the answer (`invite_left`)
/// and is said in a dialog.
fn adopt_join_key_with(
    keyring: &dyn KeyringStore,
    auto: &AutoEncrypted,
    seal: &str,
    key_id: Option<&str>,
) -> (Result<(), String>, Option<String>) {
    let named = match key_id {
        Some(text) => KeyId::from_hex(text.trim())
            .map(Some)
            .ok_or_else(|| String::from("azdrive-invite-seal-damaged")),
        None => Ok(None),
    };
    let mut invite_left = None;
    let result = named.and_then(|named| {
        let invite = device::invite_from_text(seal)
            .ok_or_else(|| String::from("azdrive-invite-seal-damaged"))?;
        device::adopt_invite_reporting(
            auto.bucket().as_ref(),
            keyring,
            auto.drive(),
            &invite,
            named.as_ref(),
        )
        .map(|adopted| {
            invite_left = adopted.invite_left.as_ref().map(ToString::to_string);
            auto.reopen();
        })
        .map_err(|e| e.to_string())
    });
    (result, invite_left)
}

/// The UI thread takes a task's answer.
pub(crate) fn on_outcome(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    outcome: EncryptionOutcome,
) {
    // A task that wrote the drive's bucket (its keys, its objects, its index): the space used
    // as its node counts it now.
    if let EncryptionOutcome::SetUp { drive_id, .. }
    | EncryptionOutcome::Recovered { drive_id, .. }
    | EncryptionOutcome::Migrated { drive_id, .. }
    | EncryptionOutcome::Recompressed { drive_id, .. }
    | EncryptionOutcome::Rotated { drive_id, .. }
    | EncryptionOutcome::Reencrypted { drive_id, .. }
    | EncryptionOutcome::Maintained { drive_id, .. } = &outcome
    {
        let drive_id = drive_id.clone();
        crate::usage_view::drive_written(info, app, s, &drive_id);
    }
    match outcome {
        EncryptionOutcome::SetUp {
            drive_id,
            new_drive,
            result,
        } => match {
            new_code_asked().retain(|d| *d != drive_id);
            result
        } {
            Ok(code) => {
                // A drive that held files before its encryption: they move in once the sheet is
                // done - also when a restart makes a new code for the sheet, which is built as a
                // new drive's otherwise (AZ-K9).
                let moves = !new_drive
                    || state_of(&s.settings.recovery.drives, &drive_id)
                        .is_some_and(|state| state.moves_files);
                crate::recovery::code_made(s, &drive_id, &code);
                if moves {
                    state_mut(&mut s.settings.recovery.drives, &drive_id).files_will_move();
                }
                crate::save_settings(info, app, s);
                // this computer made the drive key: its own key is registered for a restore
                crate::archive::register_device_key(info, app, s, &drive_id);
                // registered while the sheet shows: its kit is offered once the server has it
                begin_registration(info, app, s, &drive_id, &code, None);
                let sheet = Sheet::new(&drive_id, code);
                let sheet = if moves { sheet } else { sheet.for_new_drive() };
                // In place of the busy setup (its window), else once the dialog showing closed
                // (a paid drive that arrived while the Add drive dialog was open).
                s.popup.open_when_free(Popup::Encryption(Dialog::Sheet(sheet)));
            }
            Err(why) => {
                if matches!(s.popup.as_ref(), Some(Popup::Encryption(Dialog::Busy { .. }))) {
                    s.popup.close();
                }
                if why == ENCRYPTED_ELSEWHERE {
                    println!("AZDRIVE_ENCRYPTED_ELSEWHERE {drive_id}");
                    let name = drive_name(s, &drive_id);
                    s.info(Phrase::new(ENCRYPTED_ELSEWHERE).arg("name", name));
                } else {
                    let shows_as_dialog = set_up_failure_shows_as_dialog(&why, s.popup.as_ref());
                    let why = t_label(&why);
                    let text = if new_drive {
                        t_args("azdrive-enc-new-drive-not-encrypted", &[("why", Arg::from(why))])
                    } else {
                        why
                    };
                    if shows_as_dialog {
                        s.popup.open(Popup::Encryption(Dialog::Message {
                            title: t("azdrive-enc-not-encrypted"),
                            text,
                        }));
                    } else {
                        s.warn(text);
                    }
                }
            }
        },
        EncryptionOutcome::Recovered { drive_id, result } => match result {
            Ok(()) => {
                if matches!(s.popup.as_ref(), Some(Popup::Encryption(Dialog::Busy { .. }))) {
                    s.popup.close();
                }
                // The drive was unlocked to be opened (its menu offers it wherever the window is):
                // it shows now, read again when it showed already.
                if s.current_drive_id().as_deref() == Some(drive_id.as_str()) {
                    crate::refresh(info, app, s);
                } else {
                    crate::go(info, app, s, browse::Place::folder(&drive_id, ""), true);
                }
                let name = drive_name(s, &drive_id);
                s.info(Phrase::new("azdrive-enc-unlocked").arg("name", name));
                // this computer holds the drive key again: its own key is registered for a
                // restore (once)
                crate::archive::register_device_key(info, app, s, &drive_id);
            }
            Err(why) => {
                s.popup.open(Popup::Encryption(Dialog::Message {
                    title: t("azdrive-enc-not-unlocked"),
                    text: t_label(&why),
                }));
            }
        },
        EncryptionOutcome::JoinKeyAdopted {
            drive_id,
            result,
            invite_left,
        } => {
            let joined = result.is_ok();
            // The unlock's own ending: the drive shows, its key is registered for a restore.
            on_outcome(
                info,
                app,
                s,
                EncryptionOutcome::Recovered {
                    drive_id: drive_id.clone(),
                    result,
                },
            );
            if let (true, Some(why)) = (joined, invite_left) {
                eprintln!("AZDRIVE_INVITE_LEFT {drive_id}: {why}");
                let name = drive_name(s, &drive_id);
                s.popup.open_when_free(Popup::Encryption(Dialog::Message {
                    title: t("azdrive-join-invite-left-title"),
                    text: t_args(
                        "azdrive-join-invite-left",
                        &[("name", Arg::from(name)), ("why", Arg::from(why))],
                    ),
                }));
            }
        }
        EncryptionOutcome::Migrated { drive_id, result } => {
            migrating().retain(|other| *other != drive_id);
            if matches!(s.popup.as_ref(), Some(Popup::Encryption(Dialog::Busy { .. }))) {
                s.popup.close();
            }
            let name = drive_name(s, &drive_id);
            match result {
                Ok(state) => {
                    if state.done {
                        // Nothing is left to move: the mark that resumes the move goes.
                        state_mut(&mut s.settings.recovery.drives, &drive_id).files_moved();
                        crate::save_settings(info, app, s);
                    }
                    let mut said = Text::from(
                        Phrase::new("azdrive-enc-migrated")
                            .arg("count", state.moved)
                            .arg("name", name.as_str())
                            .arg("skipped", state.skipped.len()),
                    );
                    if !state.done {
                        said = said.then(" ").then(Text::key("azdrive-enc-migrated-changed"));
                    }
                    s.info(said);
                    crate::refresh(info, app, s);
                }
                Err(why) => s.error(
                    Phrase::new("azdrive-enc-migrate-stopped")
                        .arg("name", name.as_str())
                        .arg("why", t_label(&why)),
                ),
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
                // The rotation removed every other computer and invite: what the recovery health
                // counted of them is gone (X-DELTA-AZDRIVE-KEYS-27).
                state_mut(&mut s.settings.recovery.drives, &drive_id).keys_changed();
                // The files keep their old file keys until the pass has been over all of them:
                // the question comes back until it is done (AZ-K18).
                state_mut(&mut s.settings.recovery.drives, &drive_id).reencrypt_wanted(now_unix());
                crate::save_settings(info, app, s);
                begin_registration(
                    info,
                    app,
                    s,
                    &drive_id,
                    &done.code,
                    done.previous_code.as_deref().map(|c| c.as_str()),
                );
                let name = drive_name(s, &drive_id);
                let mut said = Text::from(
                    Phrase::new("azdrive-enc-rotated")
                        .arg("name", name.as_str())
                        .arg("members", done.members_removed)
                        .arg("codes", done.codes_removed)
                        .arg("links", done.shares_revoked),
                );
                if done.drop_key.is_some() {
                    said = said.then(" ").then(Text::key("azdrive-enc-rotated-mail"));
                }
                s.info(said);
                if !done.left_over.is_empty() {
                    s.warn(
                        Phrase::new("azdrive-enc-rotated-left-over")
                            .arg("what", done.left_over.join("; ")),
                    );
                }
                s.popup.open(Popup::Encryption(Dialog::Sheet(
                    Sheet::new(&drive_id, done.code)
                        .after_rotation()
                        .replacing(done.previous_code),
                )));
                crate::refresh(info, app, s);
            }
            Err(why) => {
                s.popup.open(Popup::Encryption(Dialog::Message {
                    title: t("azdrive-enc-not-rotated"),
                    text: t_args(
                        "azdrive-enc-not-rotated-why",
                        &[("why", Arg::from(t_label(&why)))],
                    ),
                }));
            }
        },
        // Quiet: the usage line goes without its extra information until the next look.
        EncryptionOutcome::Totals { azlin_id, result } => match result {
            Ok((original, stored)) => crate::usage_view::totals_seen(
                s,
                &azlin_id,
                original,
                stored,
                azul_storage::time::now_unix(),
            ),
            Err(why) => eprintln!("AZDRIVE_TOTALS_FAILED {azlin_id}: {why}"),
        },
        EncryptionOutcome::Reencrypted { drive_id, result } => {
            let name = drive_name(s, &drive_id);
            match result {
                Ok(state) => {
                    // Every file was written anew: nothing waits, nothing is asked again.
                    state_mut(&mut s.settings.recovery.drives, &drive_id).reencrypted();
                    crate::save_settings(info, app, s);
                    let mut said = Text::from(
                        Phrase::new("azdrive-enc-reencrypted")
                            .arg("count", state.done)
                            .arg("name", name.as_str()),
                    );
                    if state.failed > 0 {
                        said = said.then(" ").then(
                            Phrase::new("azdrive-enc-reencrypt-damaged").arg("count", state.failed),
                        );
                    }
                    s.info(said);
                }
                Err(why) => s.error(
                    Phrase::new("azdrive-enc-reencrypt-stopped")
                        .arg("name", name.as_str())
                        .arg("why", t_label(&why)),
                ),
            }
        }
        EncryptionOutcome::Forgotten {
            drive_id,
            name,
            report,
            grant,
        } => {
            if matches!(
                s.popup.as_ref(),
                Some(Popup::Encryption(Dialog::Busy { .. }))
            ) {
                s.popup.close();
            }
            // The keyring entry with the refresh token is gone whatever else failed: a sign-in
            // that stays valid is said now, not at a retry that no longer holds the token.
            if let Some(grant) = &grant {
                tell_grant(s, grant);
            }
            if report.complete() {
                println!("AZDRIVE_FORGOTTEN {drive_id}");
                crate::actions::drive_forgotten(
                    info,
                    app,
                    s,
                    &drive_id,
                    &name,
                    report.keys_kept(),
                );
            } else {
                // The drive stays on the list, its entries findable by its id: removing it again
                // does the rest (what is gone is gone; every store is tried again).
                let why = report
                    .failures()
                    .iter()
                    .map(|(store, e)| format!("{store}: {e}"))
                    .collect::<Vec<_>>()
                    .join("; ");
                eprintln!("AZDRIVE_FORGET_FAILED {drive_id}: {why}");
                s.error(
                    Phrase::new("azdrive-forget-failed")
                        .arg("name", name.as_str())
                        .arg("why", why),
                );
            }
        }
        EncryptionOutcome::Invited { drive_id, result } => match result {
            Ok(code) => {
                println!("AZDRIVE_INVITED {drive_id}");
                s.popup.open(Popup::Encryption(Dialog::Invite {
                    drive_id,
                    code,
                    note: String::new(),
                }));
            }
            Err(why) => {
                s.popup.open(Popup::Encryption(Dialog::Message {
                    title: t("azdrive-invite-title"),
                    text: t_args("azdrive-invite-not-made", &[("why", Arg::from(t_label(&why)))]),
                }));
            }
        },
        EncryptionOutcome::RecoveryKeyRegistered {
            drive_id,
            public_key,
            answer,
            why,
        } => {
            registered(&drive_id, &public_key);
            state_mut(&mut s.settings.recovery.drives, &drive_id)
                .registration_answered(&public_key, answer.clone());
            crate::save_settings(info, app, s);
            let name = drive_name(s, &drive_id);
            let why = t_label(why.as_deref().unwrap_or_default());
            match answer {
                RegistrationAnswer::Done { findable } => {
                    println!("AZDRIVE_RECOVERY_KEY {drive_id}");
                    if findable.is_some() {
                        println!("AZDRIVE_RECOVERY_FINDABLE {drive_id}");
                    }
                }
                RegistrationAnswer::KeySetOnly { .. } => {
                    println!("AZDRIVE_RECOVERY_KEY {drive_id}");
                    s.warn(
                        Phrase::new("azdrive-keys-not-findable-yet")
                            .arg("name", name.as_str())
                            .arg("why", why),
                    );
                }
                RegistrationAnswer::NotSet { last } => {
                    println!("AZDRIVE_RECOVERY_KEY_NOT_REGISTERED {drive_id}");
                    let key = if last {
                        "azdrive-enc-key-refused"
                    } else {
                        "azdrive-enc-key-not-registered"
                    };
                    s.warn(Phrase::new(key).arg("name", name.as_str()).arg("why", why));
                }
            }
        }
        EncryptionOutcome::LockdownCancelled { drive_id, result } => match result {
            Ok(()) => {
                s.popup.close();
                crate::periods::lockdown_cancelled(s, &drive_id);
            }
            Err(why) => {
                s.popup.open(Popup::Encryption(Dialog::CancelLockdown {
                    drive_id,
                    typed: Zeroizing::new(String::new()),
                    error: t_label(&why),
                }));
            }
        },
        EncryptionOutcome::LockedDown { drive_id, result } => {
            let name = drive_name(s, &drive_id);
            s.popup.open(Popup::Encryption(match result {
                Ok(until) => {
                    let until =
                        until.map_or_else(|| String::from("none"), azul_storage::time::iso8601);
                    println!("AZDRIVE_RECOVERY_LOCKDOWN {drive_id}");
                    Dialog::Message {
                        title: t_args("azdrive-enc-locked-title", &[("name", Arg::from(name))]),
                        text: t_args("azdrive-enc-locked", &[("until", Arg::from(until))]),
                    }
                }
                Err(why) => Dialog::Message {
                    title: t("azdrive-enc-not-locked"),
                    text: t_label(&why),
                },
            }));
        }
        // Quiet, like the recompression pass: a round that stopped runs again six hours on.
        EncryptionOutcome::Maintained { drive_id, result } => {
            MAINTAINING.store(false, Ordering::SeqCst);
            match result {
                Ok(Some((_, packs))) => println!("{}", maintained_marker(&drive_id, &packs)),
                Ok(None) => {}
                Err(why) => eprintln!("AZDRIVE_MAINTAIN_STOPPED {drive_id}: {why}"),
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
    s.popup.open(Popup::Encryption(Dialog::Busy {
        title: t("azdrive-enc-checking-title"),
        text: t("azdrive-enc-checking"),
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

/// "Cancel lockdown" (the bar over a drive with a pending recovery-key lockdown): the dialog
/// that asks for the recovery code.
pub(crate) fn ask_cancel_lockdown(s: &mut DriveState, drive_id: &str) {
    if s.popup.is_none() {
        s.popups_opened += 1;
        s.popup.open(Popup::Encryption(Dialog::CancelLockdown {
            drive_id: drive_id.to_string(),
            typed: Zeroizing::new(String::new()),
            error: String::new(),
        }));
    }
}

extern "C" fn on_cancel_lockdown_signed(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::Encryption(Dialog::CancelLockdown {
            drive_id, typed, ..
        })) = s.popup.as_ref()
        else {
            return;
        };
        let drive_id = drive_id.clone();
        let known = crate::recovery_health::state_of(&s.settings.recovery.drives, &drive_id)
            .and_then(|state| state.recovery_key.clone());
        let checked = current_code(known.as_deref(), &drive_id, typed).and_then(|code| {
            token_url_of(s, &drive_id)
                .map(|url| (code, url))
                .ok_or_else(|| t("azdrive-enc-no-token-server-here"))
        });
        let (code, token_url) = match checked {
            Ok(found) => found,
            Err(why) => {
                if let Some(Popup::Encryption(Dialog::CancelLockdown { error, .. })) =
                    s.popup.as_mut()
                {
                    *error = why;
                }
                return;
            }
        };
        s.popup.open(Popup::Encryption(Dialog::Busy {
            title: t("azdrive-enc-cancelling-title"),
            text: t("azdrive-enc-cancelling"),
        }));
        let job = EncryptionJob::CancelLockdown {
            drive_id,
            code,
            token_url,
        };
        spawn(info, app, s, Job::Encryption(job));
    })
}

/// "Get my drive back" on a computer without the locked drive's device key: the recovery code
/// is asked for.
pub(crate) fn ask_get_back(s: &mut DriveState, drive_id: &str, checkout: Option<String>) {
    if s.popup.is_none() {
        s.popups_opened += 1;
        s.popup.open(Popup::Encryption(Dialog::GetDriveBack {
            drive_id: drive_id.to_string(),
            checkout,
            typed: Zeroizing::new(String::new()),
            error: String::new(),
        }));
    }
}

/// The recovery code typed for a locked drive: checked as the drive's current code where this
/// computer knows it, then its key signs the token server's challenge ([`crate::archive`]).
extern "C" fn on_get_back_signed(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let Some(Popup::Encryption(Dialog::GetDriveBack {
            drive_id,
            checkout,
            typed,
            ..
        })) = s.popup.as_ref()
        else {
            return;
        };
        let drive_id = drive_id.clone();
        let checkout = checkout.clone();
        let known = crate::recovery_health::state_of(&s.settings.recovery.drives, &drive_id)
            .and_then(|state| state.recovery_key.clone());
        match current_code(known.as_deref(), &drive_id, typed) {
            Ok(code) => {
                s.popup.close();
                crate::archive::prove_start(
                    info,
                    app,
                    s,
                    &drive_id,
                    crate::archive::ProveWith::RecoveryCode(code),
                    checkout,
                );
            }
            Err(why) => {
                if let Some(Popup::Encryption(Dialog::GetDriveBack { error, .. })) =
                    s.popup.as_mut()
                {
                    *error = why;
                }
            }
        }
    })
}

/// "Lock down with the recovery code...": the dialog.
pub(crate) fn ask_recovery_lockdown(s: &mut DriveState, drive_id: &str) {
    if s.popup.is_none() {
        s.popups_opened += 1;
        s.popup.open(Popup::Encryption(Dialog::RecoveryLockdown {
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
                *error = t("azdrive-enc-lockdown-bad-code");
            }
            return;
        };
        s.popup.open(Popup::Encryption(Dialog::Busy {
            title: t("azdrive-enc-locking-title"),
            text: t("azdrive-enc-locking"),
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
    use azul::sensor::NetworkKind;
    use azul_storage::{crypto::DriveKey, footprint::KeyEntry, trust::EncState};

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

    /// F12: a lockdown is cancelled, and a new code registered, only with the drive's CURRENT
    /// code - one that is no code, or another drive's or an old one, is refused here before
    /// anything is signed; a code this computer has no public key of goes to the server.
    #[test]
    fn only_the_drives_current_recovery_code_cancels_a_lockdown_or_signs_a_new_one() {
        crate::l10n::in_english();
        let code = RecoveryCode::from_bytes([0x5A; 16]);
        let old = RecoveryCode::from_bytes([0x11; 16]);
        let known = recovery_key_of(&code, "d_1").public_base64();
        let typed = code.to_text();
        let ok = current_code(Some(&known), "d_1", &typed).expect("the drive's code");
        assert_eq!(ok.as_bytes(), code.as_bytes());
        let lower = typed.to_lowercase();
        assert!(current_code(Some(&known), "d_1", &lower).is_ok(), "as people type it");
        let refused = current_code(Some(&known), "d_1", &old.to_text()).unwrap_err();
        assert!(refused.contains("not this drive's current recovery code"), "{refused}");
        assert!(
            current_code(Some(&known), "d_2", &typed).is_err(),
            "the same code is another drive's key"
        );
        let refused = current_code(Some(&known), "d_1", "hello").unwrap_err();
        assert!(refused.contains("26 letters and digits"), "{refused}");
        assert!(
            current_code(None, "d_1", &old.to_text()).is_ok(),
            "no public key here: the token server checks the signature"
        );
    }

    /// "We always encrypt": a new Azlin drive gets its keys and its recovery sheet as it is
    /// made; a folder of this computer or another bucket does not.
    #[test]
    fn a_new_azlin_drive_is_encrypted_as_it_is_made() {
        use azul_storage::config::{DriveAuth, DriveEntry, DriveLocation};
        let azlin = DriveEntry {
            id: String::from("d_new"),
            name: String::from("Photos"),
            location: DriveLocation::S3 {
                endpoint: String::from("https://s3.example.test"),
                region: String::from("us-east-1"),
                bucket: String::from("d-new"),
                path_style: true,
                auth: DriveAuth::Azlin {
                    drive_id: String::from("d_new"),
                    account_url: String::new(),
                    encrypted: false,
                    key_id: None,
                },
            },
        };
        assert!(sets_up_encryption_at_creation(&azlin));
        let own_bucket = DriveEntry {
            location: DriveLocation::S3 {
                endpoint: String::from("https://s3.example.test"),
                region: String::from("us-east-1"),
                bucket: String::from("mine"),
                path_style: true,
                auth: DriveAuth::Keyring,
            },
            ..azlin.clone()
        };
        assert!(!sets_up_encryption_at_creation(&own_bucket));
        let code = RecoveryCode::from_bytes([0x5A; 16]);
        assert!(Sheet::new("d_new", code.to_text()).for_new_drive().new_drive);
        assert!(!Sheet::new("d_new", code.to_text()).new_drive);
    }

    /// Item 7: the recovery code is part of the purchase - shown as text to copy, the emergency
    /// kit, the four groups; a copy leaves the clipboard again; a drive whose sheet never passed
    /// is not used until a new code's sheet does.
    #[test]
    fn a_new_drives_sheet_is_the_purchases_and_the_drive_waits_for_it() {
        crate::l10n::in_english();
        let code = RecoveryCode::from_bytes([0x5A; 16]);
        let sheet = Sheet::new("d_new", code.to_text()).for_new_drive();
        assert_eq!(sheet.title(), "Your new drive's recovery code");
        assert_eq!(Sheet::new("d_new", code.to_text()).title(), "Your recovery code");
        assert_eq!(sheet.copied_note(), "");
        let copied = Sheet {
            copied: true,
            ..Sheet::new("d_new", code.to_text())
        };
        assert!(copied.copied_note().contains("cleared in a minute"), "{}", copied.copied_note());
        let mut state = crate::recovery_health::RecoveryState::new("d_new");
        assert!(!setup_unfinished(None), "a drive without encryption");
        assert!(!setup_unfinished(Some(&state)), "no code made yet");
        state.code_made(100, None);
        assert!(setup_unfinished(Some(&state)), "a code made, never typed back");
        state.setup_verified(200);
        assert!(!setup_unfinished(Some(&state)));
    }

    #[test]
    fn a_new_drives_sheet_waits_for_another_open_dialog() {
        assert!(sheet_shows_now(None));
        assert!(sheet_shows_now(Some(&Popup::Encryption(Dialog::Busy {
            title: String::new(),
            text: String::new(),
        }))));
        assert!(!sheet_shows_now(Some(&Popup::Encryption(Dialog::Message {
            title: String::new(),
            text: String::new(),
        }))));
    }

    /// A drive picked up on a second computer was encrypted by the computer it was bought on:
    /// it is not set up again, that is said on the status line, and no failed set-up takes the
    /// place of an open dialog (the Add drive dialog the claim code was typed into).
    #[test]
    fn a_set_up_that_cannot_happen_never_replaces_an_open_dialog() {
        let add = Popup::AddDrive(crate::add_drive::AddDialog::new(1));
        let busy = Popup::Encryption(Dialog::Busy {
            title: String::new(),
            text: String::new(),
        });
        let failed = "azdrive-enc-err-no-index";
        assert!(!set_up_failure_shows_as_dialog(failed, Some(&add)), "the dialog stays");
        assert!(set_up_failure_shows_as_dialog(failed, None));
        assert!(set_up_failure_shows_as_dialog(failed, Some(&busy)), "the busy note gives way");
        assert!(!set_up_failure_shows_as_dialog(ENCRYPTED_ELSEWHERE, None));
        assert!(!set_up_failure_shows_as_dialog(ENCRYPTED_ELSEWHERE, Some(&busy)));
        crate::l10n::in_english();
        let said = t_args(ENCRYPTED_ELSEWHERE, &[("name", Arg::from("Photos"))]);
        assert!(said.starts_with("Photos was encrypted on"), "{said}");
        assert!(said.contains("recovery code"), "{said}");
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
                    hotspot: false,
                },
                100,
            )
        };
        assert!(on(NetworkKind::Wired, false, false));
        assert!(!on(NetworkKind::Cellular, true, false), "a phone's hotspot");
        assert!(!on(NetworkKind::WiFi, false, true), "Low Data Mode");
        let busy = PowerState {
            on_mains: true,
            idle_secs: 0,
        };
        assert!(!recompress_allowed(busy, NetworkState::headless(), 100), "used a moment ago");
    }

    #[test]
    fn the_recompression_pass_waits_for_a_healthy_computer() {
        let idle = PowerState {
            on_mains: true,
            idle_secs: RECOMPRESS_IDLE_SECS,
        };
        assert!(recompress_allowed(idle, NetworkState::headless(), 100));
        assert!(recompress_allowed(idle, NetworkState::headless(), 85));
        assert!(
            !recompress_allowed(idle, NetworkState::headless(), 80),
            "on mains in Low Power Mode"
        );
        assert!(
            !recompress_allowed(idle, NetworkState::headless(), 70),
            "a hot computer"
        );
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

    /// Every text `dom` shows, in document order.
    fn texts_of(dom: &Dom) -> Vec<String> {
        let mut out: Vec<String> = dom
            .root
            .node_type
            .get_text()
            .into_option()
            .map(|t| t.as_str().to_string())
            .filter(|t| !t.is_empty())
            .into_iter()
            .collect();
        for child in dom.children.as_slice() {
            out.extend(texts_of(child));
        }
        out
    }

    /// AZ-K12: the sheet showed the code and asked for four of its groups on the same page, so the
    /// check was satisfied by copying and pasting from the screen - it proved that the boxes work,
    /// not that a copy of the code exists somewhere else. Now the check is a step of its own: the
    /// page that asks for the groups does not show the code, going back shows it again and wipes
    /// what was typed, and only the groups asked for, typed on that page, confirm.
    #[test]
    fn az_k12_the_page_that_asks_for_the_groups_does_not_show_the_code() {
        crate::l10n::in_english();
        let code = RecoveryCode::from_bytes([0x5A; 16]);
        let mut sheet = Sheet::new("d_1", code.to_text());
        assert!(!sheet.checking, "the first step shows the code");

        sheet.hide_code();
        assert!(sheet.checking);
        let texts = texts_of(&check_page(&sheet, &RefAny::new(0_u32)));
        assert!(!texts.is_empty(), "the page asks something");
        for group in sheet.code.split('-') {
            assert!(
                texts.iter().all(|text| !text.contains(group)),
                "group {group} of the code is on the page that checks it: {texts:?}"
            );
        }
        assert!(
            texts
                .iter()
                .any(|text| text.contains("type groups") && text.contains("from your copy")),
            "it asks for the groups from the user's copy: {texts:?}"
        );

        // What is typed there confirms; going back wipes it and shows the code again.
        type_groups(&mut sheet, str::to_lowercase);
        assert!(sheet.confirmed());
        sheet.show_code();
        assert!(!sheet.checking);
        assert!(
            !sheet.confirmed(),
            "back at the code, nothing is typed any more"
        );
        sheet.hide_code();
        assert!(!sheet.confirmed(), "and again hidden, the boxes are empty");
        sheet.error = String::from("not all");
        sheet.hide_code();
        assert!(sheet.error.is_empty(), "a step clears the last complaint");
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
    fn the_recovery_sheets_window_has_no_close_box_and_escape_leaves_it_open() {
        // Escape is the dialog window's own (azul's Dialog closes its window before the app
        // hears of it): keeping the popup in close_popup is too late, only closedby="none"
        // keeps the window up.
        use azul::widgets::DialogClosedBy;
        let code = RecoveryCode::from_bytes([0x5A; 16]);
        let sheet = crate::Popup::Encryption(Dialog::Sheet(Sheet::new("d_1", code.to_text())));
        assert_eq!(crate::ui_dialogs::window_close(&sheet), (DialogClosedBy::None, false));
        let message = crate::Popup::Encryption(Dialog::Message {
            title: String::new(),
            text: String::new(),
        });
        assert_eq!(crate::ui_dialogs::window_close(&message), (DialogClosedBy::Auto, true));
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

    /// AZ-M4. The provider an Azlin drive's slot gets asks the drive's restore announcement (the
    /// token server's word, as the drive's last refresh answered it) when its index opens: the
    /// wiring that lets the index copy take an operator's restore once instead of refusing the
    /// restored drive as a rollback for good.
    #[test]
    fn az_m4_the_provider_of_a_slot_asks_the_restore_announcement_of_its_drive() {
        use azul_storage::{crypto::DriveKey, meta::MemoryBucket};
        use std::sync::atomic::{AtomicUsize, Ordering};

        let asked = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&asked);
        let provider = restoring_provider(
            None,
            RestoreSource::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                None
            }),
        );
        let bucket: Arc<dyn Drive> = Arc::new(MemoryBucket::new());
        let key = DriveKey::generate().unwrap();
        provider.open_index("d_restore", bucket, &key).unwrap();
        assert!(
            asked.load(Ordering::SeqCst) > 0,
            "the index asked where the drive's restore stands"
        );
    }

    /// X-DELTA-AZDRIVE-SYNC-3. A job that opens the index of the slot's drive on its own
    /// (migrate, recompress, rotate, re-encrypt, totals) took `index_provider()` - a provider made
    /// without the drive's restore source, so a drive the operator restored was refused to the job
    /// as a rollback for good while the slot took the announcement. It takes the slot's provider
    /// (`AutoEncrypted::provider`) and so asks the same source. RED not run: the old job built its
    /// provider with no source and asked nothing.
    #[test]
    fn x_delta_azdrive_sync_3_a_job_opens_the_drives_index_with_the_provider_of_its_slot() {
        use azul_storage::{crypto::DriveKey, keyring::MemoryKeyring, meta::MemoryBucket};
        use std::sync::atomic::{AtomicUsize, Ordering};

        let asked = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&asked);
        let provider = restoring_provider(
            None,
            RestoreSource::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
                None
            }),
        );
        let bucket = Arc::new(MemoryBucket::new());
        let auto = wrap_with(
            Arc::new(MemoryKeyring::new()),
            Some(provider),
            "d_job",
            bucket.clone(),
            EncryptionRecord::default(),
            "slot-1",
            &crate::KeyRecords::default(),
        );
        let job = auto
            .provider()
            .expect("the provider the slot was made with");
        let key = DriveKey::generate().unwrap();
        job.open_index("d_job", bucket as Arc<dyn Drive>, &key)
            .unwrap();
        assert!(
            asked.load(Ordering::SeqCst) > 0,
            "the job's index asked where the drive's restore stands"
        );
        let none = wrap_with(
            Arc::new(MemoryKeyring::new()),
            None,
            "d_none",
            Arc::new(MemoryBucket::new()),
            EncryptionRecord::default(),
            "slot-2",
            &crate::KeyRecords::default(),
        );
        assert!(
            none.provider().is_none(),
            "a drive made without an index has none"
        );
    }

    /// AZ-M2. The reclaimer's round after a maintenance round is told in the log - in shadow mode
    /// what it would delete - or why it could not read the drive; no round, no line.
    #[test]
    fn az_m2_the_reclaimers_round_is_told_in_the_log_as_what_it_would_delete() {
        use azul_storage::meta::{ReclaimMode, ReclaimReport};

        assert_eq!(reclaim_marker("d1", None), None);
        let line =
            reclaim_marker("d1", Some(Ok(ReclaimReport::none(ReclaimMode::Shadow)))).unwrap();
        assert!(
            line.starts_with("AZDRIVE_RECLAIM d1 reclaim (shadow): would delete 0 objects"),
            "{line}"
        );
        let stopped = reclaim_marker("d1", Some(Err(String::from("no answer")))).unwrap();
        assert_eq!(stopped, "AZDRIVE_RECLAIM_STOPPED d1: no answer");
    }

    /// X-DELTA-AZDRIVE-SYNC-5 / -6. The owner reads the log to decide whether the reclaimer may be
    /// switched on: a drive over the budget says "too big to judge" and how big it is; a run that
    /// was configured to delete says what held it back; a round carries the digest of its
    /// candidates, whether the head listed again agrees, and the clean rounds so far.
    #[test]
    fn x_delta_azdrive_sync_5_the_log_line_says_too_big_to_judge_and_what_holds_the_switch_back() {
        use azul_storage::meta::{GoEvidence, Hold, NotYet, ReclaimMode, ReclaimReport};

        let mut big = ReclaimReport::too_big(ReclaimMode::Shadow, 700 << 20, 256 << 20);
        let line = reclaim_marker("d1", Some(Ok(big.clone()))).unwrap();
        assert!(
            line.starts_with("AZDRIVE_RECLAIM d1 reclaim (shadow)"),
            "{line}"
        );
        assert!(
            line.contains("700 MiB")
                && line.contains("256 MiB")
                && line.contains("too big to judge"),
            "{line}"
        );
        assert_eq!(
            big.stopped,
            Some(NotYet::TooBig {
                bytes: 700 << 20,
                budget: 256 << 20
            })
        );

        big.digest = String::from("0123456789abcdef0123456789abcdef");
        big.rederived = Some(true);
        big.evidence = Some(GoEvidence {
            clean_rounds: 3,
            days: 2,
            devices: 1,
            drive_devices: 2,
            ..GoEvidence::default()
        });
        big.held = Some(Hold::Rounds { have: 3, need: 7 });
        let line = reclaim_marker("d1", Some(Ok(big))).unwrap();
        for part in [
            "digest 0123456789abcdef",
            "agrees",
            "clean rounds 3 over 2 days by 1 of 2 devices",
            "held back: 3 clean rounds in a row, 7 wanted",
        ] {
            assert!(line.contains(part), "{part}: {line}");
        }
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
        let (done, _) =
            run_maintenance(&meta_provider(None), &bucket, &keyring, "d_upkeep", &rules)
                .unwrap()
                .expect("no other computer holds the lease");
        assert!(done.compacted);
        let page = drive.list(&ListRequest::folder("docs/")).unwrap();
        assert_eq!(page.objects.len(), 4, "the folder's marker and three files");

        // A computer without the drive's key maintains nothing.
        let stranger = MemoryKeyring::new();
        assert!(run_maintenance(&meta_provider(None), &bucket, &stranger, "d_upkeep", &rules).is_err());
    }

    /// The end of a maintenance round says what its repack did, in the fields the E2E pack
    /// proof reads.
    #[test]
    fn a_maintenance_rounds_end_says_what_its_repack_did() {
        let packs = PackUpkeep {
            recovered: 9,
            repack: azul_storage::packing::RepackReport {
                packs_read: 3,
                packs_written: 1,
                files_moved: 40,
                packs_emptied: 2,
                ..Default::default()
            },
            collected: 7,
        };
        assert_eq!(
            maintained_marker("d_upkeep", &packs),
            "AZDRIVE_MAINTAINED d_upkeep packs_read=3 packs_written=1 packs_emptied=2 \
             files_moved=40"
        );
    }

    /// After a transfer, each of its drives opened encrypted says what its reads of packs cost
    /// so far: a small file read back is one ranged GET of about its size, its pack not
    /// fetched whole. A drive that never opened encrypted (a folder here) says nothing.
    #[test]
    fn a_transfer_says_what_its_drives_reads_of_packs_cost() {
        use azul_storage::{crypto::DriveKey, keyring::MemoryKeyring, meta::MemoryBucket};
        let drive = "d_pack_reads";
        let bucket: Arc<dyn Drive> = Arc::new(MemoryBucket::new());
        let keyring = MemoryKeyring::new();
        device::store_drive_key(&keyring, drive, &DriveKey::generate().unwrap()).unwrap();
        let encrypted =
            open_encrypted(Arc::clone(&bucket), &keyring, drive, &meta_provider(None)).unwrap();
        let note = b"a small note, packed with the others. ".repeat(10);
        for i in 0..3 {
            encrypted.put(&format!("notes/{i}.txt"), &note).unwrap();
        }
        assert_eq!(
            pack_reads_lines(drive, "home"),
            vec![format!("AZDRIVE_PACK_READS {drive} ranged=0 whole=0 bytes=0")],
            "opened, nothing read yet"
        );

        assert_eq!(encrypted.get("notes/1.txt").unwrap(), note);
        let reads = pack_reads(drive).unwrap();
        assert_eq!((reads.ranged, reads.whole), (1, 0), "one ranged GET, no whole pack");
        assert!(reads.bytes > 0 && reads.bytes <= note.len() as u64 + 4096, "{reads:?}");
        assert_eq!(
            pack_reads_lines("home", drive),
            vec![format!(
                "AZDRIVE_PACK_READS {drive} ranged=1 whole=0 bytes={}",
                reads.bytes
            )]
        );
        assert!(pack_reads_lines("home", "home").is_empty());
    }

    /// AzDrive's encrypted drives pack their small files: a sync of a folder of notes sends a
    /// few packs, not one object (and one index commit) per note.
    #[test]
    fn azdrives_encrypted_drives_pack_small_files() {
        let packing = meta_provider(None).packing("d_any").expect("small files are packed");
        assert_eq!(packing.threshold, azul_storage::SMALL_FILE_BYTES);
        assert_eq!(packing.journal, None, "no cache folder: the journal in memory");
        let dir = std::env::temp_dir().join("azdrive-pack-provider");
        let with_cache = meta_provider(Some(dir.clone())).packing("d_any").unwrap();
        let journal = with_cache.journal.expect("beside the drive's index copy");
        assert!(journal.starts_with(&dir) && journal.ends_with("packs"), "{journal:?}");
    }

    /// A keyring whose prompt the user declined.
    struct Declined;

    impl KeyringStore for Declined {
        fn get(&self, _key: &str) -> Result<Option<String>, azul_storage::keyring::KeyringError> {
            Err(azul_storage::keyring::KeyringError::Denied)
        }
        fn set(&self, _key: &str, _secret: &str) -> Result<(), azul_storage::keyring::KeyringError> {
            Err(azul_storage::keyring::KeyringError::Denied)
        }
        fn delete(&self, _key: &str) -> Result<(), azul_storage::keyring::KeyringError> {
            Err(azul_storage::keyring::KeyringError::Denied)
        }
    }

    /// X-AR5b-2: "Restore as of" of a drive the keyring would not open used to read "could not
    /// tell" as "plain" and ask the token server to roll EVERY object of the bucket back. The
    /// drive is decided first and its state names what happens: open it first (the keyring
    /// refused, the bucket is reachable), or encrypt it first - an error either way, never the
    /// bucket's restore.
    #[test]
    fn x_ar5b_2_a_drive_the_keyring_will_not_open_is_not_restored_by_its_bucket() {
        use azul_storage::{local::LocalDrive, testing::TempDir};
        let dir = TempDir::new("restore-declined");
        std::fs::write(dir.path().join("a.txt"), b"plain").unwrap();
        let bucket: Arc<dyn Drive> = Arc::new(LocalDrive::new(dir.path().to_path_buf()));

        let declined = AutoEncrypted::new(Arc::clone(&bucket), "d_1", Arc::new(Declined), None);
        let said = restore_encrypted(&declined, 0).unwrap_err();
        assert!(said.contains("keyring"), "the keyring's refusal is said: {said}");
        assert_eq!(declined.state(), EncState::Undecided);
        let versions = versioned_bucket(&declined).err().expect("refused");
        assert!(versions.contains("keyring"), "{versions}");

        // A bucket with a file and no key anywhere: not encrypted, so nothing to restore from.
        let plain = AutoEncrypted::new(
            Arc::clone(&bucket),
            "d_2",
            Arc::new(azul_storage::keyring::MemoryKeyring::new()),
            None,
        );
        assert_eq!(
            restore_encrypted(&plain, 0),
            Err(String::from("azdrive-restore-encrypt-first"))
        );
        assert!(versioned_bucket(&plain).is_err(), "a plain drive keeps no versions");
        assert!(dir.path().join("a.txt").exists(), "and nothing was touched");
    }

    /// X-AR5b-12: the saved state of a pass. None saved is a new pass; one saved comes back whole
    /// (written through `durable::replace`, this user's only); one that cannot be parsed is an
    /// error and not "nothing saved" - the pass would redo, and send again, what it had done -
    /// and it is set aside whole so the next run starts anew.
    #[test]
    fn x_ar5b_12_a_damaged_state_file_is_an_error_and_is_set_aside_never_a_silent_restart() {
        use azul_storage::testing::TempDir;
        let dir = TempDir::new("job-state");
        let folder = dir.path().join("encryption");
        let file = folder.join("d_1.migration.json");
        let parse = |text: &str| MigrationState::from_json(text).ok();

        assert!(load_state(&file, parse).unwrap().is_none(), "no file: a new pass");

        let saved = MigrationState {
            moved: 3,
            ..MigrationState::default()
        };
        save_state(&file, &saved.to_json()).unwrap();
        assert_eq!(load_state(&file, parse).unwrap(), Some(saved));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&file).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "this user's only: {mode:o}");
        }
        let names: Vec<String> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["d_1.migration.json"], "no temporary file is left");

        std::fs::write(&file, "{ \"moved\": ").unwrap();
        let why = load_state(&file, parse).unwrap_err();
        assert!(why.contains("damaged") && why.contains("not started over it"), "{why}");
        assert!(!file.exists(), "set aside");
        let aside = folder.join("d_1.migration.json.damaged");
        assert_eq!(std::fs::read_to_string(&aside).unwrap(), "{ \"moved\": ");
        assert!(load_state(&file, parse).unwrap().is_none(), "the next run starts anew");

        // Not text at all is damaged too.
        std::fs::write(&file, [0xff, 0xfe, 0x00]).unwrap();
        assert!(load_state(&file, parse).unwrap_err().contains("damaged"));
    }

    /// X-AR5b-12, through the job: a migration whose saved state is damaged stops with the error
    /// before it opens the drive - it does not move the files all over again.
    #[test]
    fn x_ar5b_12_a_migration_with_a_damaged_state_stops_before_it_touches_the_drive() {
        use azul_storage::{keyring::MemoryKeyring, meta::MemoryBucket, testing::TempDir};
        let dir = TempDir::new("job-state-run");
        let file = dir.path().join("encryption").join("d_1.migration.json");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "not json at all").unwrap();
        let auto = Arc::new(AutoEncrypted::new(
            Arc::new(MemoryBucket::new()),
            "d_1",
            Arc::new(MemoryKeyring::new()),
            None,
        ));
        let outcome = run(EncryptionJob::Migrate {
            drive_id: String::from("d_1"),
            auto,
            state_file: file.clone(),
        });
        match outcome {
            EncryptionOutcome::Migrated { result: Err(why), .. } => {
                assert!(why.contains("damaged"), "{why}")
            }
            _ => panic!("a damaged state is an error"),
        }
        assert!(!file.exists(), "set aside");
    }

    /// AZ-K9: a drive whose encryption was stopped - a kill during the sheet, or during the move -
    /// left plaintext files in the bucket and nothing started the move again. It is due when the
    /// settings' mark says the files still have to move, or the journal of the move is unfinished;
    /// a drive that was made encrypted (no mark, no journal) and one whose move ended (no mark, a
    /// finished journal) have nothing to move.
    #[test]
    fn az_k9_a_stopped_move_is_due_by_its_mark_or_its_unfinished_journal() {
        let unfinished = MigrationState::default();
        let finished = MigrationState {
            done: true,
            ..MigrationState::default()
        };
        assert!(!move_is_due(false, None), "a drive made encrypted");
        assert!(
            move_is_due(true, None),
            "stopped before the journal had a line"
        );
        assert!(
            move_is_due(false, Some(&unfinished)),
            "stopped during the move"
        );
        assert!(!move_is_due(false, Some(&finished)), "the move ended");
        assert!(
            move_is_due(true, Some(&finished)),
            "ended, but the mark is not cleared yet: the move runs once more and clears it"
        );
    }

    /// An index provider with an index in memory, the stand-in for the drive's repository.
    struct MemoryProvider;

    impl IndexProvider for MemoryProvider {
        fn open_index(
            &self,
            _drive: &str,
            _bucket: Arc<dyn Drive>,
            _drive_key: &azul_storage::crypto::DriveKey,
        ) -> Result<Arc<dyn azul_storage::encrypted::NameIndex>, azul_storage::DriveError> {
            Ok(Arc::new(azul_storage::encrypted::MemoryIndex::new()))
        }
    }

    /// A bucket that takes everything but the delete of a key file: it answers an error.
    struct KeepsKeyFiles(Arc<azul_storage::meta::MemoryBucket>);

    impl Drive for KeepsKeyFiles {
        fn list(
            &self,
            request: &azul_storage::ListRequest,
        ) -> Result<azul_storage::ListPage, azul_storage::DriveError> {
            self.0.list(request)
        }
        fn get(&self, key: &str) -> Result<Vec<u8>, azul_storage::DriveError> {
            self.0.get(key)
        }
        fn get_range(
            &self,
            key: &str,
            range: azul_storage::ByteRange,
        ) -> Result<Vec<u8>, azul_storage::DriveError> {
            self.0.get_range(key, range)
        }
        fn put(&self, key: &str, bytes: &[u8]) -> Result<(), azul_storage::DriveError> {
            self.0.put(key, bytes)
        }
        fn delete(&self, _key: &str) -> Result<(), azul_storage::DriveError> {
            Err(azul_storage::DriveError::Transport(String::from(
                "the connection was cut",
            )))
        }
        fn head(&self, key: &str) -> Result<azul_storage::ObjectInfo, azul_storage::DriveError> {
            self.0.head(key)
        }
        fn put_if(
            &self,
            key: &str,
            bytes: &[u8],
            condition: &azul_storage::Precondition,
        ) -> Result<Option<String>, azul_storage::DriveError> {
            self.0.put_if(key, bytes, condition)
        }
    }

    /// X-AR5b-9: AzDrive's join takes the drive key from the join code and keeps it, whether or
    /// not the code's one-time key file can be deleted; when it cannot, the answer is still a
    /// join, and the reason comes back beside it for the dialog that says the code is live. A
    /// damaged key id or seal is a refusal and nothing is kept.
    #[test]
    fn x_ar5b_9_a_join_whose_invite_cannot_be_deleted_is_a_join_with_the_word_beside_it() {
        use azul_storage::{
            crypto::device,
            keyring::{KeyringStore, MemoryKeyring},
            meta::MemoryBucket,
        };
        let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
        let bucket = Arc::new(MemoryBucket::new());
        let (key, _code) =
            device::setup_new_drive(bucket.as_ref(), &MemoryKeyring::new(), "d_az", kdf).unwrap();
        let invite = device::seal_invite(bucket.as_ref(), "d_az", &key).unwrap();
        let seal = device::invite_text(&invite).to_string();
        let key_id = key.id().to_hex();
        let provider: Option<Arc<dyn IndexProvider>> = Some(Arc::new(MemoryProvider));
        let drive_over = |bucket: Arc<dyn Drive>, keyring: &Arc<dyn KeyringStore>| {
            wrap_with(
                Arc::clone(keyring),
                provider.clone(),
                "d_az",
                bucket,
                EncryptionRecord::default(),
                "slot-1",
                &crate::KeyRecords::default(),
            )
        };

        // A damaged seal, or a key id that is no id: refused, nothing kept, no word.
        let keyring: Arc<dyn KeyringStore> = Arc::new(MemoryKeyring::new());
        let auto = drive_over(bucket.clone(), &keyring);
        assert_eq!(
            adopt_join_key_with(keyring.as_ref(), &auto, "not hex", Some(&key_id)),
            (Err(String::from("azdrive-invite-seal-damaged")), None)
        );
        assert_eq!(
            adopt_join_key_with(keyring.as_ref(), &auto, &seal, Some("not a key id")),
            (Err(String::from("azdrive-invite-seal-damaged")), None)
        );
        assert_eq!(
            device::load_drive_key(keyring.as_ref(), "d_az").unwrap(),
            None
        );

        // A bucket that cannot delete the key file: joined, the key kept, the reason beside it.
        let auto = drive_over(Arc::new(KeepsKeyFiles(bucket.clone())), &keyring);
        let (result, left) = adopt_join_key_with(keyring.as_ref(), &auto, &seal, Some(&key_id));
        assert_eq!(result, Ok(()));
        assert!(left.is_some_and(|why| why.contains("connection was cut")));
        assert_eq!(
            device::load_drive_key(keyring.as_ref(), "d_az").unwrap(),
            Some(key.clone())
        );

        // A bucket that deletes: joined, no word. (A second device; the first one's file stays.)
        let second: Arc<dyn KeyringStore> = Arc::new(MemoryKeyring::new());
        let auto = drive_over(bucket.clone(), &second);
        assert_eq!(
            adopt_join_key_with(second.as_ref(), &auto, &seal, Some(&key_id)),
            (Ok(()), None)
        );
        assert_eq!(
            device::load_drive_key(second.as_ref(), "d_az").unwrap(),
            Some(key)
        );
    }

    /// X-AR6-26802cb54-1: the glue around AutoEncrypted. A drive that opens encrypted puts its slot's
    /// id and its key's id in the queue the UI thread writes into the drives file (once per
    /// opening), a drive that does not queues nothing, and the drives file's record is what the
    /// drive is told - recorded as encrypted, it is locked without its key, never plain.
    #[test]
    fn x_ar6_26802cb54_1_the_key_id_of_an_encrypted_drive_is_queued_for_the_drives_file() {
        use azul_storage::{
            crypto::device,
            keyring::{KeyringStore, MemoryKeyring},
            meta::MemoryBucket,
        };
        let keyring: Arc<dyn KeyringStore> = Arc::new(MemoryKeyring::new());
        let bucket = Arc::new(MemoryBucket::new());
        let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
        let (key, _code) =
            device::setup_new_drive(bucket.as_ref(), keyring.as_ref(), "d_az", kdf).unwrap();
        let provider: Option<Arc<dyn IndexProvider>> = Some(Arc::new(MemoryProvider));

        let records = crate::KeyRecords::default();
        let auto = wrap_with(
            Arc::clone(&keyring),
            provider.clone(),
            "d_az",
            bucket.clone(),
            EncryptionRecord::default(),
            "slot-1",
            &records,
        );
        assert!(
            records.lock().unwrap().is_empty(),
            "nothing is queued before the first call"
        );
        assert!(matches!(auto.decide(), Ok(EncState::Encrypted { .. })));
        {
            let queued = records.lock().unwrap();
            assert_eq!(queued.len(), 1, "once per opening");
            assert_eq!(
                queued[0].drive_id, "slot-1",
                "the slot's id, not the Azlin drive's"
            );
            assert_eq!(queued[0].key_id, key.id().to_hex());
        }
        auto.reopen();
        auto.decide().unwrap();
        assert_eq!(
            records.lock().unwrap().len(),
            2,
            "told again when it opens again"
        );

        // A drive recorded as encrypted, on a device with no key: locked, nothing queued.
        let locked_records = crate::KeyRecords::default();
        let locked = wrap_with(
            Arc::new(MemoryKeyring::new()),
            provider.clone(),
            "d_az",
            bucket.clone(),
            EncryptionRecord {
                encrypted: true,
                key_id: Some(key.id().to_hex()),
            },
            "slot-1",
            &locked_records,
        );
        assert!(matches!(
            locked.decide(),
            Err(azul_storage::DriveError::Locked { .. })
        ));
        assert!(locked_records.lock().unwrap().is_empty());

        // A drive that is not encrypted queues nothing either.
        let plain_records = crate::KeyRecords::default();
        let plain = wrap_with(
            Arc::new(MemoryKeyring::new()),
            provider,
            "d_new",
            Arc::new(MemoryBucket::new()),
            EncryptionRecord::default(),
            "slot-2",
            &plain_records,
        );
        assert!(matches!(plain.decide(), Ok(EncState::Unencrypted(_))));
        assert!(plain_records.lock().unwrap().is_empty());
    }

    /// X-AR6-26802cb54-1: the UI thread's half of the same glue. What the worker threads queued
    /// reaches the slot and the drives file - for the drive it names and no other -; a drive that
    /// is gone from the slots, or recorded with that key already, changes and writes nothing; a
    /// new key (a rotation) is written over the old; and a drives file that refuses the write is
    /// named with its drive for the window to say, the slot having the record all the same (the
    /// drive is not opened as a plain one meanwhile).
    #[test]
    fn x_ar6_26802cb54_1_a_queued_key_id_reaches_the_slot_and_the_drives_file() {
        use azul_storage::{
            config::{DriveAuth, DriveEntry, DriveLocation, DrivesFile},
            testing::TempDir,
        };
        let azlin = |slot: &str| DriveEntry {
            id: slot.to_string(),
            name: format!("Drive {slot}"),
            location: DriveLocation::S3 {
                endpoint: String::from("https://s3.example.test"),
                region: String::from("us-east-1"),
                bucket: slot.to_string(),
                path_style: true,
                auth: DriveAuth::Azlin {
                    drive_id: format!("d_{slot}"),
                    account_url: String::new(),
                    encrypted: false,
                    key_id: None,
                },
            },
        };
        let recorded = |entry: &DriveEntry| entry.encryption_record().unwrap();
        let encrypted_with = |key: &str| EncryptionRecord {
            encrypted: true,
            key_id: Some(key.to_string()),
        };
        let dir = TempDir::new("key-records");
        let file = dir.path().join("drives.json");
        DrivesFile::update(&file, |drives| {
            drives.add(azlin("slot-1"));
            drives.add(azlin("slot-2"));
        })
        .unwrap();
        let mut slots = vec![
            crate::Slot::new(azlin("slot-1")),
            crate::Slot::new(azlin("slot-2")),
        ];
        let queue = crate::KeyRecords::default();
        let queued = |slot: &str, key: &str| {
            queue.lock().unwrap().push(crate::KeyRecord {
                drive_id: slot.to_string(),
                key_id: key.to_string(),
            });
        };
        let first = "11".repeat(16);
        let second = "22".repeat(16);

        // The drive it names gets it, in its slot and in the file; the other one does not; a
        // record for a drive that is not there (removed meanwhile) is dropped.
        queued("slot-1", &first);
        queued("slot-gone", &first);
        assert!(crate::write_key_records(&mut slots, &queue, Some(&file)).is_empty());
        assert!(queue.lock().unwrap().is_empty(), "the queue is emptied");
        let written = DrivesFile::load(&file).unwrap();
        assert_eq!(recorded(&slots[0].entry), encrypted_with(&first));
        assert_eq!(
            recorded(written.get("slot-1").unwrap()),
            encrypted_with(&first)
        );
        assert_eq!(recorded(&slots[1].entry), EncryptionRecord::default());
        assert_eq!(
            recorded(written.get("slot-2").unwrap()),
            EncryptionRecord::default()
        );
        assert!(written.get("slot-gone").is_none());

        // Told again with the same key (the drive opened again): nothing to write, so a file that
        // would refuse the write is not even asked.
        let refusing = file.join("not-a-folder").join("drives.json");
        queued("slot-1", &first);
        assert!(crate::write_key_records(&mut slots, &queue, Some(&refusing)).is_empty());

        // A new key (a rotation) is written over the old one.
        queued("slot-1", &second);
        assert!(crate::write_key_records(&mut slots, &queue, Some(&file)).is_empty());
        let written = DrivesFile::load(&file).unwrap();
        assert_eq!(recorded(&slots[0].entry), encrypted_with(&second));
        assert_eq!(
            recorded(written.get("slot-1").unwrap()),
            encrypted_with(&second)
        );

        // A file that refuses the write: named with the drive, the slot has the key all the same,
        // the real file was not touched.
        queued("slot-2", &first);
        let refused = crate::write_key_records(&mut slots, &queue, Some(&refusing));
        assert_eq!(refused.len(), 1, "{refused:?}");
        assert_eq!(refused[0].0, "Drive slot-2");
        assert!(!refused[0].1.is_empty());
        assert_eq!(recorded(&slots[1].entry), encrypted_with(&first));
        assert_eq!(
            recorded(DrivesFile::load(&file).unwrap().get("slot-2").unwrap()),
            EncryptionRecord::default()
        );

        // No drives file (a run without a configuration folder): the slots only.
        queued("slot-1", &first);
        assert!(crate::write_key_records(&mut slots, &queue, None).is_empty());
        assert_eq!(recorded(&slots[0].entry), encrypted_with(&first));
    }

    // ==== X-DELTA-AZDRIVE-KEYS-29: a pass of the keys before is not the next pass ====

    /// A pass that stopped under the keys before has a cut-off and a cursor of other files. After a
    /// second rotation the next pass took it up, skipped the files at or before the cursor, and
    /// ended; the weekly reminder to re-encrypt stopped with it.
    #[test]
    fn x_delta_azdrive_keys_29_a_pass_of_the_keys_before_is_not_taken_up_by_the_next_one() {
        let mut stopped = ReencryptState::new(1_000);
        stopped.cursor = Some(String::from("photos/m.jpg"));
        stopped.done = 40;
        // a second rotation ended at 2_000: the state is of the keys before
        assert!(current_pass(Some(stopped.clone()), 2_000).is_none());
        // the same state under the keys it was made for: taken up where it stopped
        assert_eq!(
            current_pass(Some(stopped.clone()), 1_000),
            Some(stopped.clone())
        );
        assert_eq!(
            current_pass(Some(stopped.clone()), 900),
            Some(stopped.clone())
        );
        // a pass of unknown age is taken as it is; no state is no state
        assert_eq!(current_pass(Some(stopped.clone()), 0), Some(stopped));
        assert!(current_pass(None, 5).is_none());
    }

    // ==== X-DELTA-AZDRIVE-KEYS-2, -8: the proof of a removal is taken back at removal time ====

    const KEYS_DRIVE: &str = "d_k2";

    fn removal_proof_of_a_checked_code() -> azul_storage::footprint::RemovalProof {
        azul_storage::footprint::RemovalProof::from_checked_code(KEYS_DRIVE)
    }

    fn no_places() -> crate::footprint::Places {
        crate::footprint::Places {
            drives_file: None,
            spots: Vec::new(),
            open_copies: azul_storage::scratch::Scratch::tagged_under(
                std::path::Path::new("/nonexistent/azdrive-scratch"),
                "open",
                KEYS_DRIVE,
            ),
        }
    }

    /// The keyring of a device whose rotation stopped after the index was sealed under K2: the
    /// drive key is K1, `drive-key-previous` K1, `drive-key-next` K2, and a member key.
    fn keyring_of_a_stopped_rotation() -> (azul_storage::keyring::MemoryKeyring, DriveKey, DriveKey)
    {
        let keyring = azul_storage::keyring::MemoryKeyring::new();
        let (k1, k2) = (DriveKey::generate().unwrap(), DriveKey::generate().unwrap());
        device::store_drive_key(&keyring, KEYS_DRIVE, &k1).unwrap();
        let (previous, next) = device::rotation_key_entries(KEYS_DRIVE);
        drop(device::stash(&keyring, &previous, &k1).unwrap());
        drop(device::stash(&keyring, &next, &k2).unwrap());
        device::member_secret(&keyring, KEYS_DRIVE).unwrap();
        (keyring, k1, k2)
    }

    /// X-DELTA-AZDRIVE-KEYS-2: the proof was a flag in the settings; after a rotation that
    /// stopped (the index sealed under K2, the keyring's drive key K1) "Remove drive" deleted
    /// DriveKey, RotationPrevious, RotationNext and the member key of a drive that still held its
    /// data, and the code on paper opened K1, which opens no index. The removal asks the keyring
    /// when it runs, and a key change that is unfinished keeps every key.
    #[test]
    fn x_delta_azdrive_keys_2_a_removal_after_a_stopped_rotation_keeps_every_key() {
        let (keyring, k1, k2) = keyring_of_a_stopped_rotation();
        let report = forget_job(
            &keyring,
            KEYS_DRIVE,
            &no_places(),
            true,
            Some(removal_proof_of_a_checked_code()),
            None,
        );
        assert!(report.complete());
        let kept: Vec<String> = report.kept().iter().map(|k| k.what.clone()).collect();
        for entry in [
            KeyEntry::DriveKey,
            KeyEntry::RotationPrevious,
            KeyEntry::RotationNext,
            KeyEntry::MemberKey,
        ] {
            assert!(
                kept.contains(&entry.name(KEYS_DRIVE)),
                "{entry:?} stays: {kept:?}"
            );
        }
        assert_eq!(
            device::load_drive_key(&keyring, KEYS_DRIVE).unwrap(),
            Some(k1)
        );
        let (_, next) = device::rotation_key_entries(KEYS_DRIVE);
        assert_eq!(
            device::held(&keyring, &next).unwrap().map(|(key, _)| key),
            Some(k2),
            "the one copy of K2 is still in the keyring"
        );
    }

    /// X-DELTA-AZDRIVE-KEYS-2: the same removal after a rotation that FINISHED (the keyring's new
    /// key is the drive key; the leftovers of its cleanup are no key change under way) takes
    /// everything off, as before.
    #[test]
    fn x_delta_azdrive_keys_2_a_finished_rotations_leftovers_do_not_stop_a_removal() {
        let keyring = azul_storage::keyring::MemoryKeyring::new();
        let k2 = DriveKey::generate().unwrap();
        device::store_drive_key(&keyring, KEYS_DRIVE, &k2).unwrap();
        drop(device::stash(&keyring, &device::rotation_key_entries(KEYS_DRIVE).1, &k2).unwrap());
        let report = forget_job(
            &keyring,
            KEYS_DRIVE,
            &no_places(),
            true,
            Some(removal_proof_of_a_checked_code()),
            None,
        );
        assert!(report.complete());
        assert!(report.kept().is_empty(), "{:?}", report.kept());
        assert_eq!(device::load_drive_key(&keyring, KEYS_DRIVE).unwrap(), None);
        assert!(!rotation::unfinished(&keyring, KEYS_DRIVE).unwrap());
    }

    /// X-DELTA-AZDRIVE-KEYS-8: months paid that only this computer's vault holds (the drive's own
    /// could not take them, or the drive has none) go with the drive's keys and the vault's key
    /// only when the drive holds them too; a question that could not be answered is no "yes".
    #[test]
    fn x_delta_azdrive_keys_8_months_only_this_computer_holds_stay_with_their_keys() {
        use azul_storage::durable::{self, Mode};
        let dir = azul_storage::testing::TempDir::new("k8-removal");
        let vault = dir.path().join("d_k2.json");
        durable::replace(&vault, b"{}", Mode::Private).unwrap();
        let places = crate::footprint::Places {
            drives_file: None,
            spots: vec![crate::footprint::Spot {
                store: "period vault",
                paths: vec![vault.clone()],
                only_with_proof: true,
            }],
            open_copies: azul_storage::scratch::Scratch::tagged_under(
                dir.path(),
                "open",
                KEYS_DRIVE,
            ),
        };
        let keyring = azul_storage::keyring::MemoryKeyring::new();
        let drive_key = DriveKey::generate().unwrap();
        device::store_drive_key(&keyring, KEYS_DRIVE, &drive_key).unwrap();
        keyring
            .set(&KeyEntry::VaultKey.name(KEYS_DRIVE), "the vault's key")
            .unwrap();

        for not_elsewhere in [
            Ok(false),
            Err(String::from("the drive's vault was not read")),
        ] {
            let report = forget_job(
                &keyring,
                KEYS_DRIVE,
                &places,
                true,
                Some(removal_proof_of_a_checked_code()),
                Some(not_elsewhere.clone()),
            );
            assert!(report.complete());
            assert!(
                vault.exists(),
                "{not_elsewhere:?}: the only copy of the months stays"
            );
            assert_eq!(
                device::load_drive_key(&keyring, KEYS_DRIVE).unwrap(),
                Some(drive_key.clone())
            );
            assert!(
                keyring
                    .get(&KeyEntry::VaultKey.name(KEYS_DRIVE))
                    .unwrap()
                    .is_some(),
                "and so does the key that opens it"
            );
        }
        // The drive's vault holds them: everything goes.
        let report = forget_job(
            &keyring,
            KEYS_DRIVE,
            &places,
            true,
            Some(removal_proof_of_a_checked_code()),
            Some(Ok(true)),
        );
        assert!(
            report.complete() && report.kept().is_empty(),
            "{:?}",
            report.kept()
        );
        assert!(!vault.exists());
        assert_eq!(device::load_drive_key(&keyring, KEYS_DRIVE).unwrap(), None);
        assert!(keyring
            .get(&KeyEntry::VaultKey.name(KEYS_DRIVE))
            .unwrap()
            .is_none());
    }

    #[test]
    fn x_delta_azdrive_keys_8_the_proof_of_a_removal_is_kept_only_when_every_question_says_yes() {
        let proof = || Some(removal_proof_of_a_checked_code());
        // No proof from the settings: nothing is asked, nothing is taken back.
        let (kept, taken) = proof_for_removal(None, Ok(true), Some(Ok(false)));
        assert!(kept.is_none() && taken.is_none());
        let (kept, taken) = proof_for_removal(proof(), Ok(false), None);
        assert!(
            kept.is_some() && taken.is_none(),
            "a drive with no vault of its own"
        );
        let (kept, taken) = proof_for_removal(proof(), Ok(false), Some(Ok(true)));
        assert!(kept.is_some() && taken.is_none());
        for (unfinished, months, why) in [
            (Ok(true), None, ProofTaken::KeyChangeUnfinished),
            (
                Err(String::from("denied")),
                None,
                ProofTaken::KeyringUnreadable,
            ),
            (Ok(false), Some(Ok(false)), ProofTaken::PaidMonthsOnlyHere),
            (
                Ok(false),
                Some(Err(String::from("unread"))),
                ProofTaken::PaidMonthsUnknown,
            ),
        ] {
            let (kept, taken) = proof_for_removal(proof(), unfinished, months);
            assert!(kept.is_none(), "{why:?}");
            assert_eq!(taken, Some(why));
        }
    }

    /// X-DELTA-AZDRIVE-KEYS-20: "Encrypt this drive" on a drive that holds files saves the mark
    /// "the files still have to move" BEFORE the key exists. The job's own first step does it
    /// (`before`), through the real `run`: here it records whether the drive had a key, or a key
    /// file in its bucket, at that moment - the old order set the mark in the outcome handler
    /// after the key was made, and a kill in between left an encrypted drive that nothing moved
    /// the files into.
    #[test]
    fn x_delta_azdrive_keys_20_the_mark_is_on_disk_before_the_setup_makes_its_key() {
        use azul_storage::meta::MemoryBucket;
        let drive = "x-delta-keys-20-order";
        let bucket = Arc::new(MemoryBucket::new());
        let auto = Arc::new(AutoEncrypted::new(
            bucket.clone(),
            drive,
            Arc::new(azlin_keyring(Asking::User)),
            Some(Arc::new(MemoryProvider)),
        ));
        let seen: Arc<Mutex<Option<(bool, bool)>>> = Arc::new(Mutex::new(None));
        let before: Before = {
            let (seen, bucket) = (Arc::clone(&seen), Arc::clone(&bucket));
            Box::new(move || {
                let key = device::load_drive_key(&azlin_keyring(Asking::User), drive)
                    .map_err(|e| e.to_string())?
                    .is_some();
                let files = device::is_encrypted(bucket.as_ref()).map_err(|e| e.to_string())?;
                *seen.lock().unwrap() = Some((key, files));
                Ok(())
            })
        };
        let outcome = run(EncryptionJob::SetUp {
            drive_id: drive.to_string(),
            auto,
            new_drive: false,
            before: Some(before),
        });
        assert!(
            matches!(outcome, EncryptionOutcome::SetUp { result: Ok(_), .. }),
            "the setup went through"
        );
        assert_eq!(
            *seen.lock().unwrap(),
            Some((false, false)),
            "when the mark was saved there was no key in the keyring and no key file in the bucket"
        );
        assert!(
            device::load_drive_key(&azlin_keyring(Asking::User), drive)
                .unwrap()
                .is_some(),
            "and then the key was made"
        );
    }

    /// X-DELTA-AZDRIVE-KEYS-20: a mark that cannot be saved stops the setup with nothing done - no
    /// key in the keyring, nothing in the bucket (the mark vouches for the effect, so the effect
    /// does not happen without it).
    #[test]
    fn x_delta_azdrive_keys_20_a_mark_that_cannot_be_saved_stops_the_setup_before_the_key() {
        use azul_storage::meta::MemoryBucket;
        let drive = "x-delta-keys-20-refused";
        let bucket = Arc::new(MemoryBucket::new());
        let auto = Arc::new(AutoEncrypted::new(
            bucket.clone(),
            drive,
            Arc::new(azlin_keyring(Asking::User)),
            Some(Arc::new(MemoryProvider)),
        ));
        let before: Before =
            Box::new(|| Err(String::from("the settings folder cannot be written")));
        let outcome = run(EncryptionJob::SetUp {
            drive_id: drive.to_string(),
            auto,
            new_drive: false,
            before: Some(before),
        });
        match outcome {
            EncryptionOutcome::SetUp {
                result: Err(why), ..
            } => assert!(why.contains("cannot be written"), "{why}"),
            _ => panic!("a mark that is not on disk is an error"),
        }
        assert!(
            device::load_drive_key(&azlin_keyring(Asking::User), drive)
                .unwrap()
                .is_none(),
            "no key was made"
        );
        assert!(
            !device::is_encrypted(bucket.as_ref()).unwrap(),
            "nothing reached the bucket"
        );
    }

    /// X-DELTA-AZDRIVE-KEYS-20: what the job's first step writes is the window's settings with the
    /// drive's mark, in the settings file - and the resume finds the move due from it.
    #[test]
    fn x_delta_azdrive_keys_20_the_first_step_writes_the_settings_with_the_mark() {
        use azul_storage::testing::TempDir;
        let dir = TempDir::new("mark-first");
        let drive = LocalDrive::new(dir.path().to_path_buf());
        let start = crate::model::Settings::default();
        let before = mark_files_will_move_first(drive, start.clone(), start, "d_1");
        before().unwrap();
        let text = std::fs::read_to_string(dir.path().join(crate::SETTINGS_KEY)).unwrap();
        let saved = crate::model::Settings::parse(&text).expect("a settings file");
        let marked = state_of(&saved.recovery.drives, "d_1").is_some_and(|state| state.moves_files);
        assert!(marked, "the mark is in the file");
        assert!(
            move_is_due(marked, None),
            "and the resume finds the files due to move"
        );
    }

    /// X-DELTA-AZDRIVE-KEYS-19: a scan's end sets a drive up by itself only for a drive THIS run
    /// made whose first try could not start, and for a setup of this computer that stopped halfway
    /// (its key is in the keyring). A drive that merely lists empty (a server's word, on a computer
    /// with no key and no note) and a drive with files and no key file wait for the user's word -
    /// the old rule set up every drive that listed empty, with a new random key: the only thing
    /// between that and a second key family beside the drive's real one was the node's conditional
    /// write.
    #[test]
    fn x_delta_azdrive_keys_19_a_scan_sets_up_only_a_drive_this_run_made_or_a_setup_of_this_computer(
    ) {
        use azul_storage::trust::{Via, Why};
        let encrypted = EncState::Encrypted {
            key_id: None,
            via: Via::KeyInKeyring,
        };
        for (state, made_here, started, wanted) in [
            // A drive this run made, not open at its making, listing empty: set up now.
            (Some(EncState::Unencrypted(Why::New)), true, false, true),
            // A drive that lists empty and this run did not make: a server's word, it waits.
            (Some(EncState::Unencrypted(Why::New)), false, false, false),
            // This computer's own stopped setup: finished, whoever made the drive.
            (Some(EncState::SetupPending), false, false, true),
            (Some(EncState::SetupPending), true, false, true),
            // Files and no key file: never by itself, made here or not.
            (
                Some(EncState::Unencrypted(Why::HiddenOrAbsent)),
                true,
                false,
                false,
            ),
            (
                Some(EncState::Unencrypted(Why::HiddenOrAbsent)),
                false,
                false,
                false,
            ),
            // Encrypted, not decided, not open: nothing to set up.
            (Some(encrypted), true, false, false),
            (Some(EncState::Undecided), true, false, false),
            (None, true, false, false),
            // Once per drive and run.
            (Some(EncState::Unencrypted(Why::New)), true, true, false),
            (Some(EncState::SetupPending), false, true, false),
        ] {
            assert_eq!(
                resume_wanted(state, made_here, started),
                wanted,
                "{state:?} made_here={made_here} started={started}"
            );
        }
    }

    /// X-DELTA-AZDRIVE-KEYS-19: the "Encrypt this drive" confirm warns - first - for a drive that
    /// holds files and shows no key file (this computer cannot tell it from an encrypted drive
    /// whose storage hides its key files, and encrypting moves what the storage shows into a new
    /// key), and for no other; every drive gets the four lines that say what encrypting does.
    #[test]
    fn x_delta_azdrive_keys_19_the_confirm_warns_for_a_drive_this_computer_cannot_tell() {
        use azul_storage::trust::Why;
        let plain = [
            "azdrive-enc-confirm-1",
            "azdrive-enc-confirm-2",
            "azdrive-enc-confirm-3",
            "azdrive-enc-confirm-4",
        ];
        for state in [
            None,
            Some(EncState::Undecided),
            Some(EncState::SetupPending),
            Some(EncState::Unencrypted(Why::New)),
        ] {
            assert_eq!(confirm_lines(state), plain, "{state:?}");
        }
        let hidden = confirm_lines(Some(EncState::Unencrypted(Why::HiddenOrAbsent)));
        assert_eq!(hidden.first(), Some(&"azdrive-enc-confirm-cannot-tell"));
        assert_eq!(&hidden[1..], plain, "and then what encrypting does");
        // The words are in both languages' resources (the line is a key, not a text).
        for lang in ["en", "de"] {
            let ftl = std::fs::read_to_string(format!(
                "{}/resources/{lang}.ftl",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap();
            for key in &hidden {
                assert!(
                    ftl.lines().any(|l| l.starts_with(&format!("{key} ="))),
                    "{lang}.ftl has no {key}"
                );
            }
        }
    }

    /// X-DELTA-AZDRIVE-KEYS-24: a page or a join code that is saved goes into the file readable by
    /// this user only - the function the save dialog's answer writes with. `FileDialog::save_bytes`
    /// wrote it with the default mode (0644 on Unix): any user of the computer could read a recovery
    /// code, a share, a claim code or a join code from the file. The save replaces a file that is
    /// there (a world-readable one stays no longer so) and leaves no temporary file beside it.
    #[test]
    fn x_delta_azdrive_keys_24_a_saved_page_or_join_code_is_readable_by_this_user_only() {
        use azul_storage::testing::TempDir;
        let dir = TempDir::new("invite-save");
        let path = dir.path().join("join-code.txt");
        std::fs::write(&path, "an older file").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        crate::paper::write_private(&path, b"AZJ1-the-join-code").unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "AZJ1-the-join-code"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o077, 0, "this user's only: {mode:o}");
        }
        let names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["join-code.txt"], "no temporary file is left");
    }
}
