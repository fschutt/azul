//! A move into a synced folder that never loses the source: the files are copied into the
//! folder at once, and each source file is removed only after a pass committed its copy - the
//! drive's index names it with the very content the source has. A move whose upload fails (the
//! drive refuses, the network is gone) keeps the source where it was; a source changed since is
//! never removed. The moves waiting live in the pairing's state folder ([`MOVES_FILE`]).

use std::{
    fs,
    path::{Path, PathBuf},
};

use azul_storage::{
    beneath::{BeneathError, Root},
    ctl::Quiet,
    durable::{self, Expect, Mode, Publish},
    names::{OnDisk, Policy, Purpose, Volume},
};
use serde::{Deserialize, Serialize};

use super::{local::hash_file, remote::RemoteIndex};
use crate::{
    error::{fail, CloudError, CloudResult, Context},
    state::{read_json, write_json},
};

/// The moves waiting for the drive, in a pairing's state folder.
pub const MOVES_FILE: &str = "moves.json";

/// One source file waiting for its copy to land.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingMove {
    /// The source file, outside the synced folder.
    pub source: PathBuf,
    /// Its copy's key in the pairing, and the content both have (BLAKE3).
    pub key: String,
    pub hash: String,
}

/// The moves of a pairing waiting for the drive.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingMoves {
    pub moves: Vec<PendingMove>,
    /// The folders moved: removed once nothing is left in them.
    #[serde(default)]
    pub folders: Vec<PathBuf>,
}

impl PendingMoves {
    fn load(state_dir: &Path) -> PendingMoves {
        read_json(&state_dir.join(MOVES_FILE))
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    fn save(&self, state_dir: &Path) -> CloudResult<()> {
        write_json(&state_dir.join(MOVES_FILE), self, true)
    }
}

/// Copies `from` (a file, or a folder with what it holds) into the synced folder `root` at
/// `key`, and keeps each source file waiting for its copy to land ([`settle`]). A link in a
/// moved folder is never followed: it stays at the source. The keys copied.
///
/// # Errors
///
/// When `from` is inside the synced folder, the key is taken there, or a copy fails (what was
/// copied stays, its sources wait as the others do).
pub fn move_in(root: &Path, state_dir: &Path, from: &Path, key: &str) -> CloudResult<Vec<String>> {
    if from.starts_with(root) {
        fail!("{} is in the synced folder already", from.display());
    }
    let key = key.trim_matches('/');
    // Every name is reached from the folder's root without a link: nothing lands in a folder
    // that a link below the synced folder points at.
    let tree = Root::open(root).with_context(|| root.display())?;
    // A look at the name the user typed: the shape rows, no folder made for it (ARCH29 C6c).
    let taken = OnDisk::of_entry(key)
        .map_err(BeneathError::from)
        .and_then(|on_disk| {
            tree.parent_of_existing(&on_disk)
                .map(|(dir, name)| dir.lstat(name).is_ok())
        });
    match taken {
        Ok(true) => fail!("{key} is there already"),
        Err(e) if !e.is_not_found() => return Err(CloudError::from(e).context(key)),
        _ => {}
    }
    let mut pending = PendingMoves::load(state_dir);
    let mut copied = Vec::new();
    let meta = fs::symlink_metadata(from).with_context(|| from.display())?;
    let result = if meta.is_dir() {
        let copying = copy_tree(&tree, from, key, &mut pending, &mut copied);
        // A folder with no file in it has nothing for the drive to hold: its source stays.
        if !copied.is_empty() {
            pending.folders.push(from.to_path_buf());
        }
        copying
    } else if meta.is_file() {
        copy_one(&tree, from, key, &mut pending, &mut copied)
    } else {
        Err(crate::error::CloudError::failed(format!(
            "{} is no file or folder",
            from.display()
        )))
    };
    pending.save(state_dir)?;
    result.map(|()| copied)
}

/// Copies the file `from` to `key` under `tree`: into a new file in the key's folder, synced
/// and renamed in, with the source's permission bits (never setuid, setgid or sticky).
fn copy_one(
    tree: &Root,
    from: &Path,
    key: &str,
    pending: &mut PendingMoves,
    copied: &mut Vec<String>,
) -> CloudResult<()> {
    let mut source = fs::File::open(from).with_context(|| from.display())?;
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        source
            .metadata()
            .map_or(0o644, |meta| meta.permissions().mode() & 0o777)
    };
    #[cfg(not(unix))]
    let mode = 0o644;
    // The user's own folder moves in with its own names (a repository keeps its `.git`: the scan's
    // rules leave it out later): the rows of a name a person chose, then the folders are made.
    let admitted = Policy::here(Volume::default())
        .admit(key, Purpose::UserTyped)
        .map_err(BeneathError::from)
        .context(key)?;
    let (dir, name) = tree.parent_of_new(&admitted).context(key)?;
    // Only into a name nothing holds at the moment of the write: the look at the top name was made
    // before the copy began, and a file the sync brought down under this one meanwhile (or a name the
    // source's own folder shares with it) is never overwritten (X-DELTA-SYNC4-23).
    durable::publish(
        &dir,
        name,
        &mut source,
        Publish::new(Expect::Absent, Mode::FromSource(mode), &Quiet),
    )
    .map_err(BeneathError::from)
    .context(key)?;
    let hash = hash_file(from).with_context(|| from.display())?;
    pending.moves.push(PendingMove {
        source: from.to_path_buf(),
        key: key.to_string(),
        hash,
    });
    copied.push(key.to_string());
    Ok(())
}

