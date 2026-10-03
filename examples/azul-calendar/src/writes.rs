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

use crate::{store, CalState};

/// How often the main window's timer looks for writes to start.
pub(crate) const WRITE_TICK_MS: u64 = 100;

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
        s.data_batch = batch;
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
        s.task_batch = batch;
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
            let (batch, root) = if tag == store::TAG_DATA {
                (std::mem::take(&mut s.data_batch), s.data_dir.clone())
            } else {
                (std::mem::take(&mut s.task_batch), s.tasks_root.clone())
            };
            let failed = store::failures_of(&batch, &reply.outcomes);
            for write in &batch {
                let key = write.key();
                if failed.iter().any(|(f, _)| f.key() == key) {
                    continue;
                }
                eprintln!("[azcalendar] wrote {}", root.join(key).display());
                let (now, later): (Vec<_>, Vec<_>) =
                    std::mem::take(&mut s.on_landing).into_iter().partition(|(k, _)| k == key);
                s.on_landing = later;
                for (_, line) in now {
                    println!("{line}");
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
