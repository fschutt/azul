//! The conditional vocabulary on a folder on disk and the moves that never lose a file (ARCH28
//! RF43 / AR5a-2 step 3): a file has a version (its tag), a write can ask that the name still
//! holds it, a move is a rename that never lands on a file, a folder is removed only when empty.

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

use super::TempDir;
use crate::{
    BatchMove, ByteRange, Drive, DriveError, ListPage, ListRequest, LocalDrive, ObjectInfo,
    Precondition,
};

fn conflict(result: &Result<impl std::fmt::Debug, DriveError>) -> bool {
    matches!(result, Err(DriveError::Conflict { .. }))
}

/// The tag of the file now.
fn tag(drive: &dyn Drive, key: &str) -> String {
    drive
        .head(key)
        .unwrap()
        .etag
        .expect("a file on disk has a tag")
}

/// What a folder on disk holds besides what the drive lists: temporary files left behind.
fn leftovers(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".azul-storage-"))
        .collect()
}

#[test]
fn a_file_has_a_tag_that_changes_when_the_file_does() {
    let tmp = TempDir::new("local-tag");
    let drive = LocalDrive::without_manifest(tmp.path());
    drive.put("a.txt", b"one").unwrap();
    let first = tag(&drive, "a.txt");
    assert_eq!(first, tag(&drive, "a.txt"), "the same file, the same tag");
    drive.put("a.txt", b"another text").unwrap();
    assert_ne!(first, tag(&drive, "a.txt"), "written to: another version");
}

#[test]
fn a_write_that_asks_for_a_free_name_never_replaces_a_file_made_meanwhile() {
    let tmp = TempDir::new("local-absent");
    let drive = LocalDrive::without_manifest(tmp.path());
    // Free: the file is made and the tag of the new version comes back.
    let made = drive
        .put_if("new.txt", b"mine", &Precondition::Absent)
        .unwrap();
    assert_eq!(made, Some(tag(&drive, "new.txt")));
    // Made by somebody else since the plan: not replaced, and nothing left behind.
    let again = drive.put_if("new.txt", b"yours", &Precondition::Absent);
    assert!(conflict(&again), "{again:?}");
    assert_eq!(drive.get("new.txt").unwrap(), b"mine");
    let mut body: &[u8] = b"streamed";
    let streamed = drive.put_from_if("new.txt", &mut body, &Precondition::Absent);
    assert!(conflict(&streamed), "{streamed:?}");
    assert_eq!(drive.get("new.txt").unwrap(), b"mine");
    assert!(
        leftovers(tmp.path()).is_empty(),
        "no temporary file is left"
    );
}

#[test]
fn a_write_that_asks_for_a_version_replaces_that_version_and_no_other() {
    let tmp = TempDir::new("local-matches");
    let drive = LocalDrive::without_manifest(tmp.path());
    drive.put("a.txt", b"first").unwrap();
    let seen = tag(&drive, "a.txt");
    // The user saved it again after the plan was made.
    drive.put("a.txt", b"edited since").unwrap();
    let stale = drive.put_if("a.txt", b"replacement", &Precondition::Matches(seen));
    assert!(conflict(&stale), "{stale:?}");
    assert_eq!(drive.get("a.txt").unwrap(), b"edited since");
    // The version it has now is replaced.
    let now = tag(&drive, "a.txt");
    drive
        .put_if("a.txt", b"replacement", &Precondition::Matches(now))
        .unwrap();
    assert_eq!(drive.get("a.txt").unwrap(), b"replacement");
    // A tag of another kind is never this file's.
    let foreign = drive.put_if(
        "a.txt",
        b"x",
        &Precondition::Matches(String::from("\"d41d8cd98f00b204e9800998ecf8427e\"")),
    );
    assert!(conflict(&foreign), "{foreign:?}");
    // A file that is gone is not the version either.
    let gone = drive.put_if(
        "missing.txt",
        b"x",
        &Precondition::Matches(tag(&drive, "a.txt")),
    );
    assert!(conflict(&gone), "{gone:?}");
    assert!(leftovers(tmp.path()).is_empty());
}

