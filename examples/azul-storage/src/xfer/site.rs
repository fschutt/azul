//! Where a drive's files are, and whether two places overlap (ARCH28 RF43 / AR5a-2 step 1).
//!
//! The app makes more than one drive over one tree: Home is a drive on the home folder, a
//! folder added as a drive is another inside it, a synced listing is a drive over the synced
//! folder (`sync:<id>`), an upload's source is a drive over the file's folder (`os:<folder>`).
//! Their ids differ, and so do the key texts they use for the same file. "The same drive", "the
//! file is its own destination" and "the folder is moved into itself" are therefore decided
//! here, from what the drives are ([`Drive::site`]) and, on a disk, from which file is which
//! (the device and the inode of every folder on the way, links followed): never from ids and
//! never from the text of a key. A move's last step deletes the source: a wrong "disjoint" costs
//! a folder.
//!
//! [`Overlap::Unknown`] is an answer of its own. A drive nobody can look into (a data source, a
//! database) is only provably the same place as itself; two of them may or may not overlap, and
//! the engine then plans without the shortcuts that need the answer (no rename in place, no
//! copy on the service) - its copies and its retiring of sources are lossless anyway.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use crate::beneath::{self, FileId};

/// The identity of a drive object that is neither a folder nor a bucket: the address of the
/// object. Two clones of one `Arc` have the same; two drives built over the same account do not,
/// and nothing then proves they are different places.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OtherId(usize);

impl OtherId {
    /// The identity of `drive`.
    #[must_use]
    pub fn of<T: ?Sized>(drive: &T) -> OtherId {
        OtherId((drive as *const T).cast::<()>() as usize)
    }
}

/// A key space that is not a disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Space {
    /// A bucket of an S3-compatible service, as `endpoint/bucket`.
    Bucket(String),
    /// A drive object only itself can vouch for ([`OtherId`]).
    Object(OtherId),
}

/// Where a drive's files are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Site {
    /// A folder on this computer: key `a/b` is the file `<root>/a/b`.
    Disk { root: PathBuf },
    /// A key space of its own: the drive's key `k` is the key `<prefix>k` of `space`.
    Remote { space: Space, prefix: String },
}

impl Site {
    /// The folder `root` on this computer.
    #[must_use]
    pub fn disk(root: impl Into<PathBuf>) -> Site {
        Site::Disk { root: root.into() }
    }

    /// The bucket `bucket` of the service at `endpoint`.
    #[must_use]
    pub fn bucket(endpoint: &str, bucket: &str) -> Site {
        Site::Remote {
            space: Space::Bucket(format!(
                "{}/{bucket}",
                endpoint.trim_end_matches('/').to_ascii_lowercase()
            )),
            prefix: String::new(),
        }
    }

    /// The place of a drive nothing is known of but its identity: only the very same object is
    /// provably the same place ([`Drive::site`]'s default).
    #[must_use]
    pub fn other_of<T: ?Sized>(drive: &T) -> Site {
        Site::Remote {
            space: Space::Object(OtherId::of(drive)),
            prefix: String::new(),
        }
    }

    /// The same place seen through a drive whose keys all live under `prefix` of this one (a
    /// grant to a folder of a bucket, `ScopedDrive`): `prefix` ends with `/` or is empty.
    #[must_use]
    pub fn under(&self, prefix: &str) -> Site {
        match self {
            Site::Disk { root } => {
                let mut root = root.clone();
                for name in prefix.split('/').filter(|n| !n.is_empty()) {
                    root.push(name);
                }
                Site::Disk { root }
            }
            Site::Remote { space, prefix: own } => Site::Remote {
                space: space.clone(),
                prefix: format!("{own}{prefix}"),
            },
        }
    }

    /// Whether both are provably one place: every key of the one means the file the same key
    /// means in the other. What "the same drive" is, wherever it decides a shortcut.
    #[must_use]
    pub fn same_place(&self, other: &Site) -> bool {
        overlap(self, "", other, "") == Overlap::Same
    }
}

