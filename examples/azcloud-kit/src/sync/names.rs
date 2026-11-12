//! The name policy of a synced folder lives in azul-storage ([`azul_storage::names`], ARCH28 RF42):
//! one table of what a name may be, the protected names in every spelling, the key that passed
//! the policy. The sync reaches it here under the names it always used - and walks to a file here
//! by where its key came from (ARCH29 C6c): a name that is on the disk already ([`walk_entry`],
//! [`walk_record`]: never makes a folder) and a name to be made ([`make_parent_of`]: the one
//! create, after the rows of a name from the drive).

use azul_storage::beneath::{BeneathError, Dir, Root};

pub use azul_storage::names::*;

/// The folder of the file of `key` and its name, for a key that is on this disk already: an entry a
/// scan found ([`super::local::Scan`]), a listing of the trash's folder. Walked to, never made: no
/// folder appears for a name that is not there ([`OnDisk`], [`Root::parent_of_existing`]).
///
/// # Errors
///
/// [`BeneathError::BadName`] for a key that is no path of plain names; a link or another disk on
/// the way; not found when the folder is not there.
pub fn walk_entry(tree: &Root, key: &str) -> Result<(Dir, String), BeneathError> {
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
pub fn walk_record(tree: &Root, key: &str) -> Result<(Dir, String), BeneathError> {
    let on_disk = OnDisk::of_record(key)?;
    let (dir, name) = tree.parent_of_existing(&on_disk)?;
    Ok((dir, name.to_string()))
}

/// The folder of a name that is to be MADE here - a file or a link a run writes, a folder renamed to
/// the drive's new name, a file put back - made where it is missing, and the name: the sync's one
/// create. The name comes from the drive, another device or a record about one, so it passes the
/// rows of a synced name first (`policy`: [`Purpose::Sync`] on this disk, the protected names in every
/// spelling among them), and no folder is made for a name those rows refuse (ARCH29 C6c;
/// X-DELTA-AZDRIVE-FILES-1: a folder the drive renamed to `.git` was made on the victim's disk by a
/// walk that took any `&str` and a flag). What is on the disk already is no such name: it is walked
/// by [`walk_entry`] and [`walk_record`], which have no create.
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
    let admitted = policy.admit(key, Purpose::Sync)?;
    let (dir, name) = tree.parent_of_new(&admitted)?;
    Ok((dir, name.to_string()))
}
