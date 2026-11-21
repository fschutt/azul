//! The one-time move of an app's data from the folders older builds used
//! into the one data root (`<OS data dir>/Azlin`, see [`crate::data`]).
//!
//! AzShow and AzVideoCut kept their data in `<OS data dir>/azul/`, AzPhoto
//! in `<OS data dir>/Azul/`, AzNotes in `<OS data dir>/AzNotes/` - with the
//! same keys the data root uses (`show/<id>/deck.json`, `photo/<uuid>/...`,
//! `notes/<notebook>/<id>.md`). When an app starts through the kit
//! (`ui::create_kit`, the startup path every appkit app takes) its OWN
//! folder in those legacy folders is moved into the data root:
//!
//! - only for the default root (a root named by `--data-dir` or
//!   `$AZLIN_DATA` is the user's choice: nothing is moved into it);
//! - only the starting app's folder (`<legacy>/<app folder>/`): another app
//!   that still reads its legacy folder keeps its data where it reads it,
//!   and `azul/` also holds azul's own settings and styles, which stay;
//! - never over a file: a key the data root already has stays in the legacy
//!   folder (and is named in the note);
//! - a note, `MOVED-TO-AZLIN.txt`, is appended in the legacy folder each
//!   time something moved, saying where it went;
//! - idempotent: once the folder is moved there is nothing left to do (one
//!   `is_dir` check per legacy folder at each start).

use std::{fs, io::Write, path::Path};

use azul_storage::{
    beneath::{Dir, Fingerprint, Stat, Tri},
    ctl::Quiet,
    durable::{self, Expect, Publish, PublishError},
    local::source_bits,
    time::{iso8601, now_unix},
};

use crate::data::ROOT_DIR;

/// The folders under the OS data folder that older builds used as their
/// data root (AzShow / AzVideoCut, AzPhoto, AzNotes). On a case-insensitive
/// file system `azul` and `Azul` are one folder; that is fine.
pub const LEGACY_DIRS: [&str; 3] = ["azul", "Azul", "AzNotes"];

/// The note left in a legacy folder.
pub const NOTE_FILE: &str = "MOVED-TO-AZLIN.txt";

/// What one migration did, as data-root keys (`show/deck1/deck.json`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Migration {
    /// Moved into the data root.
    pub moved: Vec<String>,
    /// Left in the legacy folder: the data root has a file of that name.
    pub kept: Vec<String>,
    /// Could not be moved, with why.
    pub failed: Vec<(String, String)>,
}

/// Moves the app folder `app_folder` of every legacy folder under
/// `os_data_dir` into `root` - when `root` is the default data root
/// (`<os_data_dir>/Azlin`); otherwise nothing happens.
#[must_use]
pub fn migrate_app_data(os_data_dir: &Path, root: &Path, app_folder: &str) -> Migration {
    let mut out = Migration::default();
    if !is_one_name(app_folder) || root != os_data_dir.join(ROOT_DIR) {
        return out;
    }
    for dir in LEGACY_DIRS {
        let legacy = os_data_dir.join(dir);
        let source = legacy.join(app_folder);
        if !is_real_dir(&source) {
            continue;
        }
        let mut pass = Migration::default();
        move_tree(&source, &root.join(app_folder), app_folder, &mut pass);
        remove_empty_dirs(&source);
        pass.moved.sort();
        pass.kept.sort();
        if !pass.moved.is_empty() {
            // The note is a courtesy: a failure to write it moves nothing back.
            let _ = append_note(&legacy, root, &pass);
        }
        out.moved.extend(pass.moved);
        out.kept.extend(pass.kept);
        out.failed.extend(pass.failed);
    }
    // `azul` and `Azul` are one folder on a case-insensitive file system:
    // the second pass sees what the first one left.
    out.moved.sort();
    out.kept.sort();
    out.kept.dedup();
    out.failed.sort();
    out.failed.dedup();
    out
}

impl Migration {
    /// One line for the log: how many files moved, stayed, failed.
    #[must_use]
    pub fn summary(&self, root: &Path) -> String {
        format!(
            "{} file(s) moved from the old data folders into {} ({} left there: the data folder \
             has them already; {} could not be moved)",
            self.moved.len(),
            root.display(),
            self.kept.len(),
            self.failed.len()
        )
    }
}

/// An app folder is one plain name (`show`), never a path.
fn is_one_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':'])
}

/// A directory, not a link to one (a link is never walked).
fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
}