fn copy_tree(
    tree: &Root,
    from: &Path,
    key: &str,
    pending: &mut PendingMoves,
    copied: &mut Vec<String>,
) -> CloudResult<()> {
    // The folder is made for a name a person chose, after the rows of one (ARCH29 C6c).
    let admitted = Policy::here(Volume::default())
        .admit(key, Purpose::UserTyped)
        .map_err(BeneathError::from)
        .context(key)?;
    tree.dir_new(&admitted).context(key)?;
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(from)
        .with_context(|| from.display())?
        .flatten()
        .collect();
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let inner = format!("{key}/{name}");
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            copy_tree(tree, &path, &inner, pending, copied)?;
        } else if meta.is_file() {
            copy_one(tree, &path, &inner, pending, copied)?;
        }
    }
    Ok(())
}

/// After a pass that committed `remote` (the drive's index as it is now): every source whose
/// copy the index names with the content the source still has is removed, and a moved folder
/// left empty with it. The sources removed.
pub fn settle(state_dir: &Path, remote: &RemoteIndex) -> Vec<PathBuf> {
    let mut pending = PendingMoves::load(state_dir);
    if pending.moves.is_empty() && pending.folders.is_empty() {
        return Vec::new();
    }
    let mut removed = Vec::new();
    pending.moves.retain(|m| {
        let landed = remote.files.get(&m.key).is_some_and(|f| f.hash == m.hash);
        if !landed {
            return true;
        }
        match hash_file(&m.source) {
            // Unchanged since the move: the drive holds it, the source goes.
            Ok(hash) if hash == m.hash => {
                if fs::remove_file(&m.source).is_ok() {
                    removed.push(m.source.clone());
                }
                false
            }
            // Changed since, or gone: it is the user's now, never removed.
            _ => false,
        }
    });
    let waiting: Vec<PathBuf> = pending.moves.iter().map(|m| m.source.clone()).collect();
    pending.folders.retain(|folder| {
        if waiting.iter().any(|s| s.starts_with(folder)) {
            return true;
        }
        remove_empty(folder);
        false
    });
    let _ = pending.save(state_dir);
    removed
}

