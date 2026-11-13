//! The window's close while work runs that a close would cut off: transfers, sync passes, a
//! drive being encrypted, the jobs that hold a claim and the program of the command bar. The
//! first close waits: the window says what still runs and closes once it ended. A second close
//! stops the work and goes.
//!
//! What stopped work leaves behind, and what the next start does with it:
//! - a download keeps its hidden `.part` file and goes on from there when it is started again;
//! - an upload of a file on this computer keeps its multipart state in the cache folder's
//!   `uploads`, and the next upload of the same file sends only the missing parts;
//! - a copy on this computer leaves at most a temporary file beside its target, never a cut
//!   target; a move deletes a source only after its copy landed, so nothing is lost;
//! - a sync pass starts again by itself; an encryption goes on from its journal.
//!
//! The transfer queue itself is not kept: a stopped copy or move is started again by the user.

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use azul::prelude::*;
use azul_appkit::l10n::Phrase;

use crate::{fileops::JobState, DriveState};

/// What a close request does to the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DriveClose {
    /// Nothing runs: the window closes.
    Close,
    /// Work runs: the close is held, the window says so and closes once the work ended.
    Wait,
    /// A close was held already: the work is stopped and the window closes.
    Stop,
}

/// What a close request does while `work` pieces of work run; `closing` says a close was
/// held already.
pub(crate) fn drive_close(work: usize, closing: bool) -> DriveClose {
    if work == 0 {
        DriveClose::Close
    } else if closing {
        DriveClose::Stop
    } else {
        DriveClose::Wait
    }
}

/// A job a close must wait for, from its start to its end: counted while it lives, whatever
/// ends it (its answer, a panic of its thread). `Job::cuts_data` says which jobs hold one.
#[derive(Debug)]
pub(crate) struct Inflight {
    counter: Arc<AtomicUsize>,
}

impl Inflight {
    /// One more job counted in `counter`, until the returned claim is dropped.
    pub(crate) fn start(counter: &Arc<AtomicUsize>) -> Inflight {
        counter.fetch_add(1, Ordering::SeqCst);
        Inflight {
            counter: Arc::clone(counter),
        }
    }
}

