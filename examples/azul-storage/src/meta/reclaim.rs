//! The lifecycle of a drive's data objects (feature `encryption`; ARCH28 RF44 / AR5a-5): what may
//! take a data object out of the bucket, and on what proof.
//!
//! A data object (`data/xx/<id>`: a file's version, or a pack of small files) used to be deleted
//! at a dozen places on a dozen proofs, each proof a guess ("the index refused it", "the head does
//! not name it", "thirty days passed"). The rules of this module are few:
//!
//! - **A new object that no change named** is taken back out only by [`drop_new_object`], and
//!   only on a [`Refusal`]: the proof that the change was refused for good or never made
//!   ([`Certainty`]). A change whose outcome is not known - no answer, a service error - may have
//!   landed; the object it uploaded stays (garbage at worst, never a name without its object).
//! - **Nothing else** deletes a data object of a drive that keeps its history (the drive index is
//!   a repository: every state of the drive is a commit). Deleting is a decision about history,
//!   and only the reclaimer makes it.

use crate::{crypto::ObjectId, Drive, DriveError};

/// What a failed call says about the change it tried to make: did it happen?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Certainty {
    /// The change was refused for good: whoever looked said no and wrote nothing (an expectation
    /// that did not hold, a service that refused).
    Refused,
    /// The change may have landed, or not: no answer, a lost connection, a service error, a reply
    /// that could not be read. Neither "someone else wrote first" nor "it went through".
    Unknown,
    /// The change never went out: the call was refused before a byte was sent.
    NotAttempted,
}

impl DriveError {
    /// Whether the change this error ended is known not to have happened ([`Certainty`]). Total:
    /// a new error needs its place here.
    #[must_use]
    pub fn certainty(&self) -> Certainty {
        match self {
            DriveError::Conflict { .. } | DriveError::Denied { .. } => Certainty::Refused,
            DriveError::InvalidKey { .. }
            | DriveError::InvalidRange { .. }
            | DriveError::Locked { .. }
            | DriveError::KeyRefused { .. }
            | DriveError::Unsupported(_)
            | DriveError::InvalidConfig(_)
            | DriveError::Cancelled => Certainty::NotAttempted,
            DriveError::NotFound { .. }
            | DriveError::Service(_)
            | DriveError::Transport(_)
            | DriveError::Io(_)
            | DriveError::Protocol(_)
            | DriveError::Corrupt { .. } => Certainty::Unknown,
        }
    }
}

/// The proof that the change which was to name a new object did not happen. Made from an error
/// whose certainty is [`Certainty::Refused`] or [`Certainty::NotAttempted`], and from nothing else;
/// it cannot be copied, defaulted or read from a file.
#[derive(Debug)]
pub(crate) struct Refusal {
    _sealed: (),
}

impl Refusal {
    /// The refusal `error` is, if it is one; `None` for a change that may have landed.
    pub(crate) fn of(error: &DriveError) -> Option<Refusal> {
        match error.certainty() {
            Certainty::Unknown => None,
            Certainty::Refused | Certainty::NotAttempted => Some(Refusal { _sealed: () }),
        }
    }
}

/// Takes the new object `id` back out of `drive`'s bucket: the one compensation of a change that
/// did not name it (`_refusal` is the proof). A delete the bucket refuses leaves the object where
/// it is - named by nothing, and the caller says so.
///
/// # Errors
///
/// The bucket's error.
pub(crate) fn drop_new_object<D: Drive + ?Sized>(
    drive: &D,
    id: &ObjectId,
    _refusal: &Refusal,
) -> Result<(), DriveError> {
    drive.delete(&id.bucket_key())
}
