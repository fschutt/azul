//! The storage jobs. Every file of the library is read and written through
//! an `azul_storage::Drive` - a `LocalDrive` on the user's AzNotes folder
//! today, an `S3Drive` on the user's bucket later, with nothing else
//! changing - and every call blocks, so a job runs on an azul `Thread`
//! (`lib.rs` spawns it) and its [`Outcome`] comes back through the
//! thread's write-back. Nothing here knows azul; the tests run the jobs on
//! a `LocalDrive` in a temporary folder.

use std::path::PathBuf;

use azul_storage::{Drive, DriveError, ObjectInfo};

use crate::model;

/// A file read from the drive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileText {
    pub key: String,
    pub text: String,
    /// Its modified time, seconds since 1970 (0 when the drive does not say).
    pub modified: u64,
}

/// A note to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveJob {
    pub id: String,
    pub key: String,
    pub text: String,
    /// The note's generation when the text was taken.
    pub generation: u64,
    /// The key it was written under before it moved (deleted after the
    /// write).
    pub old_key: Option<String>,
    /// Its images move with it: `(old prefix, new prefix)`.
    pub move_assets: Option<(String, String)>,
    /// Also write the text as this version.
    pub version: Option<String>,
}

/// One blocking storage task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Job {
    /// Every note and every notebook marker.
    Load,
    /// The notes whose file changed since `known` (`(key, modified)`), the
    /// notes that are gone, the markers - edits made outside AzNotes.
    Rescan { known: Vec<(String, u64)> },
    Save(SaveJob),
    /// Removes the keys and everything under the prefixes (a note forever:
    /// its file, its images, its versions).
    Delete {
        id: String,
        keys: Vec<String>,
        prefixes: Vec<String>,
    },
    /// Writes one small text object (a notebook marker, the settings).
    PutText { key: String, text: String },
    /// The versions of a note, newest first.
    History { id: String },
    /// One version's text.
    Version { id: String, key: String },
    /// Copies files the user dropped into a note's images folder; `at` is
    /// the block the images go after.
    Import {
        id: String,
        prefix: String,
        sources: Vec<PathBuf>,
        at: usize,
    },
    /// The bytes of image objects.
    Images { keys: Vec<String> },
    /// Writes the files that do not exist yet (the sample library), then
    /// loads.
    Seed { files: Vec<(String, String)> },
}

/// What a job answers, on the UI thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Loaded {
        files: Vec<FileText>,
        markers: Vec<String>,
        errors: Vec<String>,
    },
    Rescanned {
        changed: Vec<FileText>,
        removed: Vec<String>,
        markers: Vec<String>,
        errors: Vec<String>,
    },
    Saved {
        id: String,
        key: String,
        text: String,
        generation: u64,
        modified: u64,
        result: Result<(), String>,
    },
    Deleted {
        id: String,
        result: Result<(), String>,
    },
    Done {
        what: String,
        result: Result<(), String>,
    },
    History {
        id: String,
        versions: Vec<(String, u64)>,
        result: Result<(), String>,
    },
    Version {
        id: String,
        key: String,
        result: Result<String, String>,
    },
    Imported {
        id: String,
        at: usize,
        /// `(src relative to the note's folder, alt)` per image.
        images: Vec<(String, String)>,
        errors: Vec<String>,
    },
    Images {
        images: Vec<(String, Vec<u8>)>,
        errors: Vec<String>,
    },
}

/// Every object under `prefix`, all pages.
pub use azul_storage::ops::list_all;

fn read_text(drive: &dyn Drive, info: &ObjectInfo) -> Result<FileText, String> {
    drive
        .get(&info.key)
        .map(|bytes| FileText {
            key: info.key.clone(),
            text: String::from_utf8_lossy(&bytes).into_owned(),
            modified: info.modified.unwrap_or(0),
        })
        .map_err(|e| format!("{}: {e}", info.key))
}

/// The note keys and the marker notebooks among `objects`.
fn classify(objects: &[ObjectInfo]) -> (Vec<&ObjectInfo>, Vec<String>) {
    let mut notes = Vec::new();
    let mut markers = Vec::new();
    for info in objects {
        if model::parse_note_key(&info.key).is_some() {
            notes.push(info);
        } else if let Some(notebook) = model::parse_marker_key(&info.key) {
            markers.push(notebook);
        }
    }
    (notes, markers)
}

/// A file name for an imported image: the source's name with anything but
/// letters, digits, `.`, `_` and `-` replaced, prefixed with `n` so two
/// drops of `image.png` do not collide.
#[must_use]
pub fn asset_name(source: &std::path::Path, n: usize) -> String {
    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let clean: String = azul_storage::key::safe_file_name(&name)
        .unwrap_or_else(|| String::from("image"))
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '-' })
        .collect();
    format!("{n}-{clean}")
}

