//! The UI side of `store.rs`: the main window's timer hands each write queue's batch to a file
//! thread (`azul_appkit::ui::spawn_file_jobs`), and the thread's answer finishes the batch.
//!
//! Only the MAIN window's timer starts file threads: a thread started from the editor window
//! would answer a window that Save & Close just closed. A write that did not land is kept for
//! a retry (the sync timer queues it again) and said in the notice line. On stdout, for
//! scripts, once the file landed: `AZCAL_SAVED <path>` (an event), `AZCAL_SYNCED <link>`,
//! `AZCAL_EXPORTED <count> <path>`; `AZCAL_SAVE_FAILED <key> <why>` when it did not.

use std::path::PathBuf;

use azul::{
    callbacks::{TimerCallbackInfo, TimerCallbackReturn},
    prelude::*,
};
use azul_appkit::{
    files::{FileJob, FileOutcome},
    ui as kit,
};
use azul_pim::write_queue::Write;

use crate::{store, CalState};

/// How often the main window's timer looks for writes to start.
pub(crate) const WRITE_TICK_MS: u64 = 100;

/// A batch on its way, and the lines it prints once its writes landed: the lines announced
/// for its keys before it was taken (a line announced later waits for the next write of its
/// key - an `AZCAL_SYNCED` is not said by the write that still says "pending").
#[derive(Debug, Default)]
pub(crate) struct InFlight {
    pub(crate) batch: Vec<Write>,
    pub(crate) lines: Vec<(String, String)>,
}

impl InFlight {
    /// `batch` on its way, taking the waiting lines of its keys out of `waiting`.
    fn start(batch: Vec<Write>, waiting: &mut Vec<(String, String)>) -> InFlight {
        let (lines, rest): (Vec<_>, Vec<_>) = std::mem::take(waiting)
            .into_iter()
            .partition(|(key, _)| batch.iter().any(|w| w.key() == key));
        *waiting = rest;
        InFlight { batch, lines }
    }
}

/// Starts each queue's next batch on a file thread (when none is on its way). Called from the
/// main window only.
pub(crate) fn pump(s: &mut CalState, info: &mut CallbackInfo, app: &RefAny) {
    if let Some(batch) = s.data_writes.take() {
        kit::spawn_file_jobs(
            info,
            &s.data_dir,
            store::jobs_of(&batch),
            app.clone(),
            store::TAG_DATA,
            on_writes_done,
        );
        s.data_flight = InFlight::start(batch, &mut s.on_landing);
    }
    if let Some(batch) = s.task_writes.take() {
        kit::spawn_file_jobs(
            info,
            &s.tasks_root,
            store::jobs_of(&batch),
            app.clone(),
            store::TAG_TASKS,
            on_writes_done,
        );
        s.task_flight = InFlight::start(batch, &mut s.on_landing);
    }
}

/// Writes `text` to the export file `path` on a file thread (a drive on its folder); its
/// answer says `AZCAL_EXPORTED <count> <path>`. Called from the main window only.
pub(crate) fn export(
    s: &mut CalState,
    info: &mut CallbackInfo,
    app: &RefAny,
    path: PathBuf,
    text: String,
    count: usize,
) {
    let folder = path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| s.data_dir.clone());
    let key = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from("AzCalendar.ics"));
    s.export_pending = Some((count, folder.join(&key)));
    kit::spawn_file_jobs(
        info,
        &folder,
        vec![FileJob::Put {
            key,
            bytes: text.into_bytes(),
        }],
        app.clone(),
        store::TAG_EXPORT,
        on_writes_done,
    );
}

/// Reads the .ics file at `path` on a file thread (a drive on its folder); its answer hands
/// the text to `chrome::import`. Called from the main window only.
pub(crate) fn read_import(s: &mut CalState, info: &mut CallbackInfo, app: &RefAny, path: PathBuf) {
    let folder = path.parent().map(PathBuf::from).unwrap_or_default();
    let key = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    s.import_pending = Some(path);
    kit::spawn_file_jobs(
        info,
        &folder,
        vec![FileJob::Get { key }],
        app.clone(),
        store::TAG_IMPORT,
        on_writes_done,
    );
}

