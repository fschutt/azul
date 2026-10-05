//! File jobs: what an app asks of its data folder, run on a drive.
//!
//! An app never touches files from a callback. It hands a list of jobs to
//! [`crate::ui::spawn_file_jobs`] (feature `azul`), which runs them here on
//! an azul `Thread` against the data root's drive (an azul-storage
//! `LocalDrive` today, the user's `S3Drive` later) and hands the outcomes
//! back to the UI thread. The jobs are the four things the apps need: write
//! a file, read a file, read every file of a folder with a suffix (all
//! contacts), delete a file. Errors come back as sentences for the user.

use std::path::{Path, PathBuf};

use azul_storage::{Drive, DriveError};

/// One thing to do in the data folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileJob {
    /// Create or replace the file.
    Put { key: String, bytes: Vec<u8> },
    /// Read the file; a missing file is not an error (`Ok(None)`).
    Get { key: String },
    /// Read every file under `prefix` (any depth) whose key ends with `suffix`.
    GetAll { prefix: String, suffix: String },
    /// Remove the file; a missing file is not an error.
    Delete { key: String },
}

/// What a job did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileOutcome {
    Put {
        key: String,
        result: Result<(), String>,
    },
    Got {
        key: String,
        result: Result<Option<Vec<u8>>, String>,
    },
    GotAll {
        prefix: String,
        /// `(key, bytes)` in key order.
        files: Vec<(String, Vec<u8>)>,
        /// The files that could not be read, one sentence each.
        errors: Vec<String>,
    },
    Deleted {
        key: String,
        result: Result<(), String>,
    },
}

impl FileOutcome {
    /// The first error of this outcome, if any.
    #[must_use]
    pub fn error(&self) -> Option<String> {
        match self {
            FileOutcome::Put { result, .. } | FileOutcome::Deleted { result, .. } => {
                result.as_ref().err().cloned()
            }
            FileOutcome::Got { result, .. } => result.as_ref().err().cloned(),
            FileOutcome::GotAll { errors, .. } => errors.first().cloned(),
        }
    }
}

/// Every key under `prefix`, at any depth, in key order (all pages: azul-storage's
/// `ops::list_all`).
pub fn list_all(drive: &dyn Drive, prefix: &str) -> Result<Vec<String>, DriveError> {
    let mut keys: Vec<String> = azul_storage::ops::list_all(drive, prefix)?
        .into_iter()
        .map(|o| o.key)
        .collect();
    keys.sort();
    Ok(keys)
}

/// Runs one job.
pub fn run_job(drive: &dyn Drive, job: FileJob) -> FileOutcome {
    match job {
        FileJob::Put { key, bytes } => {
            let result = drive.put(&key, &bytes).map_err(|e| e.to_string());
            FileOutcome::Put { key, result }
        }
        FileJob::Get { key } => {
            let result = match drive.get(&key) {
                Ok(bytes) => Ok(Some(bytes)),
                Err(DriveError::NotFound { .. }) => Ok(None),
                Err(e) => Err(e.to_string()),
            };
            FileOutcome::Got { key, result }
        }
        FileJob::GetAll { prefix, suffix } => {
            let mut files = Vec::new();
            let mut errors = Vec::new();
            match list_all(drive, &prefix) {
                Ok(keys) => {
                    for key in keys.into_iter().filter(|k| k.ends_with(&suffix)) {
                        match drive.get(&key) {
                            Ok(bytes) => files.push((key, bytes)),
                            // Deleted between the listing and the read: not there.
                            Err(DriveError::NotFound { .. }) => {}
                            Err(e) => errors.push(e.to_string()),
                        }
                    }
                }
                Err(e) => errors.push(e.to_string()),
            }
            FileOutcome::GotAll {
                prefix,
                files,
                errors,
            }
        }
        FileJob::Delete { key } => {
            let result = match drive.delete(&key) {
                Ok(()) | Err(DriveError::NotFound { .. }) => Ok(()),
                Err(e) => Err(e.to_string()),
            };
            FileOutcome::Deleted { key, result }
        }
    }
}

/// Runs the jobs in order (a later job sees what an earlier one wrote).
pub fn run_jobs(drive: &dyn Drive, jobs: Vec<FileJob>) -> Vec<FileOutcome> {
    jobs.into_iter().map(|job| run_job(drive, job)).collect()
}

