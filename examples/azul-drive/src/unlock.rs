//! When AzDrive reads a drive's keys from the keychain, and the two tiers of its secrets.
//!
//! The start reads the drives file only, never a drive's keys. A drive's keyring entry (an S3
//! drive's keys, an Azlin drive's session, a data source's secret settings) is read the first
//! time something needs it - its folder opened, a sync pass, a transfer - and kept for the
//! session in its slot, as text that is zeroed when it is dropped: by the user's Lock and when
//! the window closes ([`wipe`]).
//!
//! Who reads decides whether the keychain may ask:
//! - What the window does by itself - its start, the sync's poll and watch timers - reads
//!   QUIETLY (azul's `*_quietly_in` ops, which never show a prompt), every drive it wants in one
//!   round ([`Unlocks`]). An entry the keychain hands over only after asking (a build it does
//!   not trust yet, a locked keychain) makes the drive wait for the user: the source list shows
//!   it locked, its sync says so, the bar over the content offers Unlock, and no timer reads it
//!   again. So no prompt ever comes from a start or a timer. The background look at the
//!   checkouts, the periods and the recovery registrations reads quietly too
//!   (`DriveState::quiet_keyring`).
//! - What the user does - opening the drive, Unlock, "Unlock drives", "Sync now" - may ask. One
//!   round reads every drive that waits, and the user's first "No" ends it: never one prompt
//!   per drive.
//!
//! The two tiers of AzDrive's secrets, the interim rule until the key vault (a key bound to
//! this computer's hardware) replaces it:
//! - SILENT: the per-drive entries background sync needs are plain keychain items, readable
//!   while the user is logged in, without a biometric access control (`require_biometry` is
//!   false for them). A signed, installed AzDrive syncs without a prompt.
//! - GATED: Touch ID or the computer's password are asked only before an action that reveals
//!   or changes keys - a new recovery code, another device added, new keys after a hack, a
//!   drive removed (the presence check, `presence.rs`).
//!
//! Debug builds and the test or dev profiles never reach the system keychain: azul's keyring
//! keeps their secrets in memory, or in the file `AZ_KEYRING_FILE` names (`AZ_KEYRING=system`
//! asks for the keychain).
//!
//! On stdout, for scripts: `AZDRIVE_KEYS_ROUND quiet|asking <count>`, `AZDRIVE_KEYS_OPEN <drive
//! id>`, `AZDRIVE_KEYS_WAIT <drive id>` (its keys need the user), `AZDRIVE_LOCKED <drive id>`.

use std::{
    collections::BTreeSet,
    sync::atomic::Ordering,
};

use azcloud_kit::Zeroizing;
use azul::{
    callbacks::ButtonOnClickCallbackType,
    error::KeyringResult,
    prelude::*,
    shells::ShellEmptyState,
    str::String as AzString,
};
use azul_appkit::l10n::{label, t_phrase, Phrase, Text};
use azul_storage::{
    azul_keyring::AzulKeyring,
    config,
    keyring::{read_entries, Asking, KeyringError, KeyringNamespace},
};

use crate::{ids, jobs::Job, spawn, with_state, DriveState, Slot};

// ==== The rounds ====

/// The drives whose keys are wanted, read now, or waiting for the user. One round reads at a
/// time; the window's state, on the UI thread.
#[derive(Debug, Default)]
pub(crate) struct Unlocks {
    /// Set while the window's start or a timer asks for several drives: their quiet reads are
    /// gathered and read in one round when it ends ([`flush`]), not one round each.
    pub gathering: bool,
    /// The drives the next quiet round reads.
    wanted: BTreeSet<String>,
    /// The drives of the round running now (quiet, or the user's).
    reading: BTreeSet<String>,
    /// Whether a round runs.
    running: bool,
    /// The drives whose keys the keychain hands over only after asking the user (a quiet read
    /// met a prompt, the user said no, the user locked it): they wait for Unlock, and no quiet
    /// round reads them.
    waiting: BTreeSet<String>,
}

