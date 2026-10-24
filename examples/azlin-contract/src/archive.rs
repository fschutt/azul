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
//!   owner is a key only the owner has: one of the drive's recovery keys (the recovery code's,
//!   a passkey), or a device key - the key a device of the drive registered while it held the
//!   drive key ([`DeviceKeyRegistration`]). A device key proves ownership for a restore and for
//!   nothing else: it never signs a recovery lockdown or a change of the recovery keys.
//!   The app asks the token server for a challenge ([`ArchiveChallenge`]) and signs its
//!   [`archive_statement`] ([`ArchiveProof`]): the statement names its purpose, the drive, the
//!   server's nonce, the expiry and the paid checkout it unlocks, a signature counts once and
//!   only for exactly that. No key, no restore.
//! - **Payment**: the three months the drive was kept for its owner unpaid ([`RESTORE_OWED_SECS`])
//!   are paid first: period tokens redeemed on a locked drive count from its old period end, and
//!   the drive is brought back once its period reaches [`ArchiveWire::pay_through`], three months
//!   after that end. Days paid beyond it start when the drive is back; the time it spent in
//!   offline storage costs nothing.
//!
//! Then an operator copies the files back from offline storage, which can take some days, and
//! the drive opens again as it was.

use serde::{Deserialize, Serialize};

use crate::renewal::{READ_ONLY_SECS, WRITABLE_GRACE_SECS};
use crate::time::Rfc3339;

/// How long after its period's end an unpaid drive is locked: its writable grace and its
/// read-only time, 104 days.
pub const LOCK_AFTER_SECS: i64 = WRITABLE_GRACE_SECS + READ_ONLY_SECS;

/// What a locked drive's owner pays before it is brought back: three months (of
/// [`crate::INSTALMENT_SECS`]) for the time it was kept for them unpaid.
pub const RESTORE_OWED_SECS: i64 = 3 * crate::period::INSTALMENT_SECS;

/// How long an archive is kept at the least: ten years. No age deletes an archive; after this
/// an operator looks at it.
pub const ARCHIVE_KEEP_SECS: i64 = 10 * 365 * 86_400;

/// How long a challenge of [`ArchiveChallenge`] can be answered.
pub const ARCHIVE_CHALLENGE_SECS: i64 = 300;

/// The first line of an [`archive_statement`]: what the signature is for, and nothing else.
pub const ARCHIVE_STATEMENT_FORMAT: &str = "azlin-archive-restore-v1";

/// The first line of a [`device_key_statement`].
pub const DEVICE_KEY_STATEMENT_FORMAT: &str = "azlin-device-key-v1";

/// What a device signs with its new key when it registers it for a drive: the purpose, the
/// drive and the key, each line ending in a newline. It holds the private half; the statement
/// is good for nothing but this registration.
#[must_use]
pub fn device_key_statement(drive_id: &str, public_key: &str) -> String {
    format!("{DEVICE_KEY_STATEMENT_FORMAT}\ndrive={drive_id}\nkey={public_key}\n")
}

/// What the owner's key signs to get a locked drive back: the purpose, then `key=value` lines in
/// this order, each ending in a newline - the drive, the token server's nonce, the challenge's
/// expiry (seconds since 1970) and the paid checkout the proof unlocks (empty for none). A
/// signature proves nothing for another drive, another challenge, another time or another
/// payment.
#[must_use]
pub fn archive_statement(
    drive_id: &str,
    nonce: &str,
    expires_at: i64,
    checkout_id: Option<&str>,
) -> String {
    format!(
        "{ARCHIVE_STATEMENT_FORMAT}\ndrive={drive_id}\nnonce={nonce}\nexpires={expires_at}\ncheckout={}\n",
        checkout_id.unwrap_or("")
    )
}

/// The day a drive locked at `locked_at` must be paid through before it is brought back:
/// [`RESTORE_OWED_SECS`] after its old period end. Seconds since 1970.
#[must_use]
pub fn pay_through(locked_at: i64) -> i64 {
    locked_at - LOCK_AFTER_SECS + RESTORE_OWED_SECS
}

/// Whether a locked drive paid until `period_until` is paid far enough to be brought back:
/// through `pay_through`. Both in seconds since 1970.
#[must_use]
pub fn paid_through(period_until: i64, pay_through: i64) -> bool {
    period_until >= pay_through
}

/// The drive's period once it is back at `now`: the days paid beyond `pay_through` start now,
/// so its owner pays nothing for the time its files spent in offline storage.
#[must_use]
pub fn period_after_restore(period_until: i64, pay_through: i64, now: i64) -> i64 {
    now + (period_until - pay_through).max(0)
}

/// The months (of [`crate::INSTALMENT_SECS`]) a locked drive paid until `period_until` still
/// needs to reach `pay_through`; 0 when it is paid through.
#[must_use]
pub fn months_owed(period_until: i64, pay_through: i64) -> u32 {
    let owed = pay_through - period_until;
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
    /// The day its period must reach before it is brought back ([`pay_through`]): three months
    /// after its old period end.
    pub pay_through: Rfc3339,
    /// Its owner proved the drive is theirs.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub proven: bool,
}

/// `POST /v1/drives/{id}/archive/challenge`: what the owner's key signs, good for
/// [`ARCHIVE_CHALLENGE_SECS`] and answered once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveChallenge {
    /// Opaque to the app: it is sent back with the proof as it is.
    pub challenge: String,
    /// The server's nonce and the expiry the [`archive_statement`] names.
    pub nonce: String,
    pub expires_at: Rfc3339,
}

/// `POST /v1/drives/{id}/archive/proof`: a recovery key's or a device key's signature over the
/// [`archive_statement`] of a challenge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchiveProof {
    pub challenge: String,
    /// The paid checkout (a renewal of the drive) the proof unlocks: its months not issued yet
    /// pay what the drive owes at once. None: the proof alone; the months come as period tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkout_id: Option<String>,
    /// Which key signed - one of the drive's recovery keys (`rk_...`) or a device key
    /// (`dk_...`) - when the app knows; else every one of them is tried.
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
    /// The checkout the proof names is not paid (409). Nothing was spent.
    CheckoutNotPaid,
    /// The checkout the proof names is of another tier than the drive (409). Nothing was spent.
    CheckoutOtherTier,
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
            ArchiveRefusal::CheckoutNotPaid => "checkout_not_paid",
            ArchiveRefusal::CheckoutOtherTier => "checkout_other_tier",
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
            ArchiveRefusal::ChallengeUsed
            | ArchiveRefusal::NotArchived
            | ArchiveRefusal::CheckoutNotPaid
            | ArchiveRefusal::CheckoutOtherTier => 409,
            ArchiveRefusal::NotVerified => 503,
            ArchiveRefusal::Unknown => 500,
        }
    }
}

/// `POST /v1/drives/{id}/device_keys`: a device's own key for the drive, registered while the
/// drive is open by a device that holds the drive key (at its join, its pairing, an unlock), and
/// its signature over [`device_key_statement`]. It proves ownership for a restore only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceKeyRegistration {
    /// Ed25519, 32 bytes in standard base64.
    pub public_key: String,
    pub signature: String,
}

/// The answer to a registration: the key's id (`dk_...`), which a proof may name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceKeyAnswer {
    pub key_id: String,
}