/// The main window's write timer.
pub(crate) extern "C" fn on_write_tick(
    mut data: RefAny,
    mut info: TimerCallbackInfo,
) -> TimerCallbackReturn {
    let app = data.clone();
    if let Some(mut s) = data.downcast_mut::<CalState>() {
        pump(&mut s, &mut info.callback_info, &app);
    }
    TimerCallbackReturn::continue_unchanged()
}

/// A file thread's answer: the batch is finished (what did not land waits for a retry), what
/// landed is announced, the next batch starts, and a main window asked to close closes once
/// nothing waits.
extern "C" fn on_writes_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let handle = app.clone();
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<CalState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let mut refresh = false;
    match reply.tag {
        store::TAG_IMPORT => {
            let Some(path) = s.import_pending.take() else {
                return Update::DoNothing;
            };
            let read = reply.outcomes.into_iter().find_map(|o| match o {
                FileOutcome::Got { result, .. } => Some(result),
                _ => None,
            });
            match read {
                Some(Ok(Some(bytes))) => {
                    crate::chrome::import(s, &path, &String::from_utf8_lossy(&bytes));
                }
                Some(Ok(None)) => crate::chrome::report(
                    s,
                    true,
                    format!("Could not read {}: there is no such file.", path.display()),
                ),
                Some(Err(e)) => crate::chrome::report(
                    s,
                    true,
                    format!("Could not read {}: {e}", path.display()),
                ),
                None => crate::chrome::report(
                    s,
                    true,
                    format!("Could not read {}.", path.display()),
                ),
            }
            refresh = true;
        }
        store::TAG_EXPORT => {
            let Some((count, path)) = s.export_pending.take() else {
                return Update::DoNothing;
            };
            match reply.outcomes.iter().find_map(FileOutcome::error) {
                None => {
                    println!("AZCAL_EXPORTED {count} {}", path.display());
                    s.export_path = path.display().to_string();
                    crate::chrome::report(
                        s,
                        false,
                        format!("Exported {count} event(s) to {}.", path.display()),
                    );
                }
                Some(e) => crate::chrome::report(
                    s,
                    true,
                    format!("Could not write {}: {e}", path.display()),
                ),
            }
            refresh = true;
        }
        tag @ (store::TAG_DATA | store::TAG_TASKS) => {
            let (flight, root) = if tag == store::TAG_DATA {
                (std::mem::take(&mut s.data_flight), s.data_dir.clone())
            } else {
                (std::mem::take(&mut s.task_flight), s.tasks_root.clone())
            };
            let failed = store::failures_of(&flight.batch, &reply.outcomes);
            let landed = |key: &str| !failed.iter().any(|(f, _)| f.key() == key);
            for write in flight.batch.iter().filter(|w| landed(w.key())) {
                eprintln!("[azcalendar] wrote {}", root.join(write.key()).display());
            }
            for (key, line) in flight.lines {
                if landed(&key) {
                    println!("{line}");
                } else {
                    // Said when the retry lands.
                    s.on_landing.push((key, line));
                }
            }
            for (write, why) in &failed {
                println!("AZCAL_SAVE_FAILED {} {why}", write.key());
                eprintln!(
                    "[azcalendar] could not write {}: {why}",
                    root.join(write.key()).display()
                );
            }
            if let Some((write, why)) = failed.first() {
                s.notice = format!(
                    "Could not write {}: {why}. It is tried again in a moment.",
                    root.join(write.key()).display()
                );
                refresh = true;
            }
            if tag == store::TAG_DATA {
                s.data_writes.finish(failed);
            } else {
                s.task_writes.finish(failed);
            }
        }
        _ => return Update::DoNothing,
    }
    pump(s, &mut info, &handle);
    if s.closing && s.writes_idle() {
        s.closing = false;
        if s.write_failures() == 0 {
            eprintln!("[azcalendar] every write landed: the window closes");
            info.close_window();
        } else {
            // Not lost without a word: the window stays, says so, and closes on the next try.
            s.close_despite_failures = true;
            s.notice = format!(
                "{} change(s) could not be written. Close the window again to quit without them.",
                s.write_failures()
            );
            refresh = true;
        }
    }
    if refresh {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}
