//! AzDrive's folder sync as the window shows it (§13.7 of the client plan; the work is
//! azcloud-kit's `sync::session`, run by [`crate::sync_jobs`]):
//!
//! - A drive is paired with a folder on this computer ("Sync with a folder...", from the
//!   drive's menu in the source list or Share > Sync): by default `AzDrive/<drive name>` in
//!   Home, so it browses as a folder of the Home drive. The pairings and each drive's settings
//!   are in the view settings (`synced`); each pairing's states in the cache folder
//!   (`<cache>/sync/<drive>-<id>/`).
//! - Every file of a synced folder - and of an encrypted drive's own listing, whose names are
//!   the files' - has its STATE as a small icon after its name: cloud only (`cloud_queue`),
//!   downloading / uploading (`cloud_download` / `cloud_upload`), on this device
//!   (`check_circle`), on this device encrypted (`lock`), always kept (`push_pin`), a conflict
//!   (`warning`), an error (`error`) - the Material names of the icon set (Haiku's under
//!   flora). A cloud-only file is listed in its folder as a row of its own; opening it
//!   downloads (and decrypts) it first.
//! - The drive's STATUS LINE - "Up to date", "Syncing 12 files (340 MB)", "Paused",
//!   "Read-only (payment due)", "Waiting for you: 1 conflict" - is in the status bar of a
//!   synced folder and on the drive's row in the source list.
//! - A conflict asks (D52): "Someone changed this file" - keep mine, take theirs, keep both
//!   (or decide later).
//! - Options > Drives > Sync: per synced drive the auto-download, the local copies (an
//!   encrypted drive), the size cap, Sync now, Pause, Stop syncing.
//!
//! - An indexed drive's files carry §13.7's overlays from its search index (azul-search-index's
//!   `indexed_files`, read at each listing): a magnifier (`manage_search`) on a file the index
//!   read as it is now, a slashed one (`search_off`) on a file it never reads.
//!
//! SEAM for the kit's guards that stop a pass by themselves (the mass-delete guard today, the
//! burst / ransomware guard of CLIENT17 round 6, not in azcloud-kit yet): a guard's pause should
//! travel in the pass's `SyncStates` (written by azcloud-kit's `session::record` /
//! `after_failure`, as `read_only` and `last_error` are), show in [`status_text`] and
//! [`sidebar_state`] next to "Read-only" ("Paused: <why>"), and ask with a [`SyncDialog`] next
//! to `Conflict` - "Resume" (the guard allowed once: `SyncOptions::allow_mass_delete` for one
//! pass) and "Show the changes" (the files the pass would delete or change, from its plan).
//! Until then a tripped guard is the pass's error: "Not synced: <why>".
//!
//! - On a METERED or low-data network (azul's `NetworkState`, read by the poll timer: a phone's
//!   hotspot, a capped plan, Low Data Mode, Data Saver) a pass holds back the files over the
//!   drive's auto-download size, up and down ([`network_hold`]); small files and the polls go on.
//!   The status line and the drive's row say "Paused (metered network)"; Options > Drives > Sync
//!   has "Sync anyway on this network" (the setup's `sync_on_metered`).

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};

use azcloud_kit::sync::session::{AutoDownload, FileState, LocalCopies, SyncSetup, SyncStates};
use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType,
        DropDownOnChoiceChangeCallbackType, TextInputOnTextInputCallbackType,
    },
    prelude::*,
    str::String as AzString,
    vec::StringVec,
    widgets::{
        ButtonType, CheckBoxState, DropDown, OnTextInputReturn, TextInputState, TextInputValid,
    },
    window::NetworkState,
};

use azul_appkit::l10n::{self, t, t_phrase, t_text, Phrase, Text};

pub(crate) use crate::sync_store::SyncStore;
use crate::{
    browse::{self, Entry, Place},
    ids,
    sync_jobs::{self, PassProgress},
    ui_dialogs::{button, buttons, label, line, on_cancel_popup, text_line, typed_button},
    with_state, DriveState, Popup,
};

// ==== What the window keeps ====

/// A synced drive's life in this window (its files' states are in the [`SyncStore`]).
#[derive(Default)]
pub(crate) struct DriveSync {
    /// The pass running now.
    pub running: Option<Running>,
    /// Another pass once this one ends ("Sync now" while it ran, a pin).
    pub again: bool,
    /// The status line as last said (a change prints `AZDRIVE_SYNC_STATUS`).
    pub said: String,
}

/// A pass running: its stop and how far it got.
pub(crate) struct Running {
    pub cancel: Arc<AtomicBool>,
    pub progress: PassProgress,
}

/// Every synced drive's life in this window, by drive id.
#[derive(Default)]
pub(crate) struct SyncView {
    pub drives: HashMap<String, DriveSync>,
    /// The poll timer runs.
    pub timer: bool,
    /// The conflicts asked about by themselves already (`<drive id>\n<key>`): "Decide later"
    /// is not asked again at every pass - opening the file asks again.
    pub asked: HashSet<String>,
    /// Every synced drive's file states, shared with the search (its `SyncLookup`).
    pub store: SyncStore,
    /// The files each indexed drive's search index read, as of its last listing here (§13.7's
    /// "indexed" / "not indexable" overlays), by drive id.
    pub indexed: HashMap<String, azul_search_index::IndexedFiles>,
    /// The synced Azlin drives whose token server says they take no writes (unpaid past their
    /// grace): "Read-only (payment due)" - by drive id (the drives file's).
    pub payment_due: HashSet<String>,
    /// When a refused write last made AzDrive ask a drive's token server (seconds since 1970).
    pub asked_status: HashMap<String, u64>,
    /// A copy, move or download of a plain synced drive's own listing, waiting for its
    /// cloud-only files to come down.
    pub waiting_transfer: Option<sync_jobs::Transfer>,
    /// The network as the poll timer last read it (azul's `NetworkState`); `None` before.
    pub network: Option<NetworkState>,
}

/// The status line while a metered or low-data network holds big transfers back.
pub(crate) const METERED_STATUS: &str = "azdrive-sync-status-metered";

const MB: u64 = 1024 * 1024;

/// What the ribbon, the menus and the Options ask of a synced drive.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SyncAction {
    /// "Sync with a folder...": the pairing dialog.
    Pair,
    /// "Sync now".
    Now,
    /// "Always keep on this device" for the selected items (again: no longer).
    KeepOnDevice,
    /// "Free up space" for the selected items.
    FreeUpSpace,
    /// Pause syncing, or resume it.
    Pause,
    /// "Stop syncing": the pairing is forgotten (the files stay on both sides).
    Stop,
    /// The synced folder opens in the window.
    OpenFolder,
}

// ==== Plain data ====

