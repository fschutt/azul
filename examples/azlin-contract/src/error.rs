//! The error codes of the Azlin services: a node's `x-azlin-error` header, the token server's
//! `{"error": ...}` body. The client's texts for them are azcloud-kit's `user_errors`, one row
//! per code.

use serde::{Deserialize, Serialize};

/// The token server's code of a request its rate limit refused (429, with `Retry-After` in
/// seconds and `"spent": false`). It was refused before its route ran, so nothing was spent or
/// made: the same request - the same drive token, the same idempotency key - may be sent again
/// once the pause is over.
pub const RATE_LIMITED: &str = "rate_limited";

/// One error code. The order of the variants is their wire index in the services' binary
/// encodings: new codes go at the end, before [`AzlinError::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AzlinError {
    /// No Raft majority, draining, overloaded: retry on another node.
    Unavailable,
    /// A planned upgrade: retry after `Retry-After`.
    Maintenance,
    QuotaExceeded,
    ReadOnlyUnpaid,
    CredentialsRevoked,
    ObjectTooLarge,
    NameConflict,
    LinkBlocked,
    /// The stored bytes could not be verified (double fault); never wrong bytes.
    Integrity,
    /// Deletes are throttled for this drive, or paused by a lockdown.
    DeletePaused,
    /// A bucket on another block: `PermanentRedirect` with the right endpoint.
    WrongBlock,
    Internal,
    /// A banned drive: no writes and no presigned links, nothing after its grace.
    DriveBanned,
    /// The session's credentials ran out (or were minted under a signing epoch that ended): the
    /// client refreshes them with its drive token and goes on; nothing for the user to do.
    SessionExpired,
    /// The request was signed with a clock too far from the node's: nothing works until the
    /// device's clock is set.
    ClockSkew,
    /// The client's cluster map is older than the node's: the client fetches it and sends
    /// again.
    MapStale,
    /// The credentials were minted for an older state of the drive (read-only before a payment
    /// arrived): the client refreshes them and sends again.
    CredentialsStale,
    /// A recovery lockdown waits for its notice: the drive is handed over when it ends.
    LockdownPending,
    /// The client speaks a contract older than the service's minimum: the app must be
    /// updated.
    ClientTooOld,
    /// A change of the recovery keys, or a lockdown's cancel, needs a recovery key's signature.
    RecoveryKeyRequired,
    /// The drive's last recovery key stays.
    LastRecoveryKey,
    /// A recovery lookup's challenge ran out or is not the server's.
    BadChallenge,
    /// A node refuses a write: someone used the drive's recovery code, and the drive takes no
    /// changes until the lockdown's notice ends or an owner's device cancels it.
    ReadOnlyLockdown,
    /// A node refuses a write: the operator made the drive read-only for now.
    ReadOnlyOperator,
    /// The drive was unpaid past its read-only time and is locked: nothing is read or written,
    /// its files are in offline storage. Its owner gets it back by proving it is theirs and
    /// paying; restoring it can take some days.
    DriveArchived,
    /// A node refuses a write: the drive's files are being moved. Its owner's deletes go on;
    /// other writes wait until the move is done.
    ReadOnlyMoving,
    /// A code this build does not know: a newer service sent it. Never sent.
    #[serde(other)]
    Unknown,
}

impl AzlinError {
    /// Every code a service sends ([`AzlinError::Unknown`] is not one).
    pub const ALL: [AzlinError; 26] = [
        AzlinError::Unavailable,
        AzlinError::Maintenance,
        AzlinError::QuotaExceeded,
        AzlinError::ReadOnlyUnpaid,
        AzlinError::CredentialsRevoked,
        AzlinError::ObjectTooLarge,
        AzlinError::NameConflict,
        AzlinError::LinkBlocked,
        AzlinError::Integrity,
        AzlinError::DeletePaused,
        AzlinError::WrongBlock,
        AzlinError::Internal,
        AzlinError::DriveBanned,
        AzlinError::SessionExpired,
        AzlinError::ClockSkew,
        AzlinError::MapStale,
        AzlinError::CredentialsStale,
        AzlinError::LockdownPending,
        AzlinError::ClientTooOld,
        AzlinError::RecoveryKeyRequired,
        AzlinError::LastRecoveryKey,
        AzlinError::BadChallenge,
        AzlinError::ReadOnlyLockdown,
        AzlinError::ReadOnlyOperator,
        AzlinError::DriveArchived,
        AzlinError::ReadOnlyMoving,
    ];

