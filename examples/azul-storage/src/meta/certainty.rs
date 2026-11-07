//! What a failed call knows of the change it tried to make (ARCH28 RF44 / AR5a-5): did it happen?
//!
//! A change that was refused for good, or never made, can be undone - an object uploaded for it
//! taken out of the bucket, a log entry written for it removed. A change that may have landed
//! (no answer, a service error) cannot: neither "someone else wrote first" nor "it went through",
//! so nothing is undone on a guess.

use crate::DriveError;

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
