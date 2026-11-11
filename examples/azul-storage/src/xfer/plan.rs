//! The plan of a transfer: everything it will do, decided before a byte moves, so the user decides
//! each conflict - replace, skip, keep both - first (ARCH28 RF43 / AR5a-2).
//!
//! The plan lists every file under every folder, the names the target holds and the names the
//! plan itself reserves ([`TargetSet`]): a name is taken by whichever comes first, folded the way
//! the target's disk compares names, and a file of the batch whose name an earlier file of the
//! batch has gets another name at once - nothing in a batch replaces another file of it. For every
//! conflict the plan also holds what it saw at the target (a replace replaces that version and no
//! other) and the name "Keep both" gives (free at the target when the plan was made).

use super::{
    names::{copy_folder_name, copy_name},
    site::{overlap, Overlap, Site},
    targets::{volume_of, Claim, TargetSet},
};
use crate::{
    key,
    names::{Policy, Purpose, Volume},
    ops, Drive, DriveError,
};

/// What the user decided for a name that is taken at the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Choice {
    /// "Replace the file in the destination".
    Replace,
    /// "Skip this file".
    Skip,
    /// "Keep both files": the newcomer gets ` (2)`.
    KeepBoth,
    /// For a copy that runs again (Copy everything after a failure): what is there already, with
    /// the size of the file, is that file - skip it; anything else is kept beside the new one.
    /// Decides per file ([`Plan::choose`]); never stored on a file.
    Resume,
}

/// One item the user picked: a file, or a folder (`key` ending in `/`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub key: String,
    pub is_folder: bool,
    /// From the listing; `None` asks the drive when it matters.
    pub size: Option<u64>,
}

/// One file the transfer copies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    pub source_key: String,
    pub target_key: String,
    pub size: Option<u64>,
    /// Something has `target_key` already.
    pub conflict: bool,
    /// What the user chose for a conflict (`None`: not asked yet).
    pub choice: Option<Choice>,
    /// The version of the source the plan saw, when its listing says (a bucket's): a move
    /// retires only that version. A folder on disk is looked at when the file is copied.
    pub source_etag: Option<String>,
    /// The version of what is at `target_key` the plan saw (conflicts): a replace replaces that
    /// file and no other - one made there since is a new conflict, not replaced without asking.
    pub target_etag: Option<String>,
    /// Its size, as far as the target said (conflicts).
    pub target_size: Option<u64>,
    /// The name "Keep both" gives the newcomer: free at the target when the plan was made
    /// (conflicts only).
    pub keep_as: Option<String>,
}

/// Everything a transfer will do, decided before it starts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// The files to copy (under the folders too), in order.
    pub files: Vec<PlannedFile>,
    /// The folders to create at the target, parents first (empty ones too).
    pub folders: Vec<String>,
    /// Whole items renamed in place: a move within one drive with nothing in the way (one
    /// `rename`, not a copy).
    pub moves: Vec<(String, String)>,
    /// The folders a move empties, removed after their files went - and only if nothing else is
    /// in them.
    pub source_folders: Vec<String>,
    /// The items the transfer makes at the target (what to select after).
    pub tops: Vec<String>,
    /// Source and target are one place ([`Site::same_place`]), not one id. In a bucket a file then
    /// goes by the drive's own copy (CopyObject, no byte through AzDrive); on disk by the chunked
    /// file copy, which reports its progress megabyte by megabyte.
    pub same_drive: bool,
    /// Files and folders of a drive whose names this computer's disk reads as something else - a
    /// spelling of `.git` or `.azlin`, a device name, a stream - and so are not copied there:
    /// `(source key, why)`. The run lists each as a failure; nothing is made at the target for
    /// them and a move leaves them where they are (X-DELTA-AZDRIVE-FILES-32).
    pub refused: Vec<(String, String)>,
}

impl Plan {
    /// The files whose names are taken at the target, by index.
    #[must_use]
    pub fn conflicts(&self) -> Vec<usize> {
        self.files
            .iter()
            .enumerate()
            .filter(|(_, f)| f.conflict)
            .map(|(i, _)| i)
            .collect()
    }