/// A file the user picked OUTSIDE the data tree (a file to import): the
/// folder to open a drive at and the job that reads the file there (its
/// key is the file's name). `None` for a path without a file name or with
/// a name that is not UTF-8. The drive keeps no manifest
/// (`LocalDrive::without_manifest`), so the user's folder gets no `.azlin/`.
#[must_use]
pub fn outside_read(path: &Path) -> Option<(PathBuf, FileJob)> {
    let name = path.file_name()?.to_str()?.to_string();
    let folder = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    };
    Some((folder, FileJob::Get { key: name }))
}

/// Reads a file outside the data tree NOW (before the window opens: a file
/// named on the command line), the same way: [`outside_read`] on a drive
/// without a manifest. A missing file is an error here.
pub fn read_outside(path: &Path) -> Result<Vec<u8>, String> {
    let Some((folder, job)) = outside_read(path) else {
        return Err(format!("{} is not a file", path.display()));
    };
    match run_job(&azul_storage::LocalDrive::without_manifest(folder), job) {
        FileOutcome::Got {
            result: Ok(Some(bytes)),
            ..
        } => Ok(bytes),
        FileOutcome::Got { result: Ok(None), .. } => {
            Err(format!("{} does not exist", path.display()))
        }
        other => Err(other
            .error()
            .unwrap_or_else(|| format!("{} could not be read", path.display()))),
    }
}

/// What [`open_external`] may hand to the system: a web address (`http:` / `https:`) or a file or
/// folder that exists. Anything else (`javascript:`, `file:` addresses, a made-up path) is
/// refused with a sentence.
pub fn external_target(target: &str) -> Result<String, String> {
    let t = target.trim();
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        if t.chars().any(char::is_whitespace) {
            return Err(format!("\u{201c}{t}\u{201d} is not a web address."));
        }
        return Ok(t.to_string());
    }
    if !t.is_empty() && !lower.contains(':') && Path::new(t).exists() {
        return Ok(t.to_string());
    }
    if !t.is_empty() && cfg!(windows) && Path::new(t).exists() {
        return Ok(t.to_string());
    }
    Err(format!("\u{201c}{t}\u{201d} cannot be opened."))
}

/// Opens a web address in the default browser, or a file / folder in its default app (`open` on
/// macOS, `xdg-open` on Linux and the BSDs, `cmd /C start` on Windows) - see
/// [`external_target`] for what is passed on. Returns once the opener started.
// TODO(engine): an azul API for this (a platform call, also for the web build); AzReview's
// lib.rs opens its folder the same way.
pub fn open_external(target: &str) -> Result<(), String> {
    let target = external_target(target)?;
    let mut command = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg(&target);
        c
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]).arg(&target);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(&target);
        c
    };
    command
        .spawn()
        .map(|_child| ())
        .map_err(|e| format!("{target} could not be opened: {e}"))
}

#[cfg(test)]
mod tests {
    use azul_storage::LocalDrive;

    use azul_storage::testing::TempDir;
    use super::*;

    fn put(key: &str, text: &str) -> FileJob {
        FileJob::Put {
            key: key.to_string(),
            bytes: text.as_bytes().to_vec(),
        }
    }

    #[test]
    fn a_picked_file_outside_the_data_tree_is_read_by_name_and_its_folder_gets_no_manifest() {
        let dir = TempDir::new("files-outside");
        let file = dir.path().join("Report 2026.docx");
        std::fs::write(&file, b"PK").expect("the fixture");
        let (folder, job) = outside_read(&file).expect("a file name");
        assert_eq!(folder, dir.path());
        assert_eq!(
            job,
            FileJob::Get {
                key: "Report 2026.docx".to_string()
            }
        );
        let out = run_job(&LocalDrive::without_manifest(&folder), job);
        assert_eq!(
            out,
            FileOutcome::Got {
                key: "Report 2026.docx".to_string(),
                result: Ok(Some(b"PK".to_vec()))
            }
        );
        assert_eq!(read_outside(&file), Ok(b"PK".to_vec()));
        assert!(read_outside(&dir.path().join("missing.md")).is_err());
        assert!(
            !dir.path().join(azul_storage::manifest::MANIFEST_DIR).exists(),
            "reading a picked file left a manifest in the user's folder"
        );
        assert_eq!(outside_read(Path::new("/")), None);
    }

