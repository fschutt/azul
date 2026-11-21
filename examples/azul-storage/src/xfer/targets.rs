//! The names a transfer will have made at its target (ARCH28 RF43 / AR5a-2 `TargetSet`).
//!
//! A plan that knows only what is AT the target before it starts does not know that two of its
//! own files land on one name: twelve search results named `invoice.pdf`, moved into one folder,
//! were twelve plan entries with one target, none of them a conflict - each copy renamed over the
//! last and all twelve sources were deleted. And a folder copy that keeps the names it saw as the
//! disk spells them does not see that `Report.txt` and `report.txt` are one file on APFS and
//! NTFS. [`TargetSet`] holds every name of the target - the ones that are there and the ones the
//! plan reserved - folded the way the target's own disk compares names, so a name is taken by
//! whichever comes first, and "Keep both" picks from the whole set.

use std::{collections::BTreeSet, path::Path};

use crate::{
    beneath::{self, Tri},
    names::{self, Volume},
};

use super::names::{keep_both_folder_name, keep_both_name};

/// What the disk under `dir` does with names, asked without writing a file (so a folder that is
/// read-only, or one a file manager watches, is not touched): whether a name spelt in another case
/// is the same file. The strongest fold is the answer wherever the disk cannot be asked - a false
/// collision is a question to the user, a missed one is an overwrite. Unicode forms are always
/// folded for the same reason.
#[must_use]
pub fn volume_of(dir: &Path) -> Volume {
    Volume {
        case_insensitive: folds_case(dir).unwrap_or(true),
        norm_insensitive: true,
        ..Volume::default()
    }
}

/// Whether the disk of `dir` takes `Name` and `name` for one file; `None` when it cannot be
/// told (no name of the path has a letter to swap, or the disk would not say).
pub(crate) fn folds_case(dir: &Path) -> Option<bool> {
    // A folder the transfer is about to make takes the rule of the folder it is made in.
    let real = dir
        .ancestors()
        .find_map(|folder| std::fs::canonicalize(folder).ok())?;
    let mut here = real.as_path();
    loop {
        let name = here.file_name()?.to_str()?;
        // The other spelling of a name is looked for in the folder above it - which is another
        // disk when the name is a mount point (an exFAT stick under /Volumes answers for the disk
        // of /Volumes): the disk is then asked through a name inside it.
        if is_mount_point(here) {
            return folds_case_inside(here);
        }
        if let Some(twin) = swapped_case(name) {
            return same_file_in_the_other_case(&here.with_file_name(twin), here);
        }
        here = here.parent()?;
    }
}

/// Whether `path` is the root of another disk than its folder's.
fn is_mount_point(path: &Path) -> bool {
    match (
        beneath::stat_path(path),
        path.parent().map(beneath::stat_path),
    ) {
        (Ok(own), Some(Ok(above))) => own.dev != above.dev,
        _ => false,
    }
}

/// `name` with each letter's case swapped, or `None` when there is no letter to swap or a letter
/// that does not change into exactly one (`ß` becomes `SS`, `ǆ` has a title case): such a name is
/// no probe - its other spelling is not the one a disk that ignores case looks up.
pub(crate) fn swapped_case(name: &str) -> Option<String> {
    let mut changed = false;
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_lowercase() || c.is_uppercase() {
            let mut swapped = if c.is_lowercase() {
                c.to_uppercase().collect::<Vec<char>>()
            } else {
                c.to_lowercase().collect::<Vec<char>>()
            };
            if swapped.len() != 1 {
                return None;
            }
            let one = swapped.pop()?;
            // A letter that comes back as itself (no other case) swaps to nothing.
            changed |= one != c;
            out.push(one);
        } else {
            out.push(c);
        }
    }
    changed.then_some(out)
}

/// Whether `twin` (a name in the other case) is the same file as `own`: the same one, or - on a
/// disk that tells case apart - another that happens to exist; a disk that has no file of the
/// other spelling tells case apart.
fn same_file_in_the_other_case(twin: &Path, own: &Path) -> Option<bool> {
    match (beneath::stat_path(twin), beneath::stat_path(own)) {
        (Ok(other), Ok(own)) => Some(match other.id().same(&own.id()) {
            Tri::Same | Tri::Unknown => true,
            Tri::Different => false,
        }),
        (Err(e), Ok(_)) if e.kind() == std::io::ErrorKind::NotFound => Some(false),
        _ => None,
    }
}