impl Drop for Inflight {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The pieces of work a close would cut off, by kind.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Work {
    /// Transfers running or waiting.
    pub transfers: usize,
    /// Sync passes.
    pub syncs: usize,
    /// A drive being encrypted.
    pub encrypting: usize,
    /// Every job that holds a claim ([`Inflight`]): a delete, a rename, an undo, a zip, the copy
    /// of an unpaid drive, a restore, a save, a sync task that changes files.
    pub claims: usize,
    /// A program the command bar runs (`mv`, `cp -r`, `git switch`): it is not a job, and the
    /// close used to go over it (X-DELTA-AZDRIVE-FILES-30).
    pub programs: usize,
}

impl Work {
    /// How many pieces of work run. Every field is counted: a new kind of work that is not added
    /// here does not compile.
    pub(crate) fn total(&self) -> usize {
        let Work {
            transfers,
            syncs,
            encrypting,
            claims,
            programs,
        } = *self;
        transfers + syncs + encrypting + claims + programs
    }
}

/// The work running now, by kind.
pub(crate) fn work_of(s: &DriveState) -> Work {
    let transfers = s
        .queue
        .jobs()
        .iter()
        .filter(|job| matches!(job.state, JobState::Waiting | JobState::Running))
        .count();
    let syncs = s
        .sync_view
        .drives
        .values()
        .filter(|d| d.running.is_some())
        .count();
    #[cfg(feature = "encryption")]
    let encrypting = usize::from(matches!(
        s.popup.as_ref(),
        Some(crate::Popup::Encryption(
            crate::encryption::Dialog::Busy { .. }
        ))
    ));
    #[cfg(not(feature = "encryption"))]
    let encrypting = 0;
    Work {
        transfers,
        syncs,
        encrypting,
        claims: s.inflight.load(Ordering::SeqCst),
        programs: usize::from(s.command_bar.running()),
    }
}

/// The pieces of work a close would cut off now: transfers running or waiting, sync passes,
/// a drive being encrypted, every job that holds a claim ([`Inflight`]) and the program the
/// command bar runs.
pub(crate) fn work_running(s: &DriveState) -> usize {
    work_of(s).total()
}

/// The window is asked to close (its close button, Cmd+Q): see the module.
pub(crate) extern "C" fn on_close_requested(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    // A picker closed before it answered: the request is cancelled.
    crate::picker::on_close(s);
    // The first close: what was saved just now goes to the drives before the window does.
    if !s.closing {
        last_passes(&mut info, &app, s);
    }
    let work = work_running(s);
    match drive_close(work, s.closing) {
        DriveClose::Close => {
            let_spends_land();
            // A secret this window put on the clipboard does not outlive it: the clear timer
            // would never fire (X-DELTA-AZDRIVE-KEYS-24).
            azul_appkit::secret_clipboard::forget_copied(&mut info);
            // The drives' keys leave memory with the window (zeroed, not just freed).
            crate::unlock::wipe_all(s);
            Update::DoNothing
        }
        DriveClose::Wait => {
            s.closing = true;
            println!("AZDRIVE_CLOSE_WAITS {work}");
            s.info(Phrase::new("azdrive-close-waits").arg("count", work));
            info.prevent_window_close();
            Update::RefreshDom
        }
        DriveClose::Stop => {
            stop_work(s);
            println!("AZDRIVE_CLOSE_STOPPED {work}");
            let_spends_land();
            azul_appkit::secret_clipboard::forget_copied(&mut info);
            crate::unlock::wipe_all(s);
            Update::DoNothing
        }
    }
}

/// Before the window goes: every synced drive that is not paused (and can be opened without
/// asking the keyring) gets a last pass, so a file saved just before the close reaches the drive
/// first - the close waits for it, a second close stops it. A pass running now is followed by
/// one more (the save may have come after its scan). How many were asked.
pub(crate) fn last_passes(info: &mut CallbackInfo, app: &RefAny, s: &mut DriveState) -> usize {
    let due: Vec<String> = s
        .settings
        .synced
        .iter()
        .filter(|p| !p.paused)
        .filter(|p| {
            s.slot_index(&p.drive_id)
                .is_some_and(|i| !s.slots[i].locked())
        })
        .filter(|p| crate::sync_jobs::polled(&s.sync_view.store.states(&p.drive_id)))
        .map(|p| p.drive_id.clone())
        .collect();
    for drive_id in &due {
        crate::sync_jobs::request_pass(info, app, s, drive_id);
    }
    if !due.is_empty() {
        println!("AZDRIVE_CLOSE_LAST_PASSES {}", due.len());
    }
    due.len()
}

/// How long a close waits for a refresh that spent a drive token to keep its answer.
const SPEND_WAIT: Duration = Duration::from_secs(10);

/// Before the window goes: a refresh in flight on a worker thread keeps its new session in the
/// keyring first (up to [`SPEND_WAIT`]); cut off, the keyring keeps the spent token, which the
/// token server answers again at the next start while the new one was never used.
fn let_spends_land() {
    if !azcloud_kit::shared::wait_for_spends(SPEND_WAIT) {
        println!("AZDRIVE_CLOSE_SPEND_CUT");
    }
}

/// After a job's answer: a held close goes once the work ended.
pub(crate) fn after_job(info: &mut CallbackInfo, s: &mut DriveState) {
    if s.closing && work_running(s) == 0 {
        s.closing = false;
        println!("AZDRIVE_CLOSED_AFTER_WORK");
        crate::unlock::wipe_all(s);
        info.close_window();
    }
}

/// A second close: every transfer is told to stop at the next chunk of the file it copies (inside
/// the file, not between two), every sync pass between two files, and the command bar's program is
/// ended.
fn stop_work(s: &mut DriveState) {
    for job in s.transfers.values() {
        job.cancel.store(true, Ordering::SeqCst);
    }
    let ids: Vec<u64> = s.queue.jobs().iter().map(|job| job.id).collect();
    for id in ids {
        s.queue.cancel(id);
    }
    for drive in s.sync_view.drives.values() {
        if let Some(running) = &drive.running {
            running.cancel.store(true, Ordering::SeqCst);
        }
    }
    // The command bar's program: interrupted, and ended with its terminal (it gets SIGHUP).
    crate::ui_command_bar::end_run(s);
}

#[cfg(test)]
mod tests {
    use azul_storage::{Drive, LocalDrive};