/// The drive's status line (§13.7): paused, read-only, syncing (the files and bytes left), a
/// conflict waiting, the last error, never synced, up to date - from its `states` and the pass
/// `running`. An Azlin drive (`azlin`) is "Read-only (payment due)" when its token server says
/// it takes no writes (`payment_due`, its drive status); another drive is "Read-only" when it
/// refused a write. `held`: the network holds its big transfers back ([`network_hold`]).
#[must_use]
pub(crate) fn status_text(
    setup: &SyncSetup,
    states: &SyncStates,
    running: Option<&Running>,
    azlin: bool,
    payment_due: bool,
    held: bool,
) -> Phrase {
    if setup.paused {
        return Phrase::new("azdrive-sync-status-paused");
    }
    if azlin && payment_due {
        return Phrase::new("azdrive-sync-status-payment-due");
    }
    if !states.newer_format.is_empty() {
        return Phrase::new("azdrive-sync-status-newer-format");
    }
    if let Some(pause) = &states.burst {
        let said = match pause.reason {
            azcloud_kit::sync::guard::PauseReason::Burst => "azdrive-sync-status-burst",
            azcloud_kit::sync::guard::PauseReason::Encryption => "azdrive-sync-status-encrypted",
        };
        return Phrase::new(said).arg("count", pause.changes);
    }
    if let Some(asked) = &states.mass_delete {
        return Phrase::new("azdrive-sync-status-mass-delete").arg("count", asked.count);
    }
    if !azlin && states.read_only {
        return Phrase::new("azdrive-sync-status-read-only");
    }
    if held {
        return Phrase::new(METERED_STATUS);
    }
    if let Some(running) = running {
        let p = &running.progress;
        let files = p.files_total.saturating_sub(p.files_done);
        if files == 0 {
            return Phrase::new("azdrive-sync-status-syncing");
        }
        let bytes = p.bytes_total.saturating_sub(p.bytes_done);
        return Phrase::new("azdrive-sync-status-syncing-files")
            .arg("count", files)
            .arg("size", browse::format_size(Some(bytes)));
    }
    let conflicts = states.conflicts().len();
    if conflicts > 0 {
        return Phrase::new("azdrive-sync-status-conflicts").arg("count", conflicts);
    }
    if let Some(error) = &states.last_error {
        return Phrase::new("azdrive-sync-status-failed").arg("error", error.as_str());
    }
    if states.last_pass.is_none() {
        return Phrase::new("azdrive-sync-status-never");
    }
    Phrase::new("azdrive-sync-status-up-to-date")
}

/// Whether the status line says the drive takes no writes from here (its row's glyph).
fn says_read_only(status: &Text) -> bool {
    status.keys().iter().any(|key| {
        matches!(
            *key,
            "azdrive-sync-status-payment-due"
                | "azdrive-sync-status-newer-format"
                | "azdrive-sync-status-read-only"
        )
    })
}

/// What a file's sync state says, in a sentence (a row's badge, its accessible name).
#[must_use]
pub(crate) fn state_text(state: &FileState) -> Phrase {
    match state {
        FileState::CloudOnly => Phrase::new("azdrive-sync-state-cloud-only"),
        FileState::Downloading { .. } => Phrase::new("azdrive-sync-state-downloading"),
        FileState::Uploading { .. } => Phrase::new("azdrive-sync-state-uploading"),
        FileState::OnDevice => Phrase::new("azdrive-sync-state-on-device"),
        FileState::OnDeviceEncrypted => Phrase::new("azdrive-sync-state-on-device-encrypted"),
        FileState::Pinned => Phrase::new("azdrive-sync-state-pinned"),
        FileState::Conflict => Phrase::new("azdrive-sync-state-conflict"),
        FileState::Error(why) => Phrase::new("azdrive-sync-state-error").arg("error", why.as_str()),
    }
}

/// Whether a pass of `setup`'s drive on `network` holds big transfers back, and from which
/// size on: on a metered or low-data network (a phone's hotspot, Low Data Mode, Data Saver)
/// the files over the auto-download size ("New files under N MB"; the default's size for the
/// other choices) wait - uploads and downloads - while small files and the polls go on. `None`
/// on a free network, offline (the pass says why it failed), before the network was read, and
/// when the user said "Sync anyway on this network" (`sync_on_metered`).
#[must_use]
pub(crate) fn network_hold(network: Option<&NetworkState>, setup: &SyncSetup) -> Option<u64> {
    let network = network?;
    if setup.sync_on_metered || !network.connected || network.allows_background_transfer() {
        return None;
    }
    Some(under_mb(setup).saturating_mul(MB))
}

/// A file state's icon (a Material name of the icon set).
#[must_use]
pub(crate) fn state_icon(state: &FileState) -> &'static str {
    match state {
        FileState::CloudOnly => "cloud_queue",
        FileState::Downloading { .. } => "cloud_download",
        FileState::Uploading { .. } => "cloud_upload",
        FileState::OnDevice => "check_circle",
        FileState::OnDeviceEncrypted => "lock",
        FileState::Pinned => "push_pin",
        FileState::Conflict => "warning",
        FileState::Error(_) => "error",
    }
}

/// A file state as one word, for the scripts' markers.
#[must_use]
pub(crate) fn state_word(state: &FileState) -> &'static str {
    match state {
        FileState::CloudOnly => "cloud-only",
        FileState::Downloading { .. } => "downloading",
        FileState::Uploading { .. } => "uploading",
        FileState::OnDevice => "on-device",
        FileState::OnDeviceEncrypted => "on-device-encrypted",
        FileState::Pinned => "pinned",
        FileState::Conflict => "conflict",
        FileState::Error(_) => "error",
    }
}

/// The tint of a state's icon: a conflict amber, an error red, the rest quiet.
fn state_tint(state: &FileState) -> &'static str {
    match state {
        FileState::Conflict => "color: #C77700;",
        FileState::Error(_) => "color: #C42B1C;",
        FileState::OnDevice | FileState::Pinned => "color: #2E9E5B;",
        _ => "opacity: 0.6;",
    }
}

/// The rows of the cloud-only files directly in the pairing's folder `rel` (`""` its top, else
/// ending in `/`), keyed as the local drive keys them under `prefix` (the open folder).
#[must_use]
pub(crate) fn placeholders(states: &SyncStates, rel: &str, prefix: &str) -> Vec<Entry> {
    states
        .files
        .range(rel.to_string()..)
        .take_while(|(key, _)| key.starts_with(rel))
        .filter(|(key, record)| {
            record.cloud_only && !record.encrypted_copy && !key[rel.len()..].contains('/')
        })
        .map(|(key, record)| {
            let name = &key[rel.len()..];
            Entry {
                key: format!("{prefix}{name}"),
                name: name.to_string(),
                is_folder: false,
                size: Some(record.size),
                modified: u64::try_from(record.modified).ok(),
                etag: None,
                known: true,
            }
        })
        .collect()
}

/// The key under the synced `folder` of `path` (a folder's ending in `/`, the folder itself
/// `""`); `None` outside it.
#[must_use]
pub(crate) fn key_under(folder: &Path, path: &Path, is_folder: bool) -> Option<String> {
    let rest = path.strip_prefix(folder).ok()?;
    let mut key = String::new();
    for part in rest.components() {
        let std::path::Component::Normal(name) = part else {
            return None;
        };
        if !key.is_empty() {
            key.push('/');
        }
        key.push_str(name.to_str()?);
    }
    if is_folder && !key.is_empty() {
        key.push('/');
    }
    Some(key)
}

/// A new pairing's folder: `AzDrive/<drive name>` in Home (a name's slashes, colons and
/// backslashes made underscores).
#[must_use]
pub(crate) fn default_folder(home: &Path, drive_name: &str) -> PathBuf {
    let name: String = drive_name
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':') { '_' } else { c })
        .collect();
    home.join("AzDrive").join(name.trim())
}

// ==== The window's questions ====

/// The pairing of drive `drive_id`.
pub(crate) fn setup_of<'a>(s: &'a DriveState, drive_id: &str) -> Option<&'a SyncSetup> {
    s.settings.synced.iter().find(|p| p.drive_id == drive_id)
}