    /// The first conflict nobody decided yet.
    #[must_use]
    pub fn unresolved(&self) -> Option<usize> {
        self.files
            .iter()
            .position(|f| f.conflict && f.choice.is_none())
    }

    /// The unresolved conflicts (for "Do this for all").
    #[must_use]
    pub fn unresolved_count(&self) -> usize {
        self.files
            .iter()
            .filter(|f| f.conflict && f.choice.is_none())
            .count()
    }

    /// Decides the conflict of file `index`; "Keep both" gives the newcomer the name the plan
    /// found free ([`PlannedFile::keep_as`]).
    pub fn choose(&mut self, index: usize, choice: Choice) {
        let Some(file) = self.files.get_mut(index) else {
            return;
        };
        let choice = match choice {
            // Already there with this size: it is the file. Else the newcomer keeps its own name.
            Choice::Resume if file.size.is_some() && file.size == file.target_size => Choice::Skip,
            Choice::Resume => Choice::KeepBoth,
            other => other,
        };
        if choice == Choice::KeepBoth {
            let Some(name) = file.keep_as.clone() else {
                // No free name was found for it (a plan made some other way): nothing is
                // replaced for it.
                file.choice = Some(Choice::Skip);
                return;
            };
            for top in &mut self.tops {
                if *top == file.target_key {
                    top.clone_from(&name);
                }
            }
            file.target_key = name;
        }
        file.choice = Some(choice);
    }

    /// The bytes the files hold (as far as known).
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().filter_map(|f| f.size).sum()
    }

    /// Nothing to do.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
            && self.folders.is_empty()
            && self.moves.is_empty()
            && self.refused.is_empty()
    }
}

/// Whether something is at `key` of `drive`: a file, or (ending with `/`) a folder.
fn exists_at(drive: &dyn Drive, key: &str) -> Result<bool, DriveError> {
    if key.ends_with('/') {
        ops::folder_exists(drive, key)
    } else {
        ops::exists(drive, key)
    }
}

/// The target of the plan: what the plan knows of its names, and how to find out more.
struct Targets<'a> {
    drive: &'a dyn Drive,
    set: TargetSet,
    /// Folders whose content the set knows completely (listed, or known not to be there): a name
    /// below one of them that the set does not hold is free, with no look at the target.
    known: Vec<String>,
}

impl Targets<'_> {
    fn known_under(&self, key: &str) -> bool {
        self.known
            .iter()
            .any(|folder| key.starts_with(folder.as_str()))
    }

    /// Whether the target has something at `key` now (not counting what the plan reserved).
    fn is_there(&mut self, key: &str) -> Result<bool, DriveError> {
        if self.set.is_existing(key) {
            return Ok(true);
        }
        if self.known_under(key) {
            return Ok(false);
        }
        let there = exists_at(self.drive, key)?;
        if there {
            self.set.note_existing(key);
        }
        Ok(there)
    }

    /// Whether `key` is taken - by the plan or at the target - without remembering the answer.
    fn candidate_taken(&self, key: &str) -> Result<bool, DriveError> {
        if self.set.is_taken(key) {
            return Ok(true);
        }
        if self.known_under(key) {
            return Ok(false);
        }
        exists_at(self.drive, key)
    }

    /// The first free other name for `key`, asked of the target as far as the set does not know.
    fn free_key(&self, key: &str) -> Result<String, DriveError> {
        let mut failure = None;
        let free = self
            .set
            .free_key(key, &mut |candidate| match exists_below(self, candidate) {
                Ok(there) => there,
                Err(e) => {
                    failure.get_or_insert(e);
                    true
                }
            });
        match failure {
            Some(e) => Err(e),
            None => Ok(free),
        }
    }
}

/// Whether the target has something at `key`, as far as the set does not know already.
fn exists_below(targets: &Targets<'_>, key: &str) -> Result<bool, DriveError> {
    if targets.known_under(key) {
        Ok(false)
    } else {
        exists_at(targets.drive, key)
    }
}

/// The name check of a plan that writes names a drive gave into a folder of this computer: its
/// names are other people's words, and the disk may read one as something it is not
/// ([`Purpose::Copy`]).
struct NameCheck {
    policy: Policy,
    /// The folder of the target the items go into: only the names below it are the drive's.
    prefix: String,
}

