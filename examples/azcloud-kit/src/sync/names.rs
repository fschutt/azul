//! The name policy of a synced folder lives in azul-storage ([`azul_storage::names`], ARCH28 RF42):
//! one table of what a name may be, the protected names in every spelling, the key that passed
//! the policy. The sync reaches it here under the names it always used - and walks to a file here
//! by where its key came from (ARCH29 C6c): a name that is on the disk already ([`walk_entry`],
//! [`walk_record`]: never makes a folder) and a name to be made ([`make_parent_of`]: the one
//! create, after the rows of a name from the drive).

use azul_storage::beneath::{BeneathError, Dir, Existing, Root};

pub use azul_storage::names::*;

/// The folder of the file of `key` and its name, for a key that is on this disk already: an entry a
/// scan found ([`super::local::Scan`]), a listing of the trash's folder. Walked to, never made: no
/// folder appears for a name that is not there ([`OnDisk`], [`Root::parent_of_existing`]), and the folder is an
/// [`Existing`] one: it looks, removes what is there and moves it out, and makes no name in itself. Crate-private
/// (X-DELTA-STRUCT4-7: it was `pub`, a hand to every caller that held a drive's key; the doc test of
/// [`make_parent_of`] shows the door shut).
///
/// # Errors
///
/// [`BeneathError::BadName`] for a key that is no path of plain names; a link or another disk on
/// the way; not found when the folder is not there.
pub(crate) fn walk_entry(tree: &Root, key: &str) -> Result<(Existing, String), BeneathError> {
    let on_disk = OnDisk::of_entry(key)?;
    let (dir, name) = tree.parent_of_existing(&on_disk)?;
    Ok((dir, name.to_string()))
}

/// [`walk_entry`] for a key of a record this device wrote about its own folder: a base entry, a line
/// of the oplog or of the journal, a folder of the base.
///
/// # Errors
///
/// As [`walk_entry`].
pub(crate) fn walk_record(tree: &Root, key: &str) -> Result<(Existing, String), BeneathError> {
    let on_disk = OnDisk::of_record(key)?;
    let (dir, name) = tree.parent_of_existing(&on_disk)?;
    Ok((dir, name.to_string()))
}

/// A key of the drive that passed the rows of a synced name ([`Purpose::Sync`] on this disk: the protected
/// names in every spelling among them), or why not - the sync's ONE admission of a name it may make, which
/// [`make_parent_of`] asks and which a caller that only wants the reason asks too (a restore that leaves a
/// refused name in the trash says why: X-DELTA-STRUCT-DRIVE-5). `names_gate` counts the `admit` of it once.
///
/// # Errors
///
/// [`Refusal`], the first row the key breaks.
pub fn admit_synced(policy: &Policy, key: &str) -> Result<Admitted, Refusal> {
    policy.admit(key, Purpose::Sync)
}

/// The folder of a name that passed the rows of a synced name ([`admit_synced`]) and is on this disk already, and the
/// name, for a caller that puts a name into it - a file replaced in place, a step a crash cut off and a recovery
/// finishes, a copy kept beside a file: walked to and never made, and a full [`Dir`] because the name it makes was
/// asked first (X-DELTA-STRUCT4-7: these took the keys of the base and the journal as `OnDisk` ones and renamed into
/// any folder that was there). `names_gate` counts every use.
///
/// # Errors
///
/// [`BeneathError::BadName`] for a name the rows refuse; a link or another disk on the way, or a folder that is not
/// there.
pub(crate) fn walk_admitted(
    tree: &Root,
    policy: &Policy,
    key: &str,
) -> Result<(Dir, String), BeneathError> {
    let admitted = admit_synced(policy, key)?;
    let (dir, name) = tree.parent_of_admitted(&admitted)?;
    Ok((dir, name.to_string()))
}

/// The folder of a name that is to be MADE here - a file or a link a run writes, a folder renamed to
/// the drive's new name, a file put back - made where it is missing, and the name: the sync's one
/// create. The name comes from the drive, another device or a record about one, so it passes the
/// rows of a synced name first (`policy`: [`Purpose::Sync`] on this disk, the protected names in every
/// spelling among them), and no folder is made for a name those rows refuse (ARCH29 C6c;
/// X-DELTA-AZDRIVE-FILES-1: a folder the drive renamed to `.git` was made on the victim's disk by a
/// walk that took any `&str` and a flag). What is on the disk already is no such name: it is walked
/// by [`walk_entry`] and [`walk_record`], which have no create - and no other crate has them:
///
/// ```compile_fail,E0603
/// // The walks to what is there are the sync's own: no other crate takes a key of its base or its journal to a folder.
/// use azcloud_kit::sync::names::walk_record;
/// ```
///
/// ```no_run
/// // The control of the one above: the sync's one create, which asks the rows of a synced name first.
/// use azcloud_kit::sync::names::{make_parent_of, Policy, Volume};
/// fn make(tree: &azul_storage::beneath::Root) {
///     let policy = Policy::here(Volume::default());
///     let _ = make_parent_of(tree, &policy, "docs/a.txt");
/// }
/// ```
///
/// # Errors
///
/// [`BeneathError::BadName`] for a name the rows refuse; a link or another disk on the way, or a
/// folder that cannot be made.
pub fn make_parent_of(
    tree: &Root,
    policy: &Policy,
    key: &str,
) -> Result<(Dir, String), BeneathError> {
    let admitted = admit_synced(policy, key)?;
    let (dir, name) = tree.parent_of_new(&admitted)?;
    Ok((dir, name.to_string()))
}