/// Whether drive `drive_id` is an encrypted drive this build opened as one: its own listing
/// names its files.
#[cfg(feature = "encryption")]
pub(crate) fn names_its_files(s: &DriveState, drive_id: &str) -> bool {
    s.slot_index(drive_id)
        .and_then(|i| s.slots[i].auto.as_ref())
        .is_some_and(|auto| auto.is_encrypted() == Some(true))
}

#[cfg(not(feature = "encryption"))]
pub(crate) fn names_its_files(_s: &DriveState, _drive_id: &str) -> bool {
    false
}

/// The pairing `key` of drive `drive` lies in, and its key under the pairing: the synced
/// folder through a drive on this computer, or an encrypted drive's own folder ([`SyncStore`]).
pub(crate) fn pair_at(s: &DriveState, drive: &str, key: &str) -> Option<(String, String)> {
    s.sync_view.store.locate(drive, key)
}

/// The pairing the open folder lies in.
pub(crate) fn place_in_pair(s: &DriveState) -> Option<(String, String)> {
    match &s.place {
        Place::Folder { drive, prefix } => pair_at(s, drive, prefix),
        _ => None,
    }
}

/// The drive a sync command is about: the pairing of the open folder, else the open drive,
/// else the drive selected on This PC.
pub(crate) fn target_drive(s: &DriveState) -> Option<String> {
    if let Some((drive_id, _)) = place_in_pair(s) {
        return Some(drive_id);
    }
    s.current_drive_id().or_else(|| {
        (s.place == Place::ThisPc)
            .then(|| s.selected_drive.and_then(|i| s.slots.get(i)))
            .flatten()
            .map(|slot| slot.entry.id.clone())
    })
}

/// The selected items as keys under their pairing (a folder's ending in `/`): all in one.
pub(crate) fn selected_keys(s: &DriveState) -> Option<(String, Vec<String>)> {
    let drive = s.current_drive_id()?;
    let mut found: Option<String> = None;
    let mut keys = Vec::new();
    for entry in s.selected_entries() {
        let (drive_id, rel) = pair_at(s, &drive, &entry.key)?;
        if found.as_ref().is_some_and(|d| *d != drive_id) {
            return None;
        }
        found = Some(drive_id);
        keys.push(rel);
    }
    found.map(|drive_id| (drive_id, keys))
}

/// The state of a row of the open folder, when it lies in a pairing.
pub(crate) fn entry_state(s: &DriveState, entry: &Entry) -> Option<FileState> {
    let drive = s.current_drive_id()?;
    s.sync_view.store.file_state(&drive, &entry.key)
}

/// The icon after a row's name and what it says: its sync state, or - a file of a cloud drive
/// that does not sync - Explorer's cloud (fetched when it is opened).
pub(crate) fn badge(s: &DriveState, entry: &Entry) -> Option<(&'static str, String, &'static str)> {
    if let Some(state) = entry_state(s, entry) {
        return Some((state_icon(&state), t_phrase(&state_text(&state)), state_tint(&state)));
    }
    let cloud = !entry.is_folder && s.current_drive_id().is_some_and(|id| !s.is_local_drive(&id));
    cloud.then(|| ("cloud_queue", t("azdrive-sync-in-the-cloud"), "opacity: 0.55;"))
}

/// The icon of `entry`'s state after its name ([`badge`]).
pub(crate) fn badge_dom(s: &DriveState, entry: &Entry) -> Option<Dom> {
    let (icon, says, tint) = badge(s, entry)?;
    Some(
        Dom::create_icon(AzString::from(icon))
            .with_class(ids::SYNC_STATE_CLASS)
            .with_accessibility_name(says)
            .with_css(format!(
                "font-size: 14px; margin-left: 6px; flex-shrink: 0; {tint}"
            )),
    )
}

/// §13.7's overlay from a drive's search index: a magnifier on a file it read as it is now, a
/// slashed one on a file it never reads; nothing on one not read yet. (The icon, what it says
/// as a key.)
#[must_use]
pub(crate) fn index_overlay(
    indexing: azul_search_index::FileIndexing,
) -> Option<(&'static str, &'static str)> {
    match indexing {
        azul_search_index::FileIndexing::Indexed => {
            Some(("manage_search", "azdrive-index-overlay-indexed"))
        }
        azul_search_index::FileIndexing::NotIndexable => {
            Some(("search_off", "azdrive-index-overlay-not-indexable"))
        }
        azul_search_index::FileIndexing::Unread => None,
    }
}

/// The rows of a plain synced drive's own folder `rel` of the pairing (`""` its top, else
/// ending in `/`): the sync index's files directly in it and its folders, keyed as the drive's
/// listing keys them under `prefix` (the open folder) - the bucket holds only the sync's
/// bookkeeping there.
#[must_use]
pub(crate) fn index_rows(states: &SyncStates, rel: &str, prefix: &str) -> Vec<Entry> {
    let mut rows = Vec::new();
    let mut folders: Vec<String> = Vec::new();
    for (key, record) in states
        .files
        .range(rel.to_string()..)
        .take_while(|(key, _)| key.starts_with(rel))
    {
        let rest = &key[rel.len()..];
        match rest.split_once('/') {
            Some((folder, _)) => {
                if folders.last().map(String::as_str) != Some(folder) {
                    folders.push(folder.to_string());
                    rows.push(Entry {
                        key: format!("{prefix}{folder}/"),
                        name: folder.to_string(),
                        is_folder: true,
                        size: None,
                        modified: None,
                        etag: None,
                        known: true,
                    });
                }
            }
            None => rows.push(Entry {
                key: format!("{prefix}{rest}"),
                name: rest.to_string(),
                is_folder: false,
                size: Some(record.size),
                modified: u64::try_from(record.modified).ok(),
                etag: None,
                known: true,
            }),
        }
    }
    rows
}

/// The mass delete's question, in AzDrive's words.
#[must_use]
pub(crate) fn mass_delete_text(asked: &azcloud_kit::sync::MassDelete) -> Phrase {
    let said = if asked.here {
        "azdrive-sync-mass-delete-there"
    } else {
        "azdrive-sync-mass-delete-here"
    };
    Phrase::new(said).arg("count", asked.count).arg("of", asked.of)
}

/// What a synced row's preview says instead of its bytes: a file in the cloud only (its bytes
/// are not here), or - `from_index`, a plain drive's own listing showing the sync's names - a
/// file whose copy is in the synced folder. `None`: it previews as any file. The sentence's key.
#[must_use]
pub(crate) fn preview_note(state: &FileState, from_index: bool) -> Option<&'static str> {
    match state {
        FileState::CloudOnly => Some("azdrive-preview-cloud-only"),
        _ if from_index => Some("azdrive-preview-synced"),
        _ => None,
    }
}

/// Whether `drive`'s own listing at `key` shows a plain synced drive's files from its sync
/// index (its bucket holds the sync's blobs).
pub(crate) fn from_index(s: &DriveState, drive: &str, key: &str) -> bool {
    !s.is_local_drive(drive)
        && s.sync_view.store.names_its_files(drive) == Some(false)
        && setup_of(s, drive).is_some_and(|p| key.starts_with(p.prefix.as_str()))
}

/// A row as a drive's index lists its files: its key, size and date; `None` for a folder or a
/// row whose size and date are not known yet (not stat'ed).
#[must_use]
pub(crate) fn index_entry(entry: &Entry) -> Option<azul_search::FileEntry> {
    if entry.is_folder || !entry.known {
        return None;
    }
    Some(azul_search::FileEntry {
        path: entry.key.clone(),
        size: entry.size?,
        modified: entry.modified,
    })
}

