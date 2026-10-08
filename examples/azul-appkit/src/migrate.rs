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

use azul_storage::time::{iso8601, now_unix};

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
    let moved = fs::rename(source, target).or_else(|_| copy_then_remove(source, target));
    match moved {
        Ok(()) => out.moved.push(key),
        Err(e) => out.failed.push((key, e.to_string())),
    }
}

/// A move across file systems: copied next to the target, renamed into
/// place, then the source removed.
fn copy_then_remove(source: &Path, target: &Path) -> std::io::Result<()> {
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = target.with_file_name(format!(".{name}.azlin-migrate.tmp"));
    let copied = fs::copy(source, &tmp).and_then(|_| fs::rename(&tmp, target));
    if let Err(e) = copied {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::remove_file(source)
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
}