/// Runs `job` against `drive` (blocking: a worker thread's body).
#[must_use]
pub fn run_job(drive: &dyn Drive, job: Job) -> Outcome {
    let err = |e: DriveError| e.to_string();
    match job {
        Job::Load => match list_all(drive, model::NOTES_ROOT) {
            Ok(objects) => {
                let (notes, markers) = classify(&objects);
                let mut files = Vec::new();
                let mut errors = Vec::new();
                for info in notes {
                    match read_text(drive, info) {
                        Ok(file) => files.push(file),
                        Err(e) => errors.push(e),
                    }
                }
                Outcome::Loaded {
                    files,
                    markers,
                    errors,
                }
            }
            Err(e) => Outcome::Loaded {
                files: Vec::new(),
                markers: Vec::new(),
                errors: vec![format!("The notes folder could not be listed: {e}")],
            },
        },
        Job::Rescan { known } => match list_all(drive, model::NOTES_ROOT) {
            Ok(objects) => {
                let (notes, markers) = classify(&objects);
                let mut changed = Vec::new();
                let mut errors = Vec::new();
                for info in &notes {
                    let same = known
                        .iter()
                        .any(|(k, m)| *k == info.key && Some(*m) == info.modified);
                    if same {
                        continue;
                    }
                    match read_text(drive, info) {
                        Ok(file) => changed.push(file),
                        Err(e) => errors.push(e),
                    }
                }
                let removed = known
                    .iter()
                    .filter(|(k, _)| !notes.iter().any(|i| i.key == *k))
                    .map(|(k, _)| k.clone())
                    .collect();
                Outcome::Rescanned {
                    changed,
                    removed,
                    markers,
                    errors,
                }
            }
            Err(e) => Outcome::Rescanned {
                changed: Vec::new(),
                removed: Vec::new(),
                markers: Vec::new(),
                errors: vec![e.to_string()],
            },
        },
        Job::Save(save) => {
            let result = save_note(drive, &save);
            let modified = drive.head(&save.key).ok().and_then(|i| i.modified).unwrap_or(0);
            Outcome::Saved {
                id: save.id,
                key: save.key,
                text: save.text,
                generation: save.generation,
                modified,
                result,
            }
        }
        Job::Delete { id, keys, prefixes } => {
            let mut result = Ok(());
            for key in &keys {
                if let Err(e) = drive.delete(key) {
                    result = Err(err(e));
                }
            }
            for prefix in &prefixes {
                match list_all(drive, prefix) {
                    Ok(objects) => {
                        for info in objects {
                            if let Err(e) = drive.delete(&info.key) {
                                result = Err(err(e));
                            }
                        }
                    }
                    Err(e) => result = Err(err(e)),
                }
            }
            Outcome::Deleted { id, result }
        }
        Job::PutText { key, text } => Outcome::Done {
            result: drive.put(&key, text.as_bytes()).map_err(err),
            what: key,
        },
        Job::History { id } => match list_all(drive, &model::history_prefix(&id)) {
            Ok(objects) => {
                let mut versions: Vec<(String, u64)> = objects
                    .into_iter()
                    .filter_map(|info| model::history_time(&info.key).map(|t| (info.key, t)))
                    .collect();
                versions.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
                Outcome::History {
                    id,
                    versions,
                    result: Ok(()),
                }
            }
            Err(e) => Outcome::History {
                id,
                versions: Vec::new(),
                result: Err(err(e)),
            },
        },
        Job::Version { id, key } => Outcome::Version {
            result: drive
                .get(&key)
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .map_err(err),
            id,
            key,
        },
        Job::Import {
            id,
            prefix,
            sources,
            at,
        } => {
            let stamp = azul_storage::time::now_unix();
            let mut images = Vec::new();
            let mut errors = Vec::new();
            for (n, source) in sources.iter().enumerate() {
                let name = asset_name(source, usize::try_from(stamp % 100_000).unwrap_or(0) + n);
                let key = format!("{prefix}{name}");
                match azul_storage::transfer::upload_file(drive, source, &key) {
                    Ok(_) => {
                        let alt = source
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        images.push((relative_src(&key), alt));
                    }
                    Err(e) => errors.push(format!("{}: {e}", source.display())),
                }
            }
            Outcome::Imported {
                id,
                at,
                images,
                errors,
            }
        }
        Job::Images { keys } => {
            let mut images = Vec::new();
            let mut errors = Vec::new();
            for key in keys {
                match drive.get(&key) {
                    Ok(bytes) => images.push((key, bytes)),
                    Err(e) => errors.push(format!("{key}: {e}")),
                }
            }
            Outcome::Images { images, errors }
        }
        Job::Seed { files } => {
            let mut errors = Vec::new();
            for (key, text) in &files {
                match drive.head(key) {
                    Ok(_) => {}
                    Err(DriveError::NotFound { .. }) => {
                        if let Err(e) = drive.put(key, text.as_bytes()) {
                            errors.push(format!("{key}: {e}"));
                        }
                    }
                    Err(e) => errors.push(format!("{key}: {e}")),
                }
            }
            match run_job(drive, Job::Load) {
                Outcome::Loaded {
                    files,
                    markers,
                    errors: more,
                } => {
                    errors.extend(more);
                    Outcome::Loaded {
                        files,
                        markers,
                        errors,
                    }
                }
                other => other,
            }
        }
    }
}

