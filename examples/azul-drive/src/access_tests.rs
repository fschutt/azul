//! What the window lets the user do with a drive in each of its states comes from the
//! contract's one table: the action's request (a read, a write, a delete, a link) and what the
//! drive's state takes. A drive frozen by the operator or for a move keeps its owner's deletes
//! like an unpaid one; its other writes are refused, with words that say why.

use azcloud_kit::azlin_contract::{BucketOp, DriveAccess, ReadOnlyWhy};

use crate::{
    access::{op_of, passes_allowed, refusal, requests},
    actions::Action,
};

const WRITES: [Action; 6] = [
    Action::Upload,
    Action::NewFolder,
    Action::NewTextDocument,
    Action::Paste,
    Action::Rename,
    Action::Zip,
];
const DELETES: [Action; 4] = [
    Action::Delete,
    Action::DeletePermanently,
    Action::DeleteMenu,
    Action::Cut,
];

#[test]
fn each_action_names_the_request_it_sends() {
    for action in WRITES {
        assert_eq!(op_of(&action), Some(BucketOp::Write), "{action:?}");
    }
    for action in DELETES {
        assert_eq!(op_of(&action), Some(BucketOp::Delete), "{action:?}");
    }
    for action in [Action::Download, Action::Copy, Action::Open, Action::Print] {
        assert_eq!(op_of(&action), Some(BucketOp::Read), "{action:?}");
    }
    assert_eq!(op_of(&Action::Share), Some(BucketOp::Link));
    for action in [Action::SelectAll, Action::CopyPath, Action::Pin] {
        assert_eq!(op_of(&action), None, "{action:?}: the drive is not asked");
    }
}

#[test]
fn a_drive_frozen_by_the_operator_or_for_a_move_takes_deletes_like_an_unpaid_one() {
    crate::l10n::in_english();
    for why in [ReadOnlyWhy::Unpaid, ReadOnlyWhy::Operator, ReadOnlyWhy::Moving] {
        let access = DriveAccess::ReadOnly(why);
        for action in DELETES {
            assert_eq!(refusal(&action, access, false), None, "{why:?} {action:?}: deletes go on");
        }
        for action in WRITES {
            let said = refusal(&action, access, false).unwrap_or_else(|| panic!("{why:?} {action:?}"));
            assert!(said.contains("read-only"), "{why:?}: {said}");
        }
        assert_eq!(refusal(&Action::Download, access, false), None, "{why:?}: reads go on");
    }
}

#[test]
fn a_lockdown_takes_no_change_and_a_writable_drive_takes_everything() {
    crate::l10n::in_english();
    let lockdown = DriveAccess::ReadOnly(ReadOnlyWhy::LockdownPending);
    for action in WRITES.iter().chain(DELETES.iter()) {
        let said = refusal(action, lockdown, false).unwrap_or_else(|| panic!("{action:?}"));
        assert!(said.contains("lockdown"), "{said}");
    }
    for action in WRITES.iter().chain(DELETES.iter()) {
        assert_eq!(refusal(action, DriveAccess::Writable, false), None, "{action:?}");
    }
}

/// The backend cannot read an encrypted drive's index: on a frozen drive (unpaid, the operator's
/// hold, a move) a delete is an index shrink and the file's objects deleted, which it takes; an
/// upload grows the index and the stored data, which it refuses. A pending lockdown takes
/// neither.
#[test]
fn on_a_read_only_encrypted_drive_a_delete_is_an_index_shrink_and_an_index_growth_is_refused() {
    crate::l10n::in_english();
    assert_eq!(requests(BucketOp::Delete, true), [BucketOp::IndexCommit, BucketOp::Delete]);
    assert_eq!(requests(BucketOp::Delete, false), [BucketOp::Delete]);
    assert_eq!(requests(BucketOp::Write, true), [BucketOp::Write]);
    for why in [ReadOnlyWhy::Unpaid, ReadOnlyWhy::Operator] {
        let frozen = DriveAccess::ReadOnly(why);
        for action in DELETES {
            assert_eq!(refusal(&action, frozen, true), None, "{why:?} {action:?}");
        }
        for action in WRITES {
            assert!(refusal(&action, frozen, true).is_some(), "{why:?} {action:?}");
        }
    }
    let lockdown = DriveAccess::ReadOnly(ReadOnlyWhy::LockdownPending);
    assert!(refusal(&Action::Delete, lockdown, true).is_some());
}

/// While the drive is being moved an encrypted drive's deletes wait (they are index commits)
/// and say so; a plain drive's deletes go on. The sync of an encrypted drive rests until the move
/// is done, a plain drive's goes on.
#[test]
fn a_drive_being_moved_holds_an_encrypted_drives_deletes_and_its_sync_until_the_move_is_done() {
    crate::l10n::in_english();
    let moving = DriveAccess::ReadOnly(ReadOnlyWhy::Moving);
    for action in DELETES {
        let said = refusal(&action, moving, true).unwrap_or_else(|| panic!("{action:?} runs"));
        assert!(said.contains("moved") && said.contains("after the move"), "{said}");
        assert_eq!(refusal(&action, moving, false), None, "{action:?}: a plain drive deletes");
    }
    assert!(!passes_allowed(moving, true), "an encrypted drive's sync rests");
    assert!(passes_allowed(moving, false), "a plain drive's sync goes on");
    assert!(passes_allowed(DriveAccess::ReadOnly(ReadOnlyWhy::Unpaid), true));
    assert!(!passes_allowed(DriveAccess::Banned, true));
    assert_ne!(azul_appkit::l10n::t("azdrive-sync-status-moving"), "azdrive-sync-status-moving");
}