/// How two keys of two sites lie to each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlap {
    /// No file is in both.
    Disjoint,
    /// The same file or folder.
    Same,
    /// The first is a folder and the second lies strictly inside it (a folder moved there would
    /// be moved into itself).
    SourceHoldsTarget,
    /// The second is a folder and the first lies strictly inside it.
    TargetHoldsSource,
    /// It cannot be told: the sites are drives only themselves can vouch for, or the disk gave no
    /// identity and the paths could not be resolved.
    Unknown,
}

impl Overlap {
    /// Whether a folder at the first place cannot be moved or copied to the second: it is the
    /// second, or the second lies inside it.
    #[must_use]
    pub fn target_inside_source(self) -> bool {
        matches!(self, Overlap::Same | Overlap::SourceHoldsTarget)
    }
}

/// How `a_key` of `a` and `b_key` of `b` lie to each other. A key ending with `/` is a folder
/// (its whole subtree), else a file.
#[must_use]
pub fn overlap(a: &Site, a_key: &str, b: &Site, b_key: &str) -> Overlap {
    match (a, b) {
        (Site::Disk { root: ra }, Site::Disk { root: rb }) => {
            disk(&path_of(ra, a_key), &path_of(rb, b_key))
        }
        (
            Site::Remote {
                space: sa,
                prefix: pa,
            },
            Site::Remote {
                space: sb,
                prefix: pb,
            },
        ) => {
            if sa == sb {
                text(&format!("{pa}{a_key}"), &format!("{pb}{b_key}"))
            } else if matches!((sa, sb), (Space::Object(_), Space::Object(_))) {
                Overlap::Unknown
            } else {
                Overlap::Disjoint
            }
        }
        // A disk and a service share no file.
        _ => Overlap::Disjoint,
    }
}

/// The path of `key` under `root`.
fn path_of(root: &Path, key: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    for name in key.split('/').filter(|n| !n.is_empty()) {
        path.push(name);
    }
    path
}

/// Two keys of one key space, as text: names are exact there.
fn text(a: &str, b: &str) -> Overlap {
    if a == b {
        Overlap::Same
    } else if a.ends_with('/') && b.starts_with(a) {
        Overlap::SourceHoldsTarget
    } else if b.ends_with('/') && a.starts_with(b) {
        Overlap::TargetHoldsSource
    } else {
        Overlap::Disjoint
    }
}

/// A path on this disk as the disk knows it: the part that exists, resolved (links followed),
/// the id of that part and of each folder above it, and the names below it that are not made yet.
struct Chain {
    real: PathBuf,
    /// The ids of `real` and of each of its ancestors, deepest first.
    ids: Vec<FileId>,
    /// The names below `real`, outermost first (a folder a transfer is about to make).
    rest: Vec<OsString>,
}

impl Chain {
    fn of(path: &Path) -> Option<Chain> {
        let mut rest = Vec::new();
        let mut here = path;
        let real = loop {
            match std::fs::canonicalize(here) {
                Ok(real) => break real,
                Err(_) => {
                    rest.push(here.file_name()?.to_owned());
                    here = here.parent()?;
                }
            }
        };
        rest.reverse();
        let ids = real
            .ancestors()
            .map(|folder| beneath::stat_path(folder).map_or(FileId::UNKNOWN, |st| st.id()))
            .collect();
        Some(Chain { real, ids, rest })
    }

    /// Whether the whole path exists.
    fn exists(&self) -> bool {
        self.rest.is_empty()
    }

    /// Whether the disk named every folder on the way.
    fn known(&self) -> bool {
        self.ids.iter().all(FileId::is_known)
    }

    /// The path as it would be once made.
    fn full(&self) -> PathBuf {
        let mut path = self.real.clone();
        for name in &self.rest {
            path.push(name);
        }
        path
    }
}