impl NameCheck {
    /// Whether the plan may write `wanted` (a key of the target, folders too); else it is noted as
    /// refused for `source_key` and the caller goes on without it.
    fn refuses(check: Option<&NameCheck>, plan: &mut Plan, source_key: &str, wanted: &str) -> bool {
        let Some(check) = check else {
            return false;
        };
        let relative = wanted.strip_prefix(check.prefix.as_str()).unwrap_or(wanted);
        match check
            .policy
            .admit(relative.trim_end_matches('/'), Purpose::Copy)
        {
            Ok(_) => false,
            Err(refusal) => {
                plan.refused.push((source_key.to_string(), refusal.why));
                true
            }
        }
    }
}

/// Adds the file `source_key` landing at `wanted`: a free name is reserved; a name an earlier
/// file of the plan has gives this one another name; a name something at the target has is a
/// conflict, with the version seen there. The key it lands at.
fn add_file(
    plan: &mut Plan,
    targets: &mut Targets<'_>,
    source_key: &str,
    wanted: &str,
    size: Option<u64>,
    source_etag: Option<String>,
) -> Result<String, DriveError> {
    let mut target_key = wanted.to_string();
    let mut conflict = false;
    let mut target_etag = None;
    let mut target_size = None;
    if targets.set.state(wanted) == Claim::Planned {
        // An earlier file of this plan lands there: this one is another file, with another name.
        target_key = targets.free_key(wanted)?;
    } else if targets.is_there(wanted)? {
        conflict = true;
        match targets.drive.head(wanted) {
            Ok(info) => {
                target_etag = info.etag;
                target_size = Some(info.size);
            }
            // Gone since it was listed: the name is free, and the write asks for exactly that.
            Err(DriveError::NotFound { .. }) => conflict = false,
            // A target that cannot look at a file at all tells no versions: what the user is asked
            // about is replaced as it stands ([`PlannedFile::target_etag`]).
            Err(DriveError::Unsupported(_)) => {}
            // A look that FAILED is no "no version": a Replace of a name whose version the plan
            // never saw is an unconditional write over whatever another device saved there since
            // (X-DELTA-SYNC2-8). The plan fails, and nothing is moved on a name it could not vouch for.
            Err(e) => return Err(e),
        }
    }
    targets.set.reserve(&target_key);
    plan.files.push(PlannedFile {
        source_key: source_key.to_string(),
        target_key: target_key.clone(),
        size,
        conflict,
        choice: None,
        source_etag,
        target_etag,
        target_size,
        keep_as: None,
    });
    Ok(target_key)
}

/// The folder an item sits in: `docs/a.txt` -> `docs/`, `docs/sub/` -> `docs/`.
#[must_use]
pub fn parent_of(item_key: &str) -> String {
    if item_key.ends_with('/') {
        key::parent_prefix(item_key)
    } else {
        key::folder_of(item_key).to_string()
    }
}

/// Every folder below `folder` (ending in `/`) of a source that is a folder on this computer, at any
/// depth and parents first - the empty ones included, which no recursive listing shows. A link to a
/// folder is not entered (no loop) and not planned; a source that is not a disk has markers in its
/// listing and answers nothing here.
fn disk_subfolders(source: &dyn Drive, folder: &str) -> Result<Vec<String>, DriveError> {
    /// The deepest and the most folders one planned folder holds: a tree nobody made by hand ends
    /// the walk, not the plan.
    const DEPTH: usize = 64;
    const FOLDERS: usize = 100_000;
    let mut out: Vec<String> = Vec::new();
    if source.local_path(folder).is_none() {
        return Ok(out);
    }
    let mut stack: Vec<(String, usize)> = vec![(folder.to_string(), 0)];
    while let Some((prefix, depth)) = stack.pop() {
        if depth >= DEPTH {
            continue;
        }
        let level = ops::list_folder_all(source, &prefix)?;
        // Sorted, and pushed in reverse, so the walk goes in key order and a parent is planned
        // before the folders in it.
        let mut below: Vec<String> = Vec::new();
        for sub in level.folders {
            let Some(path) = source.local_path(&sub) else {
                continue;
            };
            match std::fs::symlink_metadata(path) {
                Ok(meta) if meta.is_dir() => below.push(sub),
                // A link to a folder, or something that is no folder any more.
                Ok(_) => {}
                // Gone since it was listed: its parent is read afresh when the move runs.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                // A folder that cannot be looked at is a folder the plan cannot vouch for: the
                // plan fails, and nothing is moved on the strength of a structure it did not see.
                Err(e) => return Err(DriveError::Io(format!("{sub}: {e}"))),
            }
        }
        below.sort();
        for sub in &below {
            if out.len() >= FOLDERS {
                return Ok(out);
            }
            out.push(sub.clone());
        }
        for sub in below.into_iter().rev() {
            stack.push((sub, depth + 1));
        }
    }
    // The walk's order is depth first by level; the plan wants every parent before its children.
    out.sort();
    Ok(out)
}