/// Moves every file under `source` to the same place under `target`;
/// `key` is the data-root key of `source`.
fn move_tree(source: &Path, target: &Path, key: &str, out: &mut Migration) {
    let entries = match fs::read_dir(source) {
        Ok(entries) => entries,
        Err(e) => {
            out.failed.push((format!("{key}/"), e.to_string()));
            return;
        }
    };
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue; // not UTF-8: no key can name it; it stays
        };
        let child_key = format!("{key}/{name}");
        let path = entry.path();
        if is_real_dir(&path) {
            move_tree(&path, &target.join(&name), &child_key, out);
        } else {
            move_file(&path, &target.join(&name), child_key, out);
        }
    }
}

/// The folder of `path` (links followed: the user's own folders) and the file's name in it.
fn folder_and_name(path: &Path) -> std::io::Result<(Dir, String)> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no file name"))?;
    Ok((Dir::open_following(parent)?, name.to_string()))
}

/// How a file got to its place.
enum Placed {
    Moved,
    /// A file has the name at the target: nothing was replaced.
    Taken,
    Failed(std::io::Error),
}

/// Puts the file `source` at `target`: renamed where both are on one disk - the name at the target
/// taken in the one step of the rename, so a file that turns up there between a look and the
/// move is not replaced either - else copied.
fn place(source: &Path, target: &Path) -> Placed {
    let ((from_dir, from), (to_dir, to)) = match (folder_and_name(source), folder_and_name(target))
    {
        (Ok(from), Ok(to)) => (from, to),
        (Err(e), _) | (_, Err(e)) => return Placed::Failed(e),
    };
    match from_dir.rename_noreplace(&from, &to_dir, &to) {
        Ok(()) => {
            // The entries are on the disk before the old folder forgets the file.
            to_dir.sync();
            from_dir.sync();
            Placed::Moved
        }
        Err(e) if e.is_already_exists() => Placed::Taken,
        // Another disk, or one that cannot rename: copied instead.
        Err(_) => match copy_then_remove(source, target) {
            Ok(()) => Placed::Moved,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Placed::Taken,
            Err(e) => Placed::Failed(e),
        },
    }
}

/// Moves one file, never over another one.
fn move_file(source: &Path, target: &Path, key: String, out: &mut Migration) {
    if fs::symlink_metadata(target).is_ok() {
        out.kept.push(key);
        return;
    }
    if let Some(parent) = target.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            out.failed.push((key, e.to_string()));
            return;
        }
    }
    match place(source, target) {
        Placed::Moved => out.moved.push(key),
        Placed::Taken => out.kept.push(key),
        Placed::Failed(e) => out.failed.push((key, e.to_string())),
    }
}

/// A move across file systems: copied into a new file next to the target ([`durable::publish`]:
/// written, synced, renamed in where nothing is, the folder synced), and the source removed only
/// once the copy is whole on the disk and the source is still the file that was copied. A source
/// that changed meanwhile stays where it is and the copy goes.
fn copy_then_remove(source: &Path, target: &Path) -> std::io::Result<()> {
    let mut from = fs::File::open(source)?;
    let before = Fingerprint::of(&azul_storage::beneath::stat_of(&from)?);
    let (mode, mtime) = source_bits(&from);
    let (to_dir, to) = folder_and_name(target)?;
    let mut publish = Publish::new(Expect::Absent, mode, &Quiet);
    if let Some(mtime) = mtime {
        publish = publish.with_mtime(mtime);
    }
    durable::publish(&to_dir, &to, &mut from, publish).map_err(|e| match e {
        PublishError::Changed => std::io::Error::from(std::io::ErrorKind::AlreadyExists),
        other => other.into(),
    })?;
    let (from_dir, from_name) = folder_and_name(source)?;
    let still_the_file = from_dir
        .lstat(&from_name)
        .is_ok_and(|now| unchanged_since(&before, &now, source, target));
    if !still_the_file {
        let _ = to_dir.unlink(&to);
        return Err(std::io::Error::other(
            "it changed while it was copied: it stays where it was",
        ));
    }
    fs::remove_file(source)?;
    from_dir.sync();
    Ok(())
}

