//! The workspace's files through azul-storage's `Drive`, never from a
//! callback: a folder listed (one level, as the explorer opens it), a file
//! read, a file written - on an azul `Thread`, the outcomes back on the UI
//! thread. The drive is a `LocalDrive` at the workspace's folder (without
//! the data tree's manifest for a folder of the user's; the data tree's own
//! drive for the sample), so an `S3Drive` can stand in later.
//!
//! The highlighter's far walks ([`crate::highlight::HighlightJob`]) run on
//! a Thread the same way, and so does the search over the folder's files
//! ([`search_files`]): azul-search's engine on the folder on disk, the one
//! AzDrive's search box runs on.

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use azul::{
    callbacks::{CallbackInfo, RefAny, WriteBackCallbackType},
    task::{Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg},
};
use azul_appkit::find::TextMatch;
use azul_search::{Case, ContentHit, Event, Filters, Limits, Pattern, Request};
use azul_storage::{key::last_segment, Drive, LocalDrive};

use crate::{
    git,
    highlight::{HighlightJob, JobResult},
    search::{preview_of, Hit},
    workspace::{hidden_entry, skipped_folder, Root, HIDDEN_ENTRIES, SKIPPED_FOLDERS},
};

/// The most files quick open's index holds.
pub const INDEX_MAX_FILES: usize = 20_000;
/// The most folders the index walk lists.
const INDEX_MAX_FOLDERS: usize = 4_000;
/// The most matches a search of the folder lists (VSCode's default).
pub const SEARCH_MAX_HITS: usize = 20_000;
/// A file larger than this is not searched (bytes).
pub const SEARCH_MAX_FILE: usize = 4 * 1024 * 1024;

/// One thing to do in the workspace (keys relative to the workspace).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveJob {
    /// List one folder (`""` or a key ending in `/`).
    List { folder: String },
    /// Read a file.
    Read { key: String },
    /// Create or replace a file.
    Write { key: String, bytes: Vec<u8> },
    /// Every file of the workspace for quick open, folder by folder
    /// (breadth first), the [`skipped_folder`]s left out, `limit` at most.
    Index { limit: usize },
    /// The git branch of the folder on disk (the status bar's).
    Branch { folder: PathBuf },
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
    /// The workspace's files (keys); `complete` is false when the walk
    /// stopped at its cap.
    Indexed {
        files: Vec<String>,
        complete: bool,
        error: Option<String>,
    },
    /// The branch `folder` is checked out at (`None`: no repository).
    Branch {
        folder: PathBuf,
        branch: Option<String>,
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
            DriveJob::Index { limit } => index(drive, prefix, limit),
            DriveJob::Branch { folder } => {
                let branch = git::branch(&folder);
                DriveOutcome::Branch { folder, branch }
            }
        })
        .collect()
}

/// Quick open's index: the workspace's folders listed one level at a time,
/// breadth first (the files near the top come first), never into a
/// [`skipped_folder`]; at most `limit` files and [`INDEX_MAX_FOLDERS`]
/// folders.
fn index(drive: &dyn Drive, prefix: &str, limit: usize) -> DriveOutcome {
    match index_keys(drive, prefix, limit, None) {
        Ok((files, complete)) => DriveOutcome::Indexed {
            files,
            complete,
            error: None,
        },
        Err(error) => DriveOutcome::Indexed {
            files: Vec::new(),
            complete: false,
            error: Some(error),
        },
    }
}

/// The walk of [`index`]: the files (keys) and whether the walk saw them
/// all. `Err` when the workspace itself cannot be listed; a `cancel` raised
/// stops the walk where it is.
fn index_keys(
    drive: &dyn Drive,
    prefix: &str,
    limit: usize,
    cancel: Option<&AtomicBool>,
) -> Result<(Vec<String>, bool), String> {
    let mut files: Vec<String> = Vec::new();
    let mut queue: VecDeque<String> = VecDeque::from([String::new()]);
    let mut listed = 0;
    let mut complete = true;
    while let Some(folder) = queue.pop_front() {
        if files.len() >= limit || listed >= INDEX_MAX_FOLDERS {
            complete = false;
            break;
        }
        if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            complete = false;
            break;
        }
        listed += 1;
        let level = match azul_storage::ops::list_folder_all(drive, &format!("{prefix}{folder}")) {
            Ok(level) => level,
            // The workspace itself cannot be listed: say so; a folder in it: go on without it.
            Err(e) if folder.is_empty() => return Err(e.to_string()),
            Err(_) => continue,
        };
        for object in &level.objects {
            let name = last_segment(&object.key);
            if name.is_empty() || hidden_entry(name) {
                continue;
            }
            if files.len() >= limit {
                complete = false;
                break;
            }
            files.push(format!("{folder}{name}"));
        }
        for sub in &level.folders {
            let name = last_segment(sub);
            if name.is_empty() || skipped_folder(name) || hidden_entry(name) {
                continue;
            }
            queue.push_back(format!("{folder}{name}/"));
        }
    }
    Ok((files, complete))
}

