//! The work that blocks: every storage call runs on an azul `Thread` and answers through
//! the thread's write-back; no callback waits on a file (or, later, on S3).
//!
//! The write queue (`store::WriteQueue`) is drained one batch at a time by [`pump`]: a
//! change queues its file, `pump` starts a batch when none is in flight, the batch's answer
//! finishes it and starts the next. On stdout, for scripts: `AZTASKS_SAVED <key>` /
//! `AZTASKS_REMOVED <key>` per write, `AZTASKS_SAVE_FAILED <key>`, `AZTASKS_ATTACHED <task>
//! <name>`.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use azul::{prelude::*, url::Url};
use azul_storage::{Drive, DriveError, LocalDrive};

use crate::{
    model::Attachment,
    state::{self, Tasks},
    store::{self, Write},
};

/// A piece of blocking work.
pub enum Job {
    /// Read every file under `tasks/`.
    Load,
    /// A batch of the write queue.
    Batch(Vec<Write>),
    /// Copy a file next to task `task` as `key`.
    Attach {
        task: String,
        name: String,
        key: String,
        source: PathBuf,
    },
    /// Fetch `key` into `dir` and open it with the OS.
    Open { key: String, dir: PathBuf },
    /// Move the files under each `from` folder to its `to` folder.
    MoveFiles(Vec<(String, String)>),
    /// Delete the files under each folder.
    DeleteFiles(Vec<String>),
    /// Read the iCalendar file at `path` to import its to-dos (a drive on its folder: the file
    /// is the user's, outside the data tree).
    ReadImport(PathBuf),
}

/// A job's answer.
enum Outcome {
    Loaded(Result<store::Loaded, DriveError>),
    Batch(store::BatchResult),
    Attached {
        task: String,
        name: String,
        result: Result<u64, DriveError>,
    },
    Opened(Result<PathBuf, String>),
    Files(Result<usize, DriveError>),
    ImportRead {
        path: PathBuf,
        result: Result<String, String>,
    },
}

/// What a thread starts with.
struct JobInit {
    drive: Arc<dyn Drive>,
    job: Option<Job>,
}

/// A thread's answer, taken out once by the write-back.
struct Done {
    outcome: Option<Outcome>,
}

fn run(drive: &dyn Drive, job: Job) -> Outcome {
    match job {
        Job::Load => Outcome::Loaded(store::load_all(drive)),
        Job::Batch(batch) => Outcome::Batch(store::run_batch(drive, batch)),
        Job::Attach {
            task,
            name,
            key,
            source,
        } => Outcome::Attached {
            task,
            name,
            result: store::attach_file(drive, &key, &source),
        },
        Job::Open { key, dir } => Outcome::Opened(
            store::fetch_file(drive, &key, &dir)
                .map_err(|e| e.to_string())
                .and_then(|path| open_with_os(&path).map(|()| path)),
        ),
        Job::MoveFiles(moves) => Outcome::Files(moves.iter().try_fold(0, |n, (from, to)| {
            store::move_files(drive, from, to).map(|m| n + m)
        })),
        Job::DeleteFiles(prefixes) => Outcome::Files(
            prefixes
                .iter()
                .try_fold(0, |n, p| store::delete_files(drive, p).map(|m| n + m)),
        ),
        Job::ReadImport(path) => {
            let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let result = LocalDrive::new(folder)
                .get(&name)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .map_err(|e| e.to_string());
            Outcome::ImportRead { path, result }
        }
    }
}

/// `file:///...` of a local path. The twin of AzDrive's `browse::file_url` (named in the
/// report: it belongs in azul-storage, next to `uri_encode`).
fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let bytes = text.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return format!(
            "file:///{}{}",
            &text[..2],
            azul_storage::sigv4::uri_encode(&text[2..], false)
        );
    }
    let absolute = if text.starts_with('/') {
        text
    } else {
        format!("/{text}")
    };
    format!("file://{}", azul_storage::sigv4::uri_encode(&absolute, false))
}

/// Opens `path` with the OS's default app.
fn open_with_os(path: &Path) -> Result<(), String> {
    let url = file_url(path);
    match Url::parse(url.as_str()).into_result() {
        Ok(url) if url.open() => Ok(()),
        Ok(_) => Err(format!("the system could not open {}", path.display())),
        Err(e) => Err(format!("{url} is not a URL: {}", e.message.as_str())),
    }
}

/// Runs on a worker thread: the blocking call, then its answer to the UI thread.
extern "C" fn job_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((drive, job)) = init
        .downcast_mut::<JobInit>()
        .and_then(|mut init| init.job.take().map(|job| (init.drive.clone(), job)))
    else {
        return;
    };
    let outcome = run(&*drive, job);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_job_done,
        RefAny::new(Done {
            outcome: Some(outcome),
        }),
    )));
}

/// Starts `job` on a thread.
pub fn spawn(info: &mut CallbackInfo, app: &RefAny, s: &mut Tasks, job: Job) {
    if !matches!(job, Job::Batch(_) | Job::Load) {
        s.files.running += 1;
    }
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(JobInit {
                drive: s.drive.clone(),
                job: Some(job),
            }),
            app.clone(),
            job_thread,
        ),
    );
}