/// The disk of the folder `dir`, asked through an entry inside it that has a letter to swap
/// (`None`: the folder is empty or has none).
pub(crate) fn folds_case_inside(dir: &Path) -> Option<bool> {
    /// Entries looked at: a folder with a million is not read whole for this.
    const LOOKS: usize = 64;
    for entry in std::fs::read_dir(dir).ok()?.take(LOOKS).flatten() {
        let name = entry.file_name();
        let Some(twin) = name.to_str().and_then(swapped_case) else {
            continue;
        };
        return same_file_in_the_other_case(&dir.join(twin), &entry.path());
    }
    None
}

/// Who holds a name of the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claim {
    /// Nobody: it is free.
    Free,
    /// Something is there already (the user decides what happens to it).
    Existing,
    /// An earlier file of this plan lands there.
    Planned,
}

/// Every name of the target a plan touches: those that are there, and those the plan reserved.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TargetSet {
    volume: Volume,
    /// A file and a folder of one name cannot be both on a disk: the trailing `/` of a folder
    /// key does not tell them apart there (in a bucket it does).
    on_disk: bool,
    existing: BTreeSet<String>,
    planned: BTreeSet<String>,
}

impl TargetSet {
    /// An empty set for a target whose names compare as `volume` says; `on_disk`: it is a folder
    /// on a disk.
    #[must_use]
    pub fn new(volume: Volume, on_disk: bool) -> TargetSet {
        TargetSet {
            volume,
            on_disk,
            ..TargetSet::default()
        }
    }

    /// The key as the target compares names.
    #[must_use]
    pub fn fold(&self, key: &str) -> String {
        let key = if self.on_disk {
            key.trim_end_matches('/')
        } else {
            key
        };
        names::fold(key, &self.volume)
    }

    /// Something is at `key` of the target already.
    pub fn note_existing(&mut self, key: &str) {
        let folded = self.fold(key);
        self.existing.insert(folded);
    }

    /// Whether something that was there before the plan holds `key` (the plan's own reservations
    /// do not count).
    #[must_use]
    pub fn is_existing(&self, key: &str) -> bool {
        self.existing.contains(&self.fold(key))
    }

    /// Who holds `key`: an earlier file of the plan, something that is there, or nobody.
    #[must_use]
    pub fn state(&self, key: &str) -> Claim {
        let folded = self.fold(key);
        if self.planned.contains(&folded) {
            Claim::Planned
        } else if self.existing.contains(&folded) {
            Claim::Existing
        } else {
            Claim::Free
        }
    }

    /// Whether `key` is held by anybody.
    #[must_use]
    pub fn is_taken(&self, key: &str) -> bool {
        self.state(key) != Claim::Free
    }

    /// The plan lands a file at `key`: nobody else's from here on.
    pub fn reserve(&mut self, key: &str) {
        let folded = self.fold(key);
        self.planned.insert(folded);
    }

    /// The first free OTHER name for `key` in its own folder: `name (2).ext`, `name (3).ext`, ...
    /// (a folder key, ending with `/`, keeps its trailing slash) - what "Keep both" gives the
    /// newcomer, and what a file gets whose own name an earlier file of the plan has. Not
    /// reserved: the caller does so once it uses it. `taken_elsewhere` is asked about every
    /// candidate this set holds nobody for - the target may hold names the set was not told of;
    /// it answers `true` for a candidate that is there.
    pub fn free_key(&self, key: &str, taken_elsewhere: &mut dyn FnMut(&str) -> bool) -> String {
        let folder_key = key.ends_with('/');
        let trimmed = key.trim_end_matches('/');
        let (parent, name) = match trimmed.rfind('/') {
            Some(i) => (&trimmed[..=i], &trimmed[i + 1..]),
            None => ("", trimmed),
        };
        let tail = if folder_key { "/" } else { "" };
        let mut probe = |candidate: &str| {
            let candidate_key = format!("{parent}{candidate}{tail}");
            self.is_taken(&candidate_key) || taken_elsewhere(&candidate_key)
        };
        let free = if folder_key {
            keep_both_folder_name(name, &mut probe)
        } else {
            keep_both_name(name, &mut probe)
        };
        format!("{parent}{free}{tail}")
    }
}
