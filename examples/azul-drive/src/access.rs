//! What AzDrive lets the user and the sync do with a drive in its state. Each action names the
//! request it sends the drive's bucket ([`op_of`]), and the contract's one table says whether
//! the drive's state takes it ([`DriveAccess::permits`]): the window greys what the drive would
//! refuse and says why, a transfer that would be refused is not queued, and a sync pass rests
//! while the drive takes none of what a pass sends. A freeze (unpaid, the operator's hold) keeps
//! the owner's deletes, an encrypted drive's as index commits; a move keeps plain deletes and
//! holds an encrypted drive's until it is done; a pending lockdown takes no change; a ban reads
//! only. The node judges an index commit by its byte budget, which AzDrive cannot know: it sends
//! the commit and goes by the node's answer.

use std::collections::HashMap;

use azcloud_kit::{
    azlin_contract::{BucketOp, DriveAccess, ReadOnlyWhy},
    DriveStatus,
};
use azul_appkit::l10n::{t, Phrase};

use crate::{
    actions::{now_secs, Action},
    DriveState,
};

/// The request `action` sends the drive in view; `None` for one that asks the drive nothing
/// (a selection, a view, a path copied). A move out of the drive deletes there; where it goes
/// is asked when the transfer is queued ([`refuses_op`]).
#[must_use]
pub(crate) fn op_of(action: &Action) -> Option<BucketOp> {
    match action {
        Action::Paste
        | Action::Rename
        | Action::NewFolder
        | Action::NewItemMenu
        | Action::NewTextDocument
        | Action::NewEmptyFile
        | Action::Upload
        | Action::Zip
        | Action::Undo
        | Action::HideSelected => Some(BucketOp::Write),
        Action::Cut
        | Action::MoveToMenu
        | Action::MoveTo(_)
        | Action::DeleteMenu
        | Action::Delete
        | Action::DeletePermanently => Some(BucketOp::Delete),
        Action::Copy
        | Action::CopyToMenu
        | Action::CopyTo(_)
        | Action::Open
        | Action::OpenMenu
        | Action::Edit
        | Action::Email
        | Action::Download
        | Action::Print => Some(BucketOp::Read),
        Action::Share => Some(BucketOp::Link),
        _ => None,
    }
}

/// The states the token server last said of the Azlin drives, by their Azlin id.
pub(crate) type Seen = HashMap<String, DriveAccess>;

/// What a look at the Azlin drive `azlin_id` saw of its state.
pub(crate) fn seen(s: &mut DriveState, azlin_id: &str, status: &DriveStatus) {
    s.access.insert(azlin_id.to_string(), status.access(now_secs()));
}

/// The state of drive `drive_id` (AzDrive's id): a ban as AzDrive keeps it (closed once its
/// grace is over, or once the token server said so), else what the last look said; writable
/// for a drive never looked at and for every drive that is not an Azlin drive.
#[must_use]
pub(crate) fn of(s: &DriveState, drive_id: &str) -> DriveAccess {
    if let Some(ban) = crate::ban::ban_of(s, drive_id) {
        return if ban.is_closed(now_secs()) {
            DriveAccess::Closed
        } else {
            DriveAccess::Banned
        };
    }
    s.slot_index(drive_id)
        .and_then(|index| s.slots[index].entry.azlin())
        .and_then(|(azlin_id, _)| s.access.get(azlin_id).copied())
        .unwrap_or(DriveAccess::Writable)
}

/// The requests the bucket sees of `op` on a drive that is `encrypted` or not. An encrypted
/// drive's files are entries of its index: a delete commits the index without them (an index
/// commit) and then deletes the file's objects; a write grows the index and the stored data
/// alike.
#[must_use]
pub(crate) const fn requests(op: BucketOp, encrypted: bool) -> &'static [BucketOp] {
    match op {
        BucketOp::Delete if encrypted => &[BucketOp::IndexCommit, BucketOp::Delete],
        BucketOp::Delete => &[BucketOp::Delete],
        BucketOp::Read => &[BucketOp::Read],
        BucketOp::Write => &[BucketOp::Write],
        BucketOp::IndexCommit => &[BucketOp::IndexCommit],
        BucketOp::Link => &[BucketOp::Link],
    }
}

/// Whether a sync pass may run on a drive in `access`: a pass sends what changed - on an
/// encrypted drive every change is an index commit, on a plain one its deletes are deletes - so
/// a drive that takes none of it rests (a ban, a pending lockdown, a lock; an encrypted drive
/// being moved).
#[must_use]
pub(crate) const fn passes_allowed(access: DriveAccess, encrypted: bool) -> bool {
    access.permits(BucketOp::Write)
        || if encrypted {
            access.permits(BucketOp::IndexCommit)
        } else {
            access.permits(BucketOp::Delete)
        }
}

