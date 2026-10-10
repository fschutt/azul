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
//!    copy; new ones are fetched - whole up to `full_fetch_limit`, else only their first
//!    `head_bytes` (the header block; [`fetch_message`] gets the rest when the message is
//!    opened) - and get the next UIDs in name (arrival) order.
//! 3. **Flags**: the marks made here (`flags.json`) are written as markers where the drive says
//!    otherwise, then dropped; every message's flags are the drive's markers from then on.
//!
//! The actions - [`move_messages`], [`delete_messages`], [`upload_message`], [`push_marks`],
//! [`fetch_message`] - write the drive first and the local copy after, so a refused request
//! changes nothing here. Every call blocks: azul Threads run them, one at a time
//! ([`lock_cache`]). No azul types here: tested with a folder on disk as the drive.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Mutex, MutexGuard},
};

use azul_storage::{ops, time::parse_iso8601, ByteRange, Drive, DriveError, ObjectInfo};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AzlinOptions {
    /// A message up to this many bytes is fetched whole.
    pub full_fetch_limit: u64,
    /// Of a bigger one only this many bytes: its header block.
    pub head_bytes: u64,
    /// Bytes per ranged GET when a big message is fetched ([`fetch_message`]).
    pub chunk: u64,
    /// Now, in seconds since 1970 (`synced_at`; the arrival of mail filed here without a date).
    pub now: i64,
}