/// Whether the source is, for certain, the file that was copied: `now` (an `lstat` of it) matches
/// the fingerprint taken before the copy ([`Fingerprint::matches`]: the same size, times and
/// identity). A system that gives files no identity (FAT, a share without file ids) answers
/// [`Tri::Unknown`]; a delete on a guess is no delete: the bytes of the source and of the copy are
/// compared, and the source goes only when they are one (X-DELTA-AZDRIVE-FILES-23).
fn unchanged_since(before: &Fingerprint, now: &Stat, source: &Path, copy: &Path) -> bool {
    match before.matches(now) {
        Tri::Same => true,
        Tri::Different => false,
        // collapse-ok: bytes that cannot be read to the end are not bytes known to be equal - the
        // source stays where it is.
        Tri::Unknown => same_bytes(source, copy).unwrap_or(false),
    }
}

/// Whether the files at `a` and `b` hold the same bytes.
fn same_bytes(a: &Path, b: &Path) -> std::io::Result<bool> {
    use std::io::Read;
    let (mut a, mut b) = (fs::File::open(a)?, fs::File::open(b)?);
    if a.metadata()?.len() != b.metadata()?.len() {
        return Ok(false);
    }
    let (mut left, mut right) = (vec![0u8; 64 * 1024], vec![0u8; 64 * 1024]);
    loop {
        let n = a.read(&mut left)?;
        if n == 0 {
            return Ok(b.read(&mut right)? == 0);
        }
        b.read_exact(&mut right[..n])?;
        if left[..n] != right[..n] {
            return Ok(false);
        }
    }
}

/// Removes the folders under `dir` (and `dir`) that the move left empty.
fn remove_empty_dirs(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if is_real_dir(&path) {
                remove_empty_dirs(&path);
            }
        }
    }
    let _ = fs::remove_dir(dir); // only succeeds when empty
}

