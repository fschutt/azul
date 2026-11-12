//! An Azlin account's Send/Receive and actions (`AZLIN_MAIL.md`): the mailbox in the drive -
//! `mail/<Folder>/<name>.eml` and the markers under `mail/.state/` - and its copy on this
//! computer, the same files an IMAP account keeps (`store.rs`), every index line carrying its
//! object's key (`IndexEntry::remote`).
//!
//! Send/Receive ([`sync_account`]), for every folder of the drive and the well-known ones:
//!
//! 1. **Push**: mail filed here first (a sent mail, a draft saved while offline: no `remote`
//!    yet) goes into its folder of the drive under its name (`azlin::object_name`).
//! 2. **Pull**: the folder's listing against the index. Names that are gone leave the local
//!    copy - when the drive, asked about the object itself, says it is not there, and never
//!    after a restore of the drive (below); new ones are fetched - whole up to
//!    `full_fetch_limit`, else only their first `head_bytes` (the header block;
//!    [`fetch_message`] gets the rest when the message is opened) - and get the next UIDs in
//!    name (arrival) order.
//! 3. **Flags**: the marks made here (`flags.json`) are written as markers where the drive says
//!    otherwise, then dropped; every message's flags are the drive's markers from then on.
//!
//! A drive that was RESTORED (an operator's restore from its off-site copy, "Restore as of"; the
//! token server announces it with an epoch, [`AzlinOptions::restore`]) serves an older state, so
//! the mail that came after is missing from its listing and exists here only. That is not a
//! delete: the Send/Receive that takes the announcement deletes no mail here, puts the mail the
//! drive lacks into it again (a message of which only the header block is here stays as it is)
//! and says so ([`FolderReport::kept_after_restore`]); the epoch is remembered
//! ([`DRIVE_FILE`]) once every folder is through, so a pass that stopped takes the restore
//! again (X-DELTA-SYNC4-20; `restore.rs` is the one protocol of every drive, AzDrive's sync
//! decides the same way).
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
    azlin::{self, MessageState},
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
    /// ([`sync_account`]).
    pub restore: Option<RestoreNotice>,
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
    let memory = DriveMemory {
        format: DRIVE_FORMAT.to_string(),
        version: 1,
        restore_epoch: epoch,
    };
    let mut text = serde_json::to_string_pretty(&memory)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    text.push('\n');
    store.put(DRIVE_FILE, text.as_bytes())
}

/// What the drive is to this copy: whether `notice`, the newest announcement, is a restore this
/// copy has not taken yet ([`Standing::Restored`]). The drive's counter says nothing here (an
/// Azlin drive's mail is listed, its index is not read), so only the epoch decides
/// ([`Clock::Own`]); the one decision every drive's sync makes (`azul_storage::restore::judge`).
fn restore_standing(
    store: &MailStore,
    notice: Option<&RestoreNotice>,
) -> Result<Standing, SyncError> {
    let Some(notice) = notice else {
        return Ok(Standing::InSync);
    };
    let seen = Seen {
        epoch: read_restore_epoch(store)?,
        clock: 0,
    };
    Ok(judge(
        &seen,
        0,
        Clock::Own,
        Some(notice),
        azlin::MAIL_PREFIX,
    ))
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

// ==== The drive's folders ====

// Listing the drive's mailbox is azul-mail-core's (`azlin.rs`): the Azlin Bridge lists it the
// same way.
pub use crate::azlin::{list_mailbox, list_states, local_folders, RemoteFolder};

// ==== Send/Receive ====

/// Syncs the drive's mailbox and this computer's copy in `store` (see the module
/// documentation). `progress` is told what is happening; when it answers `false` the sync
/// records what it has and stops with [`SyncError::Stopped`].
///
/// An announced restore of the drive that this copy has not taken ([`AzlinOptions::restore`])
/// makes the whole call a restore's: no mail leaves this computer, what the drive lacks goes
/// into it again, and the epoch is remembered only after the last folder was synced.
pub fn sync_account(
    drive: &dyn Drive,
    store: &MailStore,
    options: &AzlinOptions,
    progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<SyncReport, SyncError> {
    // Before anything is listed or written: a restore is told from a delete by the announcement,
    // never by what the listing lacks.
    let standing = restore_standing(store, options.restore.as_ref())?;
    let restored = matches!(standing, Standing::Restored(_));
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
            restored,
            &mut states,
            options,
            progress,
        )?);
    }
    // Every folder is through: the restore is taken (a pass that stopped before this took it
    // again, which is idempotent: what was put back is listed now).
    if let Standing::Restored(notice) = &standing {
        write_restore_epoch(store, notice.epoch).map_err(io)?;
    }
    Ok(report)
}