impl Unlocks {
    /// Drive `drive_id`'s keys are wanted by the window's own work: the next quiet round reads
    /// them - unless they are read now, or wait for the user.
    pub fn want(&mut self, drive_id: &str) {
        if !self.waiting.contains(drive_id) && !self.reading.contains(drive_id) {
            self.wanted.insert(drive_id.to_string());
        }
    }

    /// The drives of the quiet round that starts now, unless one runs, the window gathers, or
    /// none is wanted.
    pub fn next_round(&mut self) -> Option<Vec<String>> {
        if self.gathering || self.running || self.wanted.is_empty() {
            return None;
        }
        let drives: Vec<String> = std::mem::take(&mut self.wanted).into_iter().collect();
        self.begin(&drives);
        Some(drives)
    }

    /// "Unlock drives": every drive that waits, in a round that may ask - unless one runs.
    pub fn user_round(&mut self) -> Option<Vec<String>> {
        if self.running || self.waiting.is_empty() {
            return None;
        }
        let drives: Vec<String> = self.waiting.iter().cloned().collect();
        self.begin(&drives);
        Some(drives)
    }

    fn begin(&mut self, drives: &[String]) {
        self.running = true;
        self.reading = drives.iter().cloned().collect();
    }

    /// The round answered.
    pub fn round_ended(&mut self) {
        self.running = false;
        self.reading.clear();
    }

    /// Whether a round reads drive `drive_id`'s keys now.
    pub fn is_reading(&self, drive_id: &str) -> bool {
        self.reading.contains(drive_id)
    }

    /// Drive `drive_id`'s keys are handed over only after asking: it waits for the user.
    pub fn needs_user(&mut self, drive_id: &str) {
        self.wanted.remove(drive_id);
        self.waiting.insert(drive_id.to_string());
    }

    /// The user locked drive `drive_id`: it waits for Unlock.
    pub fn lock(&mut self, drive_id: &str) {
        self.needs_user(drive_id);
    }

    /// Drive `drive_id` opened (or left the window): it waits no more.
    pub fn unlocked(&mut self, drive_id: &str) {
        self.waiting.remove(drive_id);
        self.wanted.remove(drive_id);
    }

    /// Whether drive `drive_id` waits for the user's Unlock.
    pub fn waits_for_user(&self, drive_id: &str) -> bool {
        self.waiting.contains(drive_id)
    }

    /// The drives that wait for the user, by id.
    pub fn waiting(&self) -> Vec<String> {
        self.waiting.iter().cloned().collect()
    }
}

/// What a read of a drive's keyring entry came to. Never printed: it may hold the keys.
pub(crate) enum Read {
    /// The entry's text: the drive's keys, its session or its secret settings.
    Secret(Zeroizing<String>),
    /// The keyring has no entry for the drive.
    Missing,
    /// A quiet read met an entry the keychain hands over only after asking the user.
    NeedsUser,
    /// The user refused the system's prompt.
    Refused,
    /// There is no keyring on this system.
    Unavailable,
    /// The keyring failed (an error, an entry that is no text).
    Failed,
}

impl Read {
    /// The answer of a round's read; `asking`: whether the read could ask the user (a refusal
    /// is theirs then, else the keychain's need to ask).
    pub fn of_answer(answer: Result<Option<String>, KeyringError>, asking: Asking) -> Self {
        match answer {
            Ok(Some(text)) => Read::Secret(Zeroizing::new(text)),
            Ok(None) => Read::Missing,
            Err(KeyringError::Denied) if asking == Asking::Never => Read::NeedsUser,
            Err(KeyringError::Denied) => Read::Refused,
            Err(KeyringError::Unavailable) => Read::Unavailable,
            Err(KeyringError::Failed(_)) => Read::Failed,
        }
    }

    /// The answer of the window's own read (`CallbackInfo::keyring_get_in`, which may ask).
    pub fn of_result(result: &KeyringResult) -> Self {
        match result {
            KeyringResult::Retrieved(secret) => {
                Read::Secret(Zeroizing::new(secret.as_str().to_string()))
            }
            KeyringResult::NotFound => Read::Missing,
            KeyringResult::Denied => Read::Refused,
            KeyringResult::Unavailable => Read::Unavailable,
            _ => Read::Failed,
        }
    }

