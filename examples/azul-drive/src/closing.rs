//! The window's close while work runs that a close would cut off: transfers, sync passes and
//! a drive being encrypted. The first close waits: the window says what still runs and
//! closes once it ended. A second close stops the work and goes.
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

use std::sync::atomic::Ordering;

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

/// The pieces of work a close would cut off now: transfers running or waiting, sync passes,
/// a drive being encrypted.
pub(crate) fn work_running(s: &DriveState) -> usize {
    let transfers = s
        .queue
        .jobs()
        .iter()
        .filter(|job| matches!(job.state, JobState::Waiting | JobState::Running))
        .count();
    let syncs = s.sync_view.drives.values().filter(|d| d.running.is_some()).count();
    #[cfg(feature = "encryption")]
    let encrypting = usize::from(matches!(
        s.popup,
        Some(crate::Popup::Encryption(crate::encryption::Dialog::Busy { .. }))
    ));
    #[cfg(not(feature = "encryption"))]
    let encrypting = 0;
    transfers + syncs + encrypting
}

/// The window is asked to close (its close button, Cmd+Q): see the module.
pub(crate) extern "C" fn on_close_requested(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    // A picker closed before it answered: the request is cancelled.
    crate::picker::on_close(s);
    let work = work_running(s);
    match drive_close(work, s.closing) {
        DriveClose::Close => Update::DoNothing,
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
            Update::DoNothing
        }
    }
}

/// After a job's answer: a held close goes once the work ended.
pub(crate) fn after_job(info: &mut CallbackInfo, s: &mut DriveState) {
    if s.closing && work_running(s) == 0 {
        s.closing = false;
        println!("AZDRIVE_CLOSED_AFTER_WORK");
        info.close_window();
    }
}

/// A second close: every transfer and sync pass is told to stop between two files.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_close_waits_for_running_work_and_a_second_close_stops_it() {
        assert_eq!(drive_close(0, false), DriveClose::Close, "nothing runs");
        assert_eq!(drive_close(2, false), DriveClose::Wait, "a transfer and a sync pass run");
        assert_eq!(drive_close(2, true), DriveClose::Stop, "closed again while waiting");
        assert_eq!(drive_close(0, true), DriveClose::Close, "the work ended meanwhile");
    }
}