/// The index overlay after a row's name, when the open drive is indexed.
pub(crate) fn index_overlay_dom(s: &DriveState, entry: &Entry) -> Option<Dom> {
    let drive = s.current_drive_id()?;
    let files = s.sync_view.indexed.get(&drive)?;
    let (icon, says) = index_overlay(files.indexing(&index_entry(entry)?))?;
    Some(
        Dom::create_icon(AzString::from(icon))
            .with_class(ids::INDEX_STATE_CLASS)
            .with_accessibility_name(l10n::label(says))
            .with_css("font-size: 12px; margin-left: 2px; flex-shrink: 0; opacity: 0.55;"),
    )
}

/// The open folder's status line part: its pairing's status, in the window's language.
pub(crate) fn status_for_place(s: &DriveState) -> Option<String> {
    let (drive_id, _) = place_in_pair(s)?;
    drive_status(s, &drive_id).map(|status| t_text(&status))
}

/// The status line of synced drive `drive_id`.
pub(crate) fn drive_status(s: &DriveState, drive_id: &str) -> Option<Text> {
    let setup = setup_of(s, drive_id)?;
    // A banned drive's sync pauses (ban contract v1): its uploads would be refused.
    if let Some(said) = crate::ban::sync_status(s, drive_id) {
        return Some(Text::from(said));
    }
    let azlin = s
        .slot_index(drive_id)
        .is_some_and(|i| s.slots[i].entry.azlin().is_some());
    let states = s.sync_view.store.states(drive_id);
    let running = s.sync_view.drives.get(drive_id).and_then(|d| d.running.as_ref());
    let payment_due = s.sync_view.payment_due.contains(drive_id);
    let held = network_hold(s.sync_view.network.as_ref(), setup).is_some();
    Some(status_text(setup, &states, running, azlin, payment_due, held).into())
}

/// A synced drive's state on its row of the source list: its glyph and its status line.
pub(crate) fn sidebar_state(s: &DriveState, drive_id: &str) -> Option<(&'static str, String)> {
    let setup = setup_of(s, drive_id)?;
    let status = drive_status(s, drive_id)?;
    let states = s.sync_view.store.states(drive_id);
    let running = s
        .sync_view
        .drives
        .get(drive_id)
        .is_some_and(|d| d.running.is_some());
    let glyph = if setup.paused || status.keys() == [METERED_STATUS] {
        "pause_circle"
    } else if says_read_only(&status) {
        "cloud_off"
    } else if running {
        "sync"
    } else if !states.conflicts().is_empty() {
        "sync_problem"
    } else if states.last_error.is_some() {
        "error"
    } else if states.last_pass.is_some() {
        "cloud_done"
    } else {
        "cloud_queue"
    };
    Some((glyph, t_text(&status)))
}

/// The open folder's listing is in: an indexed drive's index answers are read again (its
/// overlays), and a folder of a drive on this computer that lies in a pairing gets its
/// cloud-only files as rows.
pub(crate) fn on_listed(s: &mut DriveState) {
    let Place::Folder { drive, prefix } = s.place.clone() else {
        return;
    };
    if s.settings.indexed_drives.contains(&drive) {
        if let Some(dir) = crate::index_folder(s, &drive) {
            let files = azul_search_index::indexed_files(&dir);
            s.sync_view.indexed.insert(drive.clone(), files);
        }
    } else {
        s.sync_view.indexed.remove(&drive);
    }
    if s.find.is_some() {
        return;
    }
    // A plain synced drive's own folder: the sync index's files and folders.
    if from_index(s, &drive, &prefix) {
        let Some((drive_id, rel)) = pair_at(s, &drive, &prefix) else {
            return;
        };
        let states = s.sync_view.store.states(&drive_id);
        let rows = index_rows(&states, &rel, &prefix);
        add_rows(s, rows);
        return;
    }
    if !s.is_local_drive(&drive) {
        return;
    }
    let Some((drive_id, rel)) = pair_at(s, &drive, &prefix) else {
        return;
    };
    let states = s.sync_view.store.states(&drive_id);
    let rows = placeholders(&states, &rel, &prefix);
    add_rows(s, rows);
}

/// `rows` join the open folder's (the ones it lists already stay), in the view's order.
fn add_rows(s: &mut DriveState, rows: Vec<Entry>) {
    let mut added = false;
    for row in rows {
        if !s.entries.iter().any(|e| e.key == row.key) {
            s.entries.push(row);
            added = true;
        }
    }
    if added {
        browse::sort_entries(&mut s.entries, s.settings.sort);
    }
}

/// Prints every file whose state changed (`AZDRIVE_SYNC_FILE <drive> <state> <key>`, `gone`
/// for one the drive no longer has).
pub(crate) fn print_changes(drive_id: &str, before: &SyncStates, after: &SyncStates) {
    for key in after.files.keys() {
        let now = after.state_of(key);
        if now != before.state_of(key) {
            if let Some(state) = now {
                println!("AZDRIVE_SYNC_FILE {drive_id} {} {key}", state_word(&state));
            }
        }
    }
    for key in before.files.keys().filter(|k| !after.files.contains_key(*k)) {
        println!("AZDRIVE_SYNC_FILE {drive_id} gone {key}");
    }
}

/// Prints the drive's status line when it changed (`AZDRIVE_SYNC_STATUS <drive> <status>`, the
/// status as its key and arguments: `azdrive-sync-status-up-to-date`, the same in any
/// language).
pub(crate) fn say_status(s: &mut DriveState, drive_id: &str) {
    let text = drive_status(s, drive_id).map_or_else(String::new, |status| status.to_string());
    let sync = s.sync_view.drives.entry(drive_id.to_string()).or_default();
    if sync.said != text {
        println!("AZDRIVE_SYNC_STATUS {drive_id} {text}");
        sync.said = text;
    }
}

/// Why `what` cannot run now (the ribbon greys it and says why).
pub(crate) fn why_not(s: &DriveState, what: SyncAction) -> Option<String> {
    let target = target_drive(s);
    let synced = target.as_deref().and_then(|id| setup_of(s, id));
    match what {
        SyncAction::Pair => match target.as_deref() {
            None => Some(String::from("azdrive-sync-why-open-cloud")),
            Some(id) if s.is_local_drive(id) => Some(String::from("azdrive-sync-why-local")),
            Some(_) if synced.is_some() => Some(String::from("azdrive-sync-why-synced")),
            Some(_) if s.cache_dir.is_none() => Some(String::from("azdrive-sync-why-no-cache")),
            Some(_) => None,
        },
        SyncAction::Now | SyncAction::Pause | SyncAction::Stop | SyncAction::OpenFolder => synced
            .is_none()
            .then(|| String::from("azdrive-sync-why-open-synced")),
        SyncAction::KeepOnDevice | SyncAction::FreeUpSpace => {
            if s.selection.is_empty() {
                Some(String::from("azdrive-sync-why-select"))
            } else if selected_keys(s).is_none() {
                Some(String::from("azdrive-sync-why-only-synced"))
            } else {
                None
            }
        }
    }
}