    #[test]
    fn a_written_file_reads_back_and_a_missing_one_is_none() {
        let dir = TempDir::new("files-rw");
        let drive = LocalDrive::new(dir.path());
        let out = run_jobs(
            &drive,
            vec![
                put("calculator/history.jsonl", "{}\n"),
                FileJob::Get {
                    key: "calculator/history.jsonl".to_string(),
                },
                FileJob::Get {
                    key: "calculator/missing.json".to_string(),
                },
            ],
        );
        assert_eq!(out[0].error(), None);
        assert_eq!(
            out[1],
            FileOutcome::Got {
                key: "calculator/history.jsonl".to_string(),
                result: Ok(Some(b"{}\n".to_vec()))
            }
        );
        assert_eq!(
            out[2],
            FileOutcome::Got {
                key: "calculator/missing.json".to_string(),
                result: Ok(None)
            }
        );
        assert!(dir
            .path()
            .join("calculator")
            .join("history.jsonl")
            .is_file());
    }

    #[test]
    fn get_all_reads_every_file_with_the_suffix_in_key_order() {
        let dir = TempDir::new("files-all");
        let drive = LocalDrive::new(dir.path());
        run_jobs(
            &drive,
            vec![
                put("contacts/b.vcf", "B"),
                put("contacts/a.vcf", "A"),
                put("contacts/settings.json", "{}"),
                put("calculator/x.vcf", "not a contact"),
            ],
        );
        let out = run_job(
            &drive,
            FileJob::GetAll {
                prefix: "contacts/".to_string(),
                suffix: ".vcf".to_string(),
            },
        );
        match out {
            FileOutcome::GotAll { files, errors, .. } => {
                assert!(errors.is_empty(), "{errors:?}");
                let keys: Vec<&str> = files.iter().map(|(k, _)| k.as_str()).collect();
                assert_eq!(keys, vec!["contacts/a.vcf", "contacts/b.vcf"]);
                assert_eq!(files[0].1, b"A".to_vec());
            }
            other => panic!("not a GotAll: {other:?}"),
        }
    }

    #[test]
    fn get_all_of_a_folder_that_does_not_exist_yet_is_empty_not_an_error() {
        let dir = TempDir::new("files-empty");
        let drive = LocalDrive::new(dir.path());
        let out = run_job(
            &drive,
            FileJob::GetAll {
                prefix: "contacts/".to_string(),
                suffix: ".vcf".to_string(),
            },
        );
        assert_eq!(
            out,
            FileOutcome::GotAll {
                prefix: "contacts/".to_string(),
                files: Vec::new(),
                errors: Vec::new()
            }
        );
    }

    #[test]
    fn deleting_removes_the_file_and_deleting_twice_is_fine() {
        let dir = TempDir::new("files-delete");
        let drive = LocalDrive::new(dir.path());
        let delete = || FileJob::Delete {
            key: "contacts/a.vcf".to_string(),
        };
        let out = run_jobs(&drive, vec![put("contacts/a.vcf", "A"), delete(), delete()]);
        assert_eq!(out[1].error(), None);
        assert_eq!(out[2].error(), None);
        assert!(!dir.path().join("contacts").join("a.vcf").exists());
    }

    #[test]
    fn a_key_that_leaves_the_root_is_refused_with_a_sentence() {
        let dir = TempDir::new("files-bad");
        let drive = LocalDrive::new(dir.path());
        let out = run_job(&drive, put("../escape.txt", "x"));
        let error = out.error().expect("refused");
        assert!(error.contains("not a valid name"), "{error}");
    }
    #[test]
    fn only_web_addresses_and_existing_paths_are_handed_to_the_system() {
        assert_eq!(external_target(" https://example.org/a?b=1 "), Ok("https://example.org/a?b=1".to_string()));
        assert!(external_target("HTTP://example.org").is_ok());
        assert!(external_target("javascript:alert(1)").is_err());
        assert!(external_target("file:///etc/passwd").is_err());
        assert!(external_target("https://exa mple.org").is_err());
        assert!(external_target("").is_err());
        let dir = TempDir::new("external-target");
        let path = dir.path().display().to_string();
        assert_eq!(external_target(&path), Ok(path.clone()));
        assert!(external_target(&format!("{path}/missing")).is_err());
    }
}
