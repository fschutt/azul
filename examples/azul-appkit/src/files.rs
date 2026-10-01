//! File jobs: what an app asks of its data folder, run on a drive.
//!
//! An app never touches files from a callback. It hands a list of jobs to
//! [`crate::ui::spawn_file_jobs`] (feature `azul`), which runs them here on
//! an azul `Thread` against the data root's drive (an azul-storage
//! `LocalDrive` today, the user's `S3Drive` later) and hands the outcomes
//! back to the UI thread. The jobs are the four things the apps need: write
//! a file, read a file, read every file of a folder with a suffix (all
//! contacts), delete a file. Errors come back as sentences for the user.

use azul_storage::{Drive, DriveError, ListRequest};

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

/// Every key under `prefix`, at any depth, in key order (all pages).
pub fn list_all(drive: &dyn Drive, prefix: &str) -> Result<Vec<String>, DriveError> {
    let mut keys = Vec::new();
    let mut request = ListRequest::recursive(prefix);
    // A drive that keeps answering with a token would loop forever: stop
    // after far more pages than any folder of an app has.
    for _ in 0..10_000 {
        let page = drive.list(&request)?;
        keys.extend(page.objects.into_iter().map(|o| o.key));
        match page.next {
            Some(token) => request = ListRequest::recursive(prefix).with_continuation(token),
            None => break,
        }
    }
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

#[cfg(test)]
pub(crate) mod test_dir {
    //! A fresh folder under the system's temporary folder, removed on drop.

    use std::path::{Path, PathBuf};

    pub struct TestDir(PathBuf);

    impl TestDir {
        pub fn new(name: &str) -> TestDir {
            let dir = std::env::temp_dir().join(format!(
                "azul-appkit-{name}-{}-{}",
                std::process::id(),
                crate::data::new_uuid()
            ));
            std::fs::create_dir_all(&dir).expect("a temporary folder");
            TestDir(dir)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use azul_storage::LocalDrive;

    use super::test_dir::TestDir;
    use super::*;

    fn put(key: &str, text: &str) -> FileJob {
        FileJob::Put {
            key: key.to_string(),
            bytes: text.as_bytes().to_vec(),
        }
    }

    #[test]
    fn a_written_file_reads_back_and_a_missing_one_is_none() {
        let dir = TestDir::new("files-rw");
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
        let dir = TestDir::new("files-all");
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
        let dir = TestDir::new("files-empty");
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
        let dir = TestDir::new("files-delete");
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
        let dir = TestDir::new("files-bad");
        let drive = LocalDrive::new(dir.path());
        let out = run_job(&drive, put("../escape.txt", "x"));
        let error = out.error().expect("refused");
        assert!(error.contains("not a valid name"), "{error}");
    }
}