/// AZ-U8: moving files inside a local drive and Delete-to-trash were `fs::copy` plus unlink: a
/// full disk failed them, a 20 GB move ran for minutes, the file lost its attributes.
#[cfg(unix)]
#[test]
fn az_u8_moving_files_inside_a_local_drive_is_a_rename() {
    use std::os::unix::fs::MetadataExt;
    let tmp = TempDir::new("u8");
    let drive = LocalDrive::without_manifest(tmp.path());
    drive.put("a.txt", b"alpha").unwrap();
    drive.put("b.txt", b"beta").unwrap();
    let inodes = |keys: &[&str]| -> Vec<u64> {
        keys.iter()
            .map(|k| std::fs::metadata(tmp.path().join(k)).unwrap().ino())
            .collect()
    };
    let before = inodes(&["a.txt", "b.txt"]);
    let results = drive.move_many(&[
        BatchMove::new("a.txt", ".azdrive-trash/20261105-1/a.txt"),
        BatchMove::new("b.txt", "moved/b.txt"),
    ]);
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    assert_eq!(
        inodes(&[".azdrive-trash/20261105-1/a.txt", "moved/b.txt"]),
        before,
        "the same files: a rename, not a copy and an unlink"
    );
    assert!(!tmp.path().join("a.txt").exists());
    assert!(!tmp.path().join("b.txt").exists());
}

#[test]
fn a_move_never_lands_on_a_file_and_a_file_that_changed_stays() {
    let tmp = TempDir::new("local-move-guards");
    let drive = LocalDrive::without_manifest(tmp.path());
    drive.put("a.txt", b"a").unwrap();
    drive.put("taken.txt", b"in the way").unwrap();
    drive.put("b.txt", b"b").unwrap();
    let old = tag(&drive, "b.txt");
    drive.put("b.txt", b"b, edited since").unwrap();
    drive.put("c.txt", b"c").unwrap();
    let c = tag(&drive, "c.txt");
    let results = drive.move_many(&[
        BatchMove::new("a.txt", "taken.txt"),
        BatchMove::new("b.txt", "b-moved.txt").of_version(&old),
        BatchMove::new("c.txt", "c-moved.txt").of_version(&c),
        BatchMove::new("gone.txt", "gone-moved.txt"),
    ]);
    assert!(
        matches!(results[0], Err(DriveError::InvalidKey { .. })),
        "{results:?}"
    );
    assert!(conflict(&results[1]), "{results:?}");
    assert!(results[2].is_ok(), "{results:?}");
    assert!(
        matches!(results[3], Err(DriveError::NotFound { .. })),
        "{results:?}"
    );
    assert_eq!(drive.get("taken.txt").unwrap(), b"in the way");
    assert_eq!(
        drive.get("a.txt").unwrap(),
        b"a",
        "the file in the way stays and so does the mover"
    );
    assert_eq!(drive.get("b.txt").unwrap(), b"b, edited since");
    assert_eq!(drive.get("c-moved.txt").unwrap(), b"c");
}

/// AZ-W5 (the second half): a case-only rename of the same file was refused as "something has
/// this name already" on a disk that reads both names as one.
#[test]
fn az_w5_a_case_only_rename_changes_the_case() {
    let tmp = TempDir::new("w5-case");
    let drive = LocalDrive::without_manifest(tmp.path());
    drive.put("Docs/a.txt", b"alpha").unwrap();
    drive.rename("Docs/", "docs/").unwrap();
    let names: Vec<String> = crate::ops::list_folder_all(&drive, "").unwrap().folders;
    assert_eq!(
        names,
        vec![String::from("docs/")],
        "spelt as it was renamed"
    );
    assert_eq!(drive.get("docs/a.txt").unwrap(), b"alpha");
    drive.put("Name.txt", b"n").unwrap();
    drive.rename("Name.txt", "name.txt").unwrap();
    assert_eq!(drive.get("name.txt").unwrap(), b"n");
}

/// Two names of one file that are no case change are two files' names: the rename does nothing on
/// such a pair, so it is refused.
#[cfg(unix)]
#[test]
fn a_hard_link_is_another_name_not_a_case_change() {
    let tmp = TempDir::new("w5-hardlink");
    let drive = LocalDrive::without_manifest(tmp.path());
    drive.put("a.txt", b"alpha").unwrap();
    std::fs::hard_link(tmp.path().join("a.txt"), tmp.path().join("b.txt")).unwrap();
    assert!(drive.rename("a.txt", "b.txt").is_err());
    assert_eq!(drive.get("a.txt").unwrap(), b"alpha");
    assert_eq!(drive.get("b.txt").unwrap(), b"alpha");
}