    /// What went wrong, as a key of the resources.
    pub fn problem(&self) -> &'static str {
        match self {
            Read::Missing => "azdrive-keyring-not-found",
            Read::NeedsUser | Read::Refused => "azdrive-keyring-denied",
            Read::Unavailable => "azdrive-keyring-unavailable",
            Read::Secret(_) | Read::Failed => "azdrive-keyring-failed",
        }
    }
}

/// A round of reads for a worker thread: the drives, and whether it may ask the user.
pub(crate) struct KeysRound {
    pub asking: Asking,
    pub drives: Vec<String>,
}

/// What a round read: each drive's answer, in the round's order.
pub(crate) struct KeysRead {
    pub asking: Asking,
    pub reads: Vec<(String, Read)>,
}

/// Runs on a worker thread: every drive's entry of the namespace the Azlin apps share, one
/// after the other - quietly, or asking until the user's first "No".
pub(crate) fn read_round(round: KeysRound) -> KeysRead {
    let asking = round.asking;
    let namespace = KeyringNamespace::azlin_shared();
    let keyring = match asking {
        Asking::User => AzulKeyring::new(namespace),
        Asking::Never => AzulKeyring::quiet(namespace),
    };
    let keys: Vec<String> = round.drives.iter().map(|id| config::keyring_key(id)).collect();
    let answers = read_entries(&keyring, &keys, asking);
    let reads = round
        .drives
        .into_iter()
        .zip(answers)
        .map(|(drive_id, (_, answer))| (drive_id, Read::of_answer(answer, asking)))
        .collect();
    KeysRead { asking, reads }
}

// ==== The window's side ====

/// Reads the keys of `drive_ids` quietly: in the round that starts now, or - while one runs or
/// the window gathers - in the next.
pub(crate) fn read_quietly(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_ids: &[String],
) {
    for drive_id in drive_ids {
        s.unlocks.want(drive_id);
    }
    flush(info, app, s);
}

/// Starts the quiet round of the drives wanted, unless one runs or the window gathers.
pub(crate) fn flush(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    if let Some(drives) = s.unlocks.next_round() {
        println!("AZDRIVE_KEYS_ROUND quiet {}", drives.len());
        let round = KeysRound {
            asking: Asking::Never,
            drives,
        };
        spawn(info, app, s, Job::ReadKeys(round));
    }
}

/// "Unlock drives" (and Unlock of the one drive that waits): every drive that waits, in one
/// round that may ask.
pub(crate) fn unlock_waiting(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) {
    if let Some(drives) = s.unlocks.user_round() {
        println!("AZDRIVE_KEYS_ROUND asking {}", drives.len());
        let round = KeysRound {
            asking: Asking::User,
            drives,
        };
        spawn(info, app, s, Job::ReadKeys(round));
    }
}

/// A round answered: each drive opens, waits for the user, or says what is wrong; the drives
/// wanted meanwhile are read next. After the user's "No" the drives the round did not read
/// any more wait again, without a word each.
pub(crate) fn round_read(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState, read: KeysRead) {
    s.unlocks.round_ended();
    let by_user = read.asking == Asking::User;
    let mut refused = false;
    for (drive_id, answer) in read.reads {
        if refused && matches!(answer, Read::Refused) {
            s.unlocks.needs_user(&drive_id);
            crate::sync_view::say_status(s, &drive_id);
            continue;
        }
        refused |= matches!(answer, Read::Refused);
        answered(info, app, s, &drive_id, answer, by_user);
    }
    flush(info, app, s);
}

