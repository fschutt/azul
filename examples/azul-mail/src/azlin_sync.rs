//! An Azlin account's Send/Receive and actions (`AZLIN_MAIL.md`): the mailbox in the drive -
//! `mail/<Folder>/<name>.eml` and the markers under `mail/.state/` - and its copy on this
//! computer, the same files an IMAP account keeps (`store.rs`), every index line carrying its
//! object's key (`IndexEntry::remote`).
//!
//! Send/Receive ([`sync_account`]), for every folder of the drive and the well-known ones:
//!
//! 1. **Push**: mail filed here first (a sent mail, a draft saved while offline: no `remote`
//!    yet) goes into its folder of the drive under its name (`azlin::object_name`).
//! 2. **Pull**: the folder's listing against the index. Names that are gone leave the folder -
//!    when the token server, asked NOW, says the drive was not restored
//!    ([`AskRestore`]; once per Send/Receive, and only when some folder has a name that is gone),
//!    and the drive, asked about the object itself, says it is not there; never after a restore of
//!    the drive (below), never when the token server cannot be asked, and never when the clock of
//!    this computer cannot date the hold ([`store::Clocks::hold_time`]: the lines stay where they
//!    are, the Send/Receive goes on - new mail comes, the later folders are synced, the sweep runs -
//!    and the window says why, [`FolderReport::kept_undated`]; X-DELTA-APPS5-1). They are NOT deleted:
//!    the file moves to `removed/<folder>/<stamp>/` and stays [`HOLD_DAYS`] days, like the drive's
//!    own history ([`hold`], [`sweep_removed`]; X-DELTA-APPS3-3: the user's own "Restore the drive
//!    as of" on an encrypted drive is one ordinary commit nobody announces, so no notice can tell
//!    it from a delete). New names are fetched -
//!    whole up to `full_fetch_limit`, else only their first `head_bytes` (the header block;
//!    [`fetch_message`] gets the rest when the message is opened) - and get the next UIDs in
//!    name (arrival) order.
//! 3. **Flags**: the marks made here (`flags.json`) are written as markers where the drive says
//!    otherwise, then dropped; every message's flags are the drive's markers from then on.
//!
//! A drive that was RESTORED (an operator's restore from its off-site copy, "Restore as of"; the
//! token server announces it with an epoch) serves an older state, so the mail that came after is
//! missing from its listing and exists here only. That is not a delete: the Send/Receive that
//! takes the announcement deletes no mail here, puts the mail the drive lacks into it again (a
//! message of which only the header block is here stays as it is) and says so
//! ([`FolderReport::kept_after_restore`]); the epoch is remembered ([`DRIVE_FILE`]) once every
//! folder is through, so a pass that stopped takes the restore again (X-DELTA-SYNC4-20;
//! `restore.rs` is the one protocol of every drive, AzDrive's sync decides the same way).
//!
//! WHERE the announcement comes from (X-DELTA-APPS3-2): the session's last sign-in carries the
//! newest one it saw ([`AzlinOptions::restore`]), hours old by design, so a restore announced
//! after it - on a drive another device has written to since - is in no snapshot, and the drive's
//! own answer about an object cannot tell it from a delete. Before the first folder with mail the
//! drive no longer lists is pulled, Send/Receive asks the token server for the drive's newest
//! announcement NOW ([`AskRestore`]) and judges with the answer; a failed ask is an unknown: that
//! mail stays, is not put back either, and the window says so
//! ([`FolderReport::kept_unasked`]).
//!
//! The actions - [`move_messages`], [`delete_messages`], [`upload_message`], [`push_marks`],
//! [`fetch_message`] - write the drive first and the local copy after, so a refused request
//! changes nothing here. Every call blocks: azul Threads run them, one at a time
//! ([`lock_cache`]). No azul types here: tested with a folder on disk as the drive.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Mutex, MutexGuard},
};

use azul_storage::{
    ops,
    restore::{judge, Clock, RestoreNotice, Seen, Standing},
    time::parse_iso8601,
    ByteRange, Drive, DriveError, ObjectInfo,
};
use serde::{Deserialize, Serialize};

use crate::{
    azlin::{self, AzlinSession, MessageState},
    folders::{LocalMailbox, Role},
    listing::{self, LocalFlags},
    message,
    store::{self, ClockRefusal, Clocks, FolderState, HoldAt, IndexEntry, MailStore},
    sync::{FolderReport, Progress, SyncError, SyncReport},
};

/// The UIDVALIDITY an Azlin folder's state carries: its UIDs are AzMail's own and never
/// renumbered.
pub const AZLIN_UIDVALIDITY: u32 = 1;

/// How Send/Receive fetches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AzlinOptions {
    /// A message up to this many bytes is fetched whole.
    pub full_fetch_limit: u64,
    /// Of a bigger one only this many bytes: its header block.
    pub head_bytes: u64,
    /// Bytes per ranged GET when a big message is fetched ([`fetch_message`]).
    pub chunk: u64,
    /// Now, in seconds since 1970 (`synced_at`; the arrival of mail filed here without a date).
    pub now: i64,
    /// The drive's newest restore announcement as the session's last sign-in carried it
    /// ([`crate::azlin::AzlinSession::restore`]); `None`: none was announced. One this copy has
    /// not taken yet makes the Send/Receive a restore's: it deletes no mail here
    /// ([`sync_account`]). A snapshot: the token server is asked again, when it matters, through
    /// [`AskRestore`].
    pub restore: Option<RestoreNotice>,
}

impl AzlinOptions {
    /// The options of one Send/Receive with `session` at `now`: the restore announcement is the
    /// one the session's last sign-in carried. The ONE place the session's notice becomes the
    /// sync's - it was a line in the caller that no test pinned (X-DELTA-APPS3-10).
    #[must_use]
    pub fn for_session(session: &AzlinSession, now: i64) -> AzlinOptions {
        AzlinOptions {
            now,
            restore: session.restore.clone(),
            ..AzlinOptions::default()
        }
    }
}

impl Default for AzlinOptions {
    fn default() -> AzlinOptions {
        AzlinOptions {
            full_fetch_limit: 4 * 1024 * 1024,
            head_bytes: 64 * 1024,
            chunk: azul_storage::transfer::CHUNK,
            now: 0,
            restore: None,
        }
    }
}

/// The local copies of the Azlin accounts are written by one Thread at a time (Send/Receive,
/// an action, a draft): two never interleave their writes of one folder's index.
static CACHE: Mutex<()> = Mutex::new(());

/// Waits until no other Thread writes an Azlin account's local copy; the copy is this one's
/// until the guard is dropped.
pub fn lock_cache() -> MutexGuard<'static, ()> {
    CACHE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A drive's error as the sync's: no answer is a connection problem, a refusal of the keys a
/// sign-in problem.
pub fn drive_error(e: DriveError) -> SyncError {
    let text = e.to_string();
    match e {
        DriveError::Transport(message) => SyncError::Connect(message),
        DriveError::Denied { .. } | DriveError::Locked { .. } => SyncError::Auth(text),
        DriveError::Service(service) if matches!(service.status, 401 | 403) => {
            SyncError::Auth(text)
        }
        DriveError::Io(message) => SyncError::Storage(message),
        _ => SyncError::Protocol(text),
    }
}

fn io(e: std::io::Error) -> SyncError {
    SyncError::Storage(e.to_string())
}

// ==== The local copy's files ====

/// The folder's index, by UID: empty when there is none (a file that is not there). A read that
/// FAILED is an error, never an empty folder: the index written back from nothing would drop every
/// line it held and number the next message from the start (X-DELTA-SYNC4-21).
fn read_index(store: &MailStore, folder: &str) -> Result<BTreeMap<u32, IndexEntry>, SyncError> {
    match store.get(&store::index_key(folder)) {
        Ok(bytes) => Ok(store::index_from_jsonl(&String::from_utf8_lossy(&bytes))
            .into_iter()
            .map(|entry| (entry.uid, entry))
            .collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(io(e)),
    }
}

fn write_index(
    store: &MailStore,
    folder: &str,
    index: &BTreeMap<u32, IndexEntry>,
) -> std::io::Result<()> {
    let entries: Vec<IndexEntry> = index.values().cloned().collect();
    store.put(
        &store::index_key(folder),
        store::index_to_jsonl(&entries).as_bytes(),
    )
}

/// The folder's state: `None` when there is none or it is none of this AzMail's; a read that
/// FAILED is an error (the last UID would start from 0 and a number once given would be given again).
fn read_state(store: &MailStore, folder: &str) -> Result<Option<FolderState>, SyncError> {
    match store.get(&store::state_key(folder)) {
        Ok(bytes) => Ok(String::from_utf8(bytes)
            .ok()
            .and_then(|text| FolderState::from_json(&text))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io(e)),
    }
}

/// The marks made here and not pushed yet; none when there is no file. A read that FAILED is an
/// error: the marks written back from nothing would lose them.
fn read_marks(store: &MailStore, folder: &str) -> Result<LocalFlags, SyncError> {
    match store.get(&listing::flags_key(folder)) {
        Ok(bytes) => Ok(String::from_utf8(bytes)
            .ok()
            .and_then(|text| LocalFlags::from_json(&text))
            .unwrap_or_else(LocalFlags::create)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(LocalFlags::create()),
        Err(e) => Err(io(e)),
    }
}

fn write_marks(store: &MailStore, folder: &str, marks: &LocalFlags) -> std::io::Result<()> {
    store.put(&listing::flags_key(folder), marks.to_json().as_bytes())
}

/// What this copy remembers of the drive between two Send/Receives, at the root of the account's
/// folder: the newest restore announcement it took ([`DriveMemory::restore_epoch`]).
pub const DRIVE_FILE: &str = "azlin-drive.json";

/// The `format` of [`DRIVE_FILE`].
const DRIVE_FORMAT: &str = "azmail.azlin-drive";

/// The contents of [`DRIVE_FILE`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct DriveMemory {
    format: String,
    version: u64,
    /// The newest restore epoch ([`RestoreNotice::epoch`]) a Send/Receive of this copy took
    /// completely.
    #[serde(default)]
    restore_epoch: u64,
    /// The restore epoch a pass is taking now, written down BEFORE the first message goes back into
    /// the drive and cleared (0) with the epoch above once every folder is through: a memory that
    /// cannot be written ends the call before anything is put, and a pass that was cut says what it
    /// was taking (X-DELTA-APPS3-13).
    #[serde(default)]
    taking: u64,
}

/// The restore epoch this copy took last: 0 for a copy that took none (no file, or one that is not
/// this AzMail's: the restore is then taken once more, which deletes nothing). A file that
/// cannot be READ is an error, never "no restore taken": not knowing is not a reason to treat the
/// next listing as an ordinary one.
fn read_restore_epoch(store: &MailStore) -> Result<u64, SyncError> {
    match store.get(DRIVE_FILE) {
        Ok(bytes) => Ok(serde_json::from_slice::<DriveMemory>(&bytes)
            .ok()
            .filter(|memory| memory.format == DRIVE_FORMAT)
            .map_or(0, |memory| memory.restore_epoch)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(e) => Err(io(e)),
    }
}

fn write_restore_epoch(store: &MailStore, epoch: u64) -> std::io::Result<()> {
    write_memory(store, epoch, 0)
}

/// Writes that a pass is taking the restore `taking` while the newest epoch taken is `taken`.
fn write_restore_taking(store: &MailStore, taken: u64, taking: u64) -> std::io::Result<()> {
    write_memory(store, taken, taking)
}

fn write_memory(store: &MailStore, restore_epoch: u64, taking: u64) -> std::io::Result<()> {
    let memory = DriveMemory {
        format: DRIVE_FORMAT.to_string(),
        version: 1,
        restore_epoch,
        taking,
    };
    let mut text = serde_json::to_string_pretty(&memory)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    text.push('\n');
    store.put(DRIVE_FILE, text.as_bytes())
}

/// What a Send/Receive asks the token server before it lets go of mail the drive no longer lists:
/// the drive's newest restore announcement as the token server says it NOW. The session's own
/// ([`AzlinOptions::restore`]) is the one its last sign-in carried, hours old, so a restore
/// announced after it is in no snapshot; and when another device has committed on top of the
/// restored drive since, the drive's own answer about an object cannot tell the restore from a
/// delete (X-DELTA-APPS3-2). A closure of the right shape is one.
pub trait AskRestore {
    /// `Ok(None)`: the token server says none was announced. An error: it could not be asked, or
    /// its answer did not say - an unknown, never "none".
    ///
    /// # Errors
    ///
    /// The token server's refusal, no answer, or an answer that does not say.
    fn newest(&self) -> Result<Option<RestoreNotice>, SyncError>;
}

impl<F> AskRestore for F
where
    F: Fn() -> Result<Option<RestoreNotice>, SyncError>,
{
    fn newest(&self) -> Result<Option<RestoreNotice>, SyncError> {
        self()
    }
}

/// What one Send/Receive knows of a restore of the drive.
#[derive(Debug)]
enum Knowledge {
    /// No announcement this copy has not taken, and the token server not asked yet: it is asked
    /// before the first folder with mail the drive no longer lists is pulled.
    Unasked,
    /// A restore (the session's snapshot, or the token server's answer) that this copy takes now.
    Restored(RestoreNotice),
    /// The token server was asked: the drive stands where this copy last saw it.
    Ordinary,
    /// The token server could not be asked, or did not say: an unknown, with the error's text.
    Unknown(String),
}

/// What a folder's pull does with the mail the drive no longer lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stance {
    /// The drive was not restored: what it no longer lists leaves, once the drive, asked about the
    /// object itself, says it is not there.
    Ordinary,
    /// The drive was restored: nothing leaves, what it lacks goes back into it.
    Restore,
    /// The token server could not be asked: nothing leaves and nothing goes back.
    Keep,
}

/// What one Send/Receive knows of the drive's restore standing, from the first folder to the last.
struct Pass<'a> {
    /// The newest restore epoch this copy took before this pass.
    seen: u64,
    ask: &'a dyn AskRestore,
    knowledge: Knowledge,
    /// The restore this pass takes is written into the memory ([`write_restore_taking`]).
    taking_written: bool,
    /// The readings of the time this pass dates a hold by, and the sweep decides by.
    clocks: Clocks,
    /// The moment a hold made in this pass is dated - the clock of this pass set against `clocks` - or why none is
    /// made: decided once, for every folder ([`Clocks::hold_time`]).
    hold: Result<HoldAt, ClockRefusal>,
}

impl<'a> Pass<'a> {
    /// The pass of a copy that took restore epoch `seen`, with `snapshot`, the newest announcement
    /// the session's last sign-in carried. Only the epoch decides ([`Clock::Own`]): the drive's
    /// counter says nothing here (an Azlin drive's mail is listed, its index is not read); the one
    /// decision every drive's sync makes (`azul_storage::restore::judge`).
    fn new(seen: u64, snapshot: Option<&RestoreNotice>, ask: &'a dyn AskRestore) -> Pass<'a> {
        let knowledge = match Pass::judged(seen, snapshot) {
            Standing::Restored(notice) => Knowledge::Restored(notice),
            _ => Knowledge::Unasked,
        };
        Pass {
            seen,
            ask,
            knowledge,
            taking_written: false,
            clocks: Clocks::default(),
            hold: Err(ClockRefusal::NotSet),
        }
    }

    /// The readings this pass sets the clock against, and the date a hold made at `now` gets from them.
    fn set_clocks(&mut self, clocks: Clocks, now: i64) {
        self.hold = clocks.hold_time(now);
        self.clocks = clocks;
    }

    fn judged(seen: u64, notice: Option<&RestoreNotice>) -> Standing {
        judge(
            &Seen {
                epoch: seen,
                clock: 0,
            },
            0,
            Clock::Own,
            notice,
            azlin::MAIL_PREFIX,
        )
    }

    /// What a folder does with what the drive no longer lists. `something_gone`: the listing lacks
    /// a line of the folder's index. The token server is asked here, once for the whole pass and
    /// only when something is gone: a Send/Receive with nothing gone costs no request.
    fn stance(&mut self, something_gone: bool) -> Stance {
        if something_gone && matches!(self.knowledge, Knowledge::Unasked) {
            self.knowledge = match self.ask.newest() {
                Err(e) => Knowledge::Unknown(e.to_string()),
                Ok(answer) => match Pass::judged(self.seen, answer.as_ref()) {
                    Standing::Restored(notice) => Knowledge::Restored(notice),
                    _ => Knowledge::Ordinary,
                },
            };
        }
        match self.knowledge {
            Knowledge::Restored(_) => Stance::Restore,
            Knowledge::Unknown(_) => Stance::Keep,
            Knowledge::Unasked | Knowledge::Ordinary => Stance::Ordinary,
        }
    }

    /// Writes down the restore this pass takes, once, before the first message of it goes back into
    /// the drive: a memory that cannot be written is an error here, with nothing put.
    fn record_taking(&mut self, store: &MailStore) -> Result<(), SyncError> {
        if self.taking_written {
            return Ok(());
        }
        let Knowledge::Restored(notice) = &self.knowledge else {
            return Ok(());
        };
        write_restore_taking(store, self.seen, notice.epoch).map_err(io)?;
        self.taking_written = true;
        Ok(())
    }

    /// The restore this pass takes, to be remembered once every folder is through.
    fn restore_taken(&self) -> Option<&RestoreNotice> {
        match &self.knowledge {
            Knowledge::Restored(notice) => Some(notice),
            _ => None,
        }
    }

    /// Why the token server could not be asked, when it could not.
    fn unknown(&self) -> Option<&str> {
        match &self.knowledge {
            Knowledge::Unknown(why) => Some(why),
            _ => None,
        }
    }
}

/// Whether the restore of the drive to the moment `as_of` lost the mail of `entry`, whose object
/// the restored drive lacks: this computer first saw it in the drive AFTER that moment, so the
/// restore took it away and it goes back. Mail first seen before the moment and absent was deleted
/// before it - elsewhere - and does not come back (X-DELTA-APPS3-13). Nothing known is the old
/// answer, the conservative one: a line from before `on_drive_since`, or a notice that names no
/// moment, goes back. (The moment is the token server's, the line's stamp this computer's: a clock
/// that is off puts a line on the wrong side, and a copy that does not go back is held 90 days,
/// never deleted.)
fn restore_lost(entry: &IndexEntry, as_of: Option<i64>) -> bool {
    match (entry.on_drive_since, as_of) {
        (Some(since), Some(as_of)) => since >= as_of,
        _ => true,
    }
}

/// The UID after `last` that no line of `index` has (mail filed here has UIDs far above, from
/// `send::LOCAL_UID_FLOOR`).
fn next_uid(index: &BTreeMap<u32, IndexEntry>, last: u32) -> u32 {
    let mut uid = last.saturating_add(1).max(1);
    while index.contains_key(&uid) {
        uid = uid.saturating_add(1);
    }
    uid
}

/// When a message filed here arrived: its date, else `now`.
fn arrival_of(entry: &IndexEntry, now: i64) -> u64 {
    parse_iso8601(&entry.date).unwrap_or_else(|| u64::try_from(now).unwrap_or(0))
}

/// The month a message's file goes under: its name's stamp, else its date.
fn year_month_of(entry: &IndexEntry) -> (i32, u32) {
    azlin::message_id(&entry.remote)
        .and_then(azlin::stamp_of)
        .or_else(|| parse_iso8601(&entry.date))
        .map_or((0, 0), |secs| {
            message::year_month(i64::try_from(secs).unwrap_or(0))
        })
}

/// `flags` with `flag` (`\Seen`) set or cleared.
fn set_flag(flags: &mut Vec<String>, flag: &str, on: bool) {
    flags.retain(|f| !f.eq_ignore_ascii_case(flag));
    if on {
        flags.push(flag.to_string());
    }
}

/// The drive's folder of the local folder `key`: the one its state names (Send/Receive wrote
/// it), else the well-known folder of its role, else the key itself. A state that cannot be read
/// names no folder: an error, not a guess (mail would go into a folder of the wrong name).
pub fn remote_path_of(store: &MailStore, key: &str) -> Result<String, SyncError> {
    Ok(read_state(store, key)?
        .map(|state| state.server_name)
        .filter(|name| !name.trim().is_empty())
        .or_else(|| azlin::well_known_name(Role::of_key(key)).map(str::to_string))
        .unwrap_or_else(|| key.to_string()))
}

/// The object of the message `uid` of the local folder `folder`, when it is in the drive. `None`
/// for an index that cannot be read too: the caller then deletes nothing of the drive (the draft
/// it replaced stays there as a second one).
pub fn remote_of(store: &MailStore, folder: &str, uid: u32) -> Option<String> {
    // collapse-ok: the unknown answer is the safe one here - what is not named is not deleted.
    read_index(store, folder)
        .ok()?
        .remove(&uid)
        .map(|entry| entry.remote)
        .filter(|remote| !remote.is_empty())
}

/// The message is in the drive but its file is not here yet: Send/Receive fetched only its
/// header block ([`fetch_message`] gets the rest).
pub fn needs_fetch(store: &MailStore, entry: &IndexEntry) -> bool {
    !entry.remote.is_empty() && store.size_of(&entry.path).is_none()
}

/// How long what a pull took out of a folder stays on this computer: the span a drive keeps the
/// versions of its files ([`azul_storage::meta::RETENTION`], the owner's 90 days), counted from the
/// stamp of the folder it was moved into. The one number the window's sentence and the sweep share.
pub const HOLD_DAYS: u64 = azul_storage::meta::RETENTION.versions_days();

/// [`HOLD_DAYS`] in seconds.
const HOLD_SECS: u64 = azul_storage::meta::RETENTION.versions.as_secs();

/// The newest `modified` of the mail the drive listed: the drive's own word that it lived at least until then, a
/// LOWER bound of its time (it never says a later time than it is) - the reading of the drive in the clock the
/// pull dates a hold by ([`store::Clocks`]) and the sweep is set against. `None`: the drive lists no mail.
fn drive_reading(remote: &[RemoteFolder]) -> Option<u64> {
    remote
        .iter()
        .flat_map(|folder| folder.messages.iter())
        .filter_map(|object| object.modified)
        .max()
}

/// Takes `leaving`, the lines of the folder `folder` whose objects the drive no longer has, out of
/// `index` - and keeps what they were: a pull never destroys a copy, so a delete on another device
/// or the user's own "Restore the drive as of ..." (one ordinary commit that nobody announces)
/// costs the mail on this computer nothing for [`HOLD_DAYS`] days (X-DELTA-APPS3-3).
///
/// The order, every step a crash cell: (0) the date `at`, which only [`Clocks::hold_time`] makes - a clock that
/// cannot date a hold is the caller's: it keeps every line where it is, says why and goes on (X-DELTA-APPS5-1);
/// (1) the record `removed/<folder>/<stamp>/index.jsonl` of
/// every leaving line, durable BEFORE any file moves - a store that cannot be written (a full
/// disk) ends the pull here, with nothing moved and nothing of the drive touched; a record of the
/// same stamp that is there already (a retry in the same second) is merged, its lines kept; (2) each
/// file moves (one rename that replaces nothing: a taken name ends the pull) and only then its line
/// leaves `index`. A line whose file is not here (a big message of which only the header block was
/// fetched, or one an earlier pass moved) has nothing to move: its line is in the record. A stop
/// anywhere leaves every line in the folder's index or in a record, every file where it was or in
/// the holding folder.
fn hold(
    store: &MailStore,
    folder: &str,
    leaving: &[IndexEntry],
    at: HoldAt,
    index: &mut BTreeMap<u32, IndexEntry>,
    report: &mut FolderReport,
) -> Result<(), SyncError> {
    if leaving.is_empty() {
        return Ok(());
    }
    // A holding folder is dated by `at`: the clock of this computer set against the last Send/Receive and the
    // drive, the one constructor of a hold's date - never 1970 (swept at the next sync with a right clock) or
    // 2036 (kept for ever).
    let held = at.prefix(folder);
    let record_key = format!("{held}/{}", store::INDEX_FILE);
    let mut record: BTreeMap<u32, IndexEntry> = match store.get(&record_key) {
        Ok(bytes) => store::index_from_jsonl(&String::from_utf8_lossy(&bytes))
            .into_iter()
            .map(|entry| (entry.uid, entry))
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        Err(e) => return Err(io(e)),
    };
    for entry in leaving {
        record.insert(entry.uid, entry.clone());
    }
    let lines: Vec<IndexEntry> = record.into_values().collect();
    store
        .put(&record_key, store::index_to_jsonl(&lines).as_bytes())
        .map_err(io)?;
    let inside = format!("{}/", store::folder_prefix(folder));
    for entry in leaving {
        let relative = entry
            .path
            .strip_prefix(&inside)
            .unwrap_or_else(|| entry.path.rsplit('/').next().unwrap_or(&entry.path));
        match store.move_object(&entry.path, &format!("{held}/{relative}")) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io(e)),
        }
        index.remove(&entry.uid);
        report.removed += 1;
    }
    Ok(())
}