/// AZ X-AR5a-10: the folder a move empties was removed with `remove_dir_all` after a listing that
/// cannot see a name that is not text, a socket or a broken link: what the listing did not show
/// was removed with it.
#[test]
fn x_ar5a_10_a_folder_is_removed_only_when_nothing_is_in_it() {
    let tmp = TempDir::new("x10");
    let drive = LocalDrive::without_manifest(tmp.path());
    // Only empty folders and the junk a file manager leaves: gone.
    drive.create_folder("m/sub/deeper/").unwrap();
    std::fs::write(tmp.path().join("m").join(".DS_Store"), b"finder").unwrap();
    assert!(drive.remove_empty_folder("m/").unwrap());
    assert!(!tmp.path().join("m").exists());
    // A file anywhere in it: it stays, with everything else - an empty folder in it too.
    drive.put("n/sub/keep.txt", b"keep").unwrap();
    drive.create_folder("n/empty/").unwrap();
    assert!(!drive.remove_empty_folder("n/").unwrap());
    assert_eq!(drive.get("n/sub/keep.txt").unwrap(), b"keep");
    assert!(tmp.path().join("n").join("empty").is_dir());
    // A folder that is not there is gone already; a file is no folder.
    assert!(drive.remove_empty_folder("never/").unwrap());
    drive.put("file.txt", b"x").unwrap();
    assert!(drive.remove_empty_folder("file.txt/").is_err());
    assert!(drive.remove_empty_folder("").is_err(), "never the root");
}

#[cfg(unix)]
#[test]
fn x_ar5a_10_what_a_listing_cannot_see_keeps_its_folder() {
    use std::os::unix::net::UnixListener;
    let tmp = TempDir::new("x10-hidden");
    let drive = LocalDrive::without_manifest(tmp.path());
    std::fs::create_dir_all(tmp.path().join("p")).unwrap();
    // A socket: a listing skips it.
    let socket = tmp.path().join("p").join("s");
    let _listener = UnixListener::bind(&socket).unwrap();
    assert!(
        crate::ops::list_all(&drive, "p/").unwrap().is_empty(),
        "the listing sees nothing in p/"
    );
    assert!(!drive.remove_empty_folder("p/").unwrap());
    assert!(
        socket.exists(),
        "the socket was not removed with its folder"
    );
    // A link: something in the folder too, and what it points at is untouched.
    std::fs::create_dir_all(tmp.path().join("q")).unwrap();
    std::fs::write(tmp.path().join("target.txt"), b"t").unwrap();
    std::os::unix::fs::symlink(
        tmp.path().join("target.txt"),
        tmp.path().join("q").join("l"),
    )
    .unwrap();
    assert!(!drive.remove_empty_folder("q/").unwrap());
    assert!(tmp.path().join("q").join("l").symlink_metadata().is_ok());
    assert_eq!(std::fs::read(tmp.path().join("target.txt")).unwrap(), b"t");
}

#[cfg(target_os = "linux")]
#[test]
fn x_ar5a_10_a_name_that_is_not_text_keeps_its_folder() {
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};
    let tmp = TempDir::new("x10-bytes");
    let drive = LocalDrive::without_manifest(tmp.path());
    std::fs::create_dir_all(tmp.path().join("r")).unwrap();
    let odd = tmp.path().join("r").join(OsStr::from_bytes(b"caf\xe9.txt"));
    std::fs::write(&odd, b"latin-1").unwrap();
    assert!(!drive.remove_empty_folder("r/").unwrap());
    assert_eq!(std::fs::read(&odd).unwrap(), b"latin-1");
}

// ---- a drive that only has the basic calls and tags (a bucket)

/// A bucket in memory with S3 semantics and tags that change with every write; only the basic
/// calls and `copy` - the rest is the trait's default.
#[derive(Default)]
pub(super) struct Versioned {
    files: Mutex<BTreeMap<String, (Vec<u8>, u64)>>,
    /// The key to write to right after a copy of it: an editor saving while a move copies.
    edit_after_copy: Mutex<Option<String>>,
    clock: AtomicU64,
}

impl Versioned {
    pub(super) fn with(files: &[(&str, &[u8])]) -> Versioned {
        let drive = Versioned::default();
        for (key, bytes) in files {
            drive.write(key, bytes);
        }
        drive
    }

    pub(super) fn write(&self, key: &str, bytes: &[u8]) {
        let version = self.clock.fetch_add(1, Ordering::SeqCst) + 1;
        self.files
            .lock()
            .unwrap()
            .insert(key.to_string(), (bytes.to_vec(), version));
    }

    pub(super) fn keys(&self) -> Vec<String> {
        self.files.lock().unwrap().keys().cloned().collect()
    }

    fn edit_after_copy_of(&self, key: &str) {
        *self.edit_after_copy.lock().unwrap() = Some(key.to_string());
    }
}