/// The names the target's disk compares: asked of the disk under `target_prefix`; a bucket's are
/// exact.
fn target_volume(site: &Site, target_prefix: &str) -> (Volume, bool) {
    match site {
        Site::Disk { root } => {
            let mut folder = root.clone();
            for name in target_prefix.split('/').filter(|n| !n.is_empty()) {
                folder.push(name);
            }
            (volume_of(&folder), true)
        }
        Site::Remote { .. } => (Volume::default(), false),
    }
}

/// Plans copying or moving `items` of `source` into the folder `target_prefix` of `target`.
/// Lists every folder (one recursive listing each) and looks for every taken name at the target
/// (one listing per folder that is there, one HEAD per file). Explorer's rules: a copy into the
/// folder it came from is named "a - Copy"; a move into it does nothing; a folder never goes into
/// itself - however the two drives reach it ([`overlap`]). A move within one drive with nothing
/// in the way is one rename. `copy` is the window's language's word of a copy's name.
///
/// # Errors
///
/// [`DriveError::InvalidKey`] for a folder that would go into itself; the drive's error when a
/// listing or a look at the target fails.
pub fn plan(
    source: &dyn Drive,
    items: &[Item],
    target: &dyn Drive,
    target_prefix: &str,
    removes_source: bool,
    copy: &str,
) -> Result<Plan, DriveError> {
    let (source_site, target_site) = (source.site(), target.site());
    let mut plan = Plan {
        same_drive: source_site.same_place(&target_site),
        ..Plan::default()
    };
    let (volume, on_disk) = target_volume(&target_site, target_prefix);
    let policy = Policy::here(volume);
    let mut targets = Targets {
        drive: target,
        set: TargetSet::new(volume, on_disk),
        known: Vec::new(),
    };
    // A folder item of the drive's root ("Copy everything") stands for everything in it.
    let expanded: Vec<Item>;
    let items = if items.iter().any(|i| i.is_folder && i.key.is_empty()) {
        let root = ops::list_folder_all(source, "")?;
        expanded = root
            .folders
            .into_iter()
            .map(|key| Item {
                key,
                is_folder: true,
                size: None,
            })
            .chain(root.objects.into_iter().map(|object| Item {
                key: object.key,
                is_folder: false,
                size: Some(object.size),
            }))
            .chain(
                items
                    .iter()
                    .filter(|i| !(i.is_folder && i.key.is_empty()))
                    .cloned(),
            )
            .collect();
        &expanded[..]
    } else {
        items
    };
    for item in items {
        // Names a drive gave, written to a folder of this computer (not a copy from one folder of
        // it to another: those names are on the disk already).
        let check = (on_disk && source.local_path(&item.key).is_none()).then(|| NameCheck {
            policy,
            prefix: target_prefix.to_string(),
        });
        let name = key::last_segment(&item.key).to_string();
        let slash = if item.is_folder { "/" } else { "" };
        // Where the item lands with its own name.
        let landing = format!("{target_prefix}{name}{slash}");
        let relation = overlap(&source_site, &item.key, &target_site, &landing);
        if item.is_folder && relation == Overlap::SourceHoldsTarget {
            return Err(DriveError::InvalidKey {
                key: target_prefix.to_string(),
                reason: "the destination folder is inside the folder it would receive",
            });
        }
        // The item is where it would land: a move does nothing, a copy gets a name of its own.
        let same_place = relation == Overlap::Same;
        if same_place && removes_source {
            continue;
        }
        let target_name = if same_place {
            let mut failure = None;
            let mut probe =
                |n: &str| match targets.candidate_taken(&format!("{target_prefix}{n}{slash}")) {
                    Ok(taken) => taken,
                    Err(e) => {
                        failure.get_or_insert(e);
                        true
                    }
                };
            let chosen = if item.is_folder {
                copy_folder_name(&name, copy, &mut probe)
            } else {
                copy_name(&name, copy, &mut probe)
            };
            if let Some(e) = failure {
                return Err(e);
            }
            chosen
        } else {
            name.clone()
        };
        if item.is_folder {
            let target_key = format!("{target_prefix}{target_name}/");
            if NameCheck::refuses(check.as_ref(), &mut plan, &item.key, &target_key) {
                continue;
            }
            let target_exists = targets.is_there(&target_key)?;
            let free = targets.set.state(&target_key) == Claim::Free;
            if plan.same_drive && removes_source && !target_exists && free {
                targets.set.reserve(&target_key);
                plan.tops.push(target_key.clone());
                plan.moves.push((item.key.clone(), target_key));
                continue;
            }
            plan.tops.push(target_key.clone());
            if target_exists {
                for object in ops::list_all(target, &target_key)? {
                    targets.set.note_existing(&object.key);
                }
            }
            // Everything below the folder is known now: listed, or not there at all.
            targets.known.push(target_key.clone());
            targets.set.reserve(&target_key);
            plan.folders.push(target_key.clone());
            let mut below: std::collections::HashSet<String> = std::collections::HashSet::new();
            for object in ops::list_all(source, &item.key)? {
                let rest = &object.key[item.key.len()..];
                if rest.is_empty() {
                    continue; // the folder's own marker
                }
                let to = format!("{target_key}{rest}");
                if NameCheck::refuses(check.as_ref(), &mut plan, &object.key, &to) {
                    continue;
                }
                if rest.ends_with('/') {
                    if below.insert(to.clone()) {
                        plan.folders.push(to);
                    }
                    continue;
                }
                add_file(
                    &mut plan,
                    &mut targets,
                    &object.key,
                    &to,
                    Some(object.size),
                    object.etag.clone(),
                )?;
            }
            // A folder on disk has no marker object: the empty folders below it are in no recursive
            // listing, and a Move removed them from the source without ever making them at the
            // target - the structure existed nowhere (X-DELTA-AZDRIVE-FILES-15).
            for sub in disk_subfolders(source, &item.key)? {
                let to = format!("{target_key}{}", &sub[item.key.len()..]);
                if below.insert(to.clone()) {
                    plan.folders.push(to);
                }
            }
            if removes_source {
                plan.source_folders.push(item.key.clone());
            }
        } else {
            let wanted = format!("{target_prefix}{target_name}");
            if NameCheck::refuses(check.as_ref(), &mut plan, &item.key, &wanted) {
                continue;
            }
            let state = targets.set.state(&wanted);
            let in_the_way =
                state == Claim::Existing || (state == Claim::Free && targets.is_there(&wanted)?);
            if plan.same_drive && removes_source && !in_the_way {
                // Nothing at the target holds the name; a name an earlier file of this plan has
                // is not this one's: it gets another.
                let moved_to = if state == Claim::Planned {
                    targets.free_key(&wanted)?
                } else {
                    wanted
                };
                targets.set.reserve(&moved_to);
                plan.tops.push(moved_to.clone());
                plan.moves.push((item.key.clone(), moved_to));
                continue;
            }
            let size = match item.size {
                Some(size) => Some(size),
                None => source.head(&item.key).ok().map(|info| info.size),
            };
            let landed = add_file(&mut plan, &mut targets, &item.key, &wanted, size, None)?;
            plan.tops.push(landed);
        }
    }
    // The name "Keep both" gives each conflict is looked for last, when every name of the plan is
    // reserved: it never takes the name of a file of the batch (AZ X-AR5a-11).
    for index in plan.conflicts() {
        let wanted = plan.files[index].target_key.clone();
        let name = targets.free_key(&wanted)?;
        targets.set.reserve(&name);
        plan.files[index].keep_as = Some(name);
    }
    Ok(plan)
}
