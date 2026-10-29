//! What AzDrive lets the user and the sync do with a drive in its state. Each action names the
//! request it sends the drive's bucket ([`op_of`]), and the contract's one table says whether
//! the drive's state takes it ([`DriveAccess::permits`]): the window greys what the drive would
//! refuse and says why, a transfer that would be refused is not queued, and a sync pass rests
//! while the drive takes neither writes nor deletes. A freeze (unpaid, the operator's hold, a
//! move) keeps the owner's deletes; a pending lockdown takes no change; a ban reads only.

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

/// Why drive `drive_id` does not take `op`, in the window's language; `None` when it does.
#[must_use]
pub(crate) fn refuses_op(s: &DriveState, drive_id: &str, op: BucketOp) -> Option<String> {
    let access = of(s, drive_id);
    if access.permits(op) {
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

/// Why `action` does not run on a drive in `access`; `None` when it does.
#[must_use]
pub(crate) fn refusal(action: &Action, access: DriveAccess) -> Option<String> {
    let op = op_of(action)?;
    (!access.permits(op)).then(|| why(access))
}

/// Whether a sync pass of drive `drive_id` may run: a pass sends writes and deletes, so a drive
/// that takes neither (a ban, a pending lockdown, a lock) rests until it does.
#[must_use]
pub(crate) fn takes_passes(s: &DriveState, drive_id: &str) -> bool {
    let access = of(s, drive_id);
    access.permits(BucketOp::Write) || access.permits(BucketOp::Delete)
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
        DriveAccess::ReadOnly(ReadOnlyWhy::Operator | ReadOnlyWhy::Moving) => {
            t("azdrive-why-frozen")
        }
        DriveAccess::ReadOnly(ReadOnlyWhy::Unknown) => t("azdrive-why-frozen-unknown"),
        DriveAccess::Banned | DriveAccess::Closed => t("azdrive-why-banned"),
        // A writable drive takes every request: nothing to say.
        DriveAccess::Writable => String::new(),
    }
}