impl Default for AzlinOptions {
    fn default() -> AzlinOptions {
        AzlinOptions {
            full_fetch_limit: 4 * 1024 * 1024,
            head_bytes: 64 * 1024,
            chunk: azul_storage::transfer::CHUNK,
            now: 0,
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
        DriveError::Denied { .. } => SyncError::Auth(text),
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

fn read_index(store: &MailStore, folder: &str) -> BTreeMap<u32, IndexEntry> {
    store
        .get(&store::index_key(folder))
        .map(|bytes| store::index_from_jsonl(&String::from_utf8_lossy(&bytes)))
        .unwrap_or_default()
        .into_iter()
        .map(|entry| (entry.uid, entry))
        .collect()
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

fn read_state(store: &MailStore, folder: &str) -> Option<FolderState> {
    store
        .get(&store::state_key(folder))
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|text| FolderState::from_json(&text))
}

fn read_marks(store: &MailStore, folder: &str) -> LocalFlags {
    store
        .get(&listing::flags_key(folder))
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|text| LocalFlags::from_json(&text))
        .unwrap_or_else(LocalFlags::create)
}

fn write_marks(store: &MailStore, folder: &str, marks: &LocalFlags) -> std::io::Result<()> {
    store.put(&listing::flags_key(folder), marks.to_json().as_bytes())
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
/// it), else the well-known folder of its role, else the key itself.
pub fn remote_path_of(store: &MailStore, key: &str) -> String {
    read_state(store, key)
        .map(|state| state.server_name)
        .filter(|name| !name.trim().is_empty())
        .or_else(|| azlin::well_known_name(Role::of_key(key)).map(str::to_string))
        .unwrap_or_else(|| key.to_string())
}

/// The object of the message `uid` of the local folder `folder`, when it is in the drive.
pub fn remote_of(store: &MailStore, folder: &str, uid: u32) -> Option<String> {
    read_index(store, folder)
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
pub fn sync_account(
    drive: &dyn Drive,
    store: &MailStore,
    options: &AzlinOptions,
    progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<SyncReport, SyncError> {
    let remote = list_mailbox(drive).map_err(drive_error)?;
    let paths: Vec<String> = remote.iter().map(|folder| folder.path.clone()).collect();
    let mut boxes = local_folders(&paths);
    // A folder this computer has that the drive no longer lists (emptied or deleted there): an
    // empty listing, so its messages leave too.
    for key in store.folders() {
        if boxes.iter().any(|mailbox| mailbox.key == key) {
            continue;
        }
        let Some(state) = read_state(store, &key) else {
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
            &mut states,
            options,
            progress,
        )?);
    }
    Ok(report)
}

/// Syncs one folder: push, pull, flags (the module documentation). `in_drive`: the drive lists
/// the folder.
#[allow(clippy::too_many_arguments)]
fn sync_folder(
    drive: &dyn Drive,
    store: &MailStore,
    mailbox: &LocalMailbox,
    listed: &[ObjectInfo],
    in_drive: bool,
    states: &mut HashMap<String, MessageState>,
    options: &AzlinOptions,
    progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<FolderReport, SyncError> {
    let key = mailbox.key.as_str();
    let path = mailbox.server_name.as_str();
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
    let mut index = read_index(store, key);
    let mut state = FolderState::create(path, &mailbox.display, AZLIN_UIDVALIDITY);
    state.last_uid = read_state(store, key).map_or(0, |old| old.last_uid);
    let mut marks = read_marks(store, key);
    let marks_before = marks.clone();
    let mut report = FolderReport {
        key: key.to_string(),
        display: mailbox.display.clone(),
        ..FolderReport::default()
    };

    let result = (|| -> Result<(), SyncError> {
        // 1. Push: mail filed here first goes into the folder (whatever the listing said, it
        //    is there now).
        let mut pushed: HashSet<String> = HashSet::new();
        let local_only: Vec<(u32, String, u64, Vec<String>)> = index
            .values()
            .filter(|entry| entry.remote.is_empty())
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
    let mut marks = read_marks(store, folder);
    if marks.read.is_empty() && marks.flagged.is_empty() {
        return Ok(0);
    }
    let mut index = read_index(store, folder);
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
    let to_path = remote_path_of(store, to);
    let mut source = read_index(store, from);
    let mut target = read_index(store, to);
    let mut source_marks = read_marks(store, from);
    let mut target_marks = read_marks(store, to);
    let mut state = read_state(store, to).unwrap_or_else(|| {
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
    let mut index = read_index(store, folder);
    let mut marks = read_marks(store, folder);
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
    let path = remote_path_of(store, folder);
    let mut index = read_index(store, folder);
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
    let index = read_index(store, folder);
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
pub fn sending_refused(ban: Option<&azcloud_kit::Ban>, now: u64) -> Option<String> {
    let ban = ban?;
    if ban.is_closed(now) {
        return Some(ban.closed_text());
    }
    Some(format!(
        "{} AzMail sends nothing from this account: its mail waits in the Outbox.",
        ban.banner(now)
    ))
}

#[cfg(test)]
mod tests {
    use azul_storage::{ListPage, ListRequest, LocalDrive};

    use super::*;
    use crate::testutil::{MailFolder, TempDir};

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
        }
    }

    fn run(f: &Fixture) -> SyncReport {
        sync_account(&f.bucket, &f.store, &options(), &mut |_| true).unwrap()
    }

    fn index(store: &MailStore, folder: &str) -> Vec<IndexEntry> {
        read_index(store, folder).into_values().collect()
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
        let state = read_state(&f.store, "spam").unwrap();
        assert_eq!(
            (state.server_name.as_str(), state.uidvalidity),
            ("Spam", AZLIN_UIDVALIDITY)
        );
        assert_eq!(
            read_state(&f.store, "Work.Projects").unwrap().display,
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
            read_marks(&f.store, "inbox"),
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
        assert!(read_marks(&f.store, "inbox").read.is_empty());
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
            ),
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
        let why = sending_refused(Some(&ban), until - 10 * 3_600).expect("sending stops");
        assert!(
            why.starts_with(
                "Due to spam distribution, your account has been banned, but you have 10 hours \
                 to migrate your files."
            ),
            "the same banner as AzDrive's: {why}"
        );
        assert!(why.contains("AzMail sends nothing"), "{why}");
        let closed = sending_refused(Some(&ban), until + 1).expect("closed");
        assert!(closed.starts_with("This drive was closed on 2026-10-12"), "{closed}");
        assert_eq!(sending_refused(None, until), None, "a drive in good standing sends");
    }
}
