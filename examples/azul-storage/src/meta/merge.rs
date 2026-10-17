//! Three-way merge of drive trees, and the conflicts it finds (D52).
//!
//! Per path, against the merge base: what changed on one side only is taken;
//! what changed the same way on both is taken once; two folders changed on
//! both sides are merged entry by entry (unchanged subtrees are never read).
//! What is left is a conflict, and a conflict is about files, never about
//! their bytes (the files are pointer files to encrypted objects):
//!
//! - **both changed** (or both added) a file differently;
//! - **changed and deleted**: one side changed it, the other deleted it;
//! - **file and folder**: a file on one side, a folder of that name on the other.
//!
//! The caller decides each one ([`Resolution`]): keep mine, take theirs, or keep
//! both. Keeping both leaves theirs at the path (the other devices see it
//! there already) and puts mine next to it as `name (conflict, <device>).ext`;
//! for "changed and deleted" it keeps the changed one. An unattended sync keeps
//! both ([`keep_both`]).

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use super::{
    objects::{Mode, ObjectId, Objects, Tree, TreeEntry},
    shard, MetaError,
};

/// One side's entry at a path: what it is and its id.
pub type Side = Option<(Mode, ObjectId)>;

/// What kind of conflict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    /// Both sides changed (or added) the file differently.
    BothChanged,
    /// One side changed it, the other deleted it.
    ChangedAndDeleted,
    /// A file on one side, a folder of the same name on the other.
    FileAndFolder,
}

/// A path both sides changed in different ways.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// From the root, `/`-separated.
    pub path: String,
    pub kind: ConflictKind,
    pub base: Side,
    pub mine: Side,
    pub theirs: Side,
}

/// How a conflict is resolved: "Someone changed this file" - keep mine / take
/// theirs / keep both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    KeepMine,
    TakeTheirs,
    KeepBoth,
}

/// A conflict and what became of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub conflict: Conflict,
    pub resolution: Resolution,
    /// Where "keep both" put the other version, from the root.
    pub copy: Option<String>,
}

/// The merged tree and the conflicts on the way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    pub tree: ObjectId,
    pub conflicts: Vec<Resolved>,
}

/// The resolution of an unattended sync: keep both.
#[must_use]
pub fn keep_both(_: &Conflict) -> Resolution {
    Resolution::KeepBoth
}

/// `report (conflict, Laptop).docx` for `report.docx`; `notes (conflict,
/// Laptop)` for `notes`; a leading dot is not an extension (`.profile
/// (conflict, Laptop)`). Characters a name cannot hold are dropped from the
/// device's name.
#[must_use]
pub fn conflict_name(name: &str, device: &str) -> String {
    let device: String = device
        .chars()
        .filter(|c| !matches!(c, '/' | '\0' | '\\'))
        .collect();
    let device = if device.trim().is_empty() {
        "another device".to_string()
    } else {
        device.trim().to_string()
    };
    match name.rfind('.') {
        Some(dot) if dot > 0 => format!("{} (conflict, {device}){}", &name[..dot], &name[dot..]),
        _ => format!("{name} (conflict, {device})"),
    }
}

/// A name for the copy that no entry of the folder has: the conflict name,
/// then with ` 2`, ` 3`, ... after the device.
fn free_copy_name(name: &str, device: &str, taken: &mut HashSet<String>) -> String {
    let mut candidate = conflict_name(name, device);
    let mut n = 2;
    while taken.contains(&candidate) {
        candidate = conflict_name(name, &format!("{device} {n}"));
        n += 1;
    }
    taken.insert(candidate.clone());
    candidate
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    }
}

/// Merges `mine` and `theirs` (root trees) against `base` (`None`: no common
/// state, as two drives filled apart). `device` names this device in conflict
/// copies; `resolve` decides each conflict.
pub fn merge_trees(
    objects: &mut Objects,
    base: Option<&ObjectId>,
    mine: &ObjectId,
    theirs: &ObjectId,
    device: &str,
    resolve: &mut dyn FnMut(&Conflict) -> Resolution,
) -> Result<Merged, MetaError> {
    let mut conflicts = Vec::new();
    let tree = merge_dir(
        objects,
        base,
        Some(mine),
        Some(theirs),
        "",
        device,
        resolve,
        &mut conflicts,
    )?;
    Ok(Merged { tree, conflicts })
}

/// A folder whole (its shards gathered), and whether it is stored in shards.
fn load(objects: &Objects, id: Option<&ObjectId>) -> Result<(Tree, bool), MetaError> {
    match id {
        Some(id) => shard::read_folder_sharded(objects, id),
        None => Ok((Tree::new(), false)),
    }
}

/// Every entry of a folder by name (a huge folder is looked up once per name).
fn sides(tree: &Tree) -> HashMap<&str, (Mode, ObjectId)> {
    tree.entries()
        .iter()
        .map(|e| (e.name.as_str(), (e.mode, e.id)))
        .collect()
}

