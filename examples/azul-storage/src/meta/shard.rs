//! Huge folders, sharded: a folder of more than [`SHARD_ABOVE`] entries is
//! stored as hidden subtrees `.azlin-shard-<hh>` (`hh`: the first byte of the
//! SHA-256 of the entry's name, two lowercase hex digits), each holding the
//! entries whose names hash there. A change rewrites one shard of ~1/256 of the
//! folder, not the whole folder, and finding one name reads one shard. The
//! folder goes back to one tree at [`SHARD_BELOW`] entries or fewer (the gap
//! keeps a folder near the limit from flipping on every change).
//!
//! Invisible to the drive: every reader of folders goes through [`read_folder`]
//! or [`lookup`], every writer through [`write_folder`], and a path segment
//! that starts with [`SHARD_PREFIX`] is refused. One level: 256 shards of up to
//! a few thousand entries each hold folders of about a million files.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};

use super::{
    objects::{Mode, ObjectId, Objects, Tree, TreeEntry},
    MetaError,
};

/// The names of the hidden shard subtrees start with this; no drive path may.
pub const SHARD_PREFIX: &str = ".azlin-shard-";

/// A folder of more entries than this is stored in shards.
pub const SHARD_ABOVE: usize = 2048;

/// A sharded folder of this many entries or fewer is stored as one tree again.
pub const SHARD_BELOW: usize = 1024;

/// The shard an entry called `name` lives in.
#[must_use]
pub fn shard_of(name: &str) -> String {
    let hash = Sha256::digest(name.as_bytes());
    format!("{SHARD_PREFIX}{:02x}", hash[0])
}

/// Whether `name` is the name of a shard subtree.
#[must_use]
pub fn is_shard_name(name: &str) -> bool {
    name.strip_prefix(SHARD_PREFIX).is_some_and(|hex| {
        hex.len() == 2 && hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// Whether a stored tree is a sharded folder: not empty, and every entry a
/// shard subtree.
#[must_use]
pub fn is_sharded(raw: &Tree) -> bool {
    !raw.is_empty()
        && raw
            .entries()
            .iter()
            .all(|e| e.mode == Mode::Tree && is_shard_name(&e.name))
}

/// The folder stored as the tree `id`: its entries (every shard gathered) in
/// git's order, and whether it is stored in shards.
pub fn read_folder_sharded(objects: &Objects, id: &ObjectId) -> Result<(Tree, bool), MetaError> {
    let raw = objects.tree(id)?;
    if !is_sharded(&raw) {
        return Ok((raw, false));
    }
    let mut entries = Vec::new();
    for shard in raw.entries() {
        entries.extend(objects.tree(&shard.id)?.entries().iter().cloned());
    }
    Ok((Tree::from_entries(entries)?, true))
}

/// The folder stored as the tree `id`, every shard gathered.
pub fn read_folder(objects: &Objects, id: &ObjectId) -> Result<Tree, MetaError> {
    read_folder_sharded(objects, id).map(|(tree, _)| tree)
}

/// The entry `name` of the folder stored as the tree `id`: one shard read at most.
pub fn lookup(objects: &Objects, id: &ObjectId, name: &str) -> Result<Option<TreeEntry>, MetaError> {
    let raw = objects.tree(id)?;
    if !is_sharded(&raw) {
        return Ok(raw.get(name).cloned());
    }
    match raw.get(&shard_of(name)) {
        None => Ok(None),
        Some(shard) => Ok(objects.tree(&shard.id)?.get(name).cloned()),
    }
}

/// The shard subtrees of the folder stored as `raw` (none when it is one tree).
#[must_use]
pub fn shards(raw: &Tree) -> Vec<TreeEntry> {
    if is_sharded(raw) {
        raw.entries().to_vec()
    } else {
        Vec::new()
    }
}

/// Stores `folder`: as one tree, or in shards when it has more than
/// [`SHARD_ABOVE`] entries (more than [`SHARD_BELOW`] when it `was_sharded`).
/// The same entries always make the same trees.
pub fn write_folder(
    objects: &mut Objects,
    folder: &Tree,
    was_sharded: bool,
) -> Result<ObjectId, MetaError> {
    let count = folder.entries().len();
    if count <= SHARD_BELOW || (count <= SHARD_ABOVE && !was_sharded) {
        return Ok(objects.write_tree(folder));
    }
    let mut groups: BTreeMap<String, Vec<TreeEntry>> = BTreeMap::new();
    for entry in folder.entries() {
        groups
            .entry(shard_of(&entry.name))
            .or_default()
            .push(entry.clone());
    }
    let mut top = Vec::with_capacity(groups.len());
    for (name, entries) in groups {
        let shard = Tree::from_entries(entries)?;
        top.push(TreeEntry {
            name,
            mode: Mode::Tree,
            id: objects.write_tree(&shard),
        });
    }
    Ok(objects.write_tree(&Tree::from_entries(top)?))
}