/// Writes a note: the file, then (when it moved) the old file away and its
/// images along, then its version.
fn save_note(drive: &dyn Drive, save: &SaveJob) -> Result<(), String> {
    drive.put(&save.key, save.text.as_bytes()).map_err(|e| e.to_string())?;
    if let Some(old) = save.old_key.as_deref().filter(|old| *old != save.key) {
        drive.delete(old).map_err(|e| e.to_string())?;
    }
    if let Some((from, to)) = save.move_assets.as_ref().filter(|(f, t)| f != t) {
        let objects = list_all(drive, from).map_err(|e| e.to_string())?;
        for info in objects {
            let rest = &info.key[from.len()..];
            let bytes = drive.get(&info.key).map_err(|e| e.to_string())?;
            drive
                .put(&format!("{to}{rest}"), &bytes)
                .map_err(|e| e.to_string())?;
            drive.delete(&info.key).map_err(|e| e.to_string())?;
        }
    }
    if let Some(version) = &save.version {
        drive.put(version, save.text.as_bytes()).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// An image key as the note's Markdown names it: relative to the note's
/// folder (`notes/Work/<id>/assets/a.png` -> `<id>/assets/a.png`).
#[must_use]
pub fn relative_src(key: &str) -> String {
    // The note's folder ends before `<id>/assets/`.
    match key.find("/assets/") {
        Some(at) => {
            let before = &key[..at];
            let id_start = before.rfind('/').map_or(0, |i| i + 1);
            key[id_start..].to_string()
        }
        None => key.to_string(),
    }
}

/// The key of an image `src` of a note in `notebook`; `None` for a URL.
#[must_use]
pub fn image_key(notebook: &str, src: &str) -> Option<String> {
    if src.contains("://") || src.starts_with("data:") || src.starts_with('/') || src.contains("..") {
        return None;
    }
    Some(if notebook.is_empty() {
        format!("{}{src}", model::NOTES_ROOT)
    } else {
        format!("{}{notebook}/{src}", model::NOTES_ROOT)
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use azul_storage::LocalDrive;

    use super::*;

    /// A fresh folder under the system's temporary folder, removed when
    /// dropped.
    struct Temp(PathBuf);

    impl Temp {
        fn new() -> Temp {
            static N: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "aznotes-store-{}-{}-{}",
                std::process::id(),
                azul_storage::time::now_unix(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::create_dir_all(&dir).expect("a temporary folder");
            Temp(dir)
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn save(id: &str, key: &str, text: &str) -> Job {
        Job::Save(SaveJob {
            id: id.to_string(),
            key: key.to_string(),
            text: text.to_string(),
            generation: 3,
            old_key: None,
            move_assets: None,
            version: None,
        })
    }

    #[test]
    fn a_saved_note_loads_back_with_its_notebook_marker() {
        let temp = Temp::new();
        let drive = LocalDrive::new(&temp.0);
        let saved = run_job(&drive, save("a", "notes/Work/a.md", "---\ntitle: A\n---\n"));
        assert!(matches!(&saved, Outcome::Saved { result: Ok(()), generation: 3, modified, .. } if *modified > 0));
        assert_eq!(
            run_job(&drive, Job::PutText {
                key: model::marker_key("Archive"),
                text: String::new()
            }),
            Outcome::Done {
                what: "notes/Archive/.notebook".to_string(),
                result: Ok(())
            }
        );
        let Outcome::Loaded { files, markers, errors } = run_job(&drive, Job::Load) else {
            panic!("a load answers Loaded");
        };
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].key, "notes/Work/a.md");
        assert_eq!(markers, vec!["Archive".to_string()]);
    }

    #[test]
    fn a_moved_note_leaves_no_old_file_and_takes_its_images_and_a_version() {
        let temp = Temp::new();
        let drive = LocalDrive::new(&temp.0);
        let _ = run_job(&drive, save("a", "notes/Work/a.md", "one"));
        drive.put("notes/Work/a/assets/1-x.png", b"png").expect("an image");
        let moved = run_job(
            &drive,
            Job::Save(SaveJob {
                id: "a".to_string(),
                key: "notes/Home/a.md".to_string(),
                text: "two".to_string(),
                generation: 4,
                old_key: Some("notes/Work/a.md".to_string()),
                move_assets: Some(("notes/Work/a/assets/".to_string(), "notes/Home/a/assets/".to_string())),
                version: Some(model::history_key("a", 1_790_769_600)),
            }),
        );
        assert!(matches!(moved, Outcome::Saved { result: Ok(()), .. }), "{moved:?}");
        assert!(drive.head("notes/Work/a.md").is_err());
        assert_eq!(drive.get("notes/Home/a/assets/1-x.png").expect("moved"), b"png");
        assert!(drive.head("notes/Work/a/assets/1-x.png").is_err());
        let Outcome::History { versions, .. } = run_job(&drive, Job::History { id: "a".to_string() }) else {
            panic!("a history answers History");
        };
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].1, 1_790_769_600);
        let Outcome::Version { result, .. } = run_job(
            &drive,
            Job::Version {
                id: "a".to_string(),
                key: versions[0].0.clone(),
            },
        ) else {
            panic!("a version answers Version");
        };
        assert_eq!(result, Ok("two".to_string()));
    }

    #[test]
    fn a_rescan_reads_only_changed_files_and_names_the_gone_ones() {
        let temp = Temp::new();
        let drive = LocalDrive::new(&temp.0);
        let _ = run_job(&drive, save("a", "notes/a.md", "a"));
        let _ = run_job(&drive, save("b", "notes/b.md", "b"));
        let modified = |key: &str| drive.head(key).expect("a file").modified.unwrap_or(0);
        let known = vec![
            ("notes/a.md".to_string(), modified("notes/a.md")),
            ("notes/gone.md".to_string(), 1),
        ];
        let Outcome::Rescanned { changed, removed, .. } = run_job(&drive, Job::Rescan { known }) else {
            panic!("a rescan answers Rescanned");
        };
        let keys: Vec<&str> = changed.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, vec!["notes/b.md"], "a.md is unchanged, b.md is new");
        assert_eq!(removed, vec!["notes/gone.md".to_string()]);
    }

    #[test]
    fn seeding_never_overwrites_a_file_and_deleting_takes_the_prefixes() {
        let temp = Temp::new();
        let drive = LocalDrive::new(&temp.0);
        let _ = run_job(&drive, save("a", "notes/a.md", "mine"));
        let seeded = run_job(
            &drive,
            Job::Seed {
                files: vec![
                    ("notes/a.md".to_string(), "sample".to_string()),
                    ("notes/b.md".to_string(), "sample".to_string()),
                ],
            },
        );
        let Outcome::Loaded { files, .. } = seeded else {
            panic!("a seed answers Loaded");
        };
        assert_eq!(files.len(), 2);
        assert_eq!(drive.get("notes/a.md").expect("kept"), b"mine");
        drive.put("notes/.history/a/20260930T120000Z.md", b"v").expect("a version");
        let deleted = run_job(
            &drive,
            Job::Delete {
                id: "a".to_string(),
                keys: vec!["notes/a.md".to_string()],
                prefixes: vec![model::history_prefix("a")],
            },
        );
        assert_eq!(
            deleted,
            Outcome::Deleted {
                id: "a".to_string(),
                result: Ok(())
            }
        );
        assert!(list_all(&drive, &model::history_prefix("a")).expect("a listing").is_empty());
    }

    #[test]
    fn image_sources_are_relative_to_the_notes_folder() {
        assert_eq!(relative_src("notes/Work/abc/assets/1-x.png"), "abc/assets/1-x.png");
        assert_eq!(relative_src("notes/abc/assets/1-x.png"), "abc/assets/1-x.png");
        assert_eq!(image_key("Work", "abc/assets/1-x.png"), Some("notes/Work/abc/assets/1-x.png".to_string()));
        assert_eq!(image_key("", "abc/assets/x.png"), Some("notes/abc/assets/x.png".to_string()));
        assert_eq!(image_key("Work", "https://example.org/x.png"), None);
        assert_eq!(image_key("Work", "../x.png"), None);
        assert_eq!(asset_name(std::path::Path::new("/tmp/My Photo (1).PNG"), 7), "7-My-Photo--1-.PNG");
    }
}