/// Drive `drive_id`'s keyring entry answered (a round's read, or the window's own read of one
/// drive); `by_user`: the user asked for it.
pub(crate) fn answered(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    drive_id: &str,
    read: Read,
    by_user: bool,
) {
    let Some(index) = s.slot_index(drive_id) else {
        // Removed meanwhile.
        return;
    };
    let name = s.slots[index].entry.name.clone();
    let is_current = s.current_drive() == Some(index);
    match read {
        Read::Secret(secret) => match crate::check_secret(&s.slots[index].entry, secret.as_str()) {
            Ok(()) => opened(info, app, s, index, secret, by_user),
            Err(e) => {
                stopped(s, drive_id, is_current, true);
                s.error(
                    Phrase::new("azdrive-keyring-unreadable")
                        .arg("name", name.as_str())
                        .arg("detail", e),
                );
            }
        },
        Read::NeedsUser => {
            s.unlocks.needs_user(drive_id);
            stopped(s, drive_id, is_current, false);
            println!("AZDRIVE_KEYS_WAIT {drive_id}");
        }
        problem => {
            if matches!(problem, Read::Refused) {
                s.unlocks.needs_user(drive_id);
            }
            stopped(s, drive_id, is_current, !matches!(problem, Read::Refused));
            let azlin = s.slots[index].entry.azlin().is_some();
            // A drive token cannot be typed in: a join code from another device of the drive
            // brings it back (Add a drive), else it is added again.
            let then = if azlin {
                "azdrive-keyring-token-gone"
            } else {
                "azdrive-keyring-enter-keys"
            };
            s.error(
                Phrase::new("azdrive-keyring-cannot-open")
                    .arg("name", name.as_str())
                    .then(" ")
                    .then(Text::key(problem.problem()))
                    .then(" ")
                    .then(Text::key(then)),
            );
            if !azlin && is_current && matches!(problem, Read::Missing) {
                crate::actions::open_drive_form(s, Some(index));
            }
        }
    }
    crate::sync_view::say_status(s, drive_id);
}

/// Drive `index` has its keys: kept for the session; its open folder is listed, the tree's
/// nodes that waited for it, and its sync goes on.
fn opened(
    info: &mut CallbackInfo,
    app: &RefAny,
    s: &mut DriveState,
    index: usize,
    secret: Zeroizing<String>,
    by_user: bool,
) {
    let drive_id = s.slots[index].entry.id.clone();
    s.slots[index].secret = Some(secret);
    s.unlocks.unlocked(&drive_id);
    println!("AZDRIVE_KEYS_OPEN {drive_id}");
    if by_user {
        // "Reading the keys of ..." is answered.
        s.clear_notice();
    }
    if s.current_drive() == Some(index) {
        crate::start_listing(info, app, s, false);
    }
    let pending: Vec<crate::TreeKey> = s
        .tree
        .pending
        .iter()
        .filter(|node| node.0 == drive_id)
        .cloned()
        .collect();
    s.tree.pending.retain(|node| node.0 != drive_id);
    for node in pending {
        crate::start_tree_listing(info, app, s, node);
    }
    let synced = s
        .settings
        .synced
        .iter()
        .any(|setup| setup.drive_id == drive_id && !setup.paused);
    if synced {
        crate::sync_jobs::request_pass(info, app, s, &drive_id);
    }
}

/// Drive `drive_id` did not open now: its open folder waits for nothing (`failed`: the content
/// says it could not be read; else it shows the drive locked), nor do its tree nodes.
fn stopped(s: &mut DriveState, drive_id: &str, is_current: bool, failed: bool) {
    if is_current {
        s.loading = false;
        s.listing_done = true;
        s.listing_failed = failed;
    }
    s.tree.pending.retain(|node| node.0 != drive_id);
}

/// The user's Lock: drive `drive_id`'s keys leave this window's memory and it waits for
/// Unlock; its open folder shows it locked. A transfer or sync pass running on it ends with
/// the handle it holds.
pub(crate) fn lock(s: &mut DriveState, drive_id: &str) {
    let Some(index) = s.slot_index(drive_id) else {
        return;
    };
    if !s.slots[index].entry.needs_keyring() {
        return;
    }
    wipe(&mut s.slots[index]);
    s.unlocks.lock(drive_id);
    println!("AZDRIVE_LOCKED {drive_id}");
    if s.current_drive() == Some(index) {
        // A scan of the folder still running is cut off; its rows go.
        s.list_cancel.store(true, Ordering::SeqCst);
        s.list_serial += 1;
        s.entries.clear();
        s.loading = false;
        s.listing_done = true;
        s.listing_failed = false;
    }
    let name = s.slots[index].entry.name.clone();
    s.info(Phrase::new("azdrive-locked").arg("name", name.as_str()));
    crate::sync_view::say_status(s, drive_id);
}