/// The sync's entries of a cloud drive's menu in the source list (their keys).
pub(crate) fn menu_entries(s: &DriveState, drive_id: &str) -> Vec<(String, SyncAction)> {
    match setup_of(s, drive_id) {
        None if s.cache_dir.is_some() => {
            vec![(String::from("azdrive-sync-menu-pair"), SyncAction::Pair)]
        }
        None => Vec::new(),
        Some(setup) => vec![
            (String::from("azdrive-sync-menu-open-folder"), SyncAction::OpenFolder),
            (String::from("azdrive-sync-menu-now"), SyncAction::Now),
            (
                String::from(if setup.paused {
                    "azdrive-sync-menu-resume"
                } else {
                    "azdrive-sync-menu-pause"
                }),
                SyncAction::Pause,
            ),
            (String::from("azdrive-sync-menu-stop"), SyncAction::Stop),
        ],
    }
}

// ==== The dialogs ====

/// The sync's dialogs.
pub(crate) enum SyncDialog {
    /// "Sync with a folder on this computer": the folder here and the drive's folder.
    Pair {
        drive_id: String,
        folder: String,
        prefix: String,
        error: Text,
    },
    /// D52: "Someone changed this file" - keep mine, take theirs, keep both.
    Conflict { drive_id: String, key: String },
    /// "Stop syncing?"
    Stop { drive_id: String },
    /// "Delete from the drive?": synced files kept in the cloud only, or shown from a plain
    /// drive's sync index (their keys under the pairing).
    Delete { drive_id: String, keys: Vec<String> },
    /// The burst guard paused the uploads (D42): "These changes are mine" or "I was hacked...".
    Burst { drive_id: String },
    /// "I was hacked...": lock the drive down, restore it as of a time.
    Hacked { drive_id: String },
    /// A pass would delete most of the folder here or there: "Delete them on the drive too" (or
    /// here) or "Keep them".
    MassDelete { drive_id: String },
}

/// The pairing dialog of drive `drive_id`, its folder filled in (`AzDrive/<name>` in Home) and
/// the drive's open folder.
pub(crate) fn ask_pair(s: &mut DriveState, drive_id: &str) {
    if s.popup.is_some() {
        return;
    }
    let name = s.drive_name(&Place::folder(drive_id, ""));
    let home = s
        .slot_index(crate::HOME_ID)
        .and_then(|i| s.local_root(i))
        .unwrap_or_default();
    let prefix = match &s.place {
        Place::Folder { drive, prefix } if drive == drive_id => prefix.clone(),
        _ => String::new(),
    };
    s.popups_opened += 1;
    s.popup = Some(Popup::Sync(SyncDialog::Pair {
        drive_id: drive_id.to_string(),
        folder: default_folder(&home, &name).display().to_string(),
        prefix,
        error: Text::default(),
    }));
}

/// The next question of a synced drive not asked yet (when no other dialog shows): a burst
/// pause, a mass delete, then a conflict (D52).
pub(crate) fn ask_next_question(s: &mut DriveState) {
    if s.popup.is_some() {
        return;
    }
    for setup in s.settings.synced.clone() {
        let id = setup.drive_id.clone();
        let states = s.sync_view.store.states(&id);
        let ask = if states.burst.is_some() {
            Some(("burst", SyncDialog::Burst { drive_id: id.clone() }))
        } else if states.mass_delete.is_some() {
            Some(("mass", SyncDialog::MassDelete { drive_id: id.clone() }))
        } else {
            None
        };
        if let Some((what, dialog)) = ask {
            if s.sync_view.asked.insert(format!("{id}\n{what}")) {
                println!("AZDRIVE_SYNC_QUESTION {id} {what}");
                s.popups_opened += 1;
                s.popup = Some(Popup::Sync(dialog));
                return;
            }
        }
    }
    ask_next_conflict(s);
}

/// The next conflict not asked about yet, as the D52 question (when no other dialog shows).
pub(crate) fn ask_next_conflict(s: &mut DriveState) {
    if s.popup.is_some() {
        return;
    }
    let asked = &s.sync_view.asked;
    let next = s
        .settings
        .synced
        .iter()
        .filter(|p| !p.paused)
        .find_map(|p| {
            let states = s.sync_view.store.states(&p.drive_id);
            let found = states
                .conflicts()
                .into_iter()
                .map(|held| (p.drive_id.clone(), held.key.clone()))
                .find(|(drive_id, key)| !asked.contains(&format!("{drive_id}\n{key}")));
            found
        });
    if let Some((drive_id, key)) = next {
        ask_conflict(s, &drive_id, &key);
    }
}

/// The D52 question for the conflict of `key` of drive `drive_id`.
pub(crate) fn ask_conflict(s: &mut DriveState, drive_id: &str, key: &str) {
    if s.popup.is_some() {
        return;
    }
    s.sync_view.asked.insert(format!("{drive_id}\n{key}"));
    println!("AZDRIVE_SYNC_CONFLICT {drive_id} {key}");
    s.popups_opened += 1;
    s.popup = Some(Popup::Sync(SyncDialog::Conflict {
        drive_id: drive_id.to_string(),
        key: key.to_string(),
    }));
}

fn column(children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; min-width: 420px; max-width: 540px;")
        .with_children(DomVec::from(children))
}

/// What a button of a sync dialog does.
#[derive(Clone, Copy)]
enum Answer {
    Pair,
    KeepMine,
    TakeTheirs,
    KeepBoth,
    Delete,
    BurstMine,
    Hacked,
    #[cfg(feature = "encryption")]
    LockDown,
    Restore,
    KeepFiles,
    DeleteFiles,
}

struct AnswerRef {
    app: RefAny,
    answer: Answer,
}

fn answer_button(text: &str, app: &RefAny, answer: Answer, id: AzString, primary: bool) -> Dom {
    let kind = if primary {
        ButtonType::Primary
    } else {
        ButtonType::Default
    };
    Button::with_type(l10n::label(text), kind)
        .with_on_click(
            RefAny::new(AnswerRef {
                app: app.clone(),
                answer,
            }),
            on_answer as ButtonOnClickCallbackType,
        )
        .dom()
        .with_id(id)
        .with_css("margin-left: 6px;")
}

