//! The drive's folders as git trees: reading a path, listing a folder, and
//! applying a batch of changes so that only the trees along the changed paths
//! are written again.
//!
//! A path is `/`-separated segments, each a valid tree entry name
//! ([`is_valid_name`]). Folders are kept when they become empty, as in a
//! file manager (git itself would drop them); a folder goes away when it is
//! deleted.

use std::collections::BTreeMap;

use super::{
    objects::{is_valid_name, Mode, ObjectId, Objects, Tree, TreeEntry},
    MetaError,
};

/// One change to the drive's tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// The file at `path` is now the blob `id`; missing folders on the way are made.
    Put { path: String, id: ObjectId },
    /// An (empty) folder at `path`; one that is there already stays as it is.
    Folder { path: String },
    /// Nothing at `path` any more: a file, or a folder with everything in it.
    Delete { path: String },
}

/// The segments of `path`; an error for an empty path or an invalid segment.
pub fn segments(path: &str) -> Result<Vec<&str>, MetaError> {
    let parts: Vec<&str> = path.split('/').collect();
    if path.is_empty() || parts.iter().any(|p| !is_valid_name(p)) {
        return Err(MetaError::Corrupt {
            key: path.to_string(),
            reason: "not a valid path in the drive".to_string(),
        });
    }
    Ok(parts)
}

/// The entry at `path` under the tree `root`; `None` when there is none.
pub fn entry_at(
    objects: &Objects,
    root: &ObjectId,
    path: &str,
) -> Result<Option<TreeEntry>, MetaError> {
    let parts = segments(path)?;
    let mut tree = objects.tree(root)?;
    for (i, part) in parts.iter().enumerate() {
        let Some(entry) = tree.get(part).cloned() else {
            return Ok(None);
        };
        if i + 1 == parts.len() {
            return Ok(Some(entry));
        }
        if entry.mode != Mode::Tree {
            return Ok(None);
        }
        tree = objects.tree(&entry.id)?;
    }
    Ok(None)
}

/// The folder at `path` (the root for `""`); `None` when there is no folder there.
pub fn folder_at(objects: &Objects, root: &ObjectId, path: &str) -> Result<Option<Tree>, MetaError> {
    if path.is_empty() {
        return objects.tree(root).map(Some);
    }
    match entry_at(objects, root, path)? {
        Some(entry) if entry.mode == Mode::Tree => objects.tree(&entry.id).map(Some),
        _ => Ok(None),
    }
}

