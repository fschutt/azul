//! A locked drive and how its owner gets it back.
//!
//! An unpaid drive stays writable for [`crate::WRITABLE_GRACE_SECS`] after its period, then
//! read-only for [`crate::READ_ONLY_SECS`]. After that it is locked: nothing is read or written
//! any more, and an operator moves its files to offline storage (disks kept in a drawer, not
//! the live servers). Nothing is deleted: the files stay there for [`ARCHIVE_KEEP_SECS`] at the
//! least, and only a person reviews an archive after that.
//!
//! Its owner gets the drive back in two steps, in either order:
//! - **Proof**: the drive holds no name or address of anyone, so the only way to know the
//!   owner is a key only the owner has. The app asks the token server for a challenge
//!   ([`ArchiveChallenge`]) and signs [`archive_proof_message`] with one of the drive's
//!   recovery keys or with the key of one of its devices ([`ArchiveProof`]). No key, no
//!   restore.
//! - **Payment**: the time the drive was kept unpaid, its grace and its read-only time
//!   ([`ARCHIVE_DEBT_SECS`]), is paid first: period tokens redeemed on a locked drive count from
//!   its old period end, and the drive is brought back once its period reaches the day it was
//!   locked ([`ArchiveWire::pay_through`]). Days paid beyond that start when it is back.
//!
//! Then an operator copies the files back from offline storage, which can take some days, and
//! the drive opens again as it was.

use serde::{Deserialize, Serialize};

use crate::renewal::{READ_ONLY_SECS, WRITABLE_GRACE_SECS};
use crate::time::Rfc3339;

/// The time an unpaid drive is kept before it is locked: its writable grace and its read-only
/// time. Its owner pays it before the drive is brought back.
pub const ARCHIVE_DEBT_SECS: i64 = WRITABLE_GRACE_SECS + READ_ONLY_SECS;

/// How long an archive is kept at the least: ten years. No age deletes an archive; after this
/// an operator looks at it.
pub const ARCHIVE_KEEP_SECS: i64 = 10 * 365 * 86_400;

/// How long a challenge of [`ArchiveChallenge`] can be answered.
pub const ARCHIVE_CHALLENGE_SECS: i64 = 300;

/// What the owner's key signs: the drive and the token server's challenge, so a signature
/// proves nothing for another drive or another time.
#[must_use]
pub fn archive_proof_message(drive_id: &str, challenge: &str) -> String {
    format!("archive-restore:{drive_id}:{challenge}")
}

/// What a device signs when it registers its key for a drive: it holds the private half.
#[must_use]
pub fn device_key_message(drive_id: &str, public_key: &str) -> String {
    format!("device-key:{drive_id}:{public_key}")
}

/// Whether a locked drive paid until `period_until` is paid far enough to be brought back:
/// through the day it was locked (`locked_at`). Both in seconds since 1970.
#[must_use]
pub fn paid_through_lock(period_until: i64, locked_at: i64) -> bool {
    period_until >= locked_at
}

/// The drive's period once it is back at `now`: the days paid beyond the day it was locked
/// start now, so its owner pays nothing for the time its files spent in offline storage.
#[must_use]
pub fn period_after_restore(period_until: i64, locked_at: i64, now: i64) -> i64 {
    now + (period_until - locked_at).max(0)
}

/// The months (of [`crate::INSTALMENT_SECS`]) a locked drive paid until `period_until` still
/// needs to reach the day it was locked; 0 when it is paid through.
#[must_use]
pub fn months_owed(period_until: i64, locked_at: i64) -> u32 {
    let owed = locked_at - period_until;
    if owed <= 0 {
        return 0;
    }
    let month = crate::INSTALMENT_SECS;
    u32::try_from((owed + month - 1) / month).unwrap_or(u32::MAX)
}