    use super::*;
    use crate::{fileops::SourceItem, jobs::Job};

    #[test]
    fn a_close_waits_for_running_work_and_a_second_close_stops_it() {
        assert_eq!(drive_close(0, false), DriveClose::Close, "nothing runs");
        assert_eq!(
            drive_close(2, false),
            DriveClose::Wait,
            "a transfer and a sync pass run"
        );
        assert_eq!(
            drive_close(2, true),
            DriveClose::Stop,
            "closed again while waiting"
        );
        assert_eq!(
            drive_close(0, true),
            DriveClose::Close,
            "the work ended meanwhile"
        );
    }

    /// X-DELTA-AZDRIVE-FILES-30: every kind of work is in the count, the command bar's program
    /// among them; a count of nothing but a program is a close that waits.
    #[test]
    fn x_delta_azdrive_files_30_the_command_bars_program_is_work_a_close_waits_for() {
        assert_eq!(Work::default().total(), 0);
        let program = Work {
            programs: 1,
            ..Work::default()
        };
        assert_eq!(program.total(), 1);
        assert_eq!(drive_close(program.total(), false), DriveClose::Wait);
        let all = Work {
            transfers: 1,
            syncs: 2,
            encrypting: 1,
            claims: 3,
            programs: 1,
        };
        assert_eq!(all.total(), 8);
    }

    /// AZ-U23: the first close waited for transfers, sync passes and an encryption only: a bulk
    /// delete, a bucket folder's rename or the rescue copy was cut mid-way. A claim is counted from
    /// the start of the job to its end, however the job ends.
    #[test]
    fn az_u23_a_job_that_cuts_data_is_counted_until_it_ends() {
        let counter = Arc::new(AtomicUsize::new(0));
        let first = Inflight::start(&counter);
        let second = Inflight::start(&counter);
        assert_eq!(counter.load(Ordering::SeqCst), 2);
        drop(first);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        // A thread that panics drops its claim while it unwinds.
        let held = Arc::clone(&counter);
        let ended = std::thread::spawn(move || {
            let _claim = Inflight::start(&held);
            panic!("a job's thread dies");
        })
        .join();
        assert!(ended.is_err());
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "only the second job is left"
        );
        drop(second);
        assert_eq!(
            drive_close(counter.load(Ordering::SeqCst), false),
            DriveClose::Close
        );
    }

    /// X-AR5a-20: which jobs a close waits for is decided in one exhaustive match
    /// (`Job::cuts_data`): a delete, a rename, an undo and a zip are among them, a listing is not.
    #[test]
    fn x_ar5a_20_the_jobs_that_delete_or_rename_cut_data_and_a_listing_does_not() {
        let tmp = azul_storage::testing::TempDir::new("closing-jobs");
        let drive: Arc<dyn Drive> = Arc::new(LocalDrive::without_manifest(tmp.path()));
        let item = || SourceItem {
            key: String::from("a.txt"),
            is_folder: false,
            size: None,
        };
        let delete = Job::Delete {
            drive: drive.clone(),
            drive_id: String::from("home"),
            items: vec![item()],
            stamp: None,
        };
        let rename = Job::Rename {
            drive: drive.clone(),
            drive_id: String::from("home"),
            from: String::from("a.txt"),
            to: String::from("b.txt"),
        };
        let undo = Job::Undo {
            drive: drive.clone(),
            pairs: Vec::new(),
            remove: None,
            trashed: false,
        };
        let zip = Job::Zip {
            drive: drive.clone(),
            items: vec![item()],
            zip_key: String::from("a.zip"),
        };
        for job in [&delete, &rename, &undo, &zip] {
            assert!(job.cuts_data());
        }
        let scan = Job::Scan {
            drive: drive.clone(),
            dir: None,
            prefix: String::new(),
            refresh: false,
            serial: 1,
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        let measure = Job::Measure {
            drive,
            prefix: String::new(),
            serial: 1,
        };
        assert!(!scan.cuts_data() && !measure.cuts_data());
    }
}