// ==== The search of the folder's files ====

/// What the side bar's search asks of the workspace.
#[derive(Debug, Clone)]
pub struct SearchJob {
    pub query: String,
    pub how: TextMatch,
    /// Which search of the app's this is (a reply to an older one is
    /// dropped).
    pub generation: u64,
    /// Raised by the app when a newer search starts: this one stops.
    pub cancel: Arc<AtomicBool>,
}

/// The matches in one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHits {
    pub key: String,
    pub hits: Vec<Hit>,
}

/// What a search found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchOutcome {
    pub files: Vec<FileHits>,
    /// The files read.
    pub searched: usize,
    /// Every file was searched and no cap was reached.
    pub complete: bool,
    /// The workspace could not be listed.
    pub error: Option<String>,
}

/// The folders and files the search passes over, as azul-search's exclude globs (gitignore
/// syntax): every dot folder, the [`SKIPPED_FOLDERS`] and the [`HIDDEN_ENTRIES`] - what quick
/// open's index leaves out too.
fn search_excludes() -> Vec<String> {
    let mut globs = vec![String::from(".*/")];
    globs.extend(SKIPPED_FOLDERS.iter().map(|name| format!("{name}/")));
    globs.extend(HIDDEN_ENTRIES.iter().map(|name| (*name).to_string()));
    globs
}

/// A file's matches as the results list them: one per match (a line with two has two), the
/// line counted from 0, the match in bytes of the line, the line's preview ([`preview_of`]).
fn hits_of(file: &ContentHit) -> Vec<Hit> {
    file.lines
        .iter()
        .flat_map(|line| {
            line.ranges.iter().map(move |&(start, end)| {
                let (preview, preview_start, preview_end) =
                    preview_of(&line.text, start - line.text_offset, end - line.text_offset);
                Hit {
                    line: usize::try_from(line.line.saturating_sub(1)).unwrap_or(usize::MAX),
                    start,
                    end,
                    preview,
                    preview_start,
                    preview_end,
                }
            })
        })
        .collect()
}

/// How deep a key is: its folders.
fn depth(key: &str) -> usize {
    key.matches('/').count()
}

/// Every match of the job's query in the files of the workspace's `folder` on disk, through
/// azul-search (ripgrep's parallel walker and searcher - the engine AzDrive's search box runs
/// on): binary files (a NUL byte; a UTF-16 file too, which the editor could not show), files
/// over [`SEARCH_MAX_FILE`] and the folders quick open leaves out are passed over; at most
/// [`SEARCH_MAX_HITS`] matches. The files nearest the folder come first, in name order. A
/// raised `cancel` stops it.
pub fn search_files(folder: &Path, job: &SearchJob) -> SearchOutcome {
    let mut outcome = SearchOutcome {
        files: Vec::new(),
        searched: 0,
        complete: true,
        error: None,
    };
    // A needle with a line break matches no line.
    if job.query.is_empty() || job.query.contains('\n') {
        return outcome;
    }
    let pattern = Pattern::literal(job.query.as_str())
        .with_case(if job.how.match_case {
            Case::Sensitive
        } else {
            Case::Insensitive
        })
        .with_whole_word(job.how.whole_word);
    let request = Request::new(folder)
        .with_contents(pattern)
        .with_filters(Filters {
            include: Vec::new(),
            exclude: search_excludes(),
            // Dot files are searched (.gitignore, .env); dot folders are not (the excludes).
            hidden: true,
            // Every file of the folder, a .gitignore or not (as quick open lists them).
            ignore_files: false,
        })
        .with_limits(Limits {
            max_results: usize::MAX,
            max_matches: SEARCH_MAX_HITS,
            max_lines_per_file: SEARCH_MAX_HITS,
            max_file_size: SEARCH_MAX_FILE as u64,
        })
        // The editor opens files as UTF-8: a UTF-16 file's match could not be shown.
        .with_utf16(false);
    let mut total = 0;
    let mut cut = false;
    let mut files = Vec::new();
    let searched = azul_search::search(&request, &job.cancel, &mut |event| {
        let Event::Content(file) = event else {
            return;
        };
        let mut hits = hits_of(&file);
        // A line with several matches is several results: the cap counts results (the
        // engine's, lines) - reached, the search stops here.
        if total + hits.len() > SEARCH_MAX_HITS {
            hits.truncate(SEARCH_MAX_HITS - total);
            cut = true;
            job.cancel.store(true, Ordering::Relaxed);
        }
        if hits.is_empty() {
            return;
        }
        total += hits.len();
        files.push(FileHits {
            key: file.path,
            hits,
        });
    });
    match searched {
        Ok(summary) => {
            outcome.searched = summary.searched;
            outcome.complete = !summary.cancelled && !summary.limited && !cut;
        }
        Err(e) => {
            outcome.complete = false;
            outcome.error = Some(e.to_string());
        }
    }
    files.sort_by(|a, b| {
        depth(&a.key)
            .cmp(&depth(&b.key))
            .then_with(|| a.key.cmp(&b.key))
    });
    outcome.files = files;
    outcome
}