/// Where a locked drive stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveState {
    /// Nothing is read or written; its files are on their way to offline storage or there.
    Locked,
    /// Its owner proved the drive is theirs and paid: an operator is bringing the files back,
    /// which can take some days.
    RestorePending,
    /// A state this build does not know: a newer token server sent it.
    #[serde(other)]
    Unknown,
}

/// What the token server says about a locked drive: in a refused refresh
/// ([`crate::RefreshRefusal::DriveArchived`]), in the drive's status (`archive`) and in a
/// redemption's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveWire {
    pub state: ArchiveState,
    /// When it was locked.
    pub locked_at: Rfc3339,
    /// The day its period must reach before it is brought back: the day it was locked.
    pub pay_through: Rfc3339,
    /// Its owner proved the drive is theirs.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub proven: bool,
}

/// `POST /v1/drives/{id}/archive/challenge`: what the owner's key signs, good for
/// [`ARCHIVE_CHALLENGE_SECS`] and answered once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveChallenge {
    /// Opaque to the app: it signs it as it is.
    pub challenge: String,
    pub expires_at: Rfc3339,
}

/// Which of the drive's keys signed a proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofKey {
    /// One of its recovery keys (from the recovery code the owner wrote down, or a passkey).
    RecoveryKey,
    /// The key of one of its devices, registered while the drive was open.
    DeviceKey,
    /// A kind this build does not know.
    #[serde(other)]
    Unknown,
}

/// `POST /v1/drives/{id}/archive/proof`: the owner's signature over
/// [`archive_proof_message`] of a challenge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveProof {
    pub challenge: String,
    pub key: ProofKey,
    /// Which key of that kind, when the app knows; else every one of them is tried.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
    /// Ed25519, standard base64.
    pub signature: String,
}

/// A refused proof: the `{"error": ...}` body (its `message` is for logs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum ArchiveRefusal {
    /// The request does not read (400).
    BadRequest,
    /// Not a challenge of this server for this drive, or it ran out (401).
    BadChallenge,
    /// The challenge was answered before: a proof counts once (409).
    ChallengeUsed,
    /// No key of the drive made the signature (401).
    BadSignature,
    /// The drive has no key that could prove whose it is: no recovery key, no device key. It
    /// cannot be brought back (403).
    NoKey,
    /// The drive is not locked (409).
    NotArchived,
    /// The key is not verified yet (an upgrade): ask again later (503).
    NotVerified,
    /// A refusal this build does not know: a newer token server sent it.
    #[serde(other)]
    Unknown,
}

impl ArchiveRefusal {
    /// The `error` word (`bad_signature`).
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            ArchiveRefusal::BadRequest => "bad_request",
            ArchiveRefusal::BadChallenge => "bad_challenge",
            ArchiveRefusal::ChallengeUsed => "challenge_used",
            ArchiveRefusal::BadSignature => "bad_signature",
            ArchiveRefusal::NoKey => "no_key",
            ArchiveRefusal::NotArchived => "not_archived",
            ArchiveRefusal::NotVerified => "not_verified",
            ArchiveRefusal::Unknown => "unknown",
        }
    }

    /// The HTTP status the token server answers it with.
    #[must_use]
    pub fn status(&self) -> u16 {
        match self {
            ArchiveRefusal::BadRequest => 400,
            ArchiveRefusal::BadChallenge | ArchiveRefusal::BadSignature => 401,
            ArchiveRefusal::NoKey => 403,
            ArchiveRefusal::ChallengeUsed | ArchiveRefusal::NotArchived => 409,
            ArchiveRefusal::NotVerified => 503,
            ArchiveRefusal::Unknown => 500,
        }
    }
}

/// `POST /v1/drives/{id}/device_keys`: a device's public key for the drive, registered while
/// the drive is open, and its signature over [`device_key_message`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceKeyRegistration {
    /// Ed25519, 32 bytes in standard base64.
    pub public_key: String,
    pub signature: String,
}

/// The answer to a registration: the key's id, which a proof may name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceKeyAnswer {
    pub key_id: String,
}
