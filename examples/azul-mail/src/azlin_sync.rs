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
//!    the drive (below), and never when the token server cannot be asked. They are NOT deleted:
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
    time::{amz_date, parse_iso8601},
    ByteRange, Drive, DriveError, ObjectInfo,
};
use serde::{Deserialize, Serialize};

use crate::{
    azlin::{self, AzlinSession, MessageState},
    folders::{LocalMailbox, Role},
    listing::{self, LocalFlags},
    message,
    store::{self, FolderState, IndexEntry, MailStore},
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
        }
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

/// Takes `leaving`, the lines of the folder `folder` whose objects the drive no longer has, out of
/// `index` - and keeps what they were: a pull never destroys a copy, so a delete on another device
/// or the user's own "Restore the drive as of ..." (one ordinary commit that nobody announces)
/// costs the mail on this computer nothing for [`HOLD_DAYS`] days (X-DELTA-APPS3-3).
///
/// The order, every step a crash cell: (1) the record `removed/<folder>/<stamp>/index.jsonl` of
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
    now: i64,
    index: &mut BTreeMap<u32, IndexEntry>,
    report: &mut FolderReport,
) -> Result<(), SyncError> {
    if leaving.is_empty() {
        return Ok(());
    }
    // A holding folder is dated by the clock of this computer; without one it would be dated 1970
    // and swept at once.
    let Some(at) = u64::try_from(now).ok().filter(|secs| *secs > 0) else {
        return Err(SyncError::Storage(String::from(
            "the clock of this computer is not set: nothing is removed from the folder",
        )));
    };
    let held = store::removed_prefix(folder, &amz_date(at));
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
/// `now`. Returns how many it deleted. Never early: a stamp that is no date, one in the future (the
/// clock was stepped back since) and a key of another shape stay, and so does everything when the
/// list of `removed/` cannot be read - a list that failed is no list with nothing in it. The only
/// thing that deletes a copy the drive no longer lists.
///
/// # Errors
///
/// The list could not be read, or a file could not be deleted: nothing more is deleted.
pub fn sweep_removed(store: &MailStore, now: i64) -> Result<u64, SyncError> {
    sweep_removed_with(store, now, &|prefix| store.keys_checked(prefix))
}

/// [`sweep_removed`] with the listing a test can make fail.
fn sweep_removed_with(
    store: &MailStore,
    now: i64,
    list: &dyn Fn(&str) -> std::io::Result<Vec<String>>,
) -> Result<u64, SyncError> {
    // No clock, no expiry.
    let Some(now) = u64::try_from(now).ok().filter(|secs| *secs > 0) else {
        return Ok(0);
    };
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
    // Housekeeping, after the mail is synced: what was held [`HOLD_DAYS`] days ago goes. A sweep
    // that cannot read its list deletes nothing and never fails the sync; the report says so.
    report.sweep_failed = sweep_removed(store, options.now)
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
    let listed_keys: HashSet<&str> = listed.iter().map(|object| object.key.as_str()).collect();
    let mut index = read_index(store, key)?;
    let mut state = FolderState::create(path, &mailbox.display, AZLIN_UIDVALIDITY);
    state.last_uid = read_state(store, key)?.map_or(0, |old| old.last_uid);
    let mut marks = read_marks(store, key)?;
    let marks_before = marks.clone();
    // What this folder does with what the drive no longer lists, decided BEFORE anything is put or
    // removed: when a line of the index names an object the listing lacks, the token server is
    // asked (once per Send/Receive) whether the drive was restored (X-DELTA-APPS3-2).
    let stance = pass.stance(
        index
            .values()
            .any(|entry| !entry.remote.is_empty() && !listed_keys.contains(entry.remote.as_str())),
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

        // 2. Pull: what is gone leaves, what is new comes.
        let gone: Vec<u32> = index
            .values()
            .filter(|entry| {
                !entry.remote.is_empty()
                    && !listed_keys.contains(entry.remote.as_str())
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
            // The listing's word is enough for a message another program hid with a mark. For
            // every other the drive is asked about the object itself: a listing that is short for
            // any reason must not take out a copy that may exist nowhere else, and a failed look
            // is an unknown, not an "absent" (the pass ends, nothing more leaves).
            if !on_drive.contains(entry.remote.as_str())
                && ops::exists(drive, &entry.remote).map_err(drive_error)?
            {
                continue;
            }
            leaving.push(entry.clone());
        }
        // What the drive no longer lists is held here, never deleted (X-DELTA-APPS3-3).
        hold(store, key, &leaving, options.now, &mut index, &mut report)?;
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
            let flags = states
                .get(id)
                .map(MessageState::imap_flags)
                .unwrap_or_default();
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
        state.synced_at = message::rfc3339_utc(options.now);
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
    if report.removed() > 0 {
        sentences.push(t_args(
            "azmail-removed-held",
            &[
                ("count", Arg::from(report.removed())),
                ("days", Arg::from(HOLD_DAYS)),
            ],
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

    /// Puts `bytes` into the drive's `folder` as the Worker would; returns its key.
    fn deliver(f: &Fixture, folder: &str, bytes: &[u8], stamp: i64) -> String {
        let key = azlin::message_key(folder, &azlin::object_name(bytes, stamp as u64));
        f.bucket.inner.put(&key, bytes).unwrap();
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
        // Shown on this computer first, then marked: it leaves the local copy.
        f.bucket.inner.delete(&deleted).unwrap();
        run(&f);
        assert_eq!(index(&f.store, "inbox").len(), 2);
        f.bucket.inner.put(&deleted, &[]).unwrap();
        let report = run(&f);
        assert_eq!(report.removed(), 1);
        let left: Vec<String> = index(&f.store, "inbox").into_iter().map(|e| e.remote).collect();
        assert_eq!(left, [kept]);
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
        // Let go means held 90 days (X-DELTA-APPS3-3), and the window says so.
        let said = drive_sentences(&report);
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(
            said[0].contains("2 messages were removed from the drive")
                && said[0].contains("90 days"),
            "{said:?}"
        );
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
        assert!(
            said[0].contains("1 message was removed from the drive")
                && said[0].contains("stays on this computer for 90 days"),
            "{said:?}"
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
        // The 89th day (and the last second of it): everything stays.
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
        let failed = sweep_removed_with(&f.store, SEP_30 + 400 * DAY, &failing);
        assert!(matches!(failed, Err(SyncError::Storage(_))), "{failed:?}");
        assert_eq!(held_keys(&f), before, "nothing was deleted");
        // The same sweep with a list that works deletes it: the failure was the list's.
        assert_eq!(sweep_removed(&f.store, SEP_30 + 400 * DAY).unwrap(), 1);
        assert!(held_keys(&f).is_empty());
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