/// Adds the entry `name` when the side has one.
fn put(entries: &mut Vec<TreeEntry>, name: &str, side: Side) {
    if let Some((mode, id)) = side {
        entries.push(TreeEntry {
            name: name.to_string(),
            mode,
            id,
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn merge_dir(
    objects: &mut Objects,
    base: Option<&ObjectId>,
    mine: Option<&ObjectId>,
    theirs: Option<&ObjectId>,
    prefix: &str,
    device: &str,
    resolve: &mut dyn FnMut(&Conflict) -> Resolution,
    out: &mut Vec<Resolved>,
) -> Result<ObjectId, MetaError> {
    let (b, _) = load(objects, base)?;
    let (m, mine_sharded) = load(objects, mine)?;
    let (t, theirs_sharded) = load(objects, theirs)?;
    let mut names: BTreeSet<String> = BTreeSet::new();
    for tree in [&b, &m, &t] {
        for entry in tree.entries() {
            names.insert(entry.name.clone());
        }
    }
    let mut taken: HashSet<String> = names.iter().cloned().collect();
    let mut entries: Vec<TreeEntry> = Vec::new();
    let (b_sides, m_sides, t_sides) = (sides(&b), sides(&m), sides(&t));
    for name in &names {
        let (bs, ms, ts) = (
            b_sides.get(name.as_str()).copied(),
            m_sides.get(name.as_str()).copied(),
            t_sides.get(name.as_str()).copied(),
        );
        if ms == ts || ts == bs {
            put(&mut entries, name, ms);
            continue;
        }
        if ms == bs {
            put(&mut entries, name, ts);
            continue;
        }
        let path = join(prefix, name);
        if let (Some((Mode::Tree, mine_id)), Some((Mode::Tree, theirs_id))) = (ms, ts) {
            let base_id = match bs {
                Some((Mode::Tree, id)) => Some(id),
                _ => None,
            };
            let merged = merge_dir(
                objects,
                base_id.as_ref(),
                Some(&mine_id),
                Some(&theirs_id),
                &path,
                device,
                resolve,
                out,
            )?;
            put(&mut entries, name, Some((Mode::Tree, merged)));
            continue;
        }
        let kind = match (ms, ts) {
            (Some((Mode::File, _)), Some((Mode::File, _))) => ConflictKind::BothChanged,
            (Some(_), Some(_)) => ConflictKind::FileAndFolder,
            _ => ConflictKind::ChangedAndDeleted,
        };
        let conflict = Conflict {
            path,
            kind,
            base: bs,
            mine: ms,
            theirs: ts,
        };
        let resolution = resolve(&conflict);
        let mut copy = None;
        match resolution {
            Resolution::KeepMine => put(&mut entries, name, ms),
            Resolution::TakeTheirs => put(&mut entries, name, ts),
            Resolution::KeepBoth => match kind {
                ConflictKind::BothChanged => {
                    put(&mut entries, name, ts);
                    let copy_name = free_copy_name(name, device, &mut taken);
                    put(&mut entries, &copy_name, ms);
                    copy = Some(join(prefix, &copy_name));
                }
                ConflictKind::ChangedAndDeleted => put(&mut entries, name, ms.or(ts)),
                ConflictKind::FileAndFolder => {
                    let (folder, file) = if matches!(ms, Some((Mode::Tree, _))) {
                        (ms, ts)
                    } else {
                        (ts, ms)
                    };
                    put(&mut entries, name, folder);
                    let copy_name = free_copy_name(name, device, &mut taken);
                    put(&mut entries, &copy_name, file);
                    copy = Some(join(prefix, &copy_name));
                }
            },
        }
        out.push(Resolved {
            conflict,
            resolution,
            copy,
        });
    }
    let tree = Tree::from_entries(entries)?;
    shard::write_folder(objects, &tree, mine_sharded || theirs_sharded)
}

/// The parents of the commit `id`.
fn parents(objects: &Objects, id: &ObjectId) -> Result<Vec<ObjectId>, MetaError> {
    Ok(objects.commit(id)?.parents)
}

/// Every commit `id` comes from, itself included.
fn ancestors(objects: &Objects, id: &ObjectId) -> Result<HashSet<ObjectId>, MetaError> {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([*id]);
    while let Some(next) = queue.pop_front() {
        if seen.insert(next) {
            queue.extend(parents(objects, &next)?);
        }
    }
    Ok(seen)
}

/// The best common ancestor of the commits `a` and `b` (one that is not an
/// ancestor of another common ancestor); `None` when their histories never
/// meet.
pub fn merge_base(
    objects: &Objects,
    a: &ObjectId,
    b: &ObjectId,
) -> Result<Option<ObjectId>, MetaError> {
    let of_a = ancestors(objects, a)?;
    // The common ancestors nearest to b: stop at each one found.
    let mut found: Vec<ObjectId> = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([*b]);
    while let Some(next) = queue.pop_front() {
        if !seen.insert(next) {
            continue;
        }
        if of_a.contains(&next) {
            found.push(next);
            continue;
        }
        queue.extend(parents(objects, &next)?);
    }
    // Drop the ones that are ancestors of another one.
    let mut best = Vec::new();
    for candidate in &found {
        let mut older = false;
        for other in &found {
            if other != candidate && ancestors(objects, other)?.contains(candidate) {
                older = true;
                break;
            }
        }
        if !older {
            best.push(*candidate);
        }
    }
    Ok(best.into_iter().next())
}
