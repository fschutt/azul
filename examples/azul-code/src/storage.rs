//! The workspace's files through azul-storage's `Drive`, never from a
//! callback: a folder listed (one level, as the explorer opens it), a file
//! read, a file written - on an azul `Thread`, the outcomes back on the UI
//! thread. The drive is a `LocalDrive` at the workspace's folder (without
//! the data tree's manifest for a folder of the user's; the data tree's own
//! drive for the sample), so an `S3Drive` can stand in later.
//!
//! The highlighter's far walks ([`crate::highlight::HighlightJob`]) run on
//! a Thread the same way.

use std::path::PathBuf;

use azul::{
    callbacks::{CallbackInfo, RefAny, WriteBackCallbackType},
    task::{Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg},
};
use azul_storage::{key::last_segment, Drive, LocalDrive};

use crate::{
    highlight::{HighlightJob, JobResult},
    workspace::Root,
};

/// One thing to do in the workspace (keys relative to the workspace).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveJob {
    /// List one folder (`""` or a key ending in `/`).
    List { folder: String },
    /// Read a file.
    Read { key: String },
    /// Create or replace a file.
    Write { key: String, bytes: Vec<u8> },
}

/// What a job did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveOutcome {
    Listed {
        folder: String,
        folders: Vec<String>,
        files: Vec<String>,
        error: Option<String>,
    },
    Read {
        key: String,
        result: Result<Vec<u8>, String>,
    },
    Written {
        key: String,
        result: Result<(), String>,
    },
}

/// The drive of a workspace root.
#[must_use]
pub fn drive_of(root: &Root) -> LocalDrive {
    if root.data_tree {
        LocalDrive::new(root.drive_root.clone())
    } else {
        LocalDrive::without_manifest(root.drive_root.clone())
    }
}

/// Runs `jobs` in order on `drive`, the workspace's keys under `prefix`.
pub fn run_jobs(drive: &dyn Drive, prefix: &str, jobs: Vec<DriveJob>) -> Vec<DriveOutcome> {
    jobs.into_iter()
        .map(|job| match job {
            DriveJob::List { folder } => list(drive, prefix, folder),
            DriveJob::Read { key } => {
                let result = drive.get(&format!("{prefix}{key}")).map_err(|e| e.to_string());
                DriveOutcome::Read { key, result }
            }
            DriveJob::Write { key, bytes } => {
                let result = drive.put(&format!("{prefix}{key}"), &bytes).map_err(|e| e.to_string());
                DriveOutcome::Written { key, result }
            }
        })
        .collect()
}

/// One folder level: its folders and files by name (every page).
fn list(drive: &dyn Drive, prefix: &str, folder: String) -> DriveOutcome {
    let full = format!("{prefix}{folder}");
    let (folders, files, error): (Vec<String>, Vec<String>, Option<String>) =
        match azul_storage::ops::list_folder_all(drive, &full) {
            Ok(level) => (
                level
                    .folders
                    .iter()
                    .map(|f| last_segment(f).to_string())
                    .collect(),
                level
                    .objects
                    .iter()
                    .map(|o| last_segment(&o.key).to_string())
                    .filter(|name| !name.is_empty())
                    .collect(),
                None,
            ),
            Err(e) => (Vec::new(), Vec::new(), Some(e.to_string())),
        };
    DriveOutcome::Listed {
        folder,
        folders,
        files,
        error,
    }
}

/// What a drive thread hands back.
pub struct DriveReply {
    pub outcomes: Vec<DriveOutcome>,
}

struct DriveThreadInit {
    root: Root,
    jobs: Option<Vec<DriveJob>>,
    on_done: WriteBackCallbackType,
}

extern "C" fn drive_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((root, jobs, on_done)) = init
        .downcast_mut::<DriveThreadInit>()
        .and_then(|mut i| Some((i.root.clone(), i.jobs.take()?, i.on_done)))
    else {
        return;
    };
    let drive = drive_of(&root);
    let outcomes = run_jobs(&drive, &root.prefix, jobs);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(DriveReply { outcomes }),
    )));
}

/// Runs `jobs` on an azul Thread on `root`'s drive; `on_done(reply_to,
/// DriveReply, info)` gets the outcomes on the UI thread.
pub fn spawn_drive_jobs(
    info: &mut CallbackInfo,
    root: &Root,
    jobs: Vec<DriveJob>,
    reply_to: RefAny,
    on_done: WriteBackCallbackType,
) {
    if jobs.is_empty() {
        return;
    }
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(DriveThreadInit {
                root: root.clone(),
                jobs: Some(jobs),
                on_done,
            }),
            reply_to,
            drive_thread,
        ),
    );
}

/// Takes the reply out of a write-back's message.
#[must_use]
pub fn take_drive_reply(msg: &mut RefAny) -> Option<DriveReply> {
    let mut guard = msg.downcast_mut::<DriveReply>()?;
    Some(DriveReply {
        outcomes: std::mem::take(&mut guard.outcomes),
    })
}