/// Appends what `pass` did to the note in the legacy folder.
fn append_note(legacy: &Path, root: &Path, pass: &Migration) -> std::io::Result<()> {
    let mut text = format!(
        "{} - the Azlin apps keep their data in one folder now:\n    {}\n",
        iso8601(now_unix()),
        root.display()
    );
    text.push_str("Moved there from this folder:\n");
    for key in &pass.moved {
        text.push_str(&format!("    {key}\n"));
    }
    if !pass.kept.is_empty() {
        text.push_str("Left here, because the data folder already has a file of that name:\n");
        for key in &pass.kept {
            text.push_str(&format!("    {key}\n"));
        }
    }
    if !pass.failed.is_empty() {
        text.push_str("Could not be moved:\n");
        for (key, why) in &pass.failed {
            text.push_str(&format!("    {key}: {why}\n"));
        }
    }
    text.push('\n');
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(legacy.join(NOTE_FILE))?
        .write_all(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use azul_storage::testing::TempDir;

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn read(path: &Path) -> Vec<u8> {
        std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    fn root_of(os: &Path) -> std::path::PathBuf {
        os.join(ROOT_DIR)
    }

    #[test]
    fn the_apps_legacy_folder_moves_into_the_data_root() {
        let os = TempDir::new("migrate-move");
        let legacy = os.path().join("azul");
        write(&legacy.join("show/deck1/deck.json"), b"{\"deck\":1}");
        write(&legacy.join("show/deck1/media/a.png"), b"png");
        let root = root_of(os.path());

        let done = migrate_app_data(os.path(), &root, "show");

        assert_eq!(
            done.moved,
            vec![
                "show/deck1/deck.json".to_string(),
                "show/deck1/media/a.png".to_string()
            ]
        );
        assert!(done.kept.is_empty() && done.failed.is_empty(), "{done:?}");
        assert_eq!(read(&root.join("show/deck1/deck.json")), b"{\"deck\":1}");
        assert_eq!(read(&root.join("show/deck1/media/a.png")), b"png");
        assert!(
            !legacy.join("show").exists(),
            "the emptied app folder is gone from the legacy folder"
        );
        let note = String::from_utf8(read(&legacy.join(NOTE_FILE))).unwrap();
        assert!(note.contains(&root.display().to_string()), "{note}");
        assert!(note.contains("show/deck1/deck.json"), "{note}");
    }

    #[test]
    fn a_file_the_data_root_already_has_is_never_overwritten() {
        let os = TempDir::new("migrate-keep");
        let legacy = os.path().join("azul");
        let root = root_of(os.path());
        write(&legacy.join("videocut/p1/project.json"), b"old");
        write(&legacy.join("videocut/p2/project.json"), b"other");
        write(&root.join("videocut/p1/project.json"), b"new");

        let done = migrate_app_data(os.path(), &root, "videocut");

        assert_eq!(done.moved, vec!["videocut/p2/project.json".to_string()]);
        assert_eq!(done.kept, vec!["videocut/p1/project.json".to_string()]);
        assert_eq!(read(&root.join("videocut/p1/project.json")), b"new");
        assert_eq!(
            read(&legacy.join("videocut/p1/project.json")),
            b"old",
            "the old file stays where it was"
        );
        assert_eq!(read(&root.join("videocut/p2/project.json")), b"other");
        let note = String::from_utf8(read(&legacy.join(NOTE_FILE))).unwrap();
        assert!(note.contains("videocut/p1/project.json"), "{note}");
    }

    #[test]
    fn running_it_again_changes_nothing() {
        let os = TempDir::new("migrate-again");
        let legacy = os.path().join("azul");
        let root = root_of(os.path());
        write(&legacy.join("show/d/deck.json"), b"1");
        write(&legacy.join("show/e/deck.json"), b"2");
        write(&root.join("show/e/deck.json"), b"taken");
        let first = migrate_app_data(os.path(), &root, "show");
        assert_eq!(first.moved.len(), 1);
        let note = read(&legacy.join(NOTE_FILE));

        let second = migrate_app_data(os.path(), &root, "show");

        assert!(second.moved.is_empty(), "{second:?}");
        assert_eq!(read(&legacy.join(NOTE_FILE)), note, "no second note");
        assert_eq!(read(&root.join("show/d/deck.json")), b"1");
        assert_eq!(read(&root.join("show/e/deck.json")), b"taken");
    }

    #[test]
    fn other_apps_folders_and_azuls_own_files_stay() {
        let os = TempDir::new("migrate-others");
        let legacy = os.path().join("azul");
        let root = root_of(os.path());
        write(&legacy.join("show/d/deck.json"), b"deck");
        write(&legacy.join("videocut/p/project.json"), b"project");
        write(&legacy.join("styles/AzShow.css"), b"body {}");
        write(&legacy.join("telemetry.json"), b"{}");

        let done = migrate_app_data(os.path(), &root, "show");

        assert_eq!(done.moved, vec!["show/d/deck.json".to_string()]);
        assert_eq!(read(&legacy.join("videocut/p/project.json")), b"project");
        assert_eq!(read(&legacy.join("styles/AzShow.css")), b"body {}");
        assert_eq!(read(&legacy.join("telemetry.json")), b"{}");
        assert!(!root.join("videocut").exists());
    }

    #[test]
    fn a_root_the_user_named_is_never_migrated_into() {
        let os = TempDir::new("migrate-custom");
        let legacy = os.path().join("azul");
        write(&legacy.join("show/d/deck.json"), b"deck");
        let custom = os.path().join("elsewhere");

        let done = migrate_app_data(os.path(), &custom, "show");

        assert_eq!(done, Migration::default());
        assert_eq!(read(&legacy.join("show/d/deck.json")), b"deck");
        assert!(!custom.exists());
        assert!(!legacy.join(NOTE_FILE).exists());
    }

    #[test]
    fn the_photo_and_notes_legacy_folders_move_too() {
        let os = TempDir::new("migrate-photo-notes");
        let root = root_of(os.path());
        write(&os.path().join("Azul/photo/u1/doc.json"), b"photo");
        write(&os.path().join("AzNotes/notes/Work/n1.md"), b"# note");

        let photo = migrate_app_data(os.path(), &root, "photo");
        let notes = migrate_app_data(os.path(), &root, "notes");

        assert_eq!(photo.moved, vec!["photo/u1/doc.json".to_string()]);
        assert_eq!(notes.moved, vec!["notes/Work/n1.md".to_string()]);
        assert_eq!(read(&root.join("photo/u1/doc.json")), b"photo");
        assert_eq!(read(&root.join("notes/Work/n1.md")), b"# note");
    }

    #[test]
    fn without_a_legacy_folder_nothing_is_touched() {
        let os = TempDir::new("migrate-none");
        let root = root_of(os.path());
        let done = migrate_app_data(os.path(), &root, "calculator");
        assert_eq!(done, Migration::default());
        assert!(!root.exists(), "no data root is made for nothing");
    }

    #[test]
    fn an_app_folder_that_is_not_one_name_is_refused() {
        let os = TempDir::new("migrate-bad-name");
        let root = root_of(os.path());
        write(&os.path().join("azul/styles/x.css"), b"x");
        for bad in ["", "..", ".", "a/b", "/abs"] {
            assert_eq!(
                migrate_app_data(os.path(), &root, bad),
                Migration::default(),
                "{bad:?}"
            );
        }
        assert_eq!(read(&os.path().join("azul/styles/x.css")), b"x");
    }
    #[test]
    fn x_ar5a_4_the_copy_across_disks_is_whole_and_synced_keeps_mode_and_time_and_never_replaces() {
        let os = TempDir::new("migrate-copy");
        let source = os.path().join("old/show/deck.json");
        write(&source, b"{\"deck\":1}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let long_ago = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_300_000_000);
        fs::File::options()
            .write(true)
            .open(&source)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
        let target = os.path().join("Azlin/show/deck.json");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        copy_then_remove(&source, &target).unwrap();
        assert_eq!(read(&target), b"{\"deck\":1}");
        assert!(
            !source.exists(),
            "the source goes once the copy is on the disk"
        );
        assert_eq!(fs::metadata(&target).unwrap().modified().unwrap(), long_ago);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&target).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let left: Vec<String> = fs::read_dir(target.parent().unwrap())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, vec![String::from("deck.json")], "no temporary file");
        // A file that is at the target is never replaced, and the source stays.
        write(&source, b"newer");
        let refused = copy_then_remove(&source, &target).unwrap_err();
        assert_eq!(refused.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(read(&target), b"{\"deck\":1}");
        assert_eq!(read(&source), b"newer");
    }

    /// X-DELTA-AZDRIVE-FILES-23 (X-AR5a-8). The source of a migration was removed after the copy
    /// when its fingerprint was "not different" - which an identity the system does not give
    /// (FAT, a share without file ids: inode 0) is, whatever else changed in the same size and
    /// time. Unknown is not Same: the bytes of the two files decide.
    #[test]
    fn x_delta_azdrive_files_23_a_source_whose_identity_is_unknown_goes_only_when_the_bytes_are_the_copys(
    ) {
        use azul_storage::beneath::Kind;
        let os = TempDir::new("migrate-unknown-identity");
        let source = os.path().join("legacy/deck.json");
        let copy = os.path().join("Azlin/deck.json");
        write(&source, b"{\"deck\":1}");
        write(&copy, b"{\"deck\":1}");
        // An lstat of a system with no inodes: the same size and time, no identity.
        let stat = |size: u64| Stat {
            kind: Kind::File,
            size,
            mtime_ns: 1_300_000_000_000_000_000,
            ctime_ns: 0,
            ino: 0,
            dev: 0,
            mode: 0o600,
            nlink: 1,
        };
        let before = Fingerprint::of(&stat(10));
        assert!(
            matches!(before.matches(&stat(10)), Tri::Unknown),
            "the case: everything that can be told matches and the identity cannot"
        );
        assert!(
            unchanged_since(&before, &stat(10), &source, &copy),
            "the same bytes: nothing was written since the copy"
        );
        // Written to in the same size and time (a save that keeps both): the bytes differ.
        write(&source, b"{\"deck\":2}");
        assert!(
            !unchanged_since(&before, &stat(10), &source, &copy),
            "the source was edited and would have been deleted"
        );
        // A known difference stays a difference; a known identity stays the same file.
        assert!(!unchanged_since(&before, &stat(11), &source, &copy));
        let known = Fingerprint::from_parts(Kind::File, 10, 5, 0, 1, 42);
        let mut now = stat(10);
        now.mtime_ns = 5;
        now.dev = 1;
        now.ino = 42;
        assert!(unchanged_since(&known, &now, &source, &copy));
        // Bytes that cannot be read are not bytes known to be equal.
        assert!(!unchanged_since(
            &before,
            &stat(10),
            &os.path().join("gone"),
            &copy
        ));
    }

    #[test]
    fn x_ar5a_4_a_file_that_turns_up_at_the_target_is_kept_not_replaced() {
        // The rename itself refuses a taken name: `kept`, the legacy file stays, the new one is
        // untouched (the look before it is only the quick answer).
        let os = TempDir::new("migrate-noreplace");
        let legacy = os.path().join("azul");
        let root = root_of(os.path());
        write(&legacy.join("show/d/deck.json"), b"old");
        write(&root.join("show/d/deck.json"), b"new");
        assert!(matches!(
            place(
                &legacy.join("show/d/deck.json"),
                &root.join("show/d/deck.json")
            ),
            Placed::Taken
        ));
        assert_eq!(read(&root.join("show/d/deck.json")), b"new");
        assert_eq!(read(&legacy.join("show/d/deck.json")), b"old");
    }
}