    /// The code as the wire writes it (`quota_exceeded`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AzlinError::Unavailable => "unavailable",
            AzlinError::Maintenance => "maintenance",
            AzlinError::QuotaExceeded => "quota_exceeded",
            AzlinError::ReadOnlyUnpaid => "read_only_unpaid",
            AzlinError::CredentialsRevoked => "credentials_revoked",
            AzlinError::ObjectTooLarge => "object_too_large",
            AzlinError::NameConflict => "name_conflict",
            AzlinError::LinkBlocked => "link_blocked",
            AzlinError::Integrity => "integrity",
            AzlinError::DeletePaused => "delete_paused",
            AzlinError::WrongBlock => "wrong_block",
            AzlinError::Internal => "internal",
            AzlinError::DriveBanned => "drive_banned",
            AzlinError::SessionExpired => "session_expired",
            AzlinError::ClockSkew => "clock_skew",
            AzlinError::MapStale => "map_stale",
            AzlinError::CredentialsStale => "credentials_stale",
            AzlinError::LockdownPending => "lockdown_pending",
            AzlinError::ClientTooOld => "client_too_old",
            AzlinError::RecoveryKeyRequired => "recovery_key_required",
            AzlinError::LastRecoveryKey => "last_recovery_key",
            AzlinError::BadChallenge => "bad_challenge",
            AzlinError::ReadOnlyLockdown => "read_only_lockdown",
            AzlinError::ReadOnlyOperator => "read_only_operator",
            AzlinError::DriveArchived => "drive_archived",
            AzlinError::ReadOnlyMoving => "read_only_moving",
            AzlinError::Unknown => "unknown",
        }
    }

    /// The code `text` names; `None` for one this build does not know.
    #[must_use]
    pub fn parse(text: &str) -> Option<AzlinError> {
        let text = text.trim();
        AzlinError::ALL.into_iter().find(|e| e.as_str() == text)
    }

    /// The code of a received header or body: [`AzlinError::Unknown`] for one this build does
    /// not know.
    #[must_use]
    pub fn from_wire(text: &str) -> AzlinError {
        AzlinError::parse(text).unwrap_or(AzlinError::Unknown)
    }

    /// `(HTTP status, S3 error code)` a node sends it as. The token server's own codes have
    /// the status the token server answers them with.
    #[must_use]
    pub fn s3(self) -> (u16, &'static str) {
        match self {
            AzlinError::Unavailable => (503, "SlowDown"),
            AzlinError::Maintenance => (503, "ServiceUnavailable"),
            AzlinError::QuotaExceeded => (403, "QuotaExceeded"),
            AzlinError::ReadOnlyUnpaid => (403, "AccessDenied"),
            AzlinError::CredentialsRevoked => (403, "InvalidAccessKeyId"),
            AzlinError::ObjectTooLarge => (400, "EntityTooLarge"),
            AzlinError::NameConflict => (412, "PreconditionFailed"),
            AzlinError::LinkBlocked => (403, "AccessDenied"),
            AzlinError::Integrity => (500, "InternalError"),
            AzlinError::DeletePaused => (503, "SlowDown"),
            AzlinError::WrongBlock => (301, "PermanentRedirect"),
            AzlinError::Internal => (500, "InternalError"),
            AzlinError::DriveBanned => (403, "AccessDenied"),
            AzlinError::SessionExpired => (403, "ExpiredToken"),
            AzlinError::ClockSkew => (403, "RequestTimeTooSkewed"),
            AzlinError::MapStale => (409, "AzlinMapStale"),
            AzlinError::CredentialsStale => (403, "AccessDenied"),
            AzlinError::LockdownPending => (403, "AccessDenied"),
            AzlinError::ClientTooOld => (426, "AzlinClientTooOld"),
            AzlinError::RecoveryKeyRequired => (403, "AccessDenied"),
            AzlinError::LastRecoveryKey => (409, "OperationAborted"),
            AzlinError::BadChallenge => (401, "AccessDenied"),
            AzlinError::ReadOnlyLockdown => (403, "AccessDenied"),
            AzlinError::ReadOnlyOperator => (403, "AccessDenied"),
            AzlinError::DriveArchived => (403, "AccessDenied"),
            AzlinError::ReadOnlyMoving => (403, "AccessDenied"),
            AzlinError::Unknown => (500, "InternalError"),
        }
    }

    /// Seconds for `Retry-After`, when the error is transient.
    #[must_use]
    pub fn retry_after(self) -> Option<u32> {
        match self {
            AzlinError::Unavailable => Some(0),
            AzlinError::Maintenance => Some(30),
            AzlinError::DeletePaused => Some(5),
            _ => None,
        }
    }

    /// Whether another try (on another node, after `Retry-After`) may succeed.
    #[must_use]
    pub fn is_transient(self) -> bool {
        self.retry_after().is_some()
    }
}

impl std::fmt::Display for AzlinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