/// Deletes what the pull took out of the folders [`HOLD_DAYS`] days ago and more: the folders of
/// `removed/` whose stamp - the moment of the pull, by the clock of this computer - is that old at
/// the time. The time is the EARLIER of two readings: `now`, the clock of this computer, and `drive`,
/// the newest `modified` the drive lists ([`Clocks::drive`]: it lived at least until then, so it is a
/// lower bound) - a clock that jumped ahead (a reset to a future date, a virtual machine's) deletes
/// nothing a drive that has not lived that long does not agree to (X-APPS3-1, X-DELTA-APPS4-3). A
/// reading that is missing (no clock set, a drive that lists no mail) deletes nothing. Returns how
/// many it deleted. Never early: a stamp that is no date, one in the future (the clock was stepped
/// back since) and a key of another shape stay, and so does everything when the list of `removed/`
/// cannot be read - a list that failed is no list with nothing in it. The only thing that deletes a
/// copy the drive no longer lists.
///
/// # Errors
///
/// The list could not be read, or a file could not be deleted: nothing more is deleted.
pub fn sweep_removed(store: &MailStore, now: i64, drive: Option<u64>) -> Result<u64, SyncError> {
    sweep_removed_with(store, now, drive, &|prefix| store.keys_checked(prefix))
}

/// [`sweep_removed`] with the listing a test can make fail.
fn sweep_removed_with(
    store: &MailStore,
    now: i64,
    drive: Option<u64>,
    list: &dyn Fn(&str) -> std::io::Result<Vec<String>>,
) -> Result<u64, SyncError> {
    // No clock, no expiry; and the drive must agree: the earlier of the two readings decides.
    let (Some(local), Some(drive)) = (
        u64::try_from(now).ok().filter(|secs| *secs > 0),
        drive.filter(|secs| *secs > 0),
    ) else {
        return Ok(0);
    };
    let now = local.min(drive);
    let mut folders: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for key in list(store::REMOVED_PREFIX).map_err(io)? {
        let mut parts = key.splitn(4, '/');
        if let (Some(store::REMOVED_PREFIX), Some(folder), Some(stamp), Some(_)) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        {
            folders
                .entry((folder.to_string(), stamp.to_string()))
                .or_default()
                .push(key.clone());
        }
    }
    let mut swept = 0;
    for ((folder, stamp), keys) in folders {
        let Some(at) = azlin::stamp_of(&stamp).filter(|_| stamp.len() == 16) else {
            continue;
        };
        if now < at.saturating_add(HOLD_SECS) {
            continue;
        }
        for key in &keys {
            // ARCH30-4: NONE-FITS (expiry): a copy the drive no longer lists, past its holding span.
            store.delete(key).map_err(io)?;
        }
        // The empty folders it leaves behind are tidied away; one that stays holds nothing.
        // collapse-ok: cosmetic, the next sweep tries again.
        let _ = store.remove_empty_folder(&store::removed_prefix(&folder, &stamp));
        swept += 1;
    }
    Ok(swept)
}

/// The flag a line of the index carries while another program has the message marked deleted
/// (IMAP's `\Deleted`, written by the Azlin Bridge as the marker [`azlin::DELETED`]) and has not
/// expunged it: the message is still in the drive, so its line stays and nothing is held or counted as
/// removed; the window shows no line that has it (X-DELTA-APPS4-4). Not an IMAP flag, never sent anywhere.
pub const HIDDEN_FLAG: &str = "$azlin-marked-deleted";

/// The steps of [`follow_into`] a test stops the process after ([`cut`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cut {
    /// The lines are in their new folder's index (and state and marks).
    Written,
    /// The files are in their new folder's tree.
    Moved,
}

#[cfg(test)]
thread_local! {
    /// The step after which the process dies on this thread (a test's kill).
    static CUT_AFTER: std::cell::Cell<Option<Cut>> = const { std::cell::Cell::new(None) };
}

/// Whether the process dies right after `step`: never, outside a test. The error a real kill never
/// returns.
fn cut(step: Cut) -> Result<(), SyncError> {
    #[cfg(test)]
    if CUT_AFTER.with(std::cell::Cell::get) == Some(step) {
        return Err(SyncError::Storage(String::from(
            "the process stopped here (a test's kill)",
        )));
    }
    let _ = step;
    Ok(())
}

/// A message another device MOVED into another folder - and every message of a folder it RENAMED - is
/// no longer listed in the folder it left and is listed, under the same name (`<stamp>-<hash>.eml`),
/// in the one it went to: the drive still has it. Pulled as a delete, it was held under `removed/` as
/// "removed from the drive" and fetched again as new mail: kept twice for 90 days, and the window said
/// that 300 messages were lost that are in the drive (X-DELTA-APPS4-4). So before any folder is pulled
/// the line of such a message goes to the folder that lists it now, with its file, under a UID of that
/// folder and with the marks made here: no hold, no download, no sentence. A message the drive lists
/// nowhere is not followed (it is gone, and is held); a message listed in two other folders follows
/// the first of them.
///
/// Returns how many lines it moved.
///
/// # Errors
///
/// An index, a state or a marks file that cannot be read or written ends the call: what moved stays
/// moved ([`follow_into`] is one table of crash cells), and the next Send/Receive finishes it.
fn follow_moves(
    store: &MailStore,
    boxes: &[LocalMailbox],
    remote: &[RemoteFolder],
) -> Result<u64, SyncError> {
    // Where each message is listed: its id -> the folders that list it, in the order of `boxes`.
    let mut homes: HashMap<&str, Vec<usize>> = HashMap::new();
    for (at, mailbox) in boxes.iter().enumerate() {
        for object in listing_of(remote, mailbox) {
            if let Some(id) = azlin::message_id(&object.key) {
                homes.entry(id).or_default().push(at);
            }
        }
    }
    let mut followed = 0;
    for (from, mailbox) in boxes.iter().enumerate() {
        let on_drive: HashSet<&str> = listing_of(remote, mailbox)
            .iter()
            .map(|object| object.key.as_str())
            .collect();
        let mut index = read_index(store, &mailbox.key)?;
        // The lines whose objects this folder no longer lists, by the folder that lists the message now.
        let mut leaving: BTreeMap<usize, Vec<u32>> = BTreeMap::new();
        for entry in index.values() {
            if entry.remote.is_empty() || on_drive.contains(entry.remote.as_str()) {
                continue;
            }
            let Some(id) = azlin::message_id(&entry.remote) else {
                continue;
            };
            let elsewhere = homes
                .get(id)
                .and_then(|listing| listing.iter().find(|at| **at != from));
            if let Some(to) = elsewhere {
                leaving.entry(*to).or_default().push(entry.uid);
            }
        }
        for (to, uids) in leaving {
            let Some(target) = boxes.get(to) else {
                continue;
            };
            followed += follow_into(
                store,
                target,
                listing_of(remote, target),
                mailbox,
                &mut index,
                &uids,
            )?;
        }
    }
    Ok(followed)
}

/// The drive's listing of `mailbox`'s folder: nothing for a folder the drive does not list.
fn listing_of<'a>(remote: &'a [RemoteFolder], mailbox: &LocalMailbox) -> &'a [ObjectInfo] {
    remote
        .iter()
        .find(|folder| folder.path == mailbox.server_name)
        .map(|folder| folder.messages.as_slice())
        .unwrap_or(&[])
}

/// Moves the lines `uids` of `source_index` (the index of the folder `source`) into the folder
/// `target`, whose drive listing is `listed` ([`follow_moves`]). The order, every step a crash cell:
/// (1) the lines in `target`'s index, its state's last UID and the marks made here, durable BEFORE any
/// file moves; (2) each file into `target`'s tree (one rename that replaces nothing; a line whose file is
/// not here - a header block only - has nothing to move); (3) the lines out of `source`'s index and
/// marks. A stop after (1) leaves the line in both folders, a stop after (2) the file in its new place
/// under a line of the old folder: the next Send/Receive finds the line in `target` already, takes the
/// old one out and moves the file if it is still where it was - never a second line, never a lost
/// file, never a download. Returns how many lines it moved.
fn follow_into(
    store: &MailStore,
    target: &LocalMailbox,
    listed: &[ObjectInfo],
    source: &LocalMailbox,
    source_index: &mut BTreeMap<u32, IndexEntry>,
    uids: &[u32],
) -> Result<u64, SyncError> {
    let mut into = read_index(store, &target.key)?;
    let mut state = read_state(store, &target.key)?.unwrap_or_else(|| {
        FolderState::create(&target.server_name, &target.display, AZLIN_UIDVALIDITY)
    });
    let mut into_marks = read_marks(store, &target.key)?;
    let into_marks_before = into_marks.clone();
    let mut from_marks = read_marks(store, &source.key)?;
    let from_marks_before = from_marks.clone();
    // (the line's UID in the folder it leaves, the file where it was, the file where it goes).
    let mut plan: Vec<(u32, String, String)> = Vec::new();
    for &uid in uids {
        let Some(line) = source_index.get(&uid) else {
            continue;
        };
        let Some(id) = azlin::message_id(&line.remote) else {
            continue;
        };
        let Some(object) = listed
            .iter()
            .find(|object| azlin::message_id(&object.key) == Some(id))
        else {
            continue;
        };
        // A line of the message is here already (a stop halfway): it is not made twice.
        let known = into
            .values()
            .find(|known| known.remote == object.key)
            .map(|known| (known.uid, known.path.clone()));
        let (new_uid, path) = match known {
            Some(known) => known,
            None => {
                let new_uid = next_uid(&into, state.last_uid);
                let (year, month) = year_month_of(line);
                let path = store::message_key(&target.key, year, month, new_uid);
                let mut moved = line.clone();
                moved.uid = new_uid;
                moved.path.clone_from(&path);
                moved.remote.clone_from(&object.key);
                into.insert(new_uid, moved);
                state.last_uid = state.last_uid.max(new_uid);
                (new_uid, path)
            }
        };
        if let Some(on) = from_marks.read.remove(&uid) {
            into_marks.read.entry(new_uid).or_insert(on);
        }
        if let Some(on) = from_marks.flagged.remove(&uid) {
            into_marks.flagged.entry(new_uid).or_insert(on);
        }
        plan.push((uid, line.path.clone(), path));
    }
    if plan.is_empty() {
        return Ok(0);
    }
    // (1) The lines are in their new folder before any file goes there.
    write_index(store, &target.key, &into).map_err(io)?;
    state.messages = into.len() as u64;
    store
        .put(&store::state_key(&target.key), state.to_json().as_bytes())
        .map_err(io)?;
    if into_marks != into_marks_before {
        write_marks(store, &target.key, &into_marks).map_err(io)?;
    }
    cut(Cut::Written)?;
    // (2) The files. A name that is taken is not replaced: a copy is there already.
    for (_, from, to) in &plan {
        if store.size_of(to).is_some() {
            continue;
        }
        match store.move_object(from, to) {
            Ok(()) => {}
            // Nothing to move: a header block only, or moved by a pass that stopped after this.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io(e)),
        }
    }
    cut(Cut::Moved)?;
    // (3) The lines leave the folder they were in.
    for (uid, _, _) in &plan {
        source_index.remove(uid);
    }
    write_index(store, &source.key, source_index).map_err(io)?;
    if from_marks != from_marks_before {
        write_marks(store, &source.key, &from_marks).map_err(io)?;
    }
    Ok(plan.len() as u64)
}

// ==== The drive's folders ====

// Listing the drive's mailbox is azul-mail-core's (`azlin.rs`): the Azlin Bridge lists it the
// same way.
pub use crate::azlin::{list_mailbox, list_states, local_folders, RemoteFolder};

// ==== Send/Receive ====

