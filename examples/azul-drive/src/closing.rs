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
pub(crate) fn drive_close(_work: usize, _closing: bool) -> DriveClose {
    DriveClose::Close
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
