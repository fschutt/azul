//! What a failed call knows of the change it tried to make (ARCH28 RF44 / AR5a-5): did it happen?
//!
//! A change that was never made can be undone - an object uploaded for it taken out of the bucket
//! (`reclaim::Refusal`). Nothing else is undone: a log entry or a checkpoint written for a manifest
//! swap that failed stays as space for the orphan sweep, whatever the error says (X-DELTA-SYNC6-2: no
//! code removes one on the word of an error since d7c709424). A change that may have landed (no answer,
//! a service error) cannot be undone: neither "someone else wrote first" nor "it went through", so
//! nothing is undone on a guess.
//!
//! [`Certainty::Refused`] is the meaning of the LAST answer ("the node said no"), and no more. A
//! transport sends a conditional write again after a lost answer and hands on the answer of the
//! repeat; a refusal of POLICY (`Denied`, a node's `QuotaExceeded`, `ReadOnlyMoving`, ...) says nothing
//! of the first request, which may still land after the state is read back (X-DELTA-SYNC7-1). The one
//! consumer that acts on this table, the compensation of a new object
//! ([`super::reclaim::drop_new_object_after`]), takes only [`Certainty::NotAttempted`] and the index's
//! own [`DriveError::Conflict`] for proof, until ARCH30-5 (A3 of ARCH29 9.4) mints `Refused` for a
//! leaf's first and only attempt.

use crate::DriveError;

/// What a failed call says about the change it tried to make: did it happen?
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Certainty {
    /// The LAST request of the call was refused: whoever looked said no and wrote nothing (an
    /// expectation that did not hold, a service that refused). Not a proof about an earlier request
    /// of the same call: a write the transport sent again may have landed with the first one.
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
            // A node that says no for a reason of policy - a drive that is frozen, over its quota,
            // banned, being moved - refused before it wrote anything; any other answer of a
            // service (an outage, an internal error, a code this build does not know) says nothing
            // of whether the request was applied.
            DriveError::Service(e) => match e.azlin_error {
                Some(code) if refuses_up_front(code) => Certainty::Refused,
                _ => Certainty::Unknown,
            },
            DriveError::NotFound { .. }
            | DriveError::Transport(_)
            | DriveError::Io(_)
            | DriveError::Protocol(_)
            | DriveError::Corrupt { .. } => Certainty::Unknown,
        }
    }
}

/// The codes of a node that refuses a request before it touches the drive.
fn refuses_up_front(code: azlin_contract::AzlinError) -> bool {
    use azlin_contract::AzlinError as E;
    matches!(
        code,
        E::QuotaExceeded
            | E::ReadOnlyUnpaid
            | E::ReadOnlyLockdown
            | E::ReadOnlyOperator
            | E::ReadOnlyMoving
            | E::DriveArchived
            | E::DriveBanned
            | E::CredentialsRevoked
            | E::CredentialsStale
            | E::SessionExpired
            | E::ClockSkew
            | E::ObjectTooLarge
            | E::DeletePaused
            | E::ClientTooOld
    )
}