/// Syncs one folder: push, pull, flags (the module documentation). `in_drive`: the drive lists
/// the folder. `restored`: the drive was restored and this copy takes it now (no mail leaves).
#[allow(clippy::too_many_arguments)]
fn sync_folder(
    drive: &dyn Drive,
    store: &MailStore,
    mailbox: &LocalMailbox,
    listed: &[ObjectInfo],
    in_drive: bool,
    restored: bool,
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
    state.last_uid = read_state(store, key)?.map_or(0, |old| old.last_uid);
    let mut marks = read_marks(store, key)?;
    let marks_before = marks.clone();
    let mut report = FolderReport {
        key: key.to_string(),
        display: mailbox.display.clone(),
        ..FolderReport::default()
    };

    let result = (|| -> Result<(), SyncError> {
        // 1. Push: mail filed here first goes into the folder (whatever the listing said, it
        //    is there now). After a restore also the mail the restored drive lacks: it came
        //    after the restore's moment and exists here only (its line keeps the old object's
        //    key until the new one is in the drive).
        let mut pushed: HashSet<String> = HashSet::new();
        let lost: HashSet<u32> = if restored {
            index
                .values()
                .filter(|entry| {
                    !entry.remote.is_empty() && !on_drive.contains(entry.remote.as_str())
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
                // fetched the header block of) has nothing to put back: it stays as it is.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && lost.contains(&uid) => {
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
            }
            pushed.insert(object);
            report.pushed += 1;
        }
        report.kept_after_restore = lost.len() as u64;

        // 2. Pull: what is gone leaves, what is new comes.
        let listed_keys: HashSet<&str> = listed.iter().map(|object| object.key.as_str()).collect();
        let gone: Vec<u32> = index
            .values()
            .filter(|entry| {
                !entry.remote.is_empty()
                    && !listed_keys.contains(entry.remote.as_str())
                    && !pushed.contains(&entry.remote)
            })
            .map(|entry| entry.uid)
            .collect();
        for uid in gone {
            // After a restore nothing leaves: what the listing lacks was lost, not deleted, and
            // exists here only (X-DELTA-SYNC4-20).
            if restored {
                continue;
            }
            let Some(remote) = index.get(&uid).map(|entry| entry.remote.clone()) else {
                continue;
            };
            // The listing's word is enough for a message another program hid with a mark. For
            // every other the drive is asked about the object itself: a listing that is short for
            // any reason must not delete a copy that may exist nowhere else, and a failed look
            // is an unknown, not an "absent" (the pass ends, nothing more leaves).
            if !on_drive.contains(remote.as_str())
                && ops::exists(drive, &remote).map_err(drive_error)?
            {
                continue;
            }
            if let Some(entry) = index.remove(&uid) {
                store.delete(&entry.path).map_err(io)?;
                report.removed += 1;
            }
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

    fn run(f: &Fixture) -> SyncReport {
        sync_account(&f.bucket, &f.store, &options(), &mut |_| true).unwrap()
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
        sync_account(&f.bucket, &f.store, &small, &mut |_| true).unwrap();
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
        if !crate::testutil::make_unreadable(&file) {
            return;
        }
        let failed = sync_account(&f.bucket, &f.store, &options(), &mut |_| true);
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
        if !crate::testutil::make_unreadable(&file) {
            return;
        }
        let failed = sync_account(&f.bucket, &f.store, &options(), &mut |_| true);
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
        if !crate::testutil::make_unreadable(&file) {
            return;
        }
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
        if !crate::testutil::make_unreadable(&file) {
            return;
        }
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
        sync_account(drive, &f.store, options, &mut |_| true)
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
        sync_account(&f.bucket, &f.store, &small, &mut |_| true).unwrap();
        f.bucket.inner.delete(&key).unwrap();
        let restored = AzlinOptions {
            restore: announced(1).restore,
            ..small
        };
        let report = sync_account(&f.bucket, &f.store, &restored, &mut |_| true).unwrap();
        assert_eq!(
            (
                report.removed(),
                report.pushed(),
                report.kept_after_restore()
            ),
            (0, 0, 1)
        );
        let lines = index(&f.store, "inbox");
        assert_eq!(lines.len(), 1, "the line stays");
        assert_eq!(lines[0].remote, key, "and keeps the key it had");
        assert_eq!(lines[0].subject, "Holiday photos");
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
        if !crate::testutil::make_unreadable(&file) {
            return;
        }
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
}