/// Syncs the drive's mailbox and this computer's copy in `store` (see the module
/// documentation). `progress` is told what is happening; when it answers `false` the sync
/// records what it has and stops with [`SyncError::Stopped`].
///
/// An announced restore of the drive that this copy has not taken - the session's
/// ([`AzlinOptions::restore`]) or, asked when a folder has mail the drive no longer lists, the
/// token server's now (`ask`) - makes the rest of the call a restore's: no mail leaves this
/// computer, what the drive lacks goes into it again, and the epoch is remembered only after the
/// last folder was synced. An ask that fails leaves that mail where it is.
pub fn sync_account(
    drive: &dyn Drive,
    store: &MailStore,
    options: &AzlinOptions,
    ask: &dyn AskRestore,
    progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<SyncReport, SyncError> {
    // Before anything is listed or written: a memory of the drive that cannot be read is an
    // error, never "no restore taken".
    let mut pass = Pass::new(read_restore_epoch(store)?, options.restore.as_ref(), ask);
    let remote = list_mailbox(drive).map_err(drive_error)?;
    // What the clock of this computer is set against, read before this pass writes a stamp.
    // The floor is read strictly: one that cannot be read refuses every hold of this Send/Receive and sweeps nothing,
    // and the Send/Receive goes on (X-DELTA-APPS5-6; a damaged state is made again by the folder's own write).
    pass.set_clocks(Clocks::of(store, drive_reading(&remote)), options.now);
    let paths: Vec<String> = remote.iter().map(|folder| folder.path.clone()).collect();
    let mut boxes = local_folders(&paths);
    // A folder this computer has that the drive no longer lists (emptied or deleted there): an
    // empty listing, so its messages leave too.
    for key in store.folders() {
        if boxes.iter().any(|mailbox| mailbox.key == key) {
            continue;
        }
        let Some(state) = read_state(store, &key)? else {
            continue;
        };
        boxes.push(LocalMailbox {
            server_name: state.server_name,
            display: state.display,
            role: Role::of_key(&key),
            key,
        });
    }
    // Mail another device moved to another folder, or a folder it renamed, is still in the drive: its
    // line follows it before any folder is pulled (X-DELTA-APPS4-4).
    follow_moves(store, &boxes, &remote)?;
    let mut states = list_states(drive).map_err(drive_error)?;
    let count = boxes.len();
    let mut report = SyncReport::default();
    for (index, mailbox) in boxes.iter().enumerate() {
        if !progress(Progress::Folder {
            index,
            count,
            display: mailbox.display.clone(),
        }) {
            return Err(SyncError::Stopped);
        }
        let listing = remote
            .iter()
            .find(|folder| folder.path == mailbox.server_name);
        let listed: &[ObjectInfo] = listing
            .map(|folder| folder.messages.as_slice())
            .unwrap_or(&[]);
        report.folders.push(sync_folder(
            drive,
            store,
            mailbox,
            listed,
            listing.is_some(),
            &mut pass,
            &mut states,
            options,
            progress,
        )?);
    }
    // Every folder is through: the restore is taken (a pass that stopped before this took it
    // again, which is idempotent: what was put back is listed now).
    if let Some(notice) = pass.restore_taken() {
        write_restore_epoch(store, notice.epoch).map_err(io)?;
    }
    report.restore_unknown = pass.unknown().map(str::to_string);
    // A clock that could not date a hold left lines where they are: said once, with the reason.
    if report.kept_undated() > 0 {
        report.hold_refused = pass.hold.as_ref().err().map(ToString::to_string);
    }
    // Housekeeping, after the mail is synced: what was held [`HOLD_DAYS`] days ago goes. A sweep
    // that cannot read its list deletes nothing and never fails the sync; the report says so.
    report.sweep_failed = sweep_removed(store, options.now, pass.clocks.sweep_reading())
        .err()
        .map(|e| e.to_string());
    Ok(report)
}

/// One Send/Receive of the drive with `session`'s credentials: [`sync_account`] with the options
/// the session makes ([`AzlinOptions::for_session`]). The one door the app's sync thread goes
/// through, so a test enters it with a session (X-DELTA-APPS3-10).
pub fn sync_session(
    drive: &dyn Drive,
    store: &MailStore,
    session: &AzlinSession,
    now: i64,
    ask: &dyn AskRestore,
    progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<SyncReport, SyncError> {
    sync_account(
        drive,
        store,
        &AzlinOptions::for_session(session, now),
        ask,
        progress,
    )
}

/// Syncs one folder: push, pull, flags (the module documentation). `in_drive`: the drive lists
/// the folder. `pass`: what this Send/Receive knows of a restore of the drive - it decides, once
/// the folder has something the drive no longer lists, what the pull does with it ([`Stance`]).
#[allow(clippy::too_many_arguments)]
fn sync_folder(
    drive: &dyn Drive,
    store: &MailStore,
    mailbox: &LocalMailbox,
    listed: &[ObjectInfo],
    in_drive: bool,
    pass: &mut Pass<'_>,
    states: &mut HashMap<String, MessageState>,
    options: &AzlinOptions,
    progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<FolderReport, SyncError> {
    let key = mailbox.key.as_str();
    let path = mailbox.server_name.as_str();
    // Every object the drive listed, hidden ones too.
    let on_drive: HashSet<&str> = listed.iter().map(|object| object.key.as_str()).collect();
    // A message another program marked deleted (IMAP's \Deleted through the Azlin Bridge, not
    // expunged yet) is hidden here as if it were gone, until the mark goes or the message does.
    let visible: Vec<ObjectInfo> = listed
        .iter()
        .filter(|object| {
            azlin::message_id(&object.key)
                .and_then(|id| states.get(id))
                .is_none_or(|state| !state.deleted)
        })
        .cloned()
        .collect();
    let listed = visible.as_slice();
    let mut index = read_index(store, key)?;
    let mut state = FolderState::create(path, &mailbox.display, AZLIN_UIDVALIDITY);
    let before = read_state(store, key)?;
    state.last_uid = before.as_ref().map_or(0, |old| old.last_uid);
    // What this folder's file said of the last Send/Receive: it stays when this one has no clock to record.
    let synced_before = before.map(|old| old.synced_at).unwrap_or_default();
    let mut marks = read_marks(store, key)?;
    let marks_before = marks.clone();
    // What this folder does with what the drive no longer lists, decided BEFORE anything is put or
    // removed: when a line of the index names an object the listing lacks, the token server is
    // asked (once per Send/Receive) whether the drive was restored (X-DELTA-APPS3-2).
    // A message that is in the drive but hidden by another program's mark is not gone: it stays.
    let stance = pass.stance(
        index
            .values()
            .any(|entry| !entry.remote.is_empty() && !on_drive.contains(entry.remote.as_str())),
    );
    let mut report = FolderReport {
        key: key.to_string(),
        display: mailbox.display.clone(),
        ..FolderReport::default()
    };
    // The restore this folder takes is written into the memory before its first put (a memory that
    // cannot be written ends the call, with nothing put).
    let as_of = pass.restore_taken().and_then(|notice| notice.as_of);
    if stance == Stance::Restore {
        pass.record_taking(store)?;
    }
    // This computer's clock for what it first sees in the drive now; none when the clock is not set.
    let seen_now = (options.now > 0).then_some(options.now);

    let result = (|| -> Result<(), SyncError> {
        // 1. Push: mail filed here first goes into the folder (whatever the listing said, it
        //    is there now). After a restore also the mail the restored drive lacks: it came
        //    after the restore's moment and exists here only (its line keeps the old object's
        //    key until the new one is in the drive).
        let mut pushed: HashSet<String> = HashSet::new();
        let lost: HashSet<u32> = if stance == Stance::Restore {
            index
                .values()
                .filter(|entry| {
                    !entry.remote.is_empty()
                        && !on_drive.contains(entry.remote.as_str())
                        && restore_lost(entry, as_of)
                })
                .map(|entry| entry.uid)
                .collect()
        } else {
            HashSet::new()
        };
        let local_only: Vec<(u32, String, u64, Vec<String>)> = index
            .values()
            .filter(|entry| entry.remote.is_empty() || lost.contains(&entry.uid))
            .map(|entry| {
                (
                    entry.uid,
                    entry.path.clone(),
                    arrival_of(entry, options.now),
                    entry.flags.clone(),
                )
            })
            .collect();
        for (uid, file, arrival, flags) in local_only {
            let bytes = match store.get(&file) {
                Ok(bytes) => bytes,
                // A line the restore lost whose file is not here (a big message Send/Receive
                // fetched the header block of) has nothing to put back: it stays as it is, said
                // apart from the mail that goes back - it is not "put back" (X-DELTA-APPS3-12).
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && lost.contains(&uid) => {
                    report.header_only_after_restore += 1;
                    continue;
                }
                // A line whose file is gone holds nothing to put: it goes.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    index.remove(&uid);
                    continue;
                }
                Err(e) => return Err(io(e)),
            };
            let object = azlin::message_key(path, &azlin::object_name(&bytes, arrival));
            drive.put(&object, &bytes).map_err(drive_error)?;
            // Its flags go with it (a sent mail and a draft are read): as markers.
            if let Some(id) = azlin::message_id(&object) {
                let written = write_flag_markers(drive, id, &flags)?;
                let state = states.entry(id.to_string()).or_default();
                state.seen |= written.seen;
                state.flagged |= written.flagged;
                state.answered |= written.answered;
            }
            if let Some(entry) = index.get_mut(&uid) {
                entry.remote = object.clone();
                entry.on_drive_since = seen_now;
            }
            pushed.insert(object);
            report.pushed += 1;
            if lost.contains(&uid) {
                report.kept_after_restore += 1;
            }
        }

        // 2. Pull: what is gone leaves, what is new comes. What the drive still has but this window
        //    hides - a message another program marked deleted and has not expunged - is not gone: its
        //    line stays, hidden ([`HIDDEN_FLAG`]), and is neither held nor counted as removed
        //    (X-DELTA-APPS4-4).
        let gone: Vec<u32> = index
            .values()
            .filter(|entry| {
                !entry.remote.is_empty()
                    && !on_drive.contains(entry.remote.as_str())
                    && !pushed.contains(&entry.remote)
            })
            .map(|entry| entry.uid)
            .collect();
        let mut leaving: Vec<IndexEntry> = Vec::new();
        for uid in gone {
            match stance {
                // After a restore the mail it lost does not leave: what the listing lacks was lost,
                // not deleted, and exists here only (X-DELTA-SYNC4-20). What was deleted before the
                // restore's moment is an ordinary delete (X-DELTA-APPS3-13).
                Stance::Restore if lost.contains(&uid) => continue,
                Stance::Restore => {}
                // A drive that could not be asked whether it was restored: the mail stays, and
                // is not put back either - the next Send/Receive that can ask decides
                // (X-DELTA-APPS3-2).
                Stance::Keep => {
                    report.kept_unasked += 1;
                    continue;
                }
                Stance::Ordinary => {}
            }
            let Some(entry) = index.get(&uid) else {
                continue;
            };
            // A clock that cannot date a hold leaves the line where it is - no look at the drive, nothing moved,
            // and the Send/Receive goes on: new mail still comes, the later folders are synced and the sweep
            // runs (X-DELTA-APPS5-1; it ended the whole Receive, for every folder after this one).
            if pass.hold.is_err() {
                report.kept_undated += 1;
                continue;
            }
            // The drive is asked about the object itself: a listing that is short for any reason
            // must not take out a copy that may exist nowhere else, and a failed look is an unknown,
            // not an "absent" (the pass ends, nothing more leaves).
            if ops::exists(drive, &entry.remote).map_err(drive_error)? {
                continue;
            }
            leaving.push(entry.clone());
        }
        // What the drive no longer lists is held here, never deleted (X-DELTA-APPS3-3).
        if let Ok(at) = &pass.hold {
            hold(store, key, &leaving, *at, &mut index, &mut report)?;
        }
        let known: HashSet<String> = index.values().map(|entry| entry.remote.clone()).collect();
        let mut new: Vec<&ObjectInfo> = listed
            .iter()
            .filter(|object| !known.contains(&object.key))
            .collect();
        new.sort_by(|a, b| a.key.cmp(&b.key));
        let total = new.len() as u64;
        if total > 0
            && !progress(Progress::Messages {
                display: mailbox.display.clone(),
                done: 0,
                total,
            })
        {
            return Err(SyncError::Stopped);
        }
        let mut done = 0u64;
        for object in new {
            let uid = next_uid(&index, state.last_uid);
            let id = azlin::message_id(&object.key).unwrap_or_default();
            let stamp = azlin::stamp_of(id)
                .or(object.modified)
                .unwrap_or_else(|| u64::try_from(options.now).unwrap_or(0));
            let (year, month) = message::year_month(i64::try_from(stamp).unwrap_or(0));
            let file = store::message_key(key, year, month, uid);
            let whole = object.size <= options.full_fetch_limit;
            let fetched = if whole {
                drive.get(&object.key)
            } else {
                let last = options.head_bytes.max(1) - 1;
                drive.get_range(&object.key, ByteRange::new(0, Some(last)))
            };
            let bytes = match fetched {
                Ok(bytes) => bytes,
                // Moved by another device since the listing: the next Send/Receive finds it
                // where it is now.
                Err(DriveError::NotFound { .. }) => continue,
                Err(e) => return Err(drive_error(e)),
            };
            if whole {
                store.put(&file, &bytes).map_err(io)?;
            }
            let flags = states
                .get(id)
                .map(MessageState::imap_flags)
                .unwrap_or_default();
            let mut entry =
                message::index_entry(uid, &bytes, &flags, i64::try_from(stamp).ok(), &file);
            entry.size = object.size;
            entry.remote = object.key.clone();
            entry.on_drive_since = seen_now;
            index.insert(uid, entry);
            state.last_uid = state.last_uid.max(uid);
            report.fetched += 1;
            done += 1;
            if !progress(Progress::Messages {
                display: mailbox.display.clone(),
                done,
                total,
            }) {
                return Err(SyncError::Stopped);
            }
        }

        // 3. Flags: the marks made here first, then the drive's markers for every message.
        push_marks_of(drive, &index, &mut marks, states)?;
        for entry in index.values_mut() {
            let Some(id) = azlin::message_id(&entry.remote) else {
                continue;
            };
            let marked = states.get(id);
            let mut flags = marked.map(MessageState::imap_flags).unwrap_or_default();
            if marked.is_some_and(|state| state.deleted) {
                flags.push(HIDDEN_FLAG.to_string());
            }
            entry.flags = flags;
            // The first pass after the field existed gives every line it sees in the drive its stamp:
            // one conservative restore pass for the lines from before it, never a second one.
            if entry.on_drive_since.is_none() && on_drive.contains(entry.remote.as_str()) {
                entry.on_drive_since = seen_now;
            }
        }
        Ok(())
    })();

    // However it ended, what was written is recorded: the next Send/Receive picks up here.
    let recorded = (|| -> Result<(), SyncError> {
        let well_known = azlin::well_known_name(mailbox.role).is_some();
        if !in_drive && !well_known && index.is_empty() {
            // A folder of the user's that the drive no longer has: gone here too.
            for file in [
                store::index_key(key),
                store::state_key(key),
                listing::flags_key(key),
            ] {
                store.delete(&file).map_err(io)?;
            }
            return Ok(());
        }
        write_index(store, key, &index).map_err(io)?;
        state.messages = index.len() as u64;
        // The clock of this computer when it could date a hold; a clock that was refused is not recorded - the file
        // keeps what it said, none for a folder that never synced (X-DELTA-APPS5-1).
        state.synced_at = match pass.clocks.recorded(options.now) {
            Some(at) => message::rfc3339_utc(i64::try_from(at).unwrap_or(i64::MAX)),
            None => synced_before.clone(),
        };
        store
            .put(&store::state_key(key), state.to_json().as_bytes())
            .map_err(io)?;
        if marks != marks_before {
            write_marks(store, key, &marks).map_err(io)?;
        }
        Ok(())
    })();
    result?;
    recorded?;
    report.messages = index.len() as u64;
    Ok(report)
}

/// Writes the markers of the IMAP flags `flags` (`\Seen`, `\Flagged`, `\Answered`) a message
/// filed here carries, for its id `id` in the drive; returns what they say.
fn write_flag_markers(
    drive: &dyn Drive,
    id: &str,
    flags: &[String],
) -> Result<MessageState, SyncError> {
    let has = |flag: &str| flags.iter().any(|f| f.eq_ignore_ascii_case(flag));
    let state = MessageState {
        seen: has("\\Seen"),
        flagged: has("\\Flagged"),
        answered: has("\\Answered"),
        ..MessageState::default()
    };
    for (marker, on) in [
        (azlin::SEEN, state.seen),
        (azlin::FLAGGED, state.flagged),
        (azlin::ANSWERED, state.answered),
    ] {
        if on {
            set_marker(drive, id, marker, true)?;
        }
    }
    Ok(state)
}

/// Writes the marker `flag` of the message `id` (`on`) or removes it.
pub fn set_marker(drive: &dyn Drive, id: &str, flag: &str, on: bool) -> Result<(), SyncError> {
    let key = azlin::marker_key(id, flag);
    let written = if on {
        drive.put(&key, &[])
    } else {
        drive.delete(&key)
    };
    written.map_err(drive_error)
}

/// Writes the marks made here (`marks`, read and flagged by UID) as markers where the drive's
/// `states` say otherwise, and drops each mark that is in the drive now (or whose message is
/// gone); a mark on mail not in the drive yet waits.
fn push_marks_of(
    drive: &dyn Drive,
    index: &BTreeMap<u32, IndexEntry>,
    marks: &mut LocalFlags,
    states: &mut HashMap<String, MessageState>,
) -> Result<(), SyncError> {
    for flag in [azlin::SEEN, azlin::FLAGGED] {
        let wanted: Vec<(u32, bool)> = if flag == azlin::SEEN {
            marks.read.iter().map(|(uid, on)| (*uid, *on)).collect()
        } else {
            marks.flagged.iter().map(|(uid, on)| (*uid, *on)).collect()
        };
        for (uid, on) in wanted {
            let id = match index.get(&uid).map(|entry| entry.remote.as_str()) {
                // Not in the drive yet: Send/Receive puts it there first.
                Some("") => continue,
                Some(remote) => azlin::message_id(remote),
                None => None,
            };
            if let Some(id) = id {
                let state = states.entry(id.to_string()).or_default();
                let is = if flag == azlin::SEEN {
                    &mut state.seen
                } else {
                    &mut state.flagged
                };
                if *is != on {
                    set_marker(drive, id, flag, on)?;
                    *is = on;
                }
            }
            if flag == azlin::SEEN {
                marks.read.remove(&uid);
            } else {
                marks.flagged.remove(&uid);
            }
        }
    }
    Ok(())
}

// ==== Actions ====

/// The marks made here in the local folder `folder` (`flags.json`) written into the drive now -
/// an action's push, at once after the mark; Send/Receive does the same on its way. Each mark is
/// written as it is (a PUT or DELETE says the same twice), the index takes it as the drive's
/// flag, and the mark is dropped. Returns how many were written.
pub fn push_marks(drive: &dyn Drive, store: &MailStore, folder: &str) -> Result<usize, SyncError> {
    let mut marks = read_marks(store, folder)?;
    if marks.read.is_empty() && marks.flagged.is_empty() {
        return Ok(0);
    }
    let mut index = read_index(store, folder)?;
    let mut written = 0usize;
    let result = (|| -> Result<(), SyncError> {
        for (flag, imap) in [(azlin::SEEN, "\\Seen"), (azlin::FLAGGED, "\\Flagged")] {
            let wanted: Vec<(u32, bool)> = if flag == azlin::SEEN {
                marks.read.iter().map(|(uid, on)| (*uid, *on)).collect()
            } else {
                marks.flagged.iter().map(|(uid, on)| (*uid, *on)).collect()
            };
            for (uid, on) in wanted {
                if let Some(entry) = index.get_mut(&uid) {
                    if entry.remote.is_empty() {
                        // Not in the drive yet: the mark waits for Send/Receive.
                        continue;
                    }
                    if let Some(id) = azlin::message_id(&entry.remote) {
                        set_marker(drive, id, flag, on)?;
                        written += 1;
                    }
                    set_flag(&mut entry.flags, imap, on);
                }
                if flag == azlin::SEEN {
                    marks.read.remove(&uid);
                } else {
                    marks.flagged.remove(&uid);
                }
            }
        }
        Ok(())
    })();
    write_index(store, folder, &index).map_err(io)?;
    write_marks(store, folder, &marks).map_err(io)?;
    result.map(|()| written)
}

/// The name the sidebar gives a folder AzMail makes: its role's, else its path.
fn folder_display(key: &str, path: &str) -> String {
    Role::of_key(key)
        .label()
        .map_or_else(|| path.to_string(), str::to_string)
}

/// Moves the messages `uids` of the local folder `from` into the local folder `to` (Archive,
/// Junk, Move to, Delete outside Trash): each object is copied to the same name in `to`'s
/// folder of the drive and deleted where it was - its markers stay, they are filed by name -
/// then its file and its line follow here (a new UID in `to`, its marks made here with it).
/// Mail not in the drive yet moves here only; Send/Receive puts it into its new folder.
/// Returns how many moved.
pub fn move_messages(
    drive: &dyn Drive,
    store: &MailStore,
    from: &str,
    uids: &[u32],
    to: &str,
) -> Result<usize, SyncError> {
    if from == to || uids.is_empty() {
        return Ok(0);
    }
    let to_path = remote_path_of(store, to)?;
    let mut source = read_index(store, from)?;
    let mut target = read_index(store, to)?;
    let mut source_marks = read_marks(store, from)?;
    let mut target_marks = read_marks(store, to)?;
    let mut state = read_state(store, to)?.unwrap_or_else(|| {
        FolderState::create(&to_path, &folder_display(to, &to_path), AZLIN_UIDVALIDITY)
    });
    let mut moved = 0usize;
    let result = (|| -> Result<(), SyncError> {
        for uid in uids {
            let Some(entry) = source.get(uid).cloned() else {
                continue;
            };
            let remote = if entry.remote.is_empty() {
                String::new()
            } else {
                let name = entry.remote.rsplit('/').next().unwrap_or_default();
                let object = azlin::message_key(&to_path, name);
                if object != entry.remote {
                    drive.copy(&entry.remote, &object).map_err(drive_error)?;
                    drive.delete(&entry.remote).map_err(drive_error)?;
                }
                object
            };
            let new_uid = next_uid(&target, state.last_uid);
            let (year, month) = year_month_of(&entry);
            let file = store::message_key(to, year, month, new_uid);
            match store.get(&entry.path) {
                Ok(bytes) => {
                    store.put(&file, &bytes).map_err(io)?;
                    store.delete(&entry.path).map_err(io)?;
                }
                // A big message Send/Receive left in the drive has no file here yet.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io(e)),
            }
            source.remove(uid);
            if let Some(read) = source_marks.read.remove(uid) {
                target_marks.read.insert(new_uid, read);
            }
            if let Some(flagged) = source_marks.flagged.remove(uid) {
                target_marks.flagged.insert(new_uid, flagged);
            }
            target.insert(
                new_uid,
                IndexEntry {
                    uid: new_uid,
                    path: file,
                    remote,
                    ..entry
                },
            );
            state.last_uid = state.last_uid.max(new_uid);
            moved += 1;
        }
        Ok(())
    })();
    // What moved is recorded, whatever stopped the rest.
    write_index(store, from, &source).map_err(io)?;
    write_index(store, to, &target).map_err(io)?;
    state.messages = target.len() as u64;
    store
        .put(&store::state_key(to), state.to_json().as_bytes())
        .map_err(io)?;
    write_marks(store, from, &source_marks).map_err(io)?;
    write_marks(store, to, &target_marks).map_err(io)?;
    result.map(|()| moved)
}

/// Deletes the messages `uids` of the local folder `folder` for good (Delete in Trash): each
/// object, then its markers in the drive, then its file and its line here. Returns how many
/// went.
pub fn delete_messages(
    drive: &dyn Drive,
    store: &MailStore,
    folder: &str,
    uids: &[u32],
) -> Result<usize, SyncError> {
    let mut index = read_index(store, folder)?;
    let mut marks = read_marks(store, folder)?;
    let mut deleted = 0usize;
    let result = (|| -> Result<(), SyncError> {
        for uid in uids {
            let Some(entry) = index.get(uid).cloned() else {
                continue;
            };
            if let Some(id) = azlin::message_id(&entry.remote) {
                drive.delete(&entry.remote).map_err(drive_error)?;
                for marker in ops::list_all(drive, &azlin::state_prefix(id)).map_err(drive_error)? {
                    drive.delete(&marker.key).map_err(drive_error)?;
                }
            }
            store.delete(&entry.path).map_err(io)?;
            index.remove(uid);
            marks.read.remove(uid);
            marks.flagged.remove(uid);
            deleted += 1;
        }
        Ok(())
    })();
    write_index(store, folder, &index).map_err(io)?;
    write_marks(store, folder, &marks).map_err(io)?;
    result.map(|()| deleted)
}

/// Puts the local message `uid` of the local folder `folder` (a draft just saved) into the
/// drive under its name (arrival `now`) and records the key in its line. `replaces`, the object
/// of the draft it replaced, is deleted after it (a crash between leaves two drafts, never
/// none; a delete that fails leaves the old one, which shows as a second draft). Returns the
/// key.
pub fn upload_message(
    drive: &dyn Drive,
    store: &MailStore,
    folder: &str,
    uid: u32,
    replaces: Option<&str>,
    now: i64,
) -> Result<String, SyncError> {
    let path = remote_path_of(store, folder)?;
    let mut index = read_index(store, folder)?;
    let Some(entry) = index.get_mut(&uid) else {
        return Err(SyncError::Storage(format!(
            "the message {uid} is not in the index of {folder}"
        )));
    };
    let bytes = store.get(&entry.path).map_err(io)?;
    let stamp = u64::try_from(now).unwrap_or(0);
    let object = azlin::message_key(&path, &azlin::object_name(&bytes, stamp));
    drive.put(&object, &bytes).map_err(drive_error)?;
    // A draft is read: its markers say so in the drive too.
    if let Some(id) = azlin::message_id(&object) {
        write_flag_markers(drive, id, &entry.flags)?;
    }
    entry.remote = object.clone();
    write_index(store, folder, &index).map_err(io)?;
    if let Some(old) = replaces
        .map(str::trim)
        .filter(|old| !old.is_empty() && *old != object)
    {
        let _ = drive.delete(old);
    }
    Ok(object)
}

/// Downloads the message `uid` of the local folder `folder` whole into its file, in ranged GETs
/// of `chunk` bytes (no single request comes near the HTTP client's timeout): a message
/// Send/Receive left in the drive because it is bigger than its fetch limit. Returns the bytes.
pub fn fetch_message(
    drive: &dyn Drive,
    store: &MailStore,
    folder: &str,
    uid: u32,
    chunk: u64,
) -> Result<u64, SyncError> {
    let index = read_index(store, folder)?;
    let Some(entry) = index.get(&uid) else {
        return Err(SyncError::Storage(format!(
            "the message {uid} is not in the index of {folder}"
        )));
    };
    if entry.remote.is_empty() {
        return Err(SyncError::Storage(String::from(
            "the message is not in the drive",
        )));
    }
    let chunk = chunk.max(1);
    let size = entry.size;
    let bytes = if size <= chunk {
        drive.get(&entry.remote).map_err(drive_error)?
    } else {
        let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
        let mut offset = 0u64;
        while offset < size {
            let end = (offset + chunk).min(size) - 1;
            let part = drive
                .get_range(&entry.remote, ByteRange::new(offset, Some(end)))
                .map_err(drive_error)?;
            if part.is_empty() {
                return Err(SyncError::Protocol(format!(
                    "{} ended at byte {offset} of {size}",
                    entry.remote
                )));
            }
            offset += part.len() as u64;
            bytes.extend_from_slice(&part);
        }
        bytes
    };
    store.put(&entry.path, &bytes).map_err(io)?;
    Ok(bytes.len() as u64)
}

/// Why an Azlin account on a drive under `ban` sends nothing at `now` (ban contract v1): the
/// banner every Azlin app shows, and that its mail waits; `None` for a drive in good standing.
#[must_use]
pub fn sending_refused(
    ban: Option<&azcloud_kit::Ban>,
    now: u64,
) -> Option<azul_appkit::l10n::Text> {
    use azul_appkit::l10n::{Phrase, Text};
    let ban = ban?;
    if ban.is_closed(now) {
        return Some(ban.closed_phrase().into());
    }
    Some(
        Text::from(ban.banner_phrase(now))
            .then(" ")
            .then(Phrase::new("azmail-ban-sends-nothing")),
    )
}

/// Where the mail a pull took out of the local folder `key` is held, relative to the account's mail
/// folder: `removed/<key>/` (one folder for each pull in it, named by the moment of the pull).
fn held_in(key: &str) -> String {
    format!("{}/{key}/", store::REMOVED_PREFIX)
}

/// What the window says after a Send/Receive about mail the drive no longer lists, one sentence
/// each, none when nothing was kept: a restored drive that lacks mail this computer has (it stays
/// and goes back into the drive), and mail that stays because the token server could not be asked
/// whether the drive was restored.
#[must_use]
pub fn drive_sentences(report: &SyncReport) -> Vec<String> {
    use azul_appkit::l10n::{t_args, Arg};
    let mut sentences = Vec::new();
    if report.kept_after_restore() > 0 {
        sentences.push(t_args(
            "azmail-restore-kept",
            &[("count", Arg::from(report.kept_after_restore()))],
        ));
    }
    if report.header_only_after_restore() > 0 {
        sentences.push(t_args(
            "azmail-restore-header-only",
            &[("count", Arg::from(report.header_only_after_restore()))],
        ));
    }
    // One sentence for each folder that lost mail, naming the folder and where the held files are
    // (X-DELTA-APPS4-5): a user told that 8000 messages "were removed from the drive" must be able to
    // find them.
    for folder in report.folders.iter().filter(|folder| folder.removed > 0) {
        sentences.push(t_args(
            "azmail-removed-held",
            &[
                ("count", Arg::from(folder.removed)),
                ("folder", Arg::from(folder.display.as_str())),
                ("where", Arg::from(held_in(&folder.key))),
                ("days", Arg::from(HOLD_DAYS)),
            ],
        ));
    }
    // The clock could not date a hold (X-DELTA-APPS5-1): the lines stay in their folders and nothing was removed.
    if report.kept_undated() > 0 {
        sentences.push(t_args(
            "azmail-hold-refused",
            &[
                ("count", Arg::from(report.kept_undated())),
                (
                    "why",
                    Arg::from(report.hold_refused.as_deref().unwrap_or_default()),
                ),
                ("days", Arg::from(HOLD_DAYS)),
            ],
        ));
    }
    // The holding area could not be cleaned up this time: nothing was deleted, and it grows until it can.
    if let Some(why) = &report.sweep_failed {
        sentences.push(t_args(
            "azmail-removed-sweep-failed",
            &[("why", Arg::from(why.as_str()))],
        ));
    }
    if report.kept_unasked() > 0 {
        sentences.push(t_args(
            "azmail-restore-unknown",
            &[
                ("count", Arg::from(report.kept_unasked())),
                (
                    "why",
                    Arg::from(report.restore_unknown.as_deref().unwrap_or_default()),
                ),
            ],
        ));
    }
    // Incoming mail the drive keeps in its drops that this computer's key could not open this time.
    match report.drops_waiting {
        Some(crate::mail_drive::DropsWait::OtherKey) => {
            sentences.push(azul_appkit::l10n::t("azmail-drops-wait-other-key"));
        }
        Some(crate::mail_drive::DropsWait::KeyChangeUnfinished) => {
            sentences.push(azul_appkit::l10n::t("azmail-drops-wait-key-change"));
        }
        Some(crate::mail_drive::DropsWait::Damaged) => {
            sentences.push(azul_appkit::l10n::t("azmail-drops-wait-damaged"));
        }
        None => {}
    }
    sentences
}

#[cfg(test)]
mod tests {
    use azul_storage::{ListPage, ListRequest, LocalDrive};

    use super::*;
    use crate::testutil::{MailFolder, TempDir};

    /// "We always encrypt and compress": AzMail's default build writes an Azlin account's mail
    /// through the drive's encryption.
    #[test]
    fn a_default_build_of_azmail_writes_mail_through_the_encryption() {
        assert!(
            cfg!(feature = "encryption"),
            "AzMail's default features take `encryption`"
        );
    }

    /// 2026-09-30T08:42:00Z
    const SEP_30: i64 = 1_790_757_720;

    fn mail(n: u32, subject: &str) -> Vec<u8> {
        format!(
            "Message-ID: <m{n}@example.org>\r\nDate: Wed, 30 Sep 2026 10:42:00 +0200\r\n\
             From: Ben Okafor <ben@example.org>\r\nTo: ada@example.org\r\nSubject: {subject}\r\n\
             \r\nBody {n}\r\n"
        )
        .into_bytes()
    }

    /// A drive that records what was downloaded (`key`, or `key bytes=a-b` for a range).
    struct Counting {
        inner: LocalDrive,
        gets: Mutex<Vec<String>>,
    }

    impl Drive for Counting {
        fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
            self.inner.list(request)
        }
        fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
            self.gets.lock().unwrap().push(key.to_string());
            self.inner.get(key)
        }
        fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
            self.gets
                .lock()
                .unwrap()
                .push(format!("{key} {}", range.header_value()));
            self.inner.get_range(key, range)
        }
        fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
            self.inner.put(key, bytes)
        }
        fn delete(&self, key: &str) -> Result<(), DriveError> {
            self.inner.delete(key)
        }
        fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
            self.inner.head(key)
        }
        fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
            self.inner.copy(from, to)
        }
    }

    /// The drive (a folder on disk) and this computer's copy (another one).
    struct Fixture {
        bucket_dir: TempDir,
        _local_dir: TempDir,
        bucket: Counting,
        store: MailStore,
    }

    fn fixture() -> Fixture {
        let bucket_dir = TempDir::new("azlin-bucket");
        let local_dir = TempDir::new("azlin-local");
        let bucket = Counting {
            inner: LocalDrive::without_manifest(bucket_dir.0.clone()),
            gets: Mutex::new(Vec::new()),
        };
        let store = MailStore::new(local_dir.folder());
        Fixture {
            bucket_dir,
            _local_dir: local_dir,
            bucket,
            store,
        }
    }

    /// Puts `bytes` into the drive's `folder` as the Worker would; returns its key. The drive says
    /// the mail arrived at `stamp` (the file's date): the sweep sets the clock of this computer against
    /// what the drive says, not against the minute the test ran.
    fn deliver(f: &Fixture, folder: &str, bytes: &[u8], stamp: i64) -> String {
        let key = azlin::message_key(folder, &azlin::object_name(bytes, stamp as u64));
        f.bucket.inner.put(&key, bytes).unwrap();
        let file = std::fs::File::options()
            .write(true)
            .open(f.bucket_dir.0.join(&key))
            .unwrap();
        file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(stamp as u64))
            .unwrap();
        key
    }

    fn options() -> AzlinOptions {
        AzlinOptions {
            full_fetch_limit: 1 << 20,
            head_bytes: 64 * 1024,
            chunk: 8 * 1024 * 1024,
            now: SEP_30,
            restore: None,
        }
    }

    /// The token server says that no restore was announced (and counts nothing: the tests of the
    /// ask have their own askers).
    fn no_restore() -> impl AskRestore {
        || -> Result<Option<RestoreNotice>, SyncError> { Ok(None) }
    }

    fn run(f: &Fixture) -> SyncReport {
        sync_account(&f.bucket, &f.store, &options(), &no_restore(), &mut |_| {
            true
        })
        .unwrap()
    }

    fn index(store: &MailStore, folder: &str) -> Vec<IndexEntry> {
        read_index(store, folder).unwrap().into_values().collect()
    }

    fn missing(drive: &dyn Drive, key: &str) -> bool {
        matches!(drive.head(key), Err(DriveError::NotFound { .. }))
    }

    #[test]
    fn the_first_sync_takes_every_folder_of_the_drive_and_the_well_known_ones() {
        let f = fixture();
        let a = deliver(&f, "Inbox", &mail(1, "Garden plan"), SEP_30);
        let b = deliver(&f, "Inbox", &mail(2, "Bulbs"), SEP_30 + 60);
        let s = deliver(&f, "Spam", &mail(3, "You won"), SEP_30);
        let w = deliver(&f, "Work/Projects", &mail(4, "Kickoff"), SEP_30);
        let report = run(&f);
        assert_eq!(report.fetched(), 4);
        let inbox = index(&f.store, "inbox");
        let subjects: Vec<&str> = inbox.iter().map(|e| e.subject.as_str()).collect();
        assert_eq!(subjects, ["Garden plan", "Bulbs"], "UIDs in arrival order");
        assert_eq!((inbox[0].uid, inbox[0].remote.as_str()), (1, a.as_str()));
        assert_eq!(inbox[1].remote, b);
        assert_eq!(f.store.get(&inbox[0].path).unwrap(), mail(1, "Garden plan"));
        assert_eq!(index(&f.store, "spam")[0].remote, s);
        assert_eq!(index(&f.store, "Work.Projects")[0].remote, w);
        assert_eq!(
            f.store.folders(),
            [
                "Work",
                "Work.Projects",
                "archive",
                "drafts",
                "inbox",
                "sent",
                "spam",
                "trash"
            ]
        );
        let state = read_state(&f.store, "spam").unwrap().unwrap();
        assert_eq!(
            (state.server_name.as_str(), state.uidvalidity),
            ("Spam", AZLIN_UIDVALIDITY)
        );
        assert_eq!(
            read_state(&f.store, "Work.Projects")
                .unwrap()
                .unwrap()
                .display,
            "Work/Projects"
        );
    }

    #[test]
    fn a_second_sync_downloads_nothing_twice() {
        let f = fixture();
        deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        run(&f);
        let gets = f.bucket.gets.lock().unwrap().len();
        let again = run(&f);
        assert_eq!(again.fetched(), 0);
        assert_eq!(f.bucket.gets.lock().unwrap().len(), gets, "no GET at all");
        assert_eq!(index(&f.store, "inbox").len(), 1);
    }

    #[test]
    fn a_message_gone_from_the_drive_leaves_the_local_copy() {
        let f = fixture();
        let key = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        run(&f);
        let path = index(&f.store, "inbox")[0].path.clone();
        f.bucket.inner.delete(&key).unwrap();
        let report = run(&f);
        assert_eq!(report.removed(), 1);
        assert!(index(&f.store, "inbox").is_empty());
        assert_eq!(f.store.size_of(&path), None);
        assert!(
            !f.store.keys(store::REMOVED_PREFIX).is_empty(),
            "it is held, not deleted"
        );
    }

    #[test]
    fn a_message_another_program_marked_deleted_is_hidden_here_until_the_mark_goes() {
        let f = fixture();
        let kept = deliver(&f, "Inbox", &mail(1, "Kept"), SEP_30);
        let marked = deliver(&f, "Inbox", &mail(2, "Marked"), SEP_30 + 60);
        let marked_id = azlin::message_id(&marked).unwrap().to_string();
        // Apple Mail over the Azlin Bridge set \Deleted on it (and has not expunged yet).
        let deleted = format!("{}{marked_id}/deleted", azlin::STATE_PREFIX);
        f.bucket.inner.put(&deleted, &[]).unwrap();
        run(&f);
        let subjects: Vec<String> = index(&f.store, "inbox").into_iter().map(|e| e.subject).collect();
        assert_eq!(subjects, ["Kept"]);
        // Shown on this computer first, then marked: its line STAYS, flagged, and the window shows none that is
        // flagged (X-DELTA-APPS4-4: the message is still in the drive, so it is neither held nor counted as
        // removed - it left the local copy and was held for 90 days before).
        f.bucket.inner.delete(&deleted).unwrap();
        run(&f);
        assert_eq!(index(&f.store, "inbox").len(), 2);
        f.bucket.inner.put(&deleted, &[]).unwrap();
        let report = run(&f);
        assert_eq!(report.removed(), 0);
        let lines: Vec<(String, bool)> = index(&f.store, "inbox")
            .into_iter()
            .map(|e| (e.remote, e.flags.iter().any(|flag| flag == HIDDEN_FLAG)))
            .collect();
        assert_eq!(lines, [(kept, false), (marked.clone(), true)]);
        assert_eq!(
            crate::read_index(&f.store, "inbox").len(),
            1,
            "the window shows only the line that is not marked"
        );
        assert!(held_keys(&f).is_empty(), "nothing is held");
        assert!(!missing(&f.bucket, &marked), "hidden here, still in the drive");
    }

    #[test]
    fn mail_filed_here_first_goes_into_its_folder_of_the_drive() {
        let f = fixture();
        let flags = [String::from("\\Seen")];
        let filed = crate::send::file_message(
            f.store.folder(),
            "sent",
            &mail(9, "From here"),
            &flags,
            SEP_30,
        )
        .unwrap();
        assert!(filed.remote.is_empty());
        let report = run(&f);
        assert_eq!(report.pushed(), 1);
        let sent = index(&f.store, "sent");
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].uid, filed.uid, "it keeps its UID");
        assert!(
            sent[0].remote.starts_with("mail/Sent/20260930T084200Z-"),
            "{}",
            sent[0].remote
        );
        assert_eq!(
            f.bucket.inner.get(&sent[0].remote).unwrap(),
            mail(9, "From here")
        );
        let id = azlin::message_id(&sent[0].remote).unwrap().to_string();
        assert!(
            f.bucket
                .inner
                .head(&azlin::marker_key(&id, azlin::SEEN))
                .is_ok(),
            "read, in the drive too"
        );
        assert_eq!(sent[0].flags, ["\\Seen"], "a sent mail stays read");
        let again = run(&f);
        assert_eq!(
            (again.pushed(), again.fetched(), again.removed()),
            (0, 0, 0)
        );
        assert_eq!(index(&f.store, "sent").len(), 1, "no second copy");
    }

    #[test]
    fn a_big_message_comes_as_its_header_block_and_whole_when_opened() {
        let f = fixture();
        let mut big = mail(5, "Holiday photos");
        big.extend(std::iter::repeat(b'x').take(5000));
        let key = deliver(&f, "Inbox", &big, SEP_30);
        let small = AzlinOptions {
            full_fetch_limit: 1000,
            head_bytes: 512,
            ..options()
        };
        sync_account(&f.bucket, &f.store, &small, &no_restore(), &mut |_| true).unwrap();
        let entry = index(&f.store, "inbox").remove(0);
        assert_eq!(entry.subject, "Holiday photos");
        assert_eq!(entry.size, big.len() as u64, "the listing's size");
        assert_eq!(
            f.store.size_of(&entry.path),
            None,
            "only its header block came"
        );
        assert!(needs_fetch(&f.store, &entry));
        assert_eq!(
            *f.bucket.gets.lock().unwrap(),
            vec![format!("{key} bytes=0-511")]
        );
        let fetched = fetch_message(&f.bucket, &f.store, "inbox", entry.uid, 2048).unwrap();
        assert_eq!(fetched, big.len() as u64);
        assert_eq!(f.store.get(&entry.path).unwrap(), big);
        assert!(!needs_fetch(&f.store, &entry));
        let ranges = (big.len() as u64).div_ceil(2048) as usize;
        assert_eq!(
            f.bucket.gets.lock().unwrap().len(),
            1 + ranges,
            "ranges of 2048 bytes"
        );
    }

    #[test]
    fn marks_made_here_become_markers_and_then_the_drive_decides() {
        let f = fixture();
        deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        run(&f);
        let entry = index(&f.store, "inbox").remove(0);
        let id = azlin::message_id(&entry.remote).unwrap().to_string();
        let mut marks = LocalFlags::create();
        marks.read.insert(entry.uid, true);
        marks.flagged.insert(entry.uid, true);
        write_marks(&f.store, "inbox", &marks).unwrap();
        run(&f);
        assert!(f
            .bucket
            .inner
            .head(&azlin::marker_key(&id, azlin::SEEN))
            .is_ok());
        assert!(f
            .bucket
            .inner
            .head(&azlin::marker_key(&id, azlin::FLAGGED))
            .is_ok());
        assert_eq!(
            read_marks(&f.store, "inbox").unwrap(),
            LocalFlags::create(),
            "written, so not kept here"
        );
        assert_eq!(index(&f.store, "inbox")[0].flags, ["\\Seen", "\\Flagged"]);
        // Another device marks it unread: the drive decides.
        f.bucket
            .inner
            .delete(&azlin::marker_key(&id, azlin::SEEN))
            .unwrap();
        run(&f);
        assert_eq!(index(&f.store, "inbox")[0].flags, ["\\Flagged"]);
    }

    #[test]
    fn a_mark_is_pushed_at_once_and_the_index_follows() {
        let f = fixture();
        deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        run(&f);
        let entry = index(&f.store, "inbox").remove(0);
        let id = azlin::message_id(&entry.remote).unwrap().to_string();
        let mut marks = LocalFlags::create();
        marks.read.insert(entry.uid, true);
        write_marks(&f.store, "inbox", &marks).unwrap();
        assert_eq!(push_marks(&f.bucket, &f.store, "inbox").unwrap(), 1);
        assert!(f
            .bucket
            .inner
            .head(&azlin::marker_key(&id, azlin::SEEN))
            .is_ok());
        assert_eq!(index(&f.store, "inbox")[0].flags, ["\\Seen"]);
        assert!(read_marks(&f.store, "inbox").unwrap().read.is_empty());
        marks.read.insert(entry.uid, false);
        write_marks(&f.store, "inbox", &marks).unwrap();
        assert_eq!(push_marks(&f.bucket, &f.store, "inbox").unwrap(), 1);
        assert!(missing(
            &f.bucket.inner,
            &azlin::marker_key(&id, azlin::SEEN)
        ));
        assert!(index(&f.store, "inbox")[0].flags.is_empty());
        assert_eq!(
            push_marks(&f.bucket, &f.store, "inbox").unwrap(),
            0,
            "nothing left"
        );
    }

    #[test]
    fn a_move_keeps_the_name_and_the_markers_and_takes_the_local_copy_along() {
        let f = fixture();
        let key = deliver(&f, "Inbox", &mail(1, "Garden plan"), SEP_30);
        run(&f);
        let entry = index(&f.store, "inbox").remove(0);
        let id = azlin::message_id(&key).unwrap().to_string();
        f.bucket
            .inner
            .put(&azlin::marker_key(&id, azlin::SEEN), &[])
            .unwrap();
        let moved = move_messages(&f.bucket, &f.store, "inbox", &[entry.uid], "archive").unwrap();
        assert_eq!(moved, 1);
        let archived = format!("mail/Archive/{}", key.rsplit('/').next().unwrap());
        assert_eq!(
            f.bucket.inner.get(&archived).unwrap(),
            mail(1, "Garden plan")
        );
        assert!(missing(&f.bucket.inner, &key), "not in the Inbox any more");
        assert!(
            f.bucket
                .inner
                .head(&azlin::marker_key(&id, azlin::SEEN))
                .is_ok(),
            "the marker stays"
        );
        assert!(index(&f.store, "inbox").is_empty());
        let archive = index(&f.store, "archive");
        assert_eq!(archive.len(), 1);
        assert_eq!(archive[0].remote, archived);
        assert_eq!(
            f.store.get(&archive[0].path).unwrap(),
            mail(1, "Garden plan")
        );
        assert_eq!(f.store.size_of(&entry.path), None);
        let report = run(&f);
        assert_eq!(
            (report.fetched(), report.removed()),
            (0, 0),
            "Send/Receive agrees"
        );
        assert_eq!(index(&f.store, "archive")[0].flags, ["\\Seen"]);
    }

    #[test]
    fn deleting_in_trash_removes_the_object_and_its_markers() {
        let f = fixture();
        let key = deliver(&f, "Trash", &mail(1, "Old"), SEP_30);
        run(&f);
        let id = azlin::message_id(&key).unwrap().to_string();
        f.bucket
            .inner
            .put(&azlin::marker_key(&id, azlin::SEEN), &[])
            .unwrap();
        f.bucket
            .inner
            .put(&azlin::label_marker_key(&id, "Garden"), &[])
            .unwrap();
        let uid = index(&f.store, "trash")[0].uid;
        assert_eq!(
            delete_messages(&f.bucket, &f.store, "trash", &[uid]).unwrap(),
            1
        );
        assert!(missing(&f.bucket.inner, &key));
        assert!(ops::list_all(&f.bucket.inner, &azlin::state_prefix(&id))
            .unwrap()
            .is_empty());
        assert!(index(&f.store, "trash").is_empty());
    }

    #[test]
    fn a_saved_draft_goes_into_drafts_and_replaces_the_one_before() {
        let f = fixture();
        run(&f);
        let first =
            crate::compose::save_draft(f.store.folder(), None, &mail(1, "Draft one"), SEP_30)
                .unwrap();
        let key1 = upload_message(&f.bucket, &f.store, "drafts", first.uid, None, SEP_30).unwrap();
        assert!(key1.starts_with("mail/Drafts/20260930T084200Z-"), "{key1}");
        let second = crate::compose::save_draft(
            f.store.folder(),
            Some(first.uid),
            &mail(2, "Draft two"),
            SEP_30 + 5,
        )
        .unwrap();
        let key2 = upload_message(
            &f.bucket,
            &f.store,
            "drafts",
            second.uid,
            Some(&key1),
            SEP_30 + 5,
        )
        .unwrap();
        assert_ne!(key1, key2);
        assert!(missing(&f.bucket.inner, &key1), "the draft before is gone");
        assert_eq!(f.bucket.inner.get(&key2).unwrap(), mail(2, "Draft two"));
        let drafts = index(&f.store, "drafts");
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].remote, key2);
        let id = azlin::message_id(&key2).unwrap().to_string();
        assert!(f
            .bucket
            .inner
            .head(&azlin::marker_key(&id, azlin::SEEN))
            .is_ok());
        assert_eq!(remote_of(&f.store, "drafts", second.uid), Some(key2));
        assert_eq!(run(&f).fetched(), 0, "Send/Receive knows the draft");
        assert_eq!(index(&f.store, "drafts")[0].flags, ["\\Seen"], "a draft stays read");
    }

    #[test]
    fn the_drives_folders_get_the_roles_keys_and_names_an_imap_servers_would() {
        let paths: Vec<String> = ["Inbox", "Junk", "R&D", "Work/Projects"]
            .iter()
            .map(|p| p.to_string())
            .collect();
        let boxes = local_folders(&paths);
        let got: Vec<(&str, &str, Role)> = boxes
            .iter()
            .map(|m| (m.key.as_str(), m.server_name.as_str(), m.role))
            .collect();
        for want in [
            ("inbox", "Inbox", Role::Inbox),
            ("spam", "Junk", Role::Spam),
            ("R&D", "R&D", Role::Other),
            ("Work.Projects", "Work/Projects", Role::Other),
            ("sent", "Sent", Role::Sent),
            ("drafts", "Drafts", Role::Drafts),
            ("archive", "Archive", Role::Archive),
            ("trash", "Trash", Role::Trash),
        ] {
            assert!(got.contains(&want), "{want:?} in {got:?}");
        }
        assert_eq!(got.len(), 8, "Junk is the spam folder: no Spam besides it");
        let rd = boxes.iter().find(|m| m.key == "R&D").unwrap();
        assert_eq!(rd.display, "R&D");
        assert_eq!(
            remote_path_of(
                &MailStore::new(crate::store::DriveFolder::outside(
                    std::path::PathBuf::from("/nonexistent")
                )),
                "spam"
            )
            .unwrap(),
            "Spam"
        );
    }

    #[test]
    fn a_folder_the_drive_no_longer_has_leaves_with_its_messages() {
        let f = fixture();
        let key = deliver(&f, "Receipts", &mail(1, "Invoice"), SEP_30);
        run(&f);
        assert!(f.store.folders().contains(&String::from("Receipts")));
        f.bucket.inner.delete(&key).unwrap();
        // An S3 bucket lists no empty folder; the folder on disk keeps its directory.
        std::fs::remove_dir_all(f.bucket_dir.0.join("mail").join("Receipts")).unwrap();
        run(&f);
        assert!(!f.store.folders().contains(&String::from("Receipts")));
        assert!(
            f.store.folders().contains(&String::from("inbox")),
            "the well-known ones stay"
        );
    }

    #[test]
    fn a_refused_key_is_a_sign_in_problem_and_no_answer_a_connection_one() {
        let refused = DriveError::Service(azul_storage::ServiceError {
            status: 403,
            code: String::from("ExpiredToken"),
            ..azul_storage::ServiceError::default()
        });
        assert!(matches!(drive_error(refused), SyncError::Auth(_)));
        assert!(matches!(
            drive_error(DriveError::Transport(String::from("refused"))),
            SyncError::Connect(_)
        ));
        assert!(matches!(
            drive_error(DriveError::Protocol(String::from("?"))),
            SyncError::Protocol(_)
        ));
    }

    // ==== A banned drive (ban contract v1) ====

    #[test]
    fn an_azlin_account_on_a_banned_drive_sends_nothing_and_says_why() {
        let until = 1_791_799_200; // 2026-10-12T10:00:00Z
        let ban = azcloud_kit::Ban {
            reason: String::from("spam distribution"),
            until: Some(until),
            closed: false,
        };
        crate::l10n::in_english();
        let why = sending_refused(Some(&ban), until - 10 * 3_600).expect("sending stops");
        let why = azul_appkit::l10n::t_text(&why);
        assert!(
            why.starts_with(
                "Due to spam distribution, your account has been banned, but you have 10 hours \
                 to migrate your files."
            ),
            "the same banner as AzDrive's: {why}"
        );
        assert!(why.contains("AzMail sends nothing"), "{why}");
        let closed = azul_appkit::l10n::t_text(&sending_refused(Some(&ban), until + 1).expect("closed"));
        assert!(closed.starts_with("This drive was closed on 2026-10-12"), "{closed}");
        assert_eq!(sending_refused(None, until), None, "a drive in good standing sends");
    }

    // ==== X-DELTA-SYNC4-21: a read that failed is no empty file ====

    /// The folder's file `key` here as bytes (the oracle: a failed read must not rewrite it).
    fn bytes_of(f: &Fixture, key: &str) -> Vec<u8> {
        std::fs::read(f.store.folder().path().join(key)).unwrap()
    }

    /// RED on the code before: an index that could not be READ was an empty one, the folder was
    /// synced from the drive's listing alone and the index written back without the lines of the
    /// mail filed here first.
    #[test]
    fn x_delta_sync4_21_an_unreadable_index_ends_the_sync_and_rewrites_nothing() {
        let f = fixture();
        deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        let flags = [String::from("\\Seen")];
        crate::send::file_message(
            f.store.folder(),
            "sent",
            &mail(9, "From here"),
            &flags,
            SEP_30,
        )
        .unwrap();
        run(&f);
        // Another mail filed here first, not in the drive yet: its line exists in the index only.
        crate::send::file_message(
            f.store.folder(),
            "sent",
            &mail(10, "Second"),
            &flags,
            SEP_30 + 5,
        )
        .unwrap();
        let key = store::index_key("sent");
        let before = bytes_of(&f, &key);
        let file = f.store.folder().path().join(&key);
        crate::testutil::make_unreadable(&file);
        let failed = sync_account(&f.bucket, &f.store, &options(), &no_restore(), &mut |_| {
            true
        });
        crate::testutil::make_readable(&file);
        assert!(matches!(failed, Err(SyncError::Storage(_))), "{failed:?}");
        assert_eq!(
            bytes_of(&f, &key),
            before,
            "the index was not written back from nothing"
        );
    }

    #[test]
    fn x_delta_sync4_21_an_unreadable_state_ends_the_sync_and_rewrites_nothing() {
        let f = fixture();
        deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        run(&f);
        let key = store::state_key("inbox");
        let before = bytes_of(&f, &key);
        let file = f.store.folder().path().join(&key);
        crate::testutil::make_unreadable(&file);
        let failed = sync_account(&f.bucket, &f.store, &options(), &no_restore(), &mut |_| {
            true
        });
        crate::testutil::make_readable(&file);
        assert!(matches!(failed, Err(SyncError::Storage(_))), "{failed:?}");
        assert_eq!(
            bytes_of(&f, &key),
            before,
            "the last UID was not started from 0"
        );
    }

    /// RED on the code before: the move read the unreadable index as empty, found no message to
    /// move and wrote the empty index over the folder's.
    #[test]
    fn x_delta_sync4_21_a_move_with_an_unreadable_index_does_nothing() {
        let f = fixture();
        let key = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        run(&f);
        let entry = index(&f.store, "inbox").remove(0);
        let index_key = store::index_key("inbox");
        let before = bytes_of(&f, &index_key);
        let file = f.store.folder().path().join(&index_key);
        crate::testutil::make_unreadable(&file);
        let moved = move_messages(&f.bucket, &f.store, "inbox", &[entry.uid], "archive");
        crate::testutil::make_readable(&file);
        assert!(matches!(moved, Err(SyncError::Storage(_))), "{moved:?}");
        assert_eq!(
            bytes_of(&f, &index_key),
            before,
            "the index was not overwritten"
        );
        assert!(
            !missing(&f.bucket.inner, &key),
            "the object stays in the Inbox of the drive"
        );
    }

    /// The marks made here and not pushed yet are not lost to a read that failed: they stay.
    #[test]
    fn x_delta_sync4_21_unreadable_marks_end_the_action_and_stay() {
        let f = fixture();
        deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        run(&f);
        let entry = index(&f.store, "inbox").remove(0);
        let mut marks = LocalFlags::create();
        marks.read.insert(entry.uid, true);
        write_marks(&f.store, "inbox", &marks).unwrap();
        let key = listing::flags_key("inbox");
        let before = bytes_of(&f, &key);
        let file = f.store.folder().path().join(&key);
        crate::testutil::make_unreadable(&file);
        let failed = push_marks(&f.bucket, &f.store, "inbox");
        crate::testutil::make_readable(&file);
        assert!(matches!(failed, Err(SyncError::Storage(_))), "{failed:?}");
        assert_eq!(bytes_of(&f, &key), before, "the marks stay as they were");
    }

    // ==== X-DELTA-SYNC4-20: a restored drive, a short listing, a failed look ====

    /// A drive that fails where a test says. Its listing leaves out the keys of `unlisted` (the
    /// objects are there: `head` and `get` serve them), fails whole, or never ends; its `head`
    /// fails; its `put` fails after `puts_left` more puts. Everything else is the folder on disk.
    struct Faulty {
        inner: LocalDrive,
        unlisted: Mutex<Vec<String>>,
        list_fails: std::sync::atomic::AtomicBool,
        list_never_ends: std::sync::atomic::AtomicBool,
        head_fails: std::sync::atomic::AtomicBool,
        puts_left: Mutex<Option<u32>>,
    }

    impl Faulty {
        /// The drive of `f` (the same folder on disk), faultless until a test says.
        fn of(f: &Fixture) -> Faulty {
            Faulty {
                inner: LocalDrive::without_manifest(f.bucket_dir.0.clone()),
                unlisted: Mutex::new(Vec::new()),
                list_fails: false.into(),
                list_never_ends: false.into(),
                head_fails: false.into(),
                puts_left: Mutex::new(None),
            }
        }
    }

    impl Drive for Faulty {
        fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
            use std::sync::atomic::Ordering::SeqCst;
            if self.list_fails.load(SeqCst) {
                return Err(DriveError::Transport(String::from("the listing failed")));
            }
            if self.list_never_ends.load(SeqCst) {
                return Ok(ListPage {
                    next: Some(String::from("again")),
                    ..ListPage::default()
                });
            }
            let mut page = self.inner.list(request)?;
            let unlisted = self.unlisted.lock().unwrap();
            page.objects
                .retain(|object| !unlisted.contains(&object.key));
            Ok(page)
        }
        fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
            self.inner.get(key)
        }
        fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
            self.inner.get_range(key, range)
        }
        fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
            if let Some(left) = self.puts_left.lock().unwrap().as_mut() {
                if *left == 0 {
                    return Err(DriveError::Transport(String::from("the put failed")));
                }
                *left -= 1;
            }
            self.inner.put(key, bytes)
        }
        fn delete(&self, key: &str) -> Result<(), DriveError> {
            self.inner.delete(key)
        }
        fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
            if self.head_fails.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(DriveError::Transport(String::from("the look failed")));
            }
            self.inner.head(key)
        }
        fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
            self.inner.copy(from, to)
        }
    }

    /// `options()` with the drive's restore announced under `epoch`.
    fn announced(epoch: u64) -> AzlinOptions {
        AzlinOptions {
            restore: Some(RestoreNotice {
                epoch,
                ..RestoreNotice::default()
            }),
            ..options()
        }
    }

    fn sync_with(
        drive: &dyn Drive,
        f: &Fixture,
        options: &AzlinOptions,
    ) -> Result<SyncReport, SyncError> {
        sync_account(drive, &f.store, options, &no_restore(), &mut |_| true)
    }

    /// Every message of `folder` here: its line, its file's bytes, and whether the drive has the
    /// object its line names.
    fn held(f: &Fixture, folder: &str) -> Vec<(IndexEntry, Vec<u8>, bool)> {
        index(&f.store, folder)
            .into_iter()
            .map(|entry| {
                let bytes = f.store.get(&entry.path).unwrap_or_default();
                let on_the_drive =
                    !entry.remote.is_empty() && !missing(&f.bucket.inner, &entry.remote);
                (entry, bytes, on_the_drive)
            })
            .collect()
    }

    /// RED on the code before: the restored drive's listing lacked two of three mails and the
    /// Send/Receive deleted both here (`removed == 2`, an index of one line).
    #[test]
    fn x_delta_sync4_20_a_restore_that_lists_fewer_mails_keeps_every_local_mail_and_puts_it_back() {
        let f = fixture();
        deliver(&f, "Inbox", &mail(1, "Older"), SEP_30);
        let newer = deliver(&f, "Inbox", &mail(2, "Newer"), SEP_30 + 60);
        let archived = deliver(&f, "Archive", &mail(3, "Archived"), SEP_30 + 120);
        run(&f);
        assert_eq!(index(&f.store, "inbox").len(), 2);
        // The operator restored the drive from its off-site copy, which lags: two mails are
        // missing, the token server announced epoch 1.
        f.bucket.inner.delete(&newer).unwrap();
        f.bucket.inner.delete(&archived).unwrap();
        let report = sync_with(&f.bucket, &f, &announced(1)).unwrap();
        assert_eq!(report.removed(), 0, "a restore deletes no mail here");
        assert_eq!(report.kept_after_restore(), 2);
        assert_eq!(
            report.pushed(),
            2,
            "and puts what the drive lacks into it again"
        );
        let inbox = held(&f, "inbox");
        assert_eq!(inbox.len(), 2, "both lines stay");
        let subjects: Vec<&str> = inbox.iter().map(|(e, _, _)| e.subject.as_str()).collect();
        assert_eq!(subjects, ["Older", "Newer"]);
        assert_eq!(
            inbox[1].1,
            mail(2, "Newer"),
            "the file is the mail, not an empty one"
        );
        assert!(
            inbox.iter().all(|(_, _, on_the_drive)| *on_the_drive),
            "every line names an object of the drive"
        );
        let archive = held(&f, "archive");
        assert_eq!(archive.len(), 1);
        assert_eq!(
            (archive[0].1.clone(), archive[0].2),
            (mail(3, "Archived"), true)
        );
        assert!(
            archive[0].0.remote.starts_with("mail/Archive/"),
            "put back into its own folder of the drive: {}",
            archive[0].0.remote
        );
        assert_eq!(
            read_restore_epoch(&f.store).unwrap(),
            1,
            "the restore is taken once"
        );
        // The same announcement again is no restore: the drive is as it was left.
        let again = sync_with(&f.bucket, &f, &announced(1)).unwrap();
        assert_eq!(
            (
                again.removed(),
                again.pushed(),
                again.fetched(),
                again.kept_after_restore()
            ),
            (0, 0, 0, 0)
        );
        assert_eq!(index(&f.store, "inbox").len(), 2);
    }

    /// The restore is taken once; a delete made by another device afterwards is an ordinary
    /// delete again, and the next restore (a higher epoch) takes mail the same way.
    #[test]
    fn x_delta_sync4_20_after_the_restore_is_taken_a_delete_elsewhere_leaves_again() {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        let two = deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        run(&f);
        f.bucket.inner.delete(&two).unwrap();
        sync_with(&f.bucket, &f, &announced(1)).unwrap();
        assert_eq!(index(&f.store, "inbox").len(), 2, "the restore kept it");
        // Another device deletes `one` for good: the announcement is the same epoch 1.
        f.bucket.inner.delete(&one).unwrap();
        let report = sync_with(&f.bucket, &f, &announced(1)).unwrap();
        assert_eq!(report.removed(), 1, "an ordinary delete");
        assert_eq!(index(&f.store, "inbox").len(), 1);
        // A second restore (epoch 2) that lacks the remaining mail keeps it too.
        let remaining = index(&f.store, "inbox").remove(0);
        f.bucket.inner.delete(&remaining.remote).unwrap();
        let report = sync_with(&f.bucket, &f, &announced(2)).unwrap();
        assert_eq!((report.removed(), report.kept_after_restore()), (0, 1));
        assert_eq!(held(&f, "inbox").len(), 1);
        assert_eq!(read_restore_epoch(&f.store).unwrap(), 2);
    }

    /// The crash table of a restore's pass, every row by stopping the flow after a write: the
    /// first mail is put back and the second put fails (a cut line). Nothing leaves, the epoch is
    /// NOT remembered, and the next Send/Receive takes the restore again and finishes. RED on
    /// the code before: the second mail was deleted here before the first was put anywhere.
    #[test]
    fn x_delta_sync4_20_a_restore_pass_cut_after_the_first_put_takes_the_restore_again() {
        let f = fixture();
        let first = deliver(&f, "Inbox", &mail(1, "First"), SEP_30);
        let second = deliver(&f, "Inbox", &mail(2, "Second"), SEP_30 + 60);
        run(&f);
        f.bucket.inner.delete(&first).unwrap();
        f.bucket.inner.delete(&second).unwrap();
        let drive = Faulty::of(&f);
        *drive.puts_left.lock().unwrap() = Some(1);
        let stopped = sync_with(&drive, &f, &announced(1));
        assert!(matches!(stopped, Err(SyncError::Connect(_))), "{stopped:?}");
        // Row: after the first put and its line, before the second put.
        let rows = held(&f, "inbox");
        assert_eq!(rows.len(), 2, "no mail left");
        assert_eq!(rows[0].1, mail(1, "First"));
        assert_eq!(rows[1].1, mail(2, "Second"));
        assert_eq!(
            rows.iter()
                .filter(|(_, _, on_the_drive)| *on_the_drive)
                .count(),
            1,
            "one is back in the drive, one waits"
        );
        assert_eq!(
            read_restore_epoch(&f.store).unwrap(),
            0,
            "a stopped pass took nothing"
        );
        // The next Send/Receive, the drive answering now: the restore is taken again.
        *drive.puts_left.lock().unwrap() = None;
        let report = sync_with(&drive, &f, &announced(1)).unwrap();
        assert_eq!(report.removed(), 0);
        let rows = held(&f, "inbox");
        assert!(
            rows.iter().all(|(_, _, on_the_drive)| *on_the_drive),
            "both are back"
        );
        assert_eq!(read_restore_epoch(&f.store).unwrap(), 1);
    }

    /// A folder the restored drive has none of (it was made after the restore's moment): its
    /// mail stays, goes back into a folder of that name, and the folder's files stay too. RED on
    /// the code before: an empty listing, so every message left with the folder.
    #[test]
    fn x_delta_sync4_20_a_folder_the_restored_drive_lacks_keeps_its_mail() {
        let f = fixture();
        let key = deliver(&f, "Receipts", &mail(1, "Invoice"), SEP_30);
        run(&f);
        f.bucket.inner.delete(&key).unwrap();
        std::fs::remove_dir_all(f.bucket_dir.0.join("mail").join("Receipts")).unwrap();
        let report = sync_with(&f.bucket, &f, &announced(1)).unwrap();
        assert_eq!((report.removed(), report.kept_after_restore()), (0, 1));
        assert!(
            f.store.folders().contains(&String::from("Receipts")),
            "the folder stays"
        );
        let rows = held(&f, "Receipts");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, mail(1, "Invoice"));
        assert!(
            rows[0].2 && rows[0].0.remote.starts_with("mail/Receipts/"),
            "{:?}",
            rows[0].0.remote
        );
    }

    /// A message of which only the header block is here (a big one Send/Receive did not fetch)
    /// cannot be put back: its line stays as it is - never dropped for "no file", the way mail
    /// filed here without a body is.
    #[test]
    fn x_delta_sync4_20_a_header_only_line_stays_when_the_restored_drive_lacks_it() {
        let f = fixture();
        let mut big = mail(5, "Holiday photos");
        big.extend(std::iter::repeat_n(b'x', 5000));
        let key = deliver(&f, "Inbox", &big, SEP_30);
        let small = AzlinOptions {
            full_fetch_limit: 1000,
            head_bytes: 512,
            ..options()
        };
        sync_account(&f.bucket, &f.store, &small, &no_restore(), &mut |_| true).unwrap();
        f.bucket.inner.delete(&key).unwrap();
        let restored = AzlinOptions {
            restore: announced(1).restore,
            ..small
        };
        let report =
            sync_account(&f.bucket, &f.store, &restored, &no_restore(), &mut |_| true).unwrap();
        // Nothing of it goes back: it is no "put back" and is not counted as one, it is said apart
        // (X-DELTA-APPS3-12; it was counted in `kept_after_restore`, and the sentence said it
        // "goes back into the drive").
        assert_eq!(
            (
                report.removed(),
                report.pushed(),
                report.kept_after_restore(),
                report.header_only_after_restore()
            ),
            (0, 0, 0, 1)
        );
        crate::l10n::in_english();
        let said = drive_sentences(&report);
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(
            said[0].contains("only the header is here")
                && !said[0].contains("goes back into the drive"),
            "{said:?}"
        );
        let lines = index(&f.store, "inbox");
        assert_eq!(lines.len(), 1, "the line stays");
        assert_eq!(lines[0].remote, key, "and keeps the key it had");
        assert_eq!(lines[0].subject, "Holiday photos");
        // The next ordinary pass takes the line out like any message the drive does not list - its
        // line is held in the record, and the window says it was removed (not that it went back).
        let later = AzlinOptions {
            now: SEP_30 + 60,
            ..small
        };
        let report =
            sync_account(&f.bucket, &f.store, &later, &no_restore(), &mut |_| true).unwrap();
        assert_eq!(
            (report.removed(), report.header_only_after_restore()),
            (1, 0)
        );
        assert!(index(&f.store, "inbox").is_empty());
        assert!(
            !f.store.keys(store::REMOVED_PREFIX).is_empty(),
            "its line is in the record"
        );
    }

    /// The listing that FAILS deletes nothing: the call ends before any folder is read (this arm
    /// was safe before; the test pins it).
    #[test]
    fn x_delta_sync4_20_a_listing_that_fails_deletes_nothing() {
        let f = fixture();
        let key = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        run(&f);
        f.bucket.inner.delete(&key).unwrap();
        let drive = Faulty::of(&f);
        drive
            .list_fails
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let failed = sync_with(&drive, &f, &options());
        assert!(matches!(failed, Err(SyncError::Connect(_))), "{failed:?}");
        assert_eq!(
            held(&f, "inbox").len(),
            1,
            "the mail is still here, with its file"
        );
    }

    /// A listing that never ends is refused by the paging guard, not taken as a complete one
    /// (this arm was safe before; the test pins it).
    #[test]
    fn x_delta_sync4_20_a_listing_that_never_ends_deletes_nothing() {
        let f = fixture();
        deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        run(&f);
        let drive = Faulty::of(&f);
        drive
            .list_never_ends
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert!(sync_with(&drive, &f, &options()).is_err());
        assert_eq!(held(&f, "inbox").len(), 1);
    }

    /// A listing that is short for any reason: it omits an object the drive has. RED on the code
    /// before: the local copy was deleted on the listing's word alone.
    #[test]
    fn x_delta_sync4_20_a_listing_that_omits_an_object_the_drive_has_deletes_nothing() {
        let f = fixture();
        let key = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        run(&f);
        let drive = Faulty::of(&f);
        drive.unlisted.lock().unwrap().push(key.clone());
        let report = sync_with(&drive, &f, &options()).unwrap();
        assert_eq!(report.removed(), 0);
        let rows = held(&f, "inbox");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].0.remote.as_str(), rows[0].1.clone()),
            (key.as_str(), mail(1, "One"))
        );
    }

    /// The drive that cannot be asked about the object is an unknown, not an absent: the pass
    /// ends with the error and the mail stays. RED on the code before (no look at all).
    #[test]
    fn x_delta_sync4_20_a_failed_look_at_the_object_ends_the_pass_and_deletes_nothing() {
        let f = fixture();
        let key = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        run(&f);
        f.bucket.inner.delete(&key).unwrap();
        let drive = Faulty::of(&f);
        drive
            .head_fails
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let failed = sync_with(&drive, &f, &options());
        assert!(matches!(failed, Err(SyncError::Connect(_))), "{failed:?}");
        assert_eq!(held(&f, "inbox").len(), 1, "line and file stay");
        // The drive answers again and says the object is gone: now it leaves.
        let report = run(&f);
        assert_eq!(report.removed(), 1);
    }

    /// A memory of the drive that cannot be read is an unknown, never "no restore taken yet"
    /// (that would turn the next listing into a restore's: mail put back that a delete took).
    #[test]
    fn x_delta_sync4_20_a_drive_memory_that_cannot_be_read_ends_the_sync_before_it_lists() {
        let f = fixture();
        deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        run(&f);
        write_restore_epoch(&f.store, 0).unwrap();
        // No permission to read it: the read fails, and is no "not found" (skips where it cannot).
        let file = f.store.folder().path().join(DRIVE_FILE);
        crate::testutil::make_unreadable(&file);
        let drive = Faulty::of(&f);
        drive
            .list_fails
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let failed = sync_with(&drive, &f, &announced(1));
        crate::testutil::make_readable(&file);
        assert!(
            matches!(failed, Err(SyncError::Storage(_))),
            "ended on the memory, before the failing listing: {failed:?}"
        );
    }

    /// A memory file that is not this AzMail's reads as "no restore taken": the next restore is
    /// taken once more, which deletes nothing.
    #[test]
    fn x_delta_sync4_20_a_damaged_drive_memory_means_no_restore_was_taken() {
        let f = fixture();
        assert_eq!(read_restore_epoch(&f.store).unwrap(), 0, "no file");
        f.store.put(DRIVE_FILE, b"{ not json").unwrap();
        assert_eq!(read_restore_epoch(&f.store).unwrap(), 0);
        f.store
            .put(
                DRIVE_FILE,
                br#"{"format":"other","version":1,"restore_epoch":9}"#,
            )
            .unwrap();
        assert_eq!(read_restore_epoch(&f.store).unwrap(), 0);
        write_restore_epoch(&f.store, 4).unwrap();
        assert_eq!(read_restore_epoch(&f.store).unwrap(), 4);
    }

    // ==== X-DELTA-APPS3-2: the token server is asked before the pull lets go ====

    /// An asker that answers `answer` and counts its questions.
    struct Asked {
        answer: Result<Option<RestoreNotice>, SyncError>,
        questions: std::cell::Cell<u32>,
    }

    impl Asked {
        fn answering(answer: Result<Option<RestoreNotice>, SyncError>) -> Asked {
            Asked {
                answer,
                questions: std::cell::Cell::new(0),
            }
        }

        /// The token server announces a restore of epoch `epoch`.
        fn announcing(epoch: u64) -> Asked {
            Asked::answering(Ok(Some(RestoreNotice {
                epoch,
                ..RestoreNotice::default()
            })))
        }

        /// The token server says that none was announced.
        fn none() -> Asked {
            Asked::answering(Ok(None))
        }

        /// The token server cannot be asked.
        fn failing() -> Asked {
            Asked::answering(Err(SyncError::Connect(String::from(
                "the token server did not answer",
            ))))
        }

        fn questions(&self) -> u32 {
            self.questions.get()
        }
    }

    impl AskRestore for Asked {
        fn newest(&self) -> Result<Option<RestoreNotice>, SyncError> {
            self.questions.set(self.questions.get() + 1);
            self.answer.clone()
        }
    }

    fn sync_asking(
        f: &Fixture,
        options: &AzlinOptions,
        ask: &Asked,
    ) -> Result<SyncReport, SyncError> {
        sync_account(&f.bucket, &f.store, options, ask, &mut |_| true)
    }

    /// RED on the code before: the session's snapshot was the only notice - the last sign-in, hours
    /// old - so a restore announced AFTER it was an ordinary pull: the drive's listing lacked the
    /// newer mail, the drive's own answer about each object agreed (another device had committed
    /// on top of the restored drive, so no look could tell), and the local copies - the only ones -
    /// went. Now the token server is asked before the first folder with something gone is pulled:
    /// `options.restore` is `None` here and the asker says epoch 1.
    #[test]
    fn x_delta_apps3_2_a_restore_announced_after_the_last_sign_in_deletes_no_mail_and_puts_it_back()
    {
        let f = fixture();
        deliver(&f, "Inbox", &mail(1, "Older"), SEP_30);
        let newer = deliver(&f, "Inbox", &mail(2, "Newer"), SEP_30 + 60);
        let archived = deliver(&f, "Archive", &mail(3, "Archived"), SEP_30 + 120);
        run(&f);
        f.bucket.inner.delete(&newer).unwrap();
        f.bucket.inner.delete(&archived).unwrap();
        assert_eq!(options().restore, None, "the session knows nothing of it");
        let ask = Asked::announcing(1);
        let report = sync_asking(&f, &options(), &ask).unwrap();
        assert_eq!(report.removed(), 0, "a restore deletes no mail here");
        assert_eq!(report.kept_after_restore(), 2);
        assert_eq!(
            report.pushed(),
            2,
            "and puts what the drive lacks into it again"
        );
        assert_eq!(
            ask.questions(),
            1,
            "one question for the whole Send/Receive, two folders had something gone"
        );
        let inbox = held(&f, "inbox");
        assert_eq!(inbox.len(), 2, "both lines stay");
        assert_eq!(inbox[1].1, mail(2, "Newer"), "the file is the mail");
        assert!(inbox.iter().all(|(_, _, on_the_drive)| *on_the_drive));
        let archive = held(&f, "archive");
        assert_eq!(
            (archive[0].1.clone(), archive[0].2),
            (mail(3, "Archived"), true)
        );
        assert_eq!(
            read_restore_epoch(&f.store).unwrap(),
            1,
            "the restore is taken"
        );
        crate::l10n::in_english();
        let said = drive_sentences(&report);
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(
            said[0].contains("restored to an earlier state") && said[0].contains("2 messages"),
            "{said:?}"
        );
        // Taken once: the same answer on the next Send/Receive is the course of things.
        let again = Asked::announcing(1);
        let report = sync_asking(&f, &options(), &again).unwrap();
        assert_eq!(
            (
                report.removed(),
                report.pushed(),
                report.kept_after_restore()
            ),
            (0, 0, 0)
        );
        assert_eq!(again.questions(), 0, "nothing is gone: nothing is asked");
    }

    /// A token server that cannot be asked is an unknown, not a "no restore": nothing leaves (and
    /// nothing is put back: the mail may be one a delete elsewhere took), the window says so in one
    /// sentence, the memory takes nothing, and the next Send/Receive that can ask decides.
    #[test]
    fn x_delta_apps3_2_a_token_server_that_cannot_be_asked_deletes_nothing_and_says_so_once() {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        let two = deliver(&f, "Archive", &mail(2, "Two"), SEP_30 + 60);
        run(&f);
        f.bucket.inner.delete(&one).unwrap();
        f.bucket.inner.delete(&two).unwrap();
        let ask = Asked::failing();
        let report = sync_asking(&f, &options(), &ask).unwrap();
        assert_eq!(
            (
                report.removed(),
                report.pushed(),
                report.kept_unasked(),
                report.kept_after_restore()
            ),
            (0, 0, 2, 0)
        );
        assert_eq!(ask.questions(), 1, "asked once, not once per folder");
        for folder in ["inbox", "archive"] {
            let rows = held(&f, folder);
            assert_eq!(rows.len(), 1, "{folder}: line and file stay");
            assert!(!rows[0].1.is_empty(), "{folder}: the file is the mail");
            assert!(!rows[0].2, "{folder}: and is not put back into the drive");
        }
        assert_eq!(
            read_restore_epoch(&f.store).unwrap(),
            0,
            "no restore was taken"
        );
        assert!(
            report
                .restore_unknown
                .as_deref()
                .is_some_and(|why| why.contains("did not answer")),
            "{:?}",
            report.restore_unknown
        );
        crate::l10n::in_english();
        let said = drive_sentences(&report);
        assert_eq!(said.len(), 1, "one sentence: {said:?}");
        assert!(
            said[0].contains("could not be asked whether the drive was restored")
                && said[0].contains("2 messages")
                && said[0].contains("did not answer"),
            "{said:?}"
        );
        // The token server answers again and has announced nothing: the ordinary pull lets both
        // go, as it did before.
        let report = sync_asking(&f, &options(), &Asked::none()).unwrap();
        assert_eq!((report.removed(), report.kept_unasked()), (2, 0));
        // Let go means held 90 days (X-DELTA-APPS3-3), and the window says so - for each folder, with
        // where the mail is (X-DELTA-APPS4-5).
        let said = drive_sentences(&report);
        assert_eq!(said.len(), 2, "one for each folder: {said:?}");
        assert!(
            said.iter()
                .all(|sentence| sentence.contains("1 message of ")
                    && sentence.contains(" was removed from the drive")
                    && sentence.contains("90 days")),
            "{said:?}"
        );
        assert!(
            said.iter()
                .any(|sentence| sentence.contains("in removed/inbox/."))
                && said
                    .iter()
                    .any(|sentence| sentence.contains("in removed/archive/.")),
            "{said:?}"
        );
    }

    /// X-DELTA-KEYS4-1: incoming mail that waits in the drive because this computer's key does not open the
    /// drive's drop key (a key change of its own that stopped, a key that is an old one, a damaged file) is ONE
    /// sentence of the sync's report - in both languages, and nothing else is said where there is nothing to say.
    /// The sync itself went on: no error of the drive (`mail_drive::file_drops`). X-DELTA-KEYS5-3: three states,
    /// three sentences - the computer whose OWN key change is not finished is told to finish it, not that it lacks
    /// the key, and a damaged file is no "waits": it will not be opened, and nothing here can replace it.
    #[test]
    fn x_delta_keys4_1_mail_waiting_for_its_drop_key_is_one_sentence_of_the_report_in_both_languages(
    ) {
        use crate::mail_drive::DropsWait;
        crate::l10n::in_english();
        assert!(
            drive_sentences(&SyncReport::default()).is_empty(),
            "nothing waits, nothing is said"
        );
        for (wait, en_start, english, de_start, german) in [
            (
                DropsWait::OtherKey,
                "Incoming mail waits in the drive",
                "another drive key",
                "Eingehende E-Mails warten im Laufwerk",
                "anderen Laufwerksschlüssel",
            ),
            (
                DropsWait::KeyChangeUnfinished,
                "Incoming mail waits in the drive",
                "Finish the key change in AzDrive",
                "Eingehende E-Mails warten im Laufwerk",
                "Schließe den Schlüsselwechsel in AzDrive ab",
            ),
            (
                DropsWait::Damaged,
                "Incoming mail cannot be filed",
                "cannot replace it yet",
                "Eingehende E-Mails können nicht einsortiert werden",
                "können sie noch nicht ersetzen",
            ),
        ] {
            let report = SyncReport {
                drops_waiting: Some(wait),
                ..SyncReport::default()
            };
            crate::l10n::in_english();
            let said = drive_sentences(&report);
            assert_eq!(said.len(), 1, "one sentence: {said:?}");
            assert!(
                said[0].starts_with(en_start) && said[0].contains(english),
                "{wait:?}: {said:?}"
            );
            azul_appkit::l10n::set_locale("de-DE");
            let said = drive_sentences(&report);
            assert_eq!(said.len(), 1, "one sentence: {said:?}");
            assert!(
                said[0].starts_with(de_start) && said[0].contains(german),
                "{wait:?}: {said:?}"
            );
        }
        // The newest computer is not told it lacks the key, and a damaged file is no wait.
        crate::l10n::in_english();
        let unfinished = drive_sentences(&SyncReport {
            drops_waiting: Some(DropsWait::KeyChangeUnfinished),
            ..SyncReport::default()
        });
        assert!(
            !unfinished[0].contains("another drive key"),
            "{unfinished:?}"
        );
        let damaged = drive_sentences(&SyncReport {
            drops_waiting: Some(DropsWait::Damaged),
            ..SyncReport::default()
        });
        assert!(
            damaged[0].contains("damaged") && !damaged[0].contains("waits"),
            "{damaged:?}"
        );
        crate::l10n::in_english();
    }

    /// The answer is the epoch this copy already took: the drive stands where it was left, so an
    /// ordinary delete by another device is an ordinary pull, as it was before.
    #[test]
    fn x_delta_apps3_2_an_answer_of_the_epoch_already_taken_lets_the_ordinary_pull_delete() {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        let two = deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        run(&f);
        f.bucket.inner.delete(&two).unwrap();
        sync_with(&f.bucket, &f, &announced(1)).unwrap();
        assert_eq!(read_restore_epoch(&f.store).unwrap(), 1);
        // Another device deletes `one` for good; the session has no news, the token server still
        // says epoch 1.
        f.bucket.inner.delete(&one).unwrap();
        let ask = Asked::announcing(1);
        let report = sync_asking(&f, &options(), &ask).unwrap();
        assert_eq!((report.removed(), report.kept_after_restore()), (1, 0));
        assert_eq!(ask.questions(), 1);
        assert_eq!(index(&f.store, "inbox").len(), 1);
        // And an announcement that does not cover the mail (another prefix) is none for it.
        let remaining = index(&f.store, "inbox").remove(0);
        f.bucket.inner.delete(&remaining.remote).unwrap();
        let elsewhere = Asked::answering(Ok(Some(RestoreNotice {
            epoch: 7,
            prefix: String::from("docs/"),
            ..RestoreNotice::default()
        })));
        let report = sync_asking(&f, &options(), &elsewhere).unwrap();
        assert_eq!((report.removed(), report.kept_after_restore()), (1, 0));
    }

    /// Only when something is gone, and a snapshot that already says "restored" needs no question.
    #[test]
    fn x_delta_apps3_2_the_token_server_is_asked_only_when_something_is_gone() {
        let f = fixture();
        let key = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        run(&f);
        let ask = Asked::announcing(9);
        let report = sync_asking(&f, &options(), &ask).unwrap();
        assert_eq!(ask.questions(), 0, "nothing is gone: no request");
        assert_eq!(report.kept_after_restore(), 0);
        assert_eq!(
            read_restore_epoch(&f.store).unwrap(),
            0,
            "no question, no epoch taken"
        );
        // Something is gone and the session's snapshot already says it: no question either.
        f.bucket.inner.delete(&key).unwrap();
        let ask = Asked::none();
        let report = sync_asking(&f, &announced(1), &ask).unwrap();
        assert_eq!(ask.questions(), 0, "the snapshot decided");
        assert_eq!((report.removed(), report.kept_after_restore()), (0, 1));
    }

    /// X-DELTA-APPS3-10 (b), through the door the app's sync thread uses: the notice of the
    /// session's last sign-in is what the sync takes. RED on a caller that forgets it (the old
    /// line `restore: session.restore.clone()` was pinned by no test): a session that carries
    /// epoch 2 makes the pass a restore's, one that does not makes it an ordinary pull.
    #[test]
    fn x_delta_apps3_10_the_session_of_the_sign_in_makes_the_options_of_the_sync() {
        let mut session = AzlinSession::with_token("d_1", "dt_f.0.a");
        assert_eq!(AzlinOptions::for_session(&session, 77).restore, None);
        assert_eq!(AzlinOptions::for_session(&session, 77).now, 77);
        session.restore = Some(RestoreNotice {
            epoch: 2,
            ..RestoreNotice::default()
        });
        assert_eq!(
            AzlinOptions::for_session(&session, 77).restore,
            session.restore
        );
        let lost = |session: &AzlinSession| {
            let f = fixture();
            let key = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
            run(&f);
            f.bucket.inner.delete(&key).unwrap();
            let ask = Asked::none();
            let report =
                sync_session(&f.bucket, &f.store, session, SEP_30, &ask, &mut |_| true).unwrap();
            (
                report.removed(),
                report.kept_after_restore(),
                ask.questions(),
            )
        };
        assert_eq!(
            lost(&session),
            (0, 1, 0),
            "the session's notice: a restore, nothing leaves"
        );
        session.restore = None;
        assert_eq!(
            lost(&session),
            (1, 0, 1),
            "no notice: the token server is asked, and an ordinary pull"
        );
    }
    // ==== X-DELTA-APPS3-3: a pull never destroys a copy ====

    const DAY: i64 = 86_400;

    /// `options()` at `now`.
    fn options_at(now: i64) -> AzlinOptions {
        AzlinOptions { now, ..options() }
    }

    fn run_at(f: &Fixture, now: i64) -> SyncReport {
        sync_account(
            &f.bucket,
            &f.store,
            &options_at(now),
            &no_restore(),
            &mut |_| true,
        )
        .unwrap()
    }

    /// Every key of the drive, the hidden folders and the markers too.
    fn drive_keys(f: &Fixture) -> Vec<String> {
        ops::list_all(&f.bucket.inner, "")
            .unwrap()
            .into_iter()
            .map(|object| object.key)
            .collect()
    }

    /// Every key under `removed/` of the local copy.
    fn held_keys(f: &Fixture) -> Vec<String> {
        f.store.keys(store::REMOVED_PREFIX)
    }

    /// The holding folder a pull at `now` makes for `folder`.
    fn holding(folder: &str, now: i64) -> String {
        store::removed_prefix(folder, &azul_storage::time::amz_date(now as u64))
    }

    /// RED on the code before: the pull deleted the local copy of every message the drive no longer
    /// listed. That is right for a delete on another device and wrong for the user's own "Restore the
    /// drive as of Tuesday" on an encrypted drive - one ordinary commit nobody announces, which
    /// looks like a delete of every file made since - and the local copies may be the only ones. The
    /// file moves to `removed/<folder>/<stamp>/` with the line it had, and the window says so.
    #[test]
    fn x_delta_apps3_3_a_message_the_drive_no_longer_lists_is_held_and_not_deleted() {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        run(&f);
        let gone = index(&f.store, "inbox").remove(0);
        f.bucket.inner.delete(&one).unwrap();
        let report = run(&f);
        assert_eq!(report.removed(), 1);
        let subjects: Vec<String> = index(&f.store, "inbox")
            .into_iter()
            .map(|e| e.subject)
            .collect();
        assert_eq!(subjects, ["Two"]);
        assert_eq!(f.store.size_of(&gone.path), None, "not under its old name");
        let held = holding("inbox", SEP_30);
        let relative = gone.path.strip_prefix("mail/inbox/").unwrap();
        assert_eq!(
            f.store.get(&format!("{held}/{relative}")).unwrap(),
            mail(1, "One"),
            "the file is there, byte for byte"
        );
        let record = store::index_from_jsonl(
            &String::from_utf8(f.store.get(&format!("{held}/index.jsonl")).unwrap()).unwrap(),
        );
        assert_eq!(record.len(), 1);
        assert_eq!(
            (record[0].subject.as_str(), record[0].remote.as_str()),
            ("One", one.as_str())
        );
        crate::l10n::in_english();
        let said = drive_sentences(&report);
        assert_eq!(said.len(), 1, "{said:?}");
        let display = &report
            .folders
            .iter()
            .find(|f| f.removed > 0)
            .unwrap()
            .display;
        assert!(
            said[0].contains(&format!(
                "1 message of {display} was removed from the drive"
            )) && said[0].contains("stays on this computer for 90 days, in removed/inbox/."),
            "the sentence names the folder and where the mail is held: {said:?}"
        );
        assert_eq!(HOLD_DAYS, 90, "the drive's own history");
    }

    /// The sweep is the only thing that deletes it: after 90 days by the stamp, never early - not a
    /// day early, not on a clock stepped back, not a folder that is no stamp's, and not at all when
    /// the list of `removed/` cannot be read.
    #[test]
    fn x_delta_apps3_3_the_holding_area_is_swept_after_90_days_and_never_early() {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        let two = deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        deliver(&f, "Inbox", &mail(3, "Three"), SEP_30 + 120);
        run(&f);
        f.bucket.inner.delete(&one).unwrap();
        run_at(&f, SEP_30);
        let first = holding("inbox", SEP_30);
        // Thirty days on, another message leaves.
        f.bucket.inner.delete(&two).unwrap();
        let report = run_at(&f, SEP_30 + 30 * DAY);
        assert_eq!(report.removed(), 1);
        let second = holding("inbox", SEP_30 + 30 * DAY);
        let stays = |f: &Fixture, prefix: &str| held_keys(f).iter().any(|k| k.starts_with(prefix));
        // A clock stepped back a year: everything stays.
        run_at(&f, SEP_30 - 365 * DAY);
        assert!(stays(&f, &first) && stays(&f, &second));
        // The 89th day (and the last second of it): everything stays. The drive has mail from that
        // moment (the sweep needs the drive to agree that it is that late, X-DELTA-APPS4-3), so it is
        // the clock of this computer alone that holds the sweep back here.
        deliver(&f, "Inbox", &mail(4, "Day 89"), SEP_30 + 90 * DAY - 1);
        run_at(&f, SEP_30 + 90 * DAY - 1);
        assert!(stays(&f, &first) && stays(&f, &second));
        // A folder that is no stamp's, a stamp in the future and a key of another shape stay for ever.
        f.store
            .put("removed/inbox/notes/readme.txt", b"mine")
            .unwrap();
        f.store
            .put("removed/inbox/29990101T000000Z/x.eml", b"future")
            .unwrap();
        f.store.put("removed/stray.txt", b"stray").unwrap();
        // The 90th day by the first stamp: the first folder goes whole, the second stays.
        deliver(&f, "Inbox", &mail(5, "Day 90"), SEP_30 + 90 * DAY);
        let report = run_at(&f, SEP_30 + 90 * DAY);
        assert_eq!(report.sweep_failed, None);
        assert!(!stays(&f, &first), "{:?}", held_keys(&f));
        assert!(stays(&f, &second));
        for kept in [
            "removed/inbox/notes/readme.txt",
            "removed/inbox/29990101T000000Z/x.eml",
            "removed/stray.txt",
        ] {
            assert!(
                held_keys(&f).contains(&kept.to_string()),
                "{kept}: {:?}",
                held_keys(&f)
            );
        }
        // The second goes thirty days later, by ITS stamp.
        deliver(&f, "Inbox", &mail(6, "Day 120"), SEP_30 + 120 * DAY);
        run_at(&f, SEP_30 + 120 * DAY);
        assert!(!stays(&f, &second));
        // Nothing is swept by a pull that has no clock.
        f.store
            .put(&format!("{}/x.eml", holding("inbox", SEP_30)), b"again")
            .unwrap();
        run_at(&f, 0);
        assert!(stays(&f, &first));
    }

    /// A sweep that cannot read its list deletes nothing, however old the stamps are.
    #[test]
    fn x_delta_apps3_3_a_sweep_that_cannot_read_its_list_deletes_nothing() {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        run(&f);
        f.bucket.inner.delete(&one).unwrap();
        run_at(&f, SEP_30);
        let before = held_keys(&f);
        assert_eq!(before.len(), 2, "the file and the record: {before:?}");
        let failing = |_: &str| -> std::io::Result<Vec<String>> {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "the list cannot be read",
            ))
        };
        let late = SEP_30 + 400 * DAY;
        let failed = sweep_removed_with(&f.store, late, Some(late as u64), &failing);
        assert!(matches!(failed, Err(SyncError::Storage(_))), "{failed:?}");
        assert_eq!(held_keys(&f), before, "nothing was deleted");
        // The same sweep with a list that works deletes it: the failure was the list's.
        assert_eq!(sweep_removed(&f.store, late, Some(late as u64)).unwrap(), 1);
        assert!(held_keys(&f).is_empty());
    }

    // ==== X-DELTA-APPS4-3 + X-APPS3-1: the clock of this computer is set against the last sync and the drive ====

    /// RED on the code before: a pull on a clock that was a year back (a reset RTC, a virtual machine
    /// resumed before the time sync) dated its hold a year back, and the next Send/Receive with a right
    /// clock swept it - the held copy lasted ONE sync. Now the hold is refused with nothing moved - and the
    /// Receive goes on and says why (X-DELTA-APPS5-1: it was a failed Receive) - the clock that is wrong is not
    /// recorded, the right clock holds it, dated now, and the files are still there afterwards.
    #[test]
    fn x_delta_apps4_3_a_pull_at_a_clock_a_year_back_holds_nothing_and_the_right_clock_finds_the_files(
    ) {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        run(&f);
        let gone = index(&f.store, "inbox").remove(0);
        f.bucket.inner.delete(&one).unwrap();
        // The clock of this computer is a year back: nothing is held, the Receive says why.
        let report = run_at(&f, SEP_30 - 365 * DAY);
        assert_eq!(
            (report.removed(), report.kept_undated()),
            (0, 1),
            "{report:?}"
        );
        let why = report.hold_refused.clone().expect("the report says why");
        assert!(
            why.contains("more than a day before the last Send/Receive"),
            "{why}"
        );
        assert_eq!(held_rows(&f), ["One", "Two"], "the line is where it was");
        assert_eq!(
            f.store.get(&gone.path).unwrap(),
            mail(1, "One"),
            "and so is its file"
        );
        assert!(held_keys(&f).is_empty(), "{:?}", held_keys(&f));
        let state = read_state(&f.store, "inbox").unwrap().unwrap();
        assert_eq!(
            parse_iso8601(&state.synced_at),
            Some(SEP_30 as u64),
            "the clock that was refused is not the time of the sync: {}",
            state.synced_at
        );
        // A right clock holds it, dated now - and the sweep of that very run keeps it.
        let report = run_at(&f, SEP_30 + DAY);
        assert_eq!(
            (report.removed(), report.kept_undated(), report.hold_refused),
            (1, 0, None)
        );
        let held = holding("inbox", SEP_30 + DAY);
        let relative = gone.path.strip_prefix("mail/inbox/").unwrap();
        assert_eq!(
            f.store.get(&format!("{held}/{relative}")).unwrap(),
            mail(1, "One"),
            "the files are still there"
        );
        assert_eq!(held_rows(&f), ["Two"]);
    }

    /// A clock a few hours behind (a zone changed by hand, the winter time) is no reason to refuse: the
    /// hold is dated at the last Send/Receive - never before it - and is swept 90 days after that.
    #[test]
    fn x_delta_apps4_3_a_clock_a_few_hours_behind_dates_the_hold_at_the_last_sync() {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        run(&f);
        f.bucket.inner.delete(&one).unwrap();
        let report = run_at(&f, SEP_30 - 3 * 3600);
        assert_eq!(report.removed(), 1);
        assert!(
            held_keys(&f)
                .iter()
                .all(|key| key.starts_with(&holding("inbox", SEP_30))),
            "dated at the last Send/Receive, not three hours before it: {:?}",
            held_keys(&f)
        );
        // And the clock behind does not lower what the next Send/Receive is set against.
        let state = read_state(&f.store, "inbox").unwrap().unwrap();
        assert_eq!(
            parse_iso8601(&state.synced_at),
            Some(SEP_30 as u64),
            "{}",
            state.synced_at
        );
    }

    /// RED on the code before: a pull on a clock ten years ahead dated its hold 2036, which no sweep of
    /// the years to come reaches - the held copy was kept for ever. Now the hold is refused with nothing
    /// moved - the Receive goes on and says why - and the clock that is wrong is not recorded as the time of the
    /// sync (a right clock afterwards would be ten years "behind" it).
    #[test]
    fn x_delta_apps4_3_a_pull_at_a_clock_ten_years_ahead_is_refused_and_nothing_moves() {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        run(&f);
        f.bucket.inner.delete(&one).unwrap();
        let report = run_at(&f, SEP_30 + 10 * 366 * DAY);
        assert_eq!(
            (report.removed(), report.kept_undated()),
            (0, 1),
            "{report:?}"
        );
        let why = report.hold_refused.clone().expect("the report says why");
        assert!(
            why.contains("more than a year after the last Send/Receive"),
            "{why}"
        );
        assert_eq!(held_rows(&f), ["One", "Two"], "nothing moved");
        assert!(held_keys(&f).is_empty(), "{:?}", held_keys(&f));
        let state = read_state(&f.store, "inbox").unwrap().unwrap();
        assert_eq!(
            parse_iso8601(&state.synced_at),
            Some(SEP_30 as u64),
            "the wrong clock was not recorded: {}",
            state.synced_at
        );
        // The right clock afterwards is not "behind" anything: it holds.
        let report = run_at(&f, SEP_30 + DAY);
        assert_eq!(report.removed(), 1);
    }

    // ==== X-DELTA-APPS5-1: a refused hold is not a failed Receive, and a refused clock is never recorded ====

    /// RED on the code before (a): the FIRST Send/Receive of a copy at a clock more than a year ahead of the drive's
    /// newest mail was refused by `hold_time` and then recorded all the same - `recorded` fell to `unwrap_or(now)` -
    /// so every Send/Receive with the corrected clock was "more than a day behind" it and every line that left a
    /// folder failed the whole Receive. Now it records NO stamp (the folder states say none), the mail comes, and
    /// the corrected clock holds what leaves.
    #[test]
    fn x_delta_apps5_1_a_first_send_receive_at_a_clock_ten_years_ahead_records_no_stamp_and_the_corrected_clock_holds(
    ) {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        // The first Send/Receive of this copy, on a clock ten years ahead of a drive that has mail.
        let report = run_at(&f, SEP_30 + 10 * 366 * DAY);
        assert_eq!(report.fetched(), 2, "the mail comes: {report:?}");
        assert!(!f.store.folders().is_empty());
        for key in f.store.folders() {
            let state = read_state(&f.store, &key).unwrap().unwrap();
            assert_eq!(
                state.synced_at, "",
                "{key}: the clock that was refused is no stamp"
            );
        }
        // The clock is corrected: what leaves is held, not refused against a record of 2036.
        f.bucket.inner.delete(&one).unwrap();
        let report = run_at(&f, SEP_30 + DAY);
        assert_eq!(
            (report.removed(), report.kept_undated()),
            (1, 0),
            "{report:?}"
        );
        assert_eq!(held_rows(&f), ["Two"]);
        let state = read_state(&f.store, "inbox").unwrap().unwrap();
        assert_eq!(
            parse_iso8601(&state.synced_at),
            Some((SEP_30 + DAY) as u64),
            "now the clock is recorded: {}",
            state.synced_at
        );
    }

    /// RED on the code before: the hold of a folder that cannot be dated ended the whole Receive - the folders after
    /// it were not synced (no new mail), and the sweep did not run - with the sentence that said the clock was wrong,
    /// every time, for as long as it was. Now the lines stay where they are in the folder, the new mail of that folder
    /// and of the later ones comes, the sweep runs (it is set against the earlier of the clock and the drive, and
    /// the drive has lived 100 days since the old hold), and the report says it once.
    #[test]
    fn x_delta_apps5_1_a_refused_hold_lets_new_mail_in_syncs_the_later_folders_and_runs_the_sweep()
    {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        let two = deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        deliver(&f, "Spam", &mail(3, "Three"), SEP_30);
        run(&f);
        // An old hold: One left at SEP_30.
        f.bucket.inner.delete(&one).unwrap();
        run_at(&f, SEP_30);
        let first = holding("inbox", SEP_30);
        assert!(held_keys(&f).iter().any(|key| key.starts_with(&first)));
        // Two leaves, mail comes in the folder and in a later one, and the clock is ten years ahead.
        f.bucket.inner.delete(&two).unwrap();
        deliver(&f, "Inbox", &mail(4, "Four"), SEP_30 + 100 * DAY);
        deliver(&f, "Spam", &mail(5, "Five"), SEP_30 + 100 * DAY);
        let report = run_at(&f, SEP_30 + 10 * 366 * DAY);
        assert_eq!(
            report.fetched(),
            2,
            "the new mail of both folders: {report:?}"
        );
        assert_eq!(
            (report.removed(), report.kept_undated()),
            (0, 1),
            "{report:?}"
        );
        assert_eq!(held_rows(&f), ["Two", "Four"], "Two stays, Four comes");
        let spam: Vec<String> = index(&f.store, "spam")
            .into_iter()
            .map(|e| e.subject)
            .collect();
        assert_eq!(spam, ["Three", "Five"], "the later folder was synced");
        assert_eq!(report.sweep_failed, None);
        assert!(
            !held_keys(&f).iter().any(|key| key.starts_with(&first)),
            "the sweep ran: {:?}",
            held_keys(&f)
        );
        // The sentence, once, in both languages: the lines stay, why, and what happens next.
        crate::l10n::in_english();
        let said = drive_sentences(&report);
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(
            said[0].starts_with(
                "The Azlin drive no longer lists 1 message you have, and it was left in its folder because the clock of this computer says"
            ) && said[0].contains("more than a year after the last Send/Receive")
                && said[0].ends_with("and it is kept on this computer for 90 days."),
            "{said:?}"
        );
        azul_appkit::l10n::set_locale("de-DE");
        let said = drive_sentences(&report);
        crate::l10n::in_english();
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(
            said[0].contains("blieb in ihrem Ordner") && said[0].contains("90 Tage"),
            "{said:?}"
        );
    }

    /// The review's first scenario: a clock 30 days ahead is inside the 366 days, so it is accepted and recorded as
    /// the time of every folder's sync. Corrected, it is 29 days behind that record. RED on the code before: every
    /// Send/Receive in which a line left a folder FAILED, for the 29 days (no new mail, no later folder, no sweep).
    /// Now it goes on - the lines stay and the window says why - until the real time passes the record, and then
    /// the lines are held as they should be.
    #[test]
    fn x_delta_apps5_1_a_clock_30_days_ahead_that_was_recorded_costs_a_delay_and_never_a_failed_receive(
    ) {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        run(&f);
        // A Send/Receive on a clock 30 days ahead: inside the tolerance, so it is the time of the sync.
        run_at(&f, SEP_30 + 30 * DAY);
        let state = read_state(&f.store, "inbox").unwrap().unwrap();
        assert_eq!(
            parse_iso8601(&state.synced_at),
            Some((SEP_30 + 30 * DAY) as u64)
        );
        f.bucket.inner.delete(&one).unwrap();
        deliver(&f, "Inbox", &mail(3, "Three"), SEP_30 + 2 * DAY);
        // The clock is corrected: 29 days behind the record. The Receive is no failure: the new mail comes, the line
        // that left stays, the window says why.
        let report = run_at(&f, SEP_30 + DAY);
        assert_eq!(
            (report.fetched(), report.removed(), report.kept_undated()),
            (1, 0, 1),
            "{report:?}"
        );
        assert!(report
            .hold_refused
            .as_deref()
            .is_some_and(|why| why.contains("more than a day before the last Send/Receive")));
        assert_eq!(held_rows(&f), ["One", "Two", "Three"]);
        // The real time passes the record: the line is held, dated now.
        let report = run_at(&f, SEP_30 + 31 * DAY);
        assert_eq!(
            (report.removed(), report.kept_undated(), report.hold_refused),
            (1, 0, None)
        );
        assert_eq!(held_rows(&f), ["Two", "Three"]);
    }

    /// The review's second scenario, as far as a local rule goes: a computer that was off for 14 months and an
    /// account whose newest mail is older than a year (an archive). The RIGHT clock is more than a year after both
    /// readings, so the hold is refused - it was a failed Receive until a new mail arrived. Now the Receive goes on
    /// and says why; a mail that arrives is the drive's second word (its newest mail is then as late as the clock)
    /// and the hold is made. (The independent reading that would move the floor without a new mail - the token
    /// server's `Date` - is X-APPS4B-2: the kit does not give it yet.)
    #[test]
    fn x_delta_apps5_1_an_archive_account_on_a_computer_that_was_off_a_year_is_no_failed_receive() {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        run(&f);
        f.bucket.inner.delete(&one).unwrap();
        let later = SEP_30 + 14 * 30 * DAY;
        let report = run_at(&f, later);
        assert_eq!(
            (report.removed(), report.kept_undated()),
            (0, 1),
            "{report:?}"
        );
        assert!(
            report
                .hold_refused
                .as_deref()
                .is_some_and(|why| why.contains(
                    "more than a year after the last Send/Receive and the newest mail of the drive"
                )),
            "{report:?}"
        );
        assert_eq!(held_rows(&f), ["One", "Two"], "nothing was lost or moved");
        // A mail arrives: the drive's newest mail is as late as the clock, which is then no longer "ahead" of it.
        deliver(&f, "Inbox", &mail(3, "Three"), later - 60);
        let report = run_at(&f, later);
        assert_eq!(
            (report.fetched(), report.removed(), report.kept_undated()),
            (1, 1, 0),
            "{report:?}"
        );
        assert_eq!(held_rows(&f), ["Two", "Three"]);
    }

    /// The sweep deletes only when the drive agrees that the hold is old: a clock of this computer
    /// that jumped ahead (here: ten years) deletes nothing from a drive whose newest mail is from the
    /// day of the hold, and a drive that lists no mail at all says nothing: nothing is deleted then
    /// either. The clock that is right and a drive that has lived 90 days deletes it.
    #[test]
    fn x_delta_apps4_3_the_sweep_waits_for_the_drive_to_agree_that_the_hold_is_old() {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        run(&f);
        f.bucket.inner.delete(&one).unwrap();
        run_at(&f, SEP_30);
        let before = held_keys(&f);
        assert_eq!(before.len(), 2, "the file and the record: {before:?}");
        let ahead = SEP_30 + 10 * 366 * DAY;
        let newest = SEP_30 as u64 + 60;
        assert_eq!(
            sweep_removed(&f.store, ahead, Some(newest)).unwrap(),
            0,
            "the drive has not lived 90 days since the hold"
        );
        assert_eq!(
            sweep_removed(&f.store, ahead, None).unwrap(),
            0,
            "no mail, no reading"
        );
        assert_eq!(
            sweep_removed(&f.store, 0, Some(ahead as u64)).unwrap(),
            0,
            "no clock"
        );
        assert_eq!(held_keys(&f), before, "nothing was deleted");
        // The drive has lived: its newest mail is 90 days after the hold, the clock agrees.
        let late = SEP_30 + 90 * DAY;
        assert_eq!(sweep_removed(&f.store, late, Some(late as u64)).unwrap(), 1);
        assert!(held_keys(&f).is_empty());
    }

    // ==== X-DELTA-APPS5-2: Delete in the Outbox is the second writer of `removed/` ====

    const ACCOUNT_ID: &str = "ada@example.org";

    /// The fixture of an account whose data folder is `f`'s local folder: the folder the sync keeps its copy
    /// in is the account's own (`root/ada@example.org/`), where Delete in the Outbox keeps what it removes.
    fn account_fixture() -> (Fixture, store::DriveFolder) {
        let f = fixture();
        let root = f._local_dir.folder();
        let store = MailStore::new(crate::account::account_dir(&root, ACCOUNT_ID));
        (Fixture { store, ..f }, root)
    }

    /// A mail that says "Not sent" in the Outbox of the account of [`account_fixture`], with its file.
    fn failed_in_outbox(root: &store::DriveFolder, name: &str) {
        use crate::send::{self, OutboxEntry, OutboxState};
        let outbox = MailStore::new(send::outbox_dir(root, ACCOUNT_ID));
        let entry = OutboxEntry {
            format: String::from(send::OUTBOX_FORMAT),
            version: 1,
            id: name.to_string(),
            message_id: format!("{name}@example.org"),
            from: String::from(ACCOUNT_ID),
            subject: name.to_string(),
            created: SEP_30,
            attempts: 1,
            last_attempt: 0,
            next_attempt: 0,
            state: OutboxState::Failed,
            recipients: Vec::new(),
        };
        outbox
            .put(
                &format!("{name}.json"),
                serde_json::to_string(&entry).unwrap().as_bytes(),
            )
            .unwrap();
        outbox
            .put(
                &format!("{name}.eml"),
                b"Subject: x\r\n\r\nbody of the letter",
            )
            .unwrap();
    }

    /// Delete in the Outbox ([`crate::send::remove_failed_entry`]) at the clock `now`. The Outbox's lock is the
    /// process's and the tests of this binary run side by side: another test's Delete or send may hold it for a
    /// moment, and the call never waits for it (a lock that is taken is `WouldBlock` and nothing was done), so it
    /// is asked again until the lock is free.
    fn delete_in_outbox(root: &store::DriveFolder, name: &str, now: i64) -> std::io::Result<()> {
        let started = std::time::Instant::now();
        loop {
            match crate::send::remove_failed_entry(root, ACCOUNT_ID, name, now) {
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        && started.elapsed() < std::time::Duration::from_secs(60) =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                other => return other,
            }
        }
    }

    /// The names in the Outbox of the account.
    fn outbox_names(root: &store::DriveFolder) -> Vec<String> {
        MailStore::new(crate::account::account_dir(root, ACCOUNT_ID)).keys(crate::send::OUTBOX_DIR)
    }

    /// RED on the code before (the review's scenario): Delete in the Outbox dated its hold by the raw clock. A
    /// computer whose clock was a year back (a reset RTC) held the only copy of the letter's text under that
    /// stamp, and the next Send/Receive with a right clock swept it (`min(clock, drive) >= stamp + 90 days`):
    /// the held copy lasted ONE sync. Now Delete is set against the account's last Send/Receive as the pull is:
    /// on that clock it removes nothing - both files stay in the Outbox, nothing is held - and the right-clock
    /// Send/Receive deletes nothing of it; a Delete at the right clock holds both files, which the SAME sweep
    /// keeps for 90 days from that Delete and deletes after them.
    #[test]
    fn x_delta_apps5_2_a_delete_on_a_clock_a_year_back_keeps_both_files_through_a_right_clock_sync()
    {
        let (f, root) = account_fixture();
        deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        run_at(&f, SEP_30);
        failed_in_outbox(&root, "a-failed");
        let both = outbox_names(&root);
        assert_eq!(both.len(), 2, "{both:?}");
        // The clock of this computer is a year back (and, in the other case, ten years ahead): refused.
        for now in [SEP_30 - 365 * DAY, SEP_30 + 10 * 366 * DAY] {
            let refused = delete_in_outbox(&root, "a-failed", now).unwrap_err();
            assert!(
                refused
                    .to_string()
                    .ends_with(": nothing is removed from the Outbox"),
                "{refused}"
            );
            assert_eq!(outbox_names(&root), both, "both files stay: {refused}");
            assert!(
                held_keys(&f).is_empty(),
                "nothing is held: {:?}",
                held_keys(&f)
            );
        }
        // The Send/Receive with a right clock, on a drive that has lived 100 days since: it sweeps what is
        // held, and nothing of the Outbox is held.
        deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 100 * DAY);
        let report = run_at(&f, SEP_30 + 100 * DAY);
        assert_eq!(report.sweep_failed, None);
        assert_eq!(outbox_names(&root), both, "the Outbox is not the sweep's");
        // A Delete at a right clock holds both files, dated by that clock.
        delete_in_outbox(&root, "a-failed", SEP_30 + 100 * DAY).unwrap();
        assert!(outbox_names(&root).is_empty());
        let held = held_keys(&f);
        assert_eq!(held.len(), 2, "{held:?}");
        let place = holding("outbox", SEP_30 + 100 * DAY);
        assert!(held.iter().all(|key| key.starts_with(&place)), "{held:?}");
        // The same sweep keeps them 89 days (the drive and the clock agree it is that late) ...
        deliver(&f, "Inbox", &mail(3, "Three"), SEP_30 + 189 * DAY);
        run_at(&f, SEP_30 + 189 * DAY);
        assert_eq!(held_keys(&f), held, "89 days after the Delete: held");
        // ... and deletes them on the 90th.
        deliver(&f, "Inbox", &mail(4, "Four"), SEP_30 + 190 * DAY);
        run_at(&f, SEP_30 + 190 * DAY);
        assert!(held_keys(&f).is_empty(), "{:?}", held_keys(&f));
    }

    // ==== X-DELTA-APPS5-6: a floor that cannot be read is not "no Send/Receive yet" ====

    /// RED on the code before: `Clocks::read` took the listing of `mail/` that FAILED for no folder at all, so the
    /// floor was "never synced" - and on a clock that was behind (the case the rule exists for) the pull dated its
    /// hold by the wrong clock, which the next right-clock sweep deleted. Now the floor is an unreadable one: no hold
    /// is dated (the line stays, the window says why), the Receive goes on, and the sweep is skipped - the old hold
    /// is still there afterwards, and the next Send/Receive that can read the floor sweeps it.
    #[cfg(unix)]
    #[test]
    fn x_delta_apps5_6_a_listing_of_mail_that_fails_dates_no_hold_and_sweeps_nothing() {
        use std::os::unix::fs::PermissionsExt;
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        let two = deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        // The drive has lived 100 days since the old hold (the sweep needs it to agree).
        deliver(&f, "Spam", &mail(3, "Three"), SEP_30 + 100 * DAY);
        run(&f);
        f.bucket.inner.delete(&one).unwrap();
        run_at(&f, SEP_30);
        let first = holding("inbox", SEP_30);
        assert!(held_keys(&f).iter().any(|key| key.starts_with(&first)));
        // `mail/` cannot be listed for a moment (a scanner holding the folder, a permission); what is in it can be
        // read and written.
        f.bucket.inner.delete(&two).unwrap();
        let mail_folder = f.store.folder().path().join(store::MAIL_PREFIX);
        std::fs::set_permissions(&mail_folder, std::fs::Permissions::from_mode(0o111)).unwrap();
        let premise = std::fs::read_dir(&mail_folder).is_err();
        let behind = sync_account(
            &f.bucket,
            &f.store,
            &options_at(SEP_30 - 365 * DAY),
            &no_restore(),
            &mut |_| true,
        );
        let late = sync_account(
            &f.bucket,
            &f.store,
            &options_at(SEP_30 + 100 * DAY),
            &no_restore(),
            &mut |_| true,
        );
        std::fs::set_permissions(&mail_folder, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            premise,
            "the premise of the test: `mail/` cannot be listed (running as root?)"
        );
        // A clock a year back dates no hold: the line stays, the Receive goes on and says why.
        let behind = behind.expect("a floor that cannot be read is no failed Receive");
        assert_eq!(
            (behind.removed(), behind.kept_undated()),
            (0, 1),
            "{behind:?}"
        );
        let why = behind.hold_refused.clone().expect("the report says why");
        assert!(
            why.starts_with("the time of the last Send/Receive cannot be read ("),
            "{why}"
        );
        assert_eq!(held_rows(&f), ["Two"], "the line is where it was");
        assert!(
            !held_keys(&f)
                .iter()
                .any(|key| key.starts_with("removed/inbox/2025")),
            "no hold was dated by the wrong clock: {:?}",
            held_keys(&f)
        );
        // On a clock that would sweep the old hold, with the drive agreeing: nothing is swept while the floor cannot
        // be read (and nothing is held either).
        let late = late.expect("no failed Receive");
        assert_eq!((late.removed(), late.kept_undated()), (0, 1), "{late:?}");
        assert!(
            held_keys(&f).iter().any(|key| key.starts_with(&first)),
            "nothing was swept: {:?}",
            held_keys(&f)
        );
        // The listing works again: the sweep that was skipped runs, and the line that waited is held.
        let report = run_at(&f, SEP_30 + 100 * DAY);
        assert_eq!(
            (report.removed(), report.kept_undated(), report.sweep_failed),
            (1, 0, None)
        );
        assert!(
            !held_keys(&f).iter().any(|key| key.starts_with(&first)),
            "swept: {:?}",
            held_keys(&f)
        );
    }

    /// RED on the code before: a state file that was there and no state this AzMail reads was "no reading", so
    /// with every state damaged the floor was "never synced" and a clock a year back dated the hold. Now the floor
    /// is unreadable: the hold is refused with the file named, nothing is swept, and the Receive makes the states
    /// again (the record of the time went with them: the next one is as a first Send/Receive, which holds on a right
    /// clock).
    #[test]
    fn x_delta_apps5_6_states_that_are_damaged_date_no_hold_sweep_nothing_and_are_made_again() {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        let two = deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        deliver(&f, "Spam", &mail(3, "Three"), SEP_30 + 100 * DAY);
        run(&f);
        f.bucket.inner.delete(&one).unwrap();
        run_at(&f, SEP_30);
        let first = holding("inbox", SEP_30);
        let before = held_keys(&f);
        assert!(before.iter().any(|key| key.starts_with(&first)));
        // Every folder's state is damaged (a disk that lost its way, a program that wrote over them).
        let folders = f.store.folders();
        assert!(folders.len() >= 2, "{folders:?}");
        for key in &folders {
            f.store
                .put(&store::state_key(key), b"{ this is not a state")
                .unwrap();
        }
        f.bucket.inner.delete(&two).unwrap();
        for now in [SEP_30 - 365 * DAY, SEP_30 + 100 * DAY] {
            let report = run_at(&f, now);
            assert_eq!(
                (report.removed(), report.kept_undated()),
                (0, 1),
                "{now}: {report:?}"
            );
            assert!(
                report
                    .hold_refused
                    .as_deref()
                    .is_some_and(|why| why.contains("is no state this AzMail reads")),
                "{now}: {report:?}"
            );
            assert_eq!(held_keys(&f), before, "{now}: nothing was held or swept");
            assert_eq!(held_rows(&f), ["Two"], "{now}");
            // The Receive wrote the states again, with no stamp: the record of the time was lost with them.
            for key in &folders {
                let state = read_state(&f.store, key).unwrap().unwrap();
                assert_eq!(state.synced_at, "", "{now}: {key}");
            }
            // Damaged again for the next clock.
            for key in &folders {
                f.store
                    .put(&store::state_key(key), b"{ this is not a state")
                    .unwrap();
            }
        }
        // States that read again (a first Send/Receive as far as the clock goes): a right clock holds the line.
        run_at(&f, SEP_30 + 100 * DAY);
        let report = run_at(&f, SEP_30 + 100 * DAY);
        assert_eq!(
            (report.removed(), report.kept_undated(), report.sweep_failed),
            (1, 0, None)
        );
        assert!(
            !held_keys(&f).iter().any(|key| key.starts_with(&first)),
            "and the old hold is swept now: {:?}",
            held_keys(&f)
        );
    }

    // ==== X-DELTA-APPS4-4 + X-DELTA-APPS4-5: what is still in the drive is not "removed", and the window says where ====

    /// What another device's MOVE does to the drive: the object is copied to the same name in the new
    /// folder and the old one is deleted. Returns the new key.
    fn moved_on_the_drive(f: &Fixture, key: &str, folder: &str) -> String {
        let name = key.rsplit('/').next().unwrap();
        let to = azlin::message_key(folder, name);
        f.bucket.inner.copy(key, &to).unwrap();
        f.bucket.inner.delete(key).unwrap();
        to
    }

    fn downloads(f: &Fixture) -> usize {
        f.bucket.gets.lock().unwrap().len()
    }

    /// RED on the code before: every message another device archived was "gone" from the Inbox - held
    /// under `removed/` for 90 days, counted in "N messages were removed from the drive" - and fetched
    /// again as new mail in Archive: kept twice, downloaded twice, and the window said mail was lost
    /// that the drive has. Now the line follows its object before any folder is pulled: the same file
    /// in the new folder's tree, under that folder's UID, and nothing held, said or downloaded.
    #[test]
    fn x_delta_apps4_4_messages_moved_to_another_folder_are_not_held_fetched_again_or_said() {
        let f = fixture();
        let keys: Vec<String> = (0..300_u32)
            .map(|n| {
                deliver(
                    &f,
                    "Inbox",
                    &mail(n, &format!("Mail {n}")),
                    SEP_30 + i64::from(n),
                )
            })
            .collect();
        run(&f);
        assert_eq!(index(&f.store, "inbox").len(), 300);
        let fetched = downloads(&f);
        let archived: Vec<String> = keys
            .iter()
            .map(|key| moved_on_the_drive(&f, key, "Archive"))
            .collect();
        let report = run(&f);
        assert_eq!(report.removed(), 0, "moved is not removed");
        assert_eq!(report.fetched(), 0, "and is not new mail");
        assert!(
            drive_sentences(&report).is_empty(),
            "no sentence: {:?}",
            drive_sentences(&report)
        );
        assert!(
            held_keys(&f).is_empty(),
            "nothing is held: {:?}",
            held_keys(&f)
        );
        assert_eq!(downloads(&f), fetched, "nothing is downloaded again");
        assert!(index(&f.store, "inbox").is_empty());
        let lines = index(&f.store, "archive");
        assert_eq!(lines.len(), 300);
        for line in &lines {
            assert!(
                archived.contains(&line.remote),
                "its line names the object in its new folder: {}",
                line.remote
            );
            assert!(line.path.starts_with("mail/archive/"), "{}", line.path);
            let n: u32 = line.subject.trim_start_matches("Mail ").parse().unwrap();
            assert_eq!(
                f.store.get(&line.path).unwrap(),
                mail(n, &line.subject),
                "the file followed its line"
            );
        }
        let mut uids: Vec<u32> = lines.iter().map(|line| line.uid).collect();
        uids.sort_unstable();
        assert_eq!(
            uids,
            (1..=300).collect::<Vec<u32>>(),
            "the new folder's own UIDs"
        );
        // The next Send/Receive changes nothing.
        let again = run(&f);
        assert_eq!((again.removed(), again.fetched()), (0, 0));
        assert_eq!(index(&f.store, "archive"), lines);
    }

    /// A folder another device renamed: every line of the old folder follows its object into the new
    /// one, with the marks made here (a mark waits in `flags.json` under the line's UID, which the
    /// move changes), and the old folder is gone here as it is in the drive.
    #[test]
    fn x_delta_apps4_4_a_folder_renamed_elsewhere_takes_its_lines_and_marks_with_it() {
        let f = fixture();
        let one = deliver(&f, "Work/Projects", &mail(1, "Kickoff"), SEP_30);
        let two = deliver(&f, "Work/Projects", &mail(2, "Plan"), SEP_30 + 60);
        run(&f);
        let old = index(&f.store, "Work.Projects");
        assert_eq!(old.len(), 2);
        // A mark made here, not pushed yet: the second message was opened.
        let mut marks = LocalFlags::create();
        marks.read.insert(old[1].uid, true);
        write_marks(&f.store, "Work.Projects", &marks).unwrap();
        let fetched = downloads(&f);
        moved_on_the_drive(&f, &one, "Work/Plans");
        moved_on_the_drive(&f, &two, "Work/Plans");
        // An object store keeps no empty folder, so nothing is listed under the old name any more (a folder
        // on disk would keep it listed, empty).
        assert!(f
            .bucket
            .inner
            .remove_empty_folder(&azlin::folder_prefix("Work/Projects"))
            .unwrap());
        let report = run(&f);
        assert_eq!(
            (report.removed(), report.fetched()),
            (0, 0),
            "renamed is not removed"
        );
        assert!(drive_sentences(&report).is_empty());
        assert!(held_keys(&f).is_empty(), "{:?}", held_keys(&f));
        assert_eq!(downloads(&f), fetched);
        assert!(
            !f.store.folders().contains(&String::from("Work.Projects")),
            "the old folder is gone here as there: {:?}",
            f.store.folders()
        );
        let lines = index(&f.store, "Work.Plans");
        let subjects: Vec<&str> = lines.iter().map(|line| line.subject.as_str()).collect();
        assert_eq!(subjects, ["Kickoff", "Plan"]);
        for line in &lines {
            assert!(line.path.starts_with("mail/Work.Plans/"), "{}", line.path);
            assert!(
                f.store.size_of(&line.path).is_some(),
                "the file followed: {}",
                line.path
            );
        }
        // The mark followed its message: it was "Plan", the mark was pushed as the drive's marker
        // (the pass reads it back as the flag) and is not kept here any more.
        let id = azlin::message_id(&lines[1].remote).unwrap().to_string();
        assert!(
            f.bucket
                .inner
                .head(&azlin::marker_key(&id, azlin::SEEN))
                .is_ok(),
            "the mark made on the old folder's line reached the drive for the moved message"
        );
        assert_eq!(lines[1].flags, ["\\Seen"]);
        assert_eq!(
            read_marks(&f.store, "Work.Projects").unwrap(),
            LocalFlags::create()
        );
        assert!(read_marks(&f.store, "Work.Plans").unwrap().read.is_empty());
    }

    /// The crash table of the follow, each row by stopping the process after a write: after the lines
    /// are in their new folder (the files are where they were), and after the files moved (the lines of
    /// the old folder are still there). The next Send/Receive finishes it - never a second line, a lost
    /// file, a held copy or a download.
    #[test]
    fn x_delta_apps4_4_a_follow_cut_after_any_write_is_finished_by_the_next_send_receive() {
        for cut_after in [Cut::Written, Cut::Moved] {
            let f = fixture();
            let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
            let two = deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
            run(&f);
            let fetched = downloads(&f);
            moved_on_the_drive(&f, &one, "Archive");
            moved_on_the_drive(&f, &two, "Archive");
            CUT_AFTER.with(|cut| cut.set(Some(cut_after)));
            let stopped = sync_account(&f.bucket, &f.store, &options(), &no_restore(), &mut |_| {
                true
            });
            CUT_AFTER.with(|cut| cut.set(None));
            assert!(
                matches!(stopped, Err(SyncError::Storage(_))),
                "{cut_after:?}: {stopped:?}"
            );
            // The process starts again.
            let report = run(&f);
            assert_eq!(
                (report.removed(), report.fetched()),
                (0, 0),
                "{cut_after:?}: nothing is held or fetched"
            );
            assert!(
                held_keys(&f).is_empty(),
                "{cut_after:?}: {:?}",
                held_keys(&f)
            );
            assert_eq!(downloads(&f), fetched, "{cut_after:?}");
            assert!(index(&f.store, "inbox").is_empty(), "{cut_after:?}");
            let lines = index(&f.store, "archive");
            let subjects: Vec<&str> = lines.iter().map(|line| line.subject.as_str()).collect();
            assert_eq!(subjects, ["One", "Two"], "{cut_after:?}: one line each");
            for (line, text) in lines.iter().zip([mail(1, "One"), mail(2, "Two")]) {
                assert_eq!(
                    f.store.get(&line.path).unwrap(),
                    text,
                    "{cut_after:?}: the file is under its new line"
                );
            }
        }
    }

    /// What is really gone is still removed - and counted alone: two messages another device moved
    /// and one it deleted.
    #[test]
    fn x_delta_apps4_4_only_what_is_really_gone_is_counted_as_removed() {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        let two = deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        let three = deliver(&f, "Inbox", &mail(3, "Three"), SEP_30 + 120);
        run(&f);
        moved_on_the_drive(&f, &one, "Archive");
        moved_on_the_drive(&f, &two, "Archive");
        f.bucket.inner.delete(&three).unwrap();
        let report = run(&f);
        assert_eq!(report.removed(), 1);
        assert_eq!(report.fetched(), 0);
        crate::l10n::in_english();
        let said = drive_sentences(&report);
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].starts_with("1 message of "), "{said:?}");
        assert_eq!(index(&f.store, "archive").len(), 2);
    }

    /// A message another program marked `\Deleted` and has not expunged is still in the drive: its line
    /// stays in the index (flagged, so the window does not show it), nothing is held or said, and when
    /// the mark goes the same line is back - no download, the same UID. Expunged, it is really gone: held
    /// and said. RED on the code before: the line was held the moment the mark came and fetched again as
    /// new mail under a new UID when it went.
    #[test]
    fn x_delta_apps4_4_a_message_marked_deleted_elsewhere_stays_hidden_and_is_not_held() {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        run(&f);
        let uids: Vec<u32> = index(&f.store, "inbox")
            .iter()
            .map(|line| line.uid)
            .collect();
        let fetched = downloads(&f);
        let id = azlin::message_id(&one).unwrap().to_string();
        let mark = azlin::marker_key(&id, azlin::DELETED);
        f.bucket.inner.put(&mark, &[]).unwrap();
        let report = run(&f);
        assert_eq!((report.removed(), report.fetched()), (0, 0));
        assert!(
            drive_sentences(&report).is_empty(),
            "{:?}",
            drive_sentences(&report)
        );
        assert!(held_keys(&f).is_empty(), "{:?}", held_keys(&f));
        let lines = index(&f.store, "inbox");
        assert_eq!(lines.len(), 2, "the line stays");
        assert!(
            lines[0].flags.iter().any(|flag| flag == HIDDEN_FLAG),
            "{:?}",
            lines[0].flags
        );
        let shown: Vec<String> = crate::read_index(&f.store, "inbox")
            .into_iter()
            .map(|line| line.subject)
            .collect();
        assert_eq!(shown, ["Two"], "the window does not show it");
        // The mark goes: the same line is back.
        f.bucket.inner.delete(&mark).unwrap();
        let report = run(&f);
        assert_eq!((report.removed(), report.fetched()), (0, 0));
        let lines = index(&f.store, "inbox");
        assert_eq!(lines.iter().map(|line| line.uid).collect::<Vec<_>>(), uids);
        assert!(lines[0].flags.iter().all(|flag| flag != HIDDEN_FLAG));
        assert_eq!(crate::read_index(&f.store, "inbox").len(), 2);
        assert_eq!(downloads(&f), fetched, "nothing was downloaded");
        // Marked, then expunged: really gone, held and said.
        f.bucket.inner.put(&mark, &[]).unwrap();
        run(&f);
        f.bucket.inner.delete(&one).unwrap();
        let report = run(&f);
        assert_eq!(report.removed(), 1);
        assert_eq!(drive_sentences(&report).len(), 1);
        assert_eq!(held_rows(&f), ["Two"]);
    }

    /// A drive that lists NOTHING (an empty listing nobody announced: a new drive pointed at an account
    /// with mail) still holds every line as before - and the sentence names the folder and says where the
    /// mail is: `removed/<folder>/`. In both languages.
    #[test]
    fn x_delta_apps4_5_the_sentence_names_the_folder_and_where_the_held_mail_is() {
        let f = fixture();
        let keys: Vec<String> = (0..3_u32)
            .map(|n| {
                deliver(
                    &f,
                    "Inbox",
                    &mail(n, &format!("Mail {n}")),
                    SEP_30 + i64::from(n),
                )
            })
            .collect();
        run(&f);
        for key in &keys {
            f.bucket.inner.delete(key).unwrap();
        }
        let report = run(&f);
        assert_eq!(report.removed(), 3);
        let display = &report
            .folders
            .iter()
            .find(|f| f.removed > 0)
            .unwrap()
            .display;
        crate::l10n::in_english();
        let said = drive_sentences(&report);
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(
            said[0].contains(&format!(
                "3 messages of {display} were removed from the drive"
            )) && said[0].contains("for 90 days, in removed/inbox/."),
            "{said:?}"
        );
        azul_appkit::l10n::set_locale("de-DE");
        let said = drive_sentences(&report);
        crate::l10n::in_english();
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(
            said[0].contains(&format!(
                "3 Nachrichten aus {display} wurden aus dem Laufwerk entfernt"
            )) && said[0].contains("90 Tage auf diesem Computer, in removed/inbox/."),
            "{said:?}"
        );
        assert!(
            held_keys(&f)
                .iter()
                .all(|key| key.starts_with("removed/inbox/")),
            "the sentence's place is where the mail is: {:?}",
            held_keys(&f)
        );
    }

    /// A sweep that cannot read its list says so in the report - one sentence, in both languages - and
    /// deletes nothing: it was on stderr only. Here the folder `removed/` cannot be read.
    #[cfg(unix)]
    #[test]
    fn x_delta_apps4_5_a_sweep_that_cannot_run_is_told_in_the_report() {
        use std::os::unix::fs::PermissionsExt;
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        // The drive keeps mail, so that it has a reading of the time to agree with.
        deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        run(&f);
        f.bucket.inner.delete(&one).unwrap();
        run_at(&f, SEP_30);
        let folder = f.store.folder().path().join(store::REMOVED_PREFIX);
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o000)).unwrap();
        assert!(
            std::fs::read_dir(&folder).is_err(),
            "the premise of the test: the folder cannot be read (running as root?)"
        );
        let report = run_at(&f, SEP_30 + 100 * DAY);
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
        let why = report.sweep_failed.clone().expect("the sweep failed");
        crate::l10n::in_english();
        let said = drive_sentences(&report);
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(
            said[0].starts_with("The mail removed from the drive could not be cleaned up (")
                && said[0].contains(&why)
                && said[0].ends_with("it stays on this computer."),
            "{said:?}"
        );
        azul_appkit::l10n::set_locale("de-DE");
        let said = drive_sentences(&report);
        crate::l10n::in_english();
        assert!(said[0].contains("ließen sich nicht aufräumen"), "{said:?}");
        assert!(!held_keys(&f).is_empty(), "nothing was deleted");
    }

    /// The crash table of the hold, each row by stopping the flow after a write.
    ///
    /// Row 1: the move is cut after the first message (the second's name in the holding folder is
    /// taken): the first message and its line are out, the second stays with its line and its
    /// file, the pull ends with the error, the next pull finishes.
    #[test]
    fn x_delta_apps3_3_a_move_cut_after_the_first_message_leaves_the_rest_where_it_was() {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        let two = deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        run(&f);
        let lines = index(&f.store, "inbox");
        f.bucket.inner.delete(&one).unwrap();
        f.bucket.inner.delete(&two).unwrap();
        let held = holding("inbox", SEP_30);
        let second = lines[1].path.strip_prefix("mail/inbox/").unwrap();
        f.store.put(&format!("{held}/{second}"), b"taken").unwrap();
        let failed = sync_account(&f.bucket, &f.store, &options(), &no_restore(), &mut |_| {
            true
        });
        assert!(matches!(failed, Err(SyncError::Storage(_))), "{failed:?}");
        let rows = held_rows(&f);
        assert_eq!(rows, ["Two"], "the first line is out, the second stays");
        assert_eq!(
            f.store.get(&lines[1].path).unwrap(),
            mail(2, "Two"),
            "its file was not touched"
        );
        assert_eq!(f.store.size_of(&lines[0].path), None);
        let first = lines[0].path.strip_prefix("mail/inbox/").unwrap();
        assert_eq!(
            f.store.get(&format!("{held}/{first}")).unwrap(),
            mail(1, "One")
        );
        // The next pull (the name is free again at the next stamp) finishes.
        let report = run_at(&f, SEP_30 + 60);
        assert_eq!(report.removed(), 1);
        assert!(held_rows(&f).is_empty());
        let later = holding("inbox", SEP_30 + 60);
        assert_eq!(
            f.store.get(&format!("{later}/{second}")).unwrap(),
            mail(2, "Two")
        );
        assert_eq!(
            f.store.get(&format!("{held}/{second}")).unwrap(),
            b"taken",
            "nothing is ever replaced"
        );
    }

    /// The subjects of the lines of the folder's index here.
    fn held_rows(f: &Fixture) -> Vec<String> {
        index(&f.store, "inbox")
            .into_iter()
            .map(|e| e.subject)
            .collect()
    }

    /// Row 2: the holding folder of this stamp exists already (a pull that was cut in the same
    /// second wrote its record first): the lines of that record are kept, the new ones join them.
    #[test]
    fn x_delta_apps3_3_a_holding_folder_that_exists_already_keeps_its_record() {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        run(&f);
        f.bucket.inner.delete(&one).unwrap();
        let earlier = IndexEntry {
            uid: 99,
            subject: String::from("From the cut pull"),
            path: String::from("mail/inbox/2026/09/99.eml"),
            remote: String::from("mail/Inbox/earlier.eml"),
            ..IndexEntry::default()
        };
        let record_key = format!("{}/index.jsonl", holding("inbox", SEP_30));
        f.store
            .put(&record_key, store::index_to_jsonl(&[earlier]).as_bytes())
            .unwrap();
        run(&f);
        let record =
            store::index_from_jsonl(&String::from_utf8(f.store.get(&record_key).unwrap()).unwrap());
        let subjects: Vec<&str> = record.iter().map(|e| e.subject.as_str()).collect();
        assert_eq!(subjects, ["One", "From the cut pull"], "{subjects:?}");
    }

    /// Row 3: the local store cannot be written (a file stands where the holding area's folder
    /// would be, as a full disk would refuse it): the pull ends with the error before it moves
    /// anything or touches the drive; every line and file stays; the next pull, once it can write,
    /// does it.
    #[test]
    fn x_delta_apps3_3_a_store_that_cannot_be_written_ends_the_pull_before_it_moves_or_touches_anything(
    ) {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        run(&f);
        let lines = index(&f.store, "inbox");
        f.bucket.inner.delete(&one).unwrap();
        let drive_before = drive_keys(&f);
        let blocker = f.store.folder().path().join(store::REMOVED_PREFIX);
        std::fs::write(&blocker, b"a file, not a folder").unwrap();
        let failed = sync_account(&f.bucket, &f.store, &options(), &no_restore(), &mut |_| {
            true
        });
        assert!(matches!(failed, Err(SyncError::Storage(_))), "{failed:?}");
        assert_eq!(index(&f.store, "inbox"), lines, "every line stays");
        assert_eq!(
            f.store.get(&lines[0].path).unwrap(),
            mail(1, "One"),
            "and every file"
        );
        assert_eq!(drive_keys(&f), drive_before, "the drive was not touched");
        std::fs::remove_file(&blocker).unwrap();
        let report = run(&f);
        assert_eq!(report.removed(), 1, "writable again: the pull does it");
        assert!(held_keys(&f).len() == 2);
    }

    /// A message whose file is not here (a big one of which only the header block was fetched) has
    /// nothing to move: its line is in the record, which is all there is of it.
    #[test]
    fn x_delta_apps3_3_a_header_only_line_is_recorded_when_it_leaves() {
        let f = fixture();
        let mut big = mail(5, "Holiday photos");
        big.extend(std::iter::repeat_n(b'x', 5000));
        let key = deliver(&f, "Inbox", &big, SEP_30);
        let small = AzlinOptions {
            full_fetch_limit: 1000,
            head_bytes: 512,
            ..options()
        };
        sync_account(&f.bucket, &f.store, &small, &no_restore(), &mut |_| true).unwrap();
        f.bucket.inner.delete(&key).unwrap();
        let report =
            sync_account(&f.bucket, &f.store, &small, &no_restore(), &mut |_| true).unwrap();
        assert_eq!(report.removed(), 1);
        assert!(index(&f.store, "inbox").is_empty());
        let record_key = format!("{}/index.jsonl", holding("inbox", SEP_30));
        let record =
            store::index_from_jsonl(&String::from_utf8(f.store.get(&record_key).unwrap()).unwrap());
        assert_eq!(
            (record.len(), record[0].subject.as_str()),
            (1, "Holiday photos")
        );
        assert_eq!(
            held_keys(&f),
            [record_key],
            "only the record: there was no file"
        );
    }
    // ==== X-DELTA-APPS3-13: a restore puts back only the mail it lost ====

    /// The index lines of `folder` by subject, with the stamp each carries.
    fn since_of(f: &Fixture, folder: &str) -> Vec<(String, Option<i64>)> {
        index(&f.store, folder)
            .into_iter()
            .map(|entry| (entry.subject, entry.on_drive_since))
            .collect()
    }

    /// `announced(epoch)` with the restored data's moment.
    fn announced_as_of(epoch: u64, as_of: i64) -> AzlinOptions {
        AzlinOptions {
            restore: Some(RestoreNotice {
                epoch,
                as_of: Some(as_of),
                ..RestoreNotice::default()
            }),
            ..options()
        }
    }

    /// RED on the code before: the restore pass re-uploaded EVERY line the restored drive lacked,
    /// so mail deleted elsewhere before the restore's moment came back as "lost". Three mails: A and B
    /// were first seen here before the moment, C after it. A was deleted elsewhere before it; the
    /// restored drive lacks A and C (C arrived after) and has B. Only C goes back; A is an ordinary
    /// delete, held; B stays.
    #[test]
    fn x_delta_apps3_13_a_restore_puts_back_only_the_mail_newer_than_its_moment() {
        let f = fixture();
        let a = deliver(&f, "Inbox", &mail(1, "A"), SEP_30);
        run_at(&f, SEP_30);
        let b = deliver(&f, "Inbox", &mail(2, "B"), SEP_30 + 10);
        run_at(&f, SEP_30 + 10);
        let c = deliver(&f, "Inbox", &mail(3, "C"), SEP_30 + 100);
        run_at(&f, SEP_30 + 100);
        assert_eq!(
            since_of(&f, "inbox"),
            [
                (String::from("A"), Some(SEP_30)),
                (String::from("B"), Some(SEP_30 + 10)),
                (String::from("C"), Some(SEP_30 + 100))
            ],
            "every line says when this computer first saw its object in the drive"
        );
        // The restored drive (as of SEP_30 + 50): A was deleted before, C came after.
        f.bucket.inner.delete(&a).unwrap();
        f.bucket.inner.delete(&c).unwrap();
        let options = AzlinOptions {
            now: SEP_30 + 200,
            ..announced_as_of(1, SEP_30 + 50)
        };
        let report = sync_with(&f.bucket, &f, &options).unwrap();
        assert_eq!(
            (
                report.pushed(),
                report.kept_after_restore(),
                report.removed()
            ),
            (1, 1, 1),
            "C goes back, A is an ordinary delete"
        );
        assert!(missing(&f.bucket.inner, &a), "A is not put back");
        assert!(!missing(&f.bucket.inner, &b), "B was never gone");
        let back = index(&f.store, "inbox");
        let subjects: Vec<&str> = back.iter().map(|e| e.subject.as_str()).collect();
        assert_eq!(subjects, ["B", "C"], "A left the folder, held 90 days");
        assert!(!f.store.keys(store::REMOVED_PREFIX).is_empty());
        let c_line = back.iter().find(|e| e.subject == "C").unwrap();
        assert!(
            !missing(&f.bucket.inner, &c_line.remote),
            "C is in the drive again"
        );
        assert_eq!(
            c_line.on_drive_since,
            Some(SEP_30 + 200),
            "first seen there again now"
        );
    }

    /// A notice that names no moment, and a line that has no stamp (from before the field), keep the
    /// old answer - everything the restored drive lacks goes back - and the first pass gives every
    /// line in the drive its stamp: the first sync after the upgrade is the one conservative pass.
    #[test]
    fn x_delta_apps3_13_without_a_moment_or_a_stamp_everything_goes_back_once_and_every_line_gets_its_stamp(
    ) {
        let f = fixture();
        let one = deliver(&f, "Inbox", &mail(1, "One"), SEP_30);
        let two = deliver(&f, "Inbox", &mail(2, "Two"), SEP_30 + 60);
        run(&f);
        // No moment in the notice: both go back.
        f.bucket.inner.delete(&one).unwrap();
        f.bucket.inner.delete(&two).unwrap();
        let report = sync_with(&f.bucket, &f, &announced(1)).unwrap();
        assert_eq!(
            (
                report.pushed(),
                report.kept_after_restore(),
                report.removed()
            ),
            (2, 2, 0)
        );
        // Lines from before the field: the stamps are taken away (an index written by the old code).
        let mut lines = read_index(&f.store, "inbox").unwrap();
        for line in lines.values_mut() {
            line.on_drive_since = None;
        }
        write_index(&f.store, "inbox", &lines).unwrap();
        for line in lines.values() {
            f.bucket.inner.delete(&line.remote).unwrap();
        }
        // A second restore WITH a moment finds no stamp on the lines: the same conservative answer.
        let report = sync_with(&f.bucket, &f, &announced_as_of(2, SEP_30 + 50)).unwrap();
        assert_eq!(
            (
                report.pushed(),
                report.kept_after_restore(),
                report.removed()
            ),
            (2, 2, 0),
            "no stamp: the line is treated as it always was"
        );
        // And the first pass after the upgrade gives every line it sees in the drive its stamp.
        let g = fixture();
        deliver(&g, "Inbox", &mail(1, "One"), SEP_30);
        run(&g);
        let mut lines = read_index(&g.store, "inbox").unwrap();
        for line in lines.values_mut() {
            line.on_drive_since = None;
        }
        write_index(&g.store, "inbox", &lines).unwrap();
        run_at(&g, SEP_30 + 500);
        assert_eq!(
            since_of(&g, "inbox"),
            [(String::from("One"), Some(SEP_30 + 500))]
        );
    }

    /// RED on the code before (as a cut pass): the memory said nothing of the restore a pass was
    /// taking, and a memory that cannot be written failed only after every put. Now the restore is
    /// written down BEFORE the first message goes back: a pass whose very first put fails has
    /// already said what it was taking.
    #[test]
    fn x_delta_apps3_13_a_restore_pass_writes_down_what_it_takes_before_it_puts_anything_back() {
        let f = fixture();
        let first = deliver(&f, "Inbox", &mail(1, "First"), SEP_30);
        run(&f);
        f.bucket.inner.delete(&first).unwrap();
        let drive = Faulty::of(&f);
        *drive.puts_left.lock().unwrap() = Some(0);
        let stopped = sync_with(&drive, &f, &announced(1));
        assert!(matches!(stopped, Err(SyncError::Connect(_))), "{stopped:?}");
        let text = String::from_utf8(f.store.get(DRIVE_FILE).unwrap()).unwrap();
        let memory: DriveMemory = serde_json::from_str(&text).unwrap();
        assert_eq!((memory.restore_epoch, memory.taking), (0, 1), "{text}");
        // The pass that finishes takes it: the epoch is the memory's, nothing is being taken.
        *drive.puts_left.lock().unwrap() = None;
        sync_with(&drive, &f, &announced(1)).unwrap();
        let memory: DriveMemory =
            serde_json::from_slice(&f.store.get(DRIVE_FILE).unwrap()).unwrap();
        assert_eq!((memory.restore_epoch, memory.taking), (1, 0));
    }
}