impl Drive for Versioned {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        let files = self.files.lock().unwrap();
        let mut page = ListPage::default();
        for (key, (bytes, version)) in files.iter() {
            let Some(rest) = key.strip_prefix(&request.prefix) else {
                continue;
            };
            match (request.delimiter.as_deref(), rest.find('/')) {
                (Some("/"), Some(i)) => {
                    let folder = format!("{}{}", request.prefix, &rest[..=i]);
                    if !page.folders.contains(&folder) {
                        page.folders.push(folder);
                    }
                }
                _ => page.objects.push(ObjectInfo {
                    key: key.clone(),
                    size: bytes.len() as u64,
                    modified: Some(1),
                    etag: Some(format!("v{version}")),
                }),
            }
        }
        Ok(page)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.files
            .lock()
            .unwrap()
            .get(key)
            .map(|(bytes, _)| bytes.clone())
            .ok_or_else(|| DriveError::NotFound { key: key.into() })
    }
    fn get_range(&self, key: &str, _: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.get(key)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.write(key, bytes);
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.files.lock().unwrap().remove(key);
        Ok(())
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.files
            .lock()
            .unwrap()
            .get(key)
            .map(|(bytes, version)| ObjectInfo {
                key: key.to_string(),
                size: bytes.len() as u64,
                modified: Some(1),
                etag: Some(format!("v{version}")),
            })
            .ok_or_else(|| DriveError::NotFound { key: key.into() })
    }
    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        let bytes = self.get(from)?;
        self.write(to, &bytes);
        let edit = self.edit_after_copy.lock().unwrap().clone();
        if edit.as_deref() == Some(from) {
            self.write(from, b"edited meanwhile");
        }
        Ok(())
    }
}

/// AZ-M12: a rename on a drive without one of its own copied the file and then deleted the source
/// whatever it held by then - an edit saved while the copy ran was deleted with it.
#[test]
fn az_m12_a_default_rename_deletes_the_version_it_copied_and_no_other() {
    let drive = Versioned::with(&[("a.txt", b"v1 text")]);
    drive.edit_after_copy_of("a.txt");
    let renamed = drive.rename("a.txt", "b.txt");
    assert!(conflict(&renamed), "{renamed:?}");
    assert_eq!(
        drive.get("a.txt").unwrap(),
        b"edited meanwhile",
        "the edit was not deleted with the copy"
    );
    assert_eq!(drive.get("b.txt").unwrap(), b"v1 text");
}

#[test]
fn az_m12_a_default_move_deletes_the_version_it_copied_and_no_other() {
    let drive = Versioned::with(&[("a.txt", b"v1 text"), ("b.txt", b"b")]);
    drive.edit_after_copy_of("a.txt");
    let results = drive.move_many(&[
        BatchMove::new("a.txt", "a-moved.txt"),
        BatchMove::new("b.txt", "b-moved.txt"),
    ]);
    assert!(conflict(&results[0]), "{results:?}");
    assert!(results[1].is_ok(), "{results:?}");
    assert_eq!(drive.get("a.txt").unwrap(), b"edited meanwhile");
    assert!(drive.get("b.txt").is_err(), "the untouched one moved");
    assert_eq!(drive.get("b-moved.txt").unwrap(), b"b");
}

#[test]
fn az_m12_a_default_folder_rename_deletes_the_versions_it_listed() {
    let drive = Versioned::with(&[("d/x.txt", b"x"), ("d/y.txt", b"y")]);
    drive.edit_after_copy_of("d/x.txt");
    let renamed = drive.rename("d/", "e/");
    assert!(conflict(&renamed), "{renamed:?}");
    assert_eq!(drive.get("d/x.txt").unwrap(), b"edited meanwhile");
    assert!(
        drive.get("d/y.txt").is_err(),
        "the listed version of y went"
    );
    assert_eq!(drive.get("e/x.txt").unwrap(), b"x");
    assert_eq!(drive.get("e/y.txt").unwrap(), b"y");
}

/// A bucket's folder is its markers: removed when nothing but markers is under it.
#[test]
fn a_bucket_folder_is_removed_when_only_its_markers_are_under_it() {
    let drive = Versioned::with(&[
        ("m/", b""),
        ("m/sub/", b""),
        ("n/", b""),
        ("n/keep.txt", b"keep"),
        ("other.txt", b"o"),
    ]);
    assert!(drive.remove_empty_folder("m/").unwrap());
    assert_eq!(
        drive.keys(),
        vec![
            String::from("n/"),
            String::from("n/keep.txt"),
            String::from("other.txt")
        ]
    );
    // A file in it: nothing of it is deleted, not even its marker.
    assert!(!drive.remove_empty_folder("n/").unwrap());
    assert!(drive.get("n/keep.txt").is_ok() && drive.get("n/").is_ok());
    // Not there at all.
    assert!(drive.remove_empty_folder("never/").unwrap());
}