/// The dialog's title and content.
pub(crate) fn dialog_parts(dialog: &SyncDialog, s: &DriveState, app: &RefAny) -> (String, Dom) {
    match dialog {
        SyncDialog::Pair {
            drive_id,
            folder,
            prefix,
            error,
        } => {
            let name = s.drive_name(&Place::folder(drive_id, ""));
            let mut body = column(vec![
                line("azdrive-sync-pair-what"),
                label("azdrive-sync-pair-folder"),
                TextInput::create()
                    .with_text(AzString::from(folder.as_str()))
                    .with_on_text_input(app.clone(), on_pair_folder as TextInputOnTextInputCallbackType)
                    .dom()
                    .with_id(ids::SYNC_FOLDER),
                label("azdrive-sync-pair-prefix"),
                TextInput::create()
                    .with_text(AzString::from(prefix.as_str()))
                    .with_placeholder(AzString::from("Documents/"))
                    .with_on_text_input(app.clone(), on_pair_prefix as TextInputOnTextInputCallbackType)
                    .dom()
                    .with_id(ids::SYNC_PREFIX),
            ]);
            if !error.is_empty() {
                body.add_child(text_line(error).with_css("color: #C42B1C;"));
            }
            body.add_child(buttons(vec![
                button("kit-button-cancel", app, on_cancel_popup),
                answer_button("azdrive-sync-pair-ok", app, Answer::Pair, ids::SYNC_PAIR_OK, true),
            ]));
            (
                l10n::t_args("azdrive-sync-pair-title", &[("name", l10n::Arg::from(name))]),
                body.with_id(ids::SYNC_PAIR),
            )
        }
        SyncDialog::Conflict { drive_id, key } => {
            let name = azul_storage::key::last_segment(key).to_string();
            let held = s
                .sync_view
                .store
                .states(drive_id)
                .files
                .get(key)
                .and_then(|r| r.conflict.clone());
            let mut body = column(vec![text_line(
                &Phrase::new("azdrive-sync-conflict-what")
                    .arg("name", name.as_str())
                    .into(),
            )
            .with_css("font-weight: bold;")]);
            if let Some(held) = &held {
                let device = if held.there_device.is_empty() {
                    t("azdrive-sync-another-device")
                } else {
                    held.there_device.clone()
                };
                body.add_child(text_line(
                    &Phrase::new("azdrive-sync-conflict-theirs")
                        .arg("size", browse::format_size(Some(held.there_size)))
                        .arg("device", device)
                        .into(),
                ));
            }
            let choice = |text: &str, answer: Answer, id: AzString| {
                answer_button(text, app, answer, id, false).with_css("margin-top: 8px;")
            };
            body.add_child(choice(
                "azdrive-sync-keep-mine",
                Answer::KeepMine,
                ids::SYNC_KEEP_MINE,
            ));
            body.add_child(choice(
                "azdrive-sync-take-theirs",
                Answer::TakeTheirs,
                ids::SYNC_TAKE_THEIRS,
            ));
            body.add_child(choice(
                "azdrive-sync-keep-both",
                Answer::KeepBoth,
                ids::SYNC_KEEP_BOTH,
            ));
            body.add_child(buttons(vec![button(
                "azdrive-sync-decide-later",
                app,
                on_cancel_popup,
            )]));
            (
                t("azdrive-sync-conflict-title"),
                body.with_id(ids::SYNC_CONFLICT),
            )
        }
        SyncDialog::Delete { drive_id, keys } => {
            let name = s.drive_name(&Place::folder(drive_id, ""));
            let one = match keys.as_slice() {
                [one] => azul_storage::key::last_segment(one).to_string(),
                _ => String::new(),
            };
            (
                t("azdrive-sync-delete-title"),
                column(vec![
                    text_line(
                        &Phrase::new("azdrive-sync-delete-what")
                            .arg("count", keys.len())
                            .arg("one", one)
                            .arg("drive", name)
                            .into(),
                    ),
                    buttons(vec![
                        button("kit-button-cancel", app, on_cancel_popup),
                        answer_button(
                            "azdrive-delete-button",
                            app,
                            Answer::Delete,
                            ids::SYNC_DELETE_OK,
                            true,
                        ),
                    ]),
                ])
                .with_id(ids::SYNC_DELETE),
            )
        }
        SyncDialog::Burst { drive_id } => {
            let name = s.drive_name(&Place::folder(drive_id, ""));
            let states = s.sync_view.store.states(drive_id);
            let mut body = column(vec![]);
            if let Some(pause) = &states.burst {
                let what = match pause.reason {
                    azcloud_kit::sync::guard::PauseReason::Burst => "azdrive-sync-burst-changed",
                    azcloud_kit::sync::guard::PauseReason::Encryption => {
                        "azdrive-sync-burst-encrypted"
                    }
                };
                body.add_child(text_line(
                    &Phrase::new(what)
                        .arg("count", pause.changes)
                        .then(" ")
                        .then(Phrase::new("azdrive-sync-burst-stopped").arg("name", name.as_str())),
                ));
                body.add_child(label("azdrive-sync-burst-changes"));
                for file in &pause.files {
                    body.add_child(line(file).with_css("font-size: 12px; margin-top: 2px;"));
                }
            }
            body.add_child(buttons(vec![
                button("azdrive-sync-decide-later", app, on_cancel_popup),
                answer_button(
                    "azdrive-sync-i-was-hacked-button",
                    app,
                    Answer::Hacked,
                    ids::SYNC_BURST_HACKED,
                    false,
                ),
                answer_button(
                    "azdrive-sync-changes-are-mine",
                    app,
                    Answer::BurstMine,
                    ids::SYNC_BURST_MINE,
                    true,
                ),
            ]));
            (t("azdrive-sync-burst-title"), body.with_id(ids::SYNC_BURST))
        }
        SyncDialog::Hacked { drive_id } => {
            let azlin = s
                .slot_index(drive_id)
                .is_some_and(|i| s.slots[i].entry.azlin().is_some());
            let mut body = column(vec![line("azdrive-sync-hacked-what")]);
            let mut actions = vec![button("azdrive-button-close", app, on_cancel_popup)];
            if azlin {
                #[cfg(feature = "encryption")]
                actions.push(answer_button(
                    "azdrive-sync-lock-down",
                    app,
                    Answer::LockDown,
                    ids::SYNC_HACKED_LOCKDOWN,
                    false,
                ));
                actions.push(answer_button(
                    "azdrive-options-restore",
                    app,
                    Answer::Restore,
                    ids::SYNC_HACKED_RESTORE,
                    true,
                ));
            } else {
                body.add_child(line("azdrive-sync-hacked-not-azlin"));
            }
            body.add_child(buttons(actions));
            (t("azdrive-sync-hacked-title"), body.with_id(ids::SYNC_HACKED))
        }
        SyncDialog::MassDelete { drive_id } => {
            let states = s.sync_view.store.states(drive_id);
            let mut body = column(vec![]);
            let here = states.mass_delete.as_ref().is_some_and(|m| m.here);
            if let Some(asked) = &states.mass_delete {
                body.add_child(text_line(&mass_delete_text(asked).into()));
                body.add_child(label("azdrive-sync-mass-files"));
                for key in asked.keys.iter().take(20) {
                    body.add_child(line(key).with_css("font-size: 12px; margin-top: 2px;"));
                }
            }
            body.add_child(buttons(vec![
                button("azdrive-sync-decide-later", app, on_cancel_popup),
                answer_button(
                    "azdrive-sync-mass-keep",
                    app,
                    Answer::KeepFiles,
                    ids::SYNC_MASS_KEEP,
                    false,
                ),
                answer_button(
                    if here {
                        "azdrive-sync-mass-delete-here-too"
                    } else {
                        "azdrive-sync-mass-delete-there-too"
                    },
                    app,
                    Answer::DeleteFiles,
                    ids::SYNC_MASS_DELETE,
                    true,
                ),
            ]));
            (t("azdrive-sync-mass-title"), body.with_id(ids::SYNC_MASS))
        }
        SyncDialog::Stop { drive_id } => {
            let name = s.drive_name(&Place::folder(drive_id, ""));
            let folder = setup_of(s, drive_id)
                .map(|p| p.folder.display().to_string())
                .unwrap_or_default();
            (
                l10n::t_args("azdrive-sync-stop-title", &[("name", l10n::Arg::from(name))]),
                column(vec![
                    text_line(
                        &Phrase::new("azdrive-sync-stop-what")
                            .arg("folder", folder)
                            .into(),
                    ),
                    buttons(vec![
                        button("kit-button-cancel", app, on_cancel_popup),
                        typed_button("azdrive-sync-stop-button", ButtonType::Primary, app, on_stop),
                    ]),
                ]),
            )
        }
    }
}

extern "C" fn on_pair_folder(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    typed(&mut data, &state, true)
}

extern "C" fn on_pair_prefix(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    typed(&mut data, &state, false)
}