/// Two paths on this disk. The files decide, not the spellings: a link, a second drive over the
/// same folder, a hard link and a name spelt in another case on a disk that ignores it all lie
/// where the device and inode of the folders on the way say.
fn disk(a: &Path, b: &Path) -> Overlap {
    let (Some(ca), Some(cb)) = (Chain::of(a), Chain::of(b)) else {
        return Overlap::Unknown;
    };
    if !(ca.known() && cb.known()) {
        // The disk gave no identity (Windows until its file index is read): the resolved paths,
        // compared as text, are what there is.
        return path_text(&ca.full(), &cb.full());
    }
    let (a_id, b_id) = (ca.ids[0], cb.ids[0]);
    if ca.exists() && cb.exists() && a_id.same(&b_id).is_same() {
        return Overlap::Same;
    }
    let a_holds = ca.exists() && cb.ids.iter().any(|id| a_id.same(id).is_same());
    let b_holds = cb.exists() && ca.ids.iter().any(|id| b_id.same(id).is_same());
    match (a_holds, b_holds) {
        (true, false) => Overlap::SourceHoldsTarget,
        (false, true) => Overlap::TargetHoldsSource,
        // Neither exists as far as the other is concerned, or each lies in the other (the
        // same folder reached twice): by the resolved paths.
        (true, true) => path_text(&ca.full(), &cb.full()),
        (false, false) => {
            if ca.exists() || cb.exists() {
                Overlap::Disjoint
            } else {
                // Both are folders not made yet: where they would be.
                path_text(&ca.full(), &cb.full())
            }
        }
    }
}

/// Two resolved paths by their names (component-wise; Windows ignores case).
fn path_text(a: &Path, b: &Path) -> Overlap {
    let norm = |p: &Path| -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(p.to_string_lossy().to_lowercase())
        } else {
            p.to_path_buf()
        }
    };
    let (a, b) = (norm(a), norm(b));
    if a == b {
        Overlap::Same
    } else if b.starts_with(&a) {
        Overlap::SourceHoldsTarget
    } else if a.starts_with(&b) {
        Overlap::TargetHoldsSource
    } else {
        Overlap::Disjoint
    }
}

/// The names of `path` below the folder `root`, outermost first, as the disk sees them - `None`
/// when `path` is neither `root` nor inside it. A path typed in another spelling reaches the
/// same folder: a link (`/var` and `/private/var`), a name in another case on a disk that ignores
/// it, a second drive over the same tree. (AZ X-AR5a-19: the window mapped a typed path to a
/// drive by `strip_prefix`, which sees only the text.) A name that is not UTF-8 gives `None`.
#[must_use]
pub fn names_below(root: &Path, path: &Path) -> Option<Vec<String>> {
    let (cr, cp) = (Chain::of(root)?, Chain::of(path)?);
    let below: PathBuf = if cr.exists() && cr.known() && cp.known() {
        // The folder of `path`'s own chain that is the root, by device and inode.
        let depth = cp.ids.iter().position(|id| cr.ids[0].same(id).is_same())?;
        let folder = cp.real.ancestors().nth(depth)?;
        let mut below = cp.real.strip_prefix(folder).ok()?.to_path_buf();
        for name in &cp.rest {
            below.push(name);
        }
        below
    } else {
        // Nothing to ask the disk (a root that is gone, a disk without identities): the resolved
        // paths by their names.
        let (root, path) = (cr.full(), cp.full());
        if cfg!(windows) {
            let lower = |p: &Path| PathBuf::from(p.to_string_lossy().to_lowercase());
            let (lower_root, lower_path) = (lower(&root), lower(&path));
            let rest = lower_path.strip_prefix(&lower_root).ok()?.to_path_buf();
            // The names as `path` spells them: as many as `rest` has, from the end.
            let spelt: Vec<_> = path.components().collect();
            let count = rest.components().count();
            spelt[spelt.len() - count..].iter().collect()
        } else {
            path.strip_prefix(&root).ok()?.to_path_buf()
        }
    };
    below
        .components()
        .map(|c| match c {
            std::path::Component::Normal(name) => name.to_str().map(str::to_string),
            _ => None,
        })
        .collect()
}