/// What a search thread hands back.
pub struct SearchReply {
    pub root: Root,
    pub generation: u64,
    pub outcome: SearchOutcome,
}

struct SearchThreadInit {
    root: Root,
    job: SearchJob,
    on_done: WriteBackCallbackType,
}

extern "C" fn search_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((root, job, on_done)) = init
        .downcast_ref::<SearchThreadInit>()
        .map(|i| (i.root.clone(), i.job.clone(), i.on_done))
    else {
        return;
    };
    let outcome = search_files(&root.folder(), &job);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(SearchReply {
            root,
            generation: job.generation,
            outcome,
        }),
    )));
}

/// Searches `root`'s files for `job` on an azul Thread; `on_done(reply_to,
/// SearchReply, info)` gets what it found on the UI thread.
pub fn spawn_search(
    info: &mut CallbackInfo,
    root: &Root,
    job: SearchJob,
    reply_to: RefAny,
    on_done: WriteBackCallbackType,
) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(SearchThreadInit {
                root: root.clone(),
                job,
                on_done,
            }),
            reply_to,
            search_thread,
        ),
    );
}

/// Takes the reply out of a search write-back's message.
#[must_use]
pub fn take_search_reply(msg: &mut RefAny) -> Option<SearchReply> {
    let mut guard = msg.downcast_mut::<SearchReply>()?;
    Some(SearchReply {
        root: guard.root.clone(),
        generation: guard.generation,
        outcome: std::mem::replace(
            &mut guard.outcome,
            SearchOutcome {
                files: Vec::new(),
                searched: 0,
                complete: true,
                error: None,
            },
        ),
    })
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

/// What a drive thread hands back: the root the jobs ran on (a listing of
/// a workspace that is no longer open is dropped; a file read is opened on
/// it) and every job's outcome.
pub struct DriveReply {
    pub root: Root,
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
        RefAny::new(DriveReply { root, outcomes }),
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
        root: guard.root.clone(),
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
    use crate::search::Found;

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

    #[test]
    fn a_search_of_the_folder_reads_its_text_files_but_not_binaries_or_build_output() {
        let dir = temp_dir("search");
        let root = Root {
            drive_root: dir.clone(),
            prefix: String::new(),
            data_tree: false,
            name: "ws".to_string(),
        };
        let drive = drive_of(&root);
        let files: [(&str, &[u8]); 5] = [
            ("Cargo.toml", b"[package]\nname = \"picked\"\n"),
            ("src/lib.rs", b"pub fn picked() -> u32 {\n    7\n}\n// picked twice\n"),
            ("notes.md", b"nothing here\n"),
            ("logo.bin", b"picked\0\0\0"),
            ("target/out.rs", b"picked\n"),
        ];
        let writes = files
            .iter()
            .map(|(k, b)| DriveJob::Write {
                key: (*k).to_string(),
                bytes: b.to_vec(),
            })
            .collect();
        let _ = run_jobs(&drive, &root.prefix, writes);
        let job = SearchJob {
            query: "picked".to_string(),
            how: TextMatch::default(),
            generation: 1,
            cancel: Arc::new(AtomicBool::new(false)),
        };
        let found = search_files(&root.folder(), &job);
        let mut keys: Vec<(&str, usize)> = found.files.iter().map(|f| (f.key.as_str(), f.hits.len())).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec![("Cargo.toml", 1), ("src/lib.rs", 2)]);
        assert!(found.complete && found.error.is_none());
        let lib = found.files.iter().find(|f| f.key == "src/lib.rs").expect("lib.rs");
        assert_eq!((lib.hits[1].line, lib.hits[1].start), (3, 3));
        job.cancel.store(true, Ordering::Relaxed);
        let stopped = search_files(&root.folder(), &job);
        assert!(stopped.files.is_empty() && !stopped.complete, "a raised cancel stops the search");
        let branch = run_jobs(&drive, &root.prefix, vec![DriveJob::Branch { folder: dir.clone() }]);
        assert_eq!(
            branch,
            vec![DriveOutcome::Branch {
                folder: dir.clone(),
                branch: None
            }],
            "no repository"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A user's folder as the workspace, with `files` written into it.
    fn workspace_with(name: &str, files: &[(&str, &[u8])]) -> (PathBuf, Root) {
        let dir = temp_dir(name);
        for (key, bytes) in files {
            let path = dir.join(key);
            std::fs::create_dir_all(path.parent().expect("a folder")).expect("the folder");
            std::fs::write(path, bytes).expect("the file");
        }
        let root = Root {
            drive_root: dir.clone(),
            prefix: String::new(),
            data_tree: false,
            name: "ws".to_string(),
        };
        (dir, root)
    }

    fn picked() -> SearchJob {
        SearchJob {
            query: "picked".to_string(),
            how: TextMatch::default(),
            generation: 1,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The editor opens a file as UTF-8 (invalid bytes replaced): a UTF-16 file's match could
    /// not be shown where it is, so the search passes the file over as binary (as before the
    /// shared engine, which could read it).
    #[test]
    fn a_search_of_the_folder_passes_over_a_utf16_file_the_editor_cannot_show() {
        let mut utf16 = vec![0xFF, 0xFE];
        for unit in "first line\nthe picked one\n".encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        let (dir, root) = workspace_with(
            "search-utf16",
            &[("notes.txt", &utf16[..]), ("plain.txt", &b"the picked one\n"[..])],
        );
        let found = search_files(&root.folder(), &picked());
        let keys: Vec<&str> = found.files.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, vec!["plain.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A CRLF file's lines and a UTF-8 byte-order mark read as in the editor the match opens in:
    /// no `\r` in a preview, no column for the mark; every match of a line is a result, case
    /// as asked.
    #[test]
    fn a_search_of_the_folder_reads_crlf_lines_and_a_bom_as_the_editor_does() {
        let text = "\u{feff}fn main() {\r\n    let picked = 7;\r\n    picked + picked\r\n}\r\n";
        let (dir, root) = workspace_with("search-crlf", &[("main.rs", text.as_bytes())]);
        let found = search_files(&root.folder(), &picked());
        let hits = &found.files[0].hits;
        assert_eq!(hits.len(), 3, "{hits:?}");
        assert_eq!((hits[0].line, hits[0].start, hits[0].end), (1, 8, 14));
        assert_eq!(hits[0].preview, "let picked = 7;", "the indentation and the CR left out");
        assert_eq!(&hits[0].preview[hits[0].preview_start..hits[0].preview_end], "picked");
        assert_eq!(hits[2].found(), Found { line: 2, start: 13, end: 19 });
        let case = SearchJob {
            query: "PICKED".to_string(),
            how: TextMatch {
                match_case: true,
                ..TextMatch::default()
            },
            ..picked()
        };
        assert!(search_files(&root.folder(), &case).files.is_empty(), "case as asked");
        let any = SearchJob {
            query: "PICKED".to_string(),
            ..picked()
        };
        assert_eq!(search_files(&root.folder(), &any).files[0].hits.len(), 3, "any case");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_results_list_the_files_nearest_the_folder_first_in_name_order() {
        let (dir, root) = workspace_with(
            "search-order",
            &[
                ("c.txt", &b"picked\n"[..]),
                ("a/b/c.txt", &b"picked\n"[..]),
                ("b.txt", &b"picked\n"[..]),
                ("a/z.txt", &b"picked\n"[..]),
            ],
        );
        let found = search_files(&root.folder(), &picked());
        let keys: Vec<&str> = found.files.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, vec!["b.txt", "c.txt", "a/z.txt", "a/b/c.txt"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