/// Every drive's keys out of memory: the window closes.
pub(crate) fn wipe_all(s: &mut DriveState) {
    for slot in &mut s.slots {
        if slot.entry.needs_keyring() {
            wipe(slot);
        }
    }
}

/// A drive's keys and the handles that hold them dropped: the text is zeroed as it goes, and
/// the drive is locked until its keys are read again.
pub(crate) fn wipe(slot: &mut Slot) {
    slot.secret = None;
    slot.drive = None;
    slot.azlin = None;
    #[cfg(feature = "encryption")]
    {
        slot.auto = None;
    }
}

/// The status line of a synced drive that waits for Unlock.
pub(crate) const WAIT_STATUS: &str = "azdrive-sync-status-locked";

/// Whether drive `drive_id` is locked and waits for the user's Unlock.
pub(crate) fn waits_for_user(s: &DriveState, drive_id: &str) -> bool {
    s.unlocks.waits_for_user(drive_id)
        && s.slot_index(drive_id).is_some_and(|index| s.slots[index].locked())
}

// ==== The DOM ====

/// What the Unlock buttons carry.
struct UnlockRef {
    app: RefAny,
}

fn unlock_ref(app: &RefAny) -> RefAny {
    RefAny::new(UnlockRef { app: app.clone() })
}

/// Unlock / "Unlock drives": every drive that waits, in one round that may ask.
extern "C" fn on_unlock(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut app) = data.downcast_ref::<UnlockRef>().map(|r| r.app.clone()) else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| unlock_waiting(info, app, s))
}

/// The bar over the content while drives wait for the user: which, and Unlock.
pub(crate) fn bar(s: &DriveState, app: &RefAny) -> Option<Dom> {
    let waiting: Vec<String> = s
        .unlocks
        .waiting()
        .into_iter()
        .filter(|id| waits_for_user(s, id))
        .collect();
    let (text, button) = match waiting.as_slice() {
        [] => return None,
        [one] => {
            let name = s
                .slot_index(one)
                .map(|index| s.slots[index].name())
                .unwrap_or_default();
            (
                Phrase::new("azdrive-unlock-bar-one").arg("name", name.as_str()),
                "azdrive-unlock",
            )
        }
        many => (
            Phrase::new("azdrive-unlock-bar-many").arg("count", many.len()),
            "azdrive-unlock-drives",
        ),
    };
    Some(
        Dom::create_div()
            .with_id(ids::UNLOCK_BAR)
            .with_css(
                "display: flex; flex-direction: row; align-items: center; padding: 8px 12px; \
                 background: #FFF4CE; color: #3B2E00;",
            )
            .with_child(
                Dom::create_div()
                    .with_css("flex-grow: 1; margin-right: 12px;")
                    .with_child(Dom::create_span_with_text(AzString::from(t_phrase(&text)))),
            )
            .with_child(
                Button::create(label(button))
                    .with_on_click(unlock_ref(app), on_unlock as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::UNLOCK_BUTTON),
            ),
    )
}

/// What the content shows for the open folder of a drive that waits for the user: that it is
/// locked, and Unlock.
pub(crate) fn locked_view(s: &DriveState, app: &RefAny) -> Option<Dom> {
    let index = s.current_drive()?;
    if !waits_for_user(s, &s.slots[index].entry.id) {
        return None;
    }
    Some(
        ShellEmptyState::create(label("azdrive-unlock-locked"))
            .with_icon(AzString::from("lock"))
            .with_detail(label("azdrive-unlock-locked-detail"))
            .with_action_label(label("azdrive-unlock"))
            .with_on_action(unlock_ref(app), on_unlock as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::UNLOCK_VIEW),
    )
}