/// Whether drive `drive_id`'s files are entries of an encrypted index: an Azlin drive is,
/// unless it opened as one made before encryption.
fn encrypted(s: &DriveState, drive_id: &str) -> bool {
    let Some(index) = s.slot_index(drive_id) else {
        return false;
    };
    let slot = &s.slots[index];
    if slot.entry.azlin().is_none() {
        return false;
    }
    #[cfg(feature = "encryption")]
    {
        slot.auto
            .as_ref()
            .is_none_or(|auto| auto.is_encrypted() != Some(false))
    }
    #[cfg(not(feature = "encryption"))]
    {
        false
    }
}

/// Why drive `drive_id` does not take `op`, in the window's language; `None` when it does.
#[must_use]
pub(crate) fn refuses_op(s: &DriveState, drive_id: &str, op: BucketOp) -> Option<String> {
    let access = of(s, drive_id);
    if requests(op, encrypted(s, drive_id))
        .iter()
        .all(|request| access.permits(*request))
    {
        return None;
    }
    Some(match crate::ban::ban_of(s, drive_id) {
        Some(ban) => crate::ban::refused_words(ban, now_secs()),
        None => why(access),
    })
}

/// Why `action` does not run on the drive in view, if its state refuses what it sends.
#[must_use]
pub(crate) fn refuses(s: &DriveState, action: &Action) -> Option<String> {
    let drive_id = s.current_drive_id()?;
    refuses_op(s, &drive_id, op_of(action)?)
}

/// Why `action` does not run on a drive in `access` (`encrypted` or not); `None` when it does.
#[must_use]
pub(crate) fn refusal(action: &Action, access: DriveAccess, encrypted: bool) -> Option<String> {
    let op = op_of(action)?;
    let taken = requests(op, encrypted)
        .iter()
        .all(|request| access.permits(*request));
    (!taken).then(|| why(access))
}

/// Whether a sync pass of drive `drive_id` may run ([`passes_allowed`]).
#[must_use]
pub(crate) fn takes_passes(s: &DriveState, drive_id: &str) -> bool {
    passes_allowed(of(s, drive_id), encrypted(s, drive_id))
}

/// Whether drive `drive_id`'s passes rest for a move, which ends by itself: its state is asked
/// again until it is over.
#[must_use]
pub(crate) fn waits_for_move(s: &DriveState, drive_id: &str) -> bool {
    of(s, drive_id) == DriveAccess::ReadOnly(ReadOnlyWhy::Moving) && !takes_passes(s, drive_id)
}

/// The sync's status line of a drive whose passes rest for its state, when neither the ban nor
/// the lock says it already.
#[must_use]
pub(crate) fn sync_status(s: &DriveState, drive_id: &str) -> Option<Phrase> {
    if takes_passes(s, drive_id) {
        return None;
    }
    match of(s, drive_id) {
        DriveAccess::ReadOnly(ReadOnlyWhy::LockdownPending) => {
            Some(Phrase::new("azdrive-sync-status-lockdown"))
        }
        DriveAccess::ReadOnly(ReadOnlyWhy::Moving) => Some(Phrase::new("azdrive-sync-status-moving")),
        DriveAccess::ReadOnly(ReadOnlyWhy::Unknown) => {
            Some(Phrase::new("azdrive-sync-status-read-only"))
        }
        _ => None,
    }
}

/// What a drive in `access` says of the requests it does not take.
fn why(access: DriveAccess) -> String {
    match access {
        DriveAccess::ReadOnly(ReadOnlyWhy::Unpaid) => t("azdrive-why-unpaid"),
        DriveAccess::ReadOnly(ReadOnlyWhy::LockdownPending) => t("azdrive-why-lockdown"),
        DriveAccess::ReadOnly(ReadOnlyWhy::Archived) => t("azdrive-why-archived"),
        DriveAccess::ReadOnly(ReadOnlyWhy::Operator) => t("azdrive-why-frozen"),
        DriveAccess::ReadOnly(ReadOnlyWhy::Moving) => t("azdrive-why-moving"),
        DriveAccess::ReadOnly(ReadOnlyWhy::Unknown) => t("azdrive-why-frozen-unknown"),
        DriveAccess::Banned | DriveAccess::Closed => t("azdrive-why-banned"),
        // A writable drive takes every request: nothing to say.
        DriveAccess::Writable => String::new(),
    }
}