/// Every file and folder under `root`, by path (folders as `Mode::Tree`).
pub fn walk(objects: &Objects, root: &ObjectId) -> Result<BTreeMap<String, (Mode, ObjectId)>, MetaError> {
    fn visit(
        objects: &Objects,
        id: &ObjectId,
        prefix: &str,
        out: &mut BTreeMap<String, (Mode, ObjectId)>,
    ) -> Result<(), MetaError> {
        for entry in objects.tree(id)?.entries() {
            let path = if prefix.is_empty() {
                entry.name.clone()
            } else {
                format!("{prefix}/{}", entry.name)
            };
            out.insert(path.clone(), (entry.mode, entry.id));
            if entry.mode == Mode::Tree {
                visit(objects, &entry.id, &path, out)?;
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    visit(objects, root, "", &mut out)?;
    Ok(out)
}

/// A folder while changes are applied: loaded from its tree only when a change
/// goes into it.
enum Node {
    File(ObjectId),
    Dir(Dir),
}

struct Dir {
    /// The tree it was read from (`None`: new).
    id: Option<ObjectId>,
    /// Its entries once read.
    entries: Option<BTreeMap<String, Node>>,
    changed: bool,
}

impl Dir {
    fn new() -> Self {
        Dir {
            id: None,
            entries: Some(BTreeMap::new()),
            changed: true,
        }
    }

    fn of(id: ObjectId) -> Self {
        Dir {
            id: Some(id),
            entries: None,
            changed: false,
        }
    }

    /// The entries, read from the tree on first use.
    fn entries(&mut self, objects: &Objects) -> Result<&mut BTreeMap<String, Node>, MetaError> {
        if self.entries.is_none() {
            let mut map = BTreeMap::new();
            if let Some(id) = &self.id {
                for entry in objects.tree(id)?.entries() {
                    let node = match entry.mode {
                        Mode::File => Node::File(entry.id),
                        Mode::Tree => Node::Dir(Dir::of(entry.id)),
                    };
                    map.insert(entry.name.clone(), node);
                }
            }
            self.entries = Some(map);
        }
        Ok(self.entries.get_or_insert_with(BTreeMap::new))
    }

    /// The folder `parts` below this one, made when missing (marking the way changed).
    fn descend(&mut self, objects: &Objects, parts: &[&str], path: &str) -> Result<&mut Dir, MetaError> {
        let mut dir = self;
        for part in parts {
            dir.changed = true;
            let entries = dir.entries(objects)?;
            let node = entries
                .entry((*part).to_string())
                .or_insert_with(|| Node::Dir(Dir::new()));
            dir = match node {
                Node::Dir(child) => child,
                Node::File(_) => {
                    return Err(MetaError::Corrupt {
                        key: path.to_string(),
                        reason: format!("a file is in the way at \"{part}\""),
                    })
                }
            };
        }
        dir.changed = true;
        Ok(dir)
    }

    /// The folder `parts` below this one when it is there (nothing is made).
    fn find(&mut self, objects: &Objects, parts: &[&str]) -> Result<Option<&mut Dir>, MetaError> {
        let mut dir = self;
        for part in parts {
            let entries = dir.entries(objects)?;
            dir = match entries.get_mut(*part) {
                Some(Node::Dir(child)) => child,
                _ => return Ok(None),
            };
        }
        Ok(Some(dir))
    }

    /// Writes the changed trees below and this one; its id.
    fn write(mut self, objects: &mut Objects) -> Result<ObjectId, MetaError> {
        if !self.changed {
            if let Some(id) = self.id {
                return Ok(id);
            }
        }
        // A folder marked changed without being read (an existing folder a
        // `Folder` change named) keeps what it holds.
        self.entries(objects)?;
        let mut entries = Vec::new();
        for (name, node) in self.entries.unwrap_or_default() {
            let (mode, id) = match node {
                Node::File(id) => (Mode::File, id),
                Node::Dir(dir) => (Mode::Tree, dir.write(objects)?),
            };
            entries.push(TreeEntry { name, mode, id });
        }
        Ok(objects.write_tree(&Tree::from_entries(entries)?))
    }
}

/// Applies `changes`, in order, to the tree `root` (`None`: an empty drive) and
/// writes the trees that changed into `objects`; the new root.
pub fn apply(
    objects: &mut Objects,
    root: Option<&ObjectId>,
    changes: &[Change],
) -> Result<ObjectId, MetaError> {
    let mut top = match root {
        Some(id) => Dir::of(*id),
        None => Dir::new(),
    };
    for change in changes {
        match change {
            Change::Put { path, id } => {
                let parts = segments(path)?;
                let (name, folders) = parts.split_last().expect("segments are never empty");
                let dir = top.descend(objects, folders, path)?;
                let entries = dir.entries(objects)?;
                if matches!(entries.get(*name), Some(Node::Dir(_))) {
                    return Err(MetaError::Corrupt {
                        key: path.clone(),
                        reason: "a folder has this name".to_string(),
                    });
                }
                entries.insert((*name).to_string(), Node::File(*id));
            }
            Change::Folder { path } => {
                let parts = segments(path)?;
                top.descend(objects, &parts, path)?;
            }
            Change::Delete { path } => {
                let parts = segments(path)?;
                let (name, folders) = parts.split_last().expect("segments are never empty");
                let removed = match top.find(objects, folders)? {
                    Some(dir) => dir.entries(objects)?.remove(*name).is_some(),
                    None => false,
                };
                if removed {
                    // Mark the way down changed (find did not).
                    top.descend(objects, folders, path)?;
                }
            }
        }
    }
    top.write(objects)
}