/// Removes `dir` and the folders in it, as far as they are empty (a file left keeps its
/// folders).
fn remove_empty(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir()) {
                remove_empty(&path);
            }
        }
    }
    let _ = fs::remove_dir(dir);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::{local::hash_bytes, remote::RemoteFile};
    use azul_storage::testing::TempDir;

    fn index_with(files: &[(&str, &[u8])]) -> RemoteIndex {
        let mut index = RemoteIndex::empty();
        for (key, bytes) in files {
            index.files.insert(
                (*key).to_string(),
                RemoteFile {
                    hash: hash_bytes(bytes),
                    size: bytes.len() as u64,
                    mtime: 0,
                    mtime_ns: 0,
                    gen: 1,
                    device: String::from("a"),
                    exec: false,
                    link: None,
                },
            );
        }
        index
    }

    /// X-DELTA-SYNC4-23. The copy of a file of a moved folder was published with `Expect::Any` after a
    /// look at the TOP name only: a file the sync brought down under the inner name since (or any file
    /// there) was overwritten by the source's. The copy is made only into a name that is free at the
    /// moment of the write; a taken one is an error, the file there stays, and the source waits. Pinned at
    /// `copy_one`, the step every inner name of `move_in` goes through - the window between the look and
    /// the write cannot be entered from outside. RED on the old code: the file is overwritten.
    #[test]
    fn x_delta_sync4_23_a_copy_never_overwrites_a_file_that_is_there_at_the_write() {
        let root = TempDir::new("azcloud-movein-s23-root");
        let outside = TempDir::new("azcloud-movein-s23-outside");
        let source = outside.path().join("report.txt");
        fs::write(&source, b"the source's report").unwrap();
        // The sync brought a file down under the inner name meanwhile.
        fs::create_dir_all(root.path().join("box")).unwrap();
        fs::write(
            root.path().join("box/report.txt"),
            b"the file the sync brought down",
        )
        .unwrap();
        let tree = Root::open(root.path()).unwrap();
        let mut pending = PendingMoves::default();
        let mut copied = Vec::new();
        let result = copy_one(&tree, &source, "box/report.txt", &mut pending, &mut copied);
        assert_eq!(
            fs::read(root.path().join("box/report.txt")).unwrap(),
            b"the file the sync brought down",
            "the file there stays: {result:?}"
        );
        assert!(result.is_err(), "a taken name is an error");
        assert!(
            copied.is_empty() && pending.moves.is_empty(),
            "nothing waits for a copy that was not made"
        );
        assert!(source.exists(), "the source is untouched");
    }

    #[test]
    fn a_moved_source_goes_only_once_the_drive_holds_its_copy() {
        let root = TempDir::new("azcloud-movein-root");
        let state = TempDir::new("azcloud-movein-state");
        let outside = TempDir::new("azcloud-movein-outside");
        let victim = outside.path().join("victim");
        fs::create_dir_all(victim.join("sub")).unwrap();
        fs::write(victim.join("a.txt"), b"a").unwrap();
        fs::write(victim.join("sub/b.txt"), b"b").unwrap();
        let copied = move_in(root.path(), state.path(), &victim, "moved").unwrap();
        assert_eq!(copied, vec!["moved/a.txt", "moved/sub/b.txt"]);
        assert_eq!(fs::read(root.path().join("moved/sub/b.txt")).unwrap(), b"b");
        // The upload failed: the drive holds nothing, the sources stay.
        assert!(settle(state.path(), &RemoteIndex::empty()).is_empty());
        assert!(victim.join("a.txt").exists());
        // One copy landed: its source goes, the other waits.
        let removed = settle(state.path(), &index_with(&[("moved/a.txt", b"a")]));
        assert_eq!(removed, vec![victim.join("a.txt")]);
        assert!(victim.join("sub/b.txt").exists());
        // A source changed since is never removed; the folder goes once it is empty.
        fs::write(victim.join("sub/b.txt"), b"changed").unwrap();
        let removed = settle(state.path(), &index_with(&[("moved/sub/b.txt", b"b")]));
        assert!(removed.is_empty());
        assert!(victim.join("sub/b.txt").exists(), "the user's own change stays");
        assert!(move_in(root.path(), state.path(), &root.path().join("moved"), "x").is_err());
    }

    /// ARCH29 C6c. A move-in looks at the name the user typed as an `OnDisk` key (no folder made for a
    /// look: a name that leaves the folder is refused before anything is copied) and makes the folders of
    /// each copy for a name a person chose (`Admitted`, the shape and this system's rows): the user's own
    /// repository moves in with its `.git`. Green on the old code too (`Policy::walk` with the same
    /// purpose): it pins what the types now hold.
    #[test]
    fn c6c_a_move_in_looks_without_making_and_makes_folders_for_the_users_own_names() {
        let root = TempDir::new("azcloud-movein-c6c-root");
        let state = TempDir::new("azcloud-movein-c6c-state");
        let outside = TempDir::new("azcloud-movein-c6c-outside");
        let source = outside.path().join("proj");
        fs::create_dir_all(source.join(".git")).unwrap();
        fs::write(source.join("a.txt"), b"a").unwrap();
        fs::write(source.join(".git/config"), b"c").unwrap();
        for key in ["../escape", "a/../../b", "x\0y"] {
            assert!(
                move_in(root.path(), state.path(), &source, key).is_err(),
                "{key:?}"
            );
        }
        assert_eq!(
            fs::read_dir(root.path()).unwrap().count(),
            0,
            "a folder was made"
        );
        let copied = move_in(root.path(), state.path(), &source, "proj").unwrap();
        assert_eq!(copied, vec!["proj/.git/config", "proj/a.txt"]);
        assert_eq!(
            fs::read(root.path().join("proj/.git/config")).unwrap(),
            b"c"
        );
        // The name is taken now: said, nothing copied over it.
        assert!(move_in(root.path(), state.path(), &source, "proj").is_err());
    }
}