/// A field of the pairing dialog typed into.
fn typed(data: &mut RefAny, state: &TextInputState, is_folder: bool) -> OnTextInputReturn {
    let keep = OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    };
    let Some(mut s) = data.downcast_mut::<DriveState>() else {
        return keep;
    };
    let text = state.get_text().as_str().to_string();
    if let Some(Popup::Sync(SyncDialog::Pair {
        folder,
        prefix,
        error,
        ..
    })) = s.popup.as_mut()
    {
        if is_folder {
            *folder = text;
        } else {
            *prefix = text;
        }
        *error = Text::default();
    }
    keep
}

extern "C" fn on_answer(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, answer)) = data
        .downcast_ref::<AnswerRef>()
        .map(|a| (a.app.clone(), a.answer))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| match answer {
        Answer::Pair => {
            let Some(Popup::Sync(SyncDialog::Pair {
                drive_id,
                folder,
                prefix,
                ..
            })) = s.popup.as_ref()
            else {
                return;
            };
            let (drive_id, folder, prefix) = (drive_id.clone(), folder.clone(), prefix.clone());
            match sync_jobs::pair(info, app, s, &drive_id, folder.trim(), prefix.trim()) {
                Ok(()) => s.popup = None,
                Err(why) => {
                    if let Some(Popup::Sync(SyncDialog::Pair { error, .. })) = s.popup.as_mut() {
                        *error = why;
                    }
                }
            }
        }
        Answer::BurstMine => {
            if let Some(Popup::Sync(SyncDialog::Burst { drive_id })) = s.popup.take() {
                sync_jobs::answer_burst(info, app, s, &drive_id);
            }
        }
        Answer::Hacked => {
            if let Some(Popup::Sync(SyncDialog::Burst { drive_id })) = s.popup.take() {
                println!("AZDRIVE_SYNC_HACKED {drive_id}");
                s.popups_opened += 1;
                s.popup = Some(Popup::Sync(SyncDialog::Hacked { drive_id }));
            }
        }
        #[cfg(feature = "encryption")]
        Answer::LockDown => {
            if let Some(Popup::Sync(SyncDialog::Hacked { drive_id })) = s.popup.take() {
                crate::encryption::ask_rotate(s, &drive_id);
            }
        }
        Answer::Restore => {
            if let Some(Popup::Sync(SyncDialog::Hacked { drive_id })) = s.popup.take() {
                crate::restore::open(s, &drive_id);
            }
        }
        Answer::KeepFiles | Answer::DeleteFiles => {
            if let Some(Popup::Sync(SyncDialog::MassDelete { drive_id })) = s.popup.take() {
                let delete_too = matches!(answer, Answer::DeleteFiles);
                sync_jobs::answer_mass_delete(info, app, s, &drive_id, delete_too);
            }
        }
        Answer::Delete => {
            if let Some(Popup::Sync(SyncDialog::Delete { drive_id, keys })) = s.popup.take() {
                sync_jobs::delete(info, app, s, &drive_id, keys);
            }
        }
        Answer::KeepMine | Answer::TakeTheirs | Answer::KeepBoth => {
            let Some(Popup::Sync(SyncDialog::Conflict { drive_id, key })) = s.popup.take() else {
                return;
            };
            let choice = match answer {
                Answer::KeepMine => azcloud_kit::sync::session::Resolution::KeepMine,
                Answer::TakeTheirs => azcloud_kit::sync::session::Resolution::TakeTheirs,
                _ => azcloud_kit::sync::session::Resolution::KeepBoth,
            };
            sync_jobs::resolve(info, app, s, &drive_id, &key, choice);
        }
    })
}

extern "C" fn on_stop(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        if let Some(Popup::Sync(SyncDialog::Stop { drive_id })) = s.popup.take() {
            sync_jobs::stop(info, app, s, &drive_id);
        }
    })
}

// ==== Options > Drives > Sync ====

/// What a setting of a synced drive carries.
struct SettingRef {
    app: RefAny,
    drive_id: String,
}

fn setting_ref(app: &RefAny, drive_id: &str) -> RefAny {
    RefAny::new(SettingRef {
        app: app.clone(),
        drive_id: drive_id.to_string(),
    })
}

fn setting_parts(data: &mut RefAny) -> Option<(RefAny, String)> {
    data.downcast_ref::<SettingRef>()
        .map(|r| (r.app.clone(), r.drive_id.clone()))
}

/// The auto-download choices, `mb` the "new files under" size.
fn auto_choices(mb: u64) -> [AutoDownload; 4] {
    [
        AutoDownload::Everything,
        AutoDownload::NewUnder(mb),
        AutoDownload::PinnedOnly,
        AutoDownload::Nothing,
    ]
}

/// An auto-download choice as the settings page says it.
fn auto_label(choice: AutoDownload) -> String {
    match choice {
        AutoDownload::Everything => t("azdrive-sync-auto-everything"),
        AutoDownload::NewUnder(mb) => {
            l10n::t_args("azdrive-sync-auto-new-under", &[("mb", l10n::Arg::from(mb))])
        }
        AutoDownload::PinnedOnly => t("azdrive-sync-auto-pinned"),
        AutoDownload::Nothing => t("azdrive-sync-auto-nothing"),
    }
}

/// The setup's "new files under" size (the default while another choice is made).
fn under_mb(setup: &SyncSetup) -> u64 {
    match setup.auto_download {
        AutoDownload::NewUnder(mb) => mb,
        _ => AutoDownload::DEFAULT_MB,
    }
}