/// What a highlight thread hands back: whose walk it was, what it found.
pub struct HighlightReply {
    pub doc: u64,
    pub result: Option<JobResult>,
}

struct HighlightThreadInit {
    doc: u64,
    job: Option<HighlightJob>,
    on_done: WriteBackCallbackType,
}

extern "C" fn highlight_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((doc, job, on_done)) = init
        .downcast_mut::<HighlightThreadInit>()
        .and_then(|mut i| Some((i.doc, i.job.take()?, i.on_done)))
    else {
        return;
    };
    let result = Some(job.run());
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(HighlightReply { doc, result }),
    )));
}

/// Walks `job` on an azul Thread for document `doc`.
pub fn spawn_highlight(
    info: &mut CallbackInfo,
    doc: u64,
    job: HighlightJob,
    reply_to: RefAny,
    on_done: WriteBackCallbackType,
) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(HighlightThreadInit {
                doc,
                job: Some(job),
                on_done,
            }),
            reply_to,
            highlight_thread,
        ),
    );
}

/// Takes the reply out of a highlight write-back's message.
#[must_use]
pub fn take_highlight_reply(msg: &mut RefAny) -> Option<HighlightReply> {
    let mut guard = msg.downcast_mut::<HighlightReply>()?;
    Some(HighlightReply {
        doc: guard.doc,
        result: guard.result.take(),
    })
}

/// A fresh folder under the system's temporary folder (tests).
#[cfg(test)]
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "azcode-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&dir).expect("a temporary folder");
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_workspace_folder_is_listed_one_level_and_files_read_and_write_through_the_drive() {
        let dir = temp_dir("drive");
        let root = Root {
            drive_root: dir.clone(),
            prefix: String::new(),
            data_tree: false,
            name: "ws".to_string(),
        };
        let drive = drive_of(&root);
        let written = run_jobs(
            &drive,
            &root.prefix,
            vec![
                DriveJob::Write {
                    key: "src/main.rs".to_string(),
                    bytes: b"fn main() {}\n".to_vec(),
                },
                DriveJob::Write {
                    key: "Cargo.toml".to_string(),
                    bytes: b"[package]\n".to_vec(),
                },
            ],
        );
        assert!(written
            .iter()
            .all(|o| matches!(o, DriveOutcome::Written { result: Ok(()), .. })));
        let out = run_jobs(
            &drive,
            &root.prefix,
            vec![
                DriveJob::List { folder: String::new() },
                DriveJob::Read {
                    key: "src/main.rs".to_string(),
                },
            ],
        );
        match &out[0] {
            DriveOutcome::Listed {
                folders, files, error, ..
            } => {
                assert_eq!(error, &None);
                assert_eq!(folders, &vec!["src".to_string()]);
                assert_eq!(files, &vec!["Cargo.toml".to_string()]);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            out[1],
            DriveOutcome::Read {
                key: "src/main.rs".to_string(),
                result: Ok(b"fn main() {}\n".to_vec())
            }
        );
        assert!(
            !dir.join(azul_storage::manifest::MANIFEST_DIR).exists(),
            "a folder of the user's gets no manifest"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_quick_open_index_walks_the_folders_but_not_build_output_or_version_control() {
        let dir = temp_dir("index");
        let root = Root {
            drive_root: dir.clone(),
            prefix: String::new(),
            data_tree: false,
            name: "ws".to_string(),
        };
        let drive = drive_of(&root);
        let keys = [
            "Cargo.toml",
            "src/main.rs",
            "src/deep/mod.rs",
            "target/debug/out.rs",
            ".git/config",
            "node_modules/x/index.js",
        ];
        let writes = keys
            .iter()
            .map(|k| DriveJob::Write {
                key: (*k).to_string(),
                bytes: b"x".to_vec(),
            })
            .collect();
        let written = run_jobs(&drive, &root.prefix, writes);
        assert!(written
            .iter()
            .all(|o| matches!(o, DriveOutcome::Written { result: Ok(()), .. })));
        let out = run_jobs(&drive, &root.prefix, vec![DriveJob::Index { limit: 100 }]);
        match &out[0] {
            DriveOutcome::Indexed {
                files,
                complete,
                error,
            } => {
                let mut files = files.clone();
                files.sort();
                assert_eq!(files, vec!["Cargo.toml", "src/deep/mod.rs", "src/main.rs"]);
                assert!(*complete);
                assert_eq!(error, &None);
            }
            other => panic!("{other:?}"),
        }
        let capped = run_jobs(&drive, &root.prefix, vec![DriveJob::Index { limit: 2 }]);
        assert!(
            matches!(&capped[0], DriveOutcome::Indexed { files, complete: false, .. } if files.len() == 2),
            "{capped:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