/// Starts the next batch of the write queue when none is in flight.
pub fn pump(info: &mut CallbackInfo, app: &RefAny, s: &mut Tasks) {
    if let Some(batch) = s.queue.take() {
        spawn(info, app, s, Job::Batch(batch));
    }
}

/// Deletes the files under `prefixes` (a deleted task's attachments, a deleted list).
pub fn delete_files(info: &mut CallbackInfo, app: &RefAny, s: &mut Tasks, prefixes: Vec<String>) {
    if !prefixes.is_empty() {
        spawn(info, app, s, Job::DeleteFiles(prefixes));
    }
}

/// Moves attachment folders (tasks moved to another list).
pub fn move_files(info: &mut CallbackInfo, app: &RefAny, s: &mut Tasks, moves: Vec<(String, String)>) {
    if !moves.is_empty() {
        spawn(info, app, s, Job::MoveFiles(moves));
    }
}

/// A thread's answer, on the UI thread.
extern "C" fn on_job_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let handle = app.clone();
    let Some(outcome) = msg
        .downcast_mut::<Done>()
        .and_then(|mut done| done.outcome.take())
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<Tasks>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    match outcome {
        Outcome::Loaded(Ok(loaded)) => {
            for skipped in &loaded.skipped {
                eprintln!("[aztasks] left out {}: {}", skipped.key, skipped.reason);
            }
            s.take_loaded(loaded, state::now());
            crate::check_reminders(&mut info, &handle, s);
        }
        Outcome::Loaded(Err(e)) => {
            s.loaded = true;
            s.load_error = azul_appkit::l10n::t_args(
                "aztasks-load-failed",
                &[("why", azul_appkit::l10n::Arg::from(e.to_string()))],
            );
            eprintln!("[aztasks] {}", s.load_error);
        }
        Outcome::Batch(result) => {
            for key in &result.done {
                println!("AZTASKS_SAVED {key}");
            }
            for (write, why) in &result.failed {
                println!("AZTASKS_SAVE_FAILED {}", write.key());
                eprintln!("[aztasks] could not write {}: {why}", write.key());
            }
            s.queue.finish(result.failed);
        }
        Outcome::Attached { task, name, result } => {
            s.files.running = s.files.running.saturating_sub(1);
            match (result, s.index_of(&task)) {
                (Ok(size), Some(i)) => {
                    s.tasks[i].attachments.retain(|a| a.name != name);
                    s.tasks[i].attachments.push(Attachment { name: name.clone(), size });
                    s.save_task(i);
                    println!("AZTASKS_ATTACHED {task} {name}");
                }
                (Ok(_), None) => {}
                (Err(e), _) => {
                    s.files.last_error = azul_appkit::l10n::t_args(
                        "aztasks-attach-failed",
                        &[
                            ("name", azul_appkit::l10n::Arg::from(name.as_str())),
                            ("why", azul_appkit::l10n::Arg::from(e.to_string())),
                        ],
                    );
                }
            }
        }
        Outcome::Opened(result) => {
            s.files.running = s.files.running.saturating_sub(1);
            if let Err(e) = result {
                s.files.last_error = e;
            }
        }
        Outcome::Files(result) => {
            s.files.running = s.files.running.saturating_sub(1);
            if let Err(e) = result {
                s.files.last_error = azul_appkit::l10n::t_args(
                    "aztasks-files-failed",
                    &[("why", azul_appkit::l10n::Arg::from(e.to_string()))],
                );
            }
        }
        Outcome::ImportRead { path, result } => {
            s.files.running = s.files.running.saturating_sub(1);
            s.io_message = match result {
                Ok(text) => {
                    let before = s.tasks.len();
                    let problems = s.import_tasks(&text, state::now(), &state::utc_to_local);
                    let added = s.tasks.len() - before;
                    println!("AZTASKS_IMPORTED {added} {}", path.display());
                    for p in &problems {
                        eprintln!("[aztasks] {}: {p}", path.display());
                    }
                    let file = azul_appkit::l10n::Arg::from(path.display().to_string());
                    let added = azul_appkit::l10n::Arg::from(added);
                    match problems.first() {
                        None => azul_appkit::l10n::t_args(
                            "aztasks-imported",
                            &[("count", added), ("file", file)],
                        ),
                        // The reader's own words for the first problem.
                        Some(first) => azul_appkit::l10n::t_args(
                            "aztasks-imported-some",
                            &[
                                ("count", added),
                                ("file", file),
                                ("missed", azul_appkit::l10n::Arg::from(problems.len())),
                                ("first", azul_appkit::l10n::Arg::from(first.to_string())),
                            ],
                        ),
                    }
                }
                Err(e) => azul_appkit::l10n::t_args(
                    "aztasks-import-unreadable",
                    &[
                        ("file", azul_appkit::l10n::Arg::from(path.display().to_string())),
                        ("why", azul_appkit::l10n::Arg::from(e.to_string())),
                    ],
                ),
            };
        }
    }
    pump(&mut info, &handle, s);
    Update::RefreshDom
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_path_becomes_a_file_url_with_its_spaces_encoded() {
        assert_eq!(
            file_url(Path::new("/tmp/AzTasks open/a b.pdf")),
            "file:///tmp/AzTasks%20open/a%20b.pdf"
        );
    }
}