/// Options > Drives > Sync: each synced drive with its status and settings.
pub(crate) fn options_section(s: &DriveState, app: &RefAny) -> Dom {
    let mut rows: Vec<Dom> = Vec::new();
    for setup in &s.settings.synced {
        let id = setup.drive_id.as_str();
        let name = s.drive_name(&Place::folder(id, ""));
        let mb = under_mb(setup);
        let choices = auto_choices(mb);
        let selected = choices
            .iter()
            .position(|c| *c == setup.auto_download)
            .unwrap_or(1);
        let small = "font-size: 12px; opacity: 0.75;";
        let mut row = Dom::create_div()
            .with_css("display: flex; flex-direction: column; padding: 6px 0px;")
            .with_child(Dom::create_span_with_text(AzString::from(format!(
                "{name} - {}",
                drive_status(s, id).map(|status| t_text(&status)).unwrap_or_default()
            ))))
            .with_child(
                Dom::create_span_with_text(AzString::from(l10n::t_args(
                    if setup.prefix.is_empty() {
                        "azdrive-sync-pairing-whole"
                    } else {
                        "azdrive-sync-pairing-folder"
                    },
                    &[
                        ("folder", l10n::Arg::from(setup.folder.display().to_string())),
                        ("prefix", l10n::Arg::from(setup.prefix.as_str())),
                    ],
                )))
                .with_css(small),
            )
            .with_child(label("azdrive-sync-auto-download"))
            .with_child(
                DropDown::create(StringVec::from(
                    choices
                        .iter()
                        .map(|c| AzString::from(auto_label(*c)))
                        .collect::<Vec<_>>(),
                ))
                .with_selected(selected)
                .with_accessibility_name(l10n::label("azdrive-sync-auto-download"))
                .with_on_choice_change(
                    setting_ref(app, id),
                    on_auto_download as DropDownOnChoiceChangeCallbackType,
                )
                .dom(),
            )
            .with_child(label("azdrive-sync-under-mb"))
            .with_child(
                TextInput::create()
                    .with_text(AzString::from(mb.to_string()))
                    .with_on_text_input(
                        setting_ref(app, id),
                        on_under_mb as TextInputOnTextInputCallbackType,
                    )
                    .dom(),
            )
            .with_child(label("azdrive-sync-keep-gb"))
            .with_child(
                TextInput::create()
                    .with_text(AzString::from(
                        setup.keep_gb.map(|gb| gb.to_string()).unwrap_or_default(),
                    ))
                    .with_on_text_input(
                        setting_ref(app, id),
                        on_keep_gb as TextInputOnTextInputCallbackType,
                    )
                    .dom(),
            )
            .with_child(
                line(
                    "The least recently used files are freed first; files kept on this device \
                     (pinned) never are.",
                )
                .with_css(small),
            )
            .with_child(
                Dom::create_div()
                    .with_css(
                        "display: flex; flex-direction: row; align-items: center; margin-top: 8px;",
                    )
                    .with_child(
                        CheckBox::create(setup.sync_on_metered)
                            .with_accessibility_name(AzString::from(SYNC_ANYWAY))
                            .with_on_toggle(
                                setting_ref(app, id),
                                on_sync_on_metered as CheckBoxOnToggleCallbackType,
                            )
                            .dom()
                            .with_id(ids::sync_on_metered(id)),
                    )
                    .with_child(
                        Dom::create_span_with_text(AzString::from(SYNC_ANYWAY))
                            .with_css("margin-left: 8px;"),
                    ),
            )
            .with_child(
                line(&format!(
                    "On a metered or low-data network (a phone's hotspot, a capped plan, Low Data \
                     Mode) files over {mb} MB wait for a free one; smaller files sync as always."
                ))
                .with_css(small),
            );
        if names_its_files(s, id) {
            let copies = [LocalCopies::Decrypted, LocalCopies::Encrypted];
            row.add_child(label("azdrive-sync-local-copies"));
            row.add_child(
                DropDown::create(StringVec::from(
                    copies
                        .iter()
                        .map(|c| {
                            l10n::label(match c {
                                LocalCopies::Decrypted => "azdrive-sync-copies-decrypted",
                                LocalCopies::Encrypted => "azdrive-sync-copies-encrypted",
                            })
                        })
                        .collect::<Vec<_>>(),
                ))
                .with_selected(usize::from(setup.local_copies == LocalCopies::Encrypted))
                .with_accessibility_name(l10n::label("azdrive-sync-local-copies"))
                .with_on_choice_change(
                    setting_ref(app, id),
                    on_local_copies as DropDownOnChoiceChangeCallbackType,
                )
                .dom(),
            );
        }
        let action = |text: &str, what: SyncAction| {
            Button::create(l10n::label(text))
                .with_on_click(
                    RefAny::new(ActionRef {
                        app: app.clone(),
                        drive_id: id.to_string(),
                        what,
                    }),
                    on_drive_action as ButtonOnClickCallbackType,
                )
                .dom()
                .with_css("margin-right: 6px; margin-top: 8px;")
        };
        row.add_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row;")
                .with_child(action("azdrive-sync-menu-now", SyncAction::Now))
                .with_child(action(
                    if setup.paused {
                        "azdrive-sync-resume"
                    } else {
                        "azdrive-sync-pause"
                    },
                    SyncAction::Pause,
                ))
                .with_child(action("azdrive-sync-menu-stop", SyncAction::Stop)),
        );
        rows.push(row);
    }
    if rows.is_empty() {
        rows.push(line("azdrive-sync-none"));
    }
    Dom::create_div()
        .with_id(ids::SYNC_OPTIONS)
        .with_css("display: flex; flex-direction: column;")
        .with_children(DomVec::from(rows))
}

struct ActionRef {
    app: RefAny,
    drive_id: String,
    what: SyncAction,
}

extern "C" fn on_drive_action(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, drive_id, what)) = data
        .downcast_ref::<ActionRef>()
        .map(|r| (r.app.clone(), r.drive_id.clone(), r.what))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| {
        sync_jobs::run_action(info, app, s, Some(drive_id), what);
    })
}

/// The Options' check box that syncs a drive on a metered network too.
const SYNC_ANYWAY: &str = "Sync anyway on this network";

/// "Sync anyway on this network" ticked or not: kept with the drive's sync settings; the status
/// line says so at once, and a drive now free syncs its big files right away.
extern "C" fn on_sync_on_metered(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: CheckBoxState,
) -> Update {
    let Some((mut app, drive_id)) = setting_parts(&mut data) else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| {
        change_setup(info, app, s, &drive_id, |setup| {
            setup.sync_on_metered = state.checked;
        });
        println!(
            "AZDRIVE_SYNC_SETTING {drive_id} sync_on_metered {}",
            state.checked
        );
        say_status(s, &drive_id);
        let paused = setup_of(s, &drive_id).is_none_or(|p| p.paused);
        if !paused {
            sync_jobs::request_pass(info, app, s, &drive_id);
        }
    })
}

/// Changes the setup of `drive_id` and keeps the settings.
fn change_setup(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    change: impl FnOnce(&mut SyncSetup),
) {
    if let Some(setup) = s.settings.synced.iter_mut().find(|p| p.drive_id == drive_id) {
        change(setup);
        sync_jobs::publish(s);
        crate::save_settings(info, app, s);
    }
}

extern "C" fn on_auto_download(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let Some((mut app, drive_id)) = setting_parts(&mut data) else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| {
        change_setup(info, app, s, &drive_id, |setup| {
            if let Some(choice) = auto_choices(under_mb(setup)).get(index) {
                setup.auto_download = *choice;
                println!("AZDRIVE_SYNC_SETTING {drive_id} auto_download {}", choice.label());
            }
        });
    })
}

extern "C" fn on_local_copies(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let Some((mut app, drive_id)) = setting_parts(&mut data) else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| {
        change_setup(info, app, s, &drive_id, |setup| {
            setup.local_copies = if index == 1 {
                LocalCopies::Encrypted
            } else {
                LocalCopies::Decrypted
            };
        });
    })
}

/// A number typed into a setting: digits only (an empty field is "none").
fn typed_number(state: &TextInputState) -> Option<Option<u64>> {
    let text = state.get_text().as_str().trim().to_string();
    if text.is_empty() {
        return Some(None);
    }
    text.parse::<u64>().ok().map(Some)
}

/// A setting's field typed into: valid when it is a number, kept at once.
fn number_typed(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    state: &TextInputState,
    change: impl FnOnce(&mut SyncSetup, Option<u64>),
) -> OnTextInputReturn {
    let Some(number) = typed_number(state) else {
        return OnTextInputReturn {
            update: Update::DoNothing,
            valid: TextInputValid::No,
        };
    };
    let Some((mut app, drive_id)) = setting_parts(data) else {
        return OnTextInputReturn {
            update: Update::DoNothing,
            valid: TextInputValid::Yes,
        };
    };
    with_state(&mut app, info, |info, app, s| {
        change_setup(info, app, s, &drive_id, |setup| change(setup, number));
    });
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_under_mb(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    number_typed(&mut data, &mut info, &state, |setup, mb| {
        if let (Some(mb), AutoDownload::NewUnder(_)) = (mb, setup.auto_download) {
            setup.auto_download = AutoDownload::NewUnder(mb);
        }
    })
}

extern "C" fn on_keep_gb(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    number_typed(&mut data, &mut info, &state, |setup, gb| {
        setup.keep_gb = gb.filter(|gb| *gb > 0);
    })
}
