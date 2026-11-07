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

use crate::{crypto::ObjectId, BatchDelete, Drive, DriveError};

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

/// What an index releases when a change leaves objects that no entry of its head names any more.
/// Two obligations that used to share one return value: an object nobody can ask for again is
/// deleted at once; one the history still names is not deleted by whoever released it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Released {
    /// Objects no state of the drive names any more - an index that keeps no history
    /// ([`crate::encrypted::MemoryIndex`]): they may go from the bucket now.
    pub delete_now: Vec<ObjectId>,
    /// Objects the head no longer names but the history does (a repository: every state of the
    /// drive is a commit). They stay in the bucket until the reclaimer has proved them past their
    /// retention ([`super::retention::RETENTION`]). A hint: it tells a caller not to delete, and
    /// wakes the reclaimer; nothing depends on it, because the reclaimer derives what was released
    /// from the commits themselves.
    pub held_by_history: Vec<ObjectId>,
}

impl Released {
    /// Nothing released.
    #[must_use]
    pub fn none() -> Released {
        Released::default()
    }

    /// `ids` can go from the bucket now.
    #[must_use]
    pub fn delete_now(ids: Vec<ObjectId>) -> Released {
        Released {
            delete_now: ids,
            held_by_history: Vec::new(),
        }
    }

    /// `ids` are held by the history.
    #[must_use]
    pub fn held(ids: Vec<ObjectId>) -> Released {
        Released {
            delete_now: Vec::new(),
            held_by_history: ids,
        }
    }

    /// Whether nothing was released.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.delete_now.is_empty() && self.held_by_history.is_empty()
    }

    /// What `other` released as well.
    pub fn extend(&mut self, other: Released) {
        self.delete_now.extend(other.delete_now);
        self.held_by_history.extend(other.held_by_history);
    }
}

/// Deletes the objects an index without history released ([`Released::delete_now`]). The objects the
/// bucket would not delete come back with its answer: they are named by nothing and count against
/// the quota, so the caller keeps them to try again and says so (AZ-U22) - they used to be dropped
/// on the floor.
pub(crate) fn delete_released<D: Drive + ?Sized>(
    drive: &D,
    released: &Released,
) -> Vec<(ObjectId, DriveError)> {
    if released.delete_now.is_empty() {
        return Vec::new();
    }
    let keys: Vec<String> = released
        .delete_now
        .iter()
        .map(ObjectId::bucket_key)
        .collect();
    let items: Vec<BatchDelete<'_>> = keys.iter().map(|key| BatchDelete::new(key)).collect();
    let mut answers = drive.delete_many(&items).into_iter();
    let mut failed = Vec::new();
    for id in &released.delete_now {
        // A drive that answers for fewer items than it was asked is not taken to have deleted
        // the rest.
        let answer = answers.next().unwrap_or_else(|| {
            Err(DriveError::Protocol(String::from(
                "the storage answered for fewer deletes than were asked",
            )))
        });
        if let Err(e) = answer {
            failed.push((*id, e));
        }
    }
    failed
}
