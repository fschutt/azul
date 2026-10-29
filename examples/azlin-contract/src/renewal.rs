//! A drive's next months and its tier: a renewal's sealed sign-up (months for a drive that
//! exists, no new drive), what a redemption answers and how it is refused (also when it changes
//! the drive's tier), and when an unpaid drive turns read-only and is locked.

use serde::{Deserialize, Serialize};

use crate::archive::ArchiveWire;
use crate::ids::StoredBytes;
use crate::time::{Deadline, DeadlineKind, Rfc3339};

/// After its period ends a drive stays writable this long, then it turns read-only.
pub const WRITABLE_GRACE_SECS: i64 = 14 * 86_400;
/// A read-only drive is kept this long for its files to be copied. Then it is locked: nothing
/// is read or written, its files are moved to one offline copy, and paying again brings them
/// back after some days ([`crate::ArchiveWire`], [`crate::unpaid_promise`]).
pub const READ_ONLY_SECS: i64 = 90 * 86_400;
/// The reminders before each deadline of [`expiry`], in days.
pub const REMIND_BEFORE_DAYS: [i64; 3] = [30, 7, 1];
/// A move to a smaller tier is taken this long before the period ends at the earliest: the paid
/// period keeps the tier it was paid for.
pub const CHANGE_AHEAD_SECS: i64 = 7 * 86_400;

/// A node's HeadBucket header with its signed count of the bytes the bucket holds
/// (`<node id>.<at>.<held>.<signature>`): an app hands it on unread as the redeem's
/// `usage_proof` when it moves its drive to a smaller tier, and the token server takes the
/// smaller tier only when the node's count fits. The app's own figure decides nothing.
pub const USAGE_PROOF_HEADER: &str = "x-azlin-usage-proof";

/// What happens to a drive paid until `period_until` (seconds since 1970) when no month is
/// bought: the period ends, the drive turns read-only, then it is locked and moved to offline
/// storage.
#[must_use]
pub fn expiry(period_until: i64) -> [Deadline; 3] {
    let read_only = period_until + WRITABLE_GRACE_SECS;
    [
        Deadline {
            at: Rfc3339::from_unix(period_until),
            kind: DeadlineKind::PeriodEnd,
        },
        Deadline {
            at: Rfc3339::from_unix(read_only),
            kind: DeadlineKind::ReadOnlyStart,
        },
        Deadline {
            at: Rfc3339::from_unix(read_only + READ_ONLY_SECS),
            kind: DeadlineKind::ReadOnlyEnd,
        },
    ]
}

/// What a sealed sign-up without a drive grants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantKind {
    /// Period tokens only, redeemed on a drive the app has.
    PeriodTokens,
}

/// A checkout's period grant as its sealed sign-up carries it: the months paid, the key they are
/// issued against, and whether a monthly plan adds a month at each payment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantWire {
    pub checkout_id: String,
    pub months: u32,
    /// base64url of 32 bytes: a secret.
    pub issue_key: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub recurring: bool,
}

/// The sealed sign-up of a checkout that made no drive: a renewal under an earlier checkout's
/// price lock (`renewal_of`) or a checkout of period tokens only (none), for a drive the app
/// already has - its next months, or another tier for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenewalSignup {
    pub renewal_of: Option<String>,
    pub grant: GrantKind,
    pub tier: String,
    pub period_tokens: GrantWire,
}

/// What a redemption answers: the drive's new period end, and after a change of tier the tier
/// and the days its remaining period was worth in it. On a locked drive the months count from
/// its old period end, and `archive` says how far that is from bringing it back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedeemAnswer {
    pub period_until: Rfc3339,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credited_days: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive: Option<ArchiveWire>,
}

/// A refused redemption: the `{"error": ...}` body (its `message` is for logs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum RedeemRefusal {
    /// The request does not read (400).
    BadRequest,
    /// The token is for another tier and no change of tier was asked for (400).
    WrongTier,
    /// The token's issuer year is not valid any more (400).
    TokenExpired,
    /// No issuer key for the token's tier and year (400).
    UnknownIssuer,
    /// The token's signature does not verify (400).
    BadToken,
    /// Another drive redeemed the token (409).
    TokenUsed,
    /// A move to a smaller tier waits for the period's end: from `from` on (409). The token is
    /// not spent.
    ChangeNotDue { from: Rfc3339 },
    /// The drive's stored bytes do not fit the smaller tier: `stored` against its `quota` (409).
    /// The token is not spent.
    DoesNotFit {
        stored: StoredBytes,
        quota: StoredBytes,
    },
    /// A move to a smaller tier names the drive's stored bytes (400).
    StoredBytesRequired,
    /// The drive changed meanwhile: asked again, it is answered (503).
    TryAgain,
    /// The drive is locked and nobody proved it is theirs yet: a payment brings it back only to
    /// its owner, so the proof comes first (403). The token is not spent.
    ArchiveProofRequired,
    /// A refusal this build does not know: a newer token server sent it.
    #[serde(other)]
    Unknown,
}

impl RedeemRefusal {
    /// The `error` word (`does_not_fit`).
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            RedeemRefusal::BadRequest => "bad_request",
            RedeemRefusal::WrongTier => "wrong_tier",
            RedeemRefusal::TokenExpired => "token_expired",
            RedeemRefusal::UnknownIssuer => "unknown_issuer",
            RedeemRefusal::BadToken => "bad_token",
            RedeemRefusal::TokenUsed => "token_used",
            RedeemRefusal::ChangeNotDue { .. } => "change_not_due",
            RedeemRefusal::DoesNotFit { .. } => "does_not_fit",
            RedeemRefusal::StoredBytesRequired => "stored_bytes_required",
            RedeemRefusal::TryAgain => "try_again",
            RedeemRefusal::ArchiveProofRequired => "archive_proof_required",
            RedeemRefusal::Unknown => "unknown",
        }
    }

    /// The HTTP status the token server answers it with.
    #[must_use]
    pub fn status(&self) -> u16 {
        match self {
            RedeemRefusal::BadRequest
            | RedeemRefusal::WrongTier
            | RedeemRefusal::TokenExpired
            | RedeemRefusal::UnknownIssuer
            | RedeemRefusal::BadToken
            | RedeemRefusal::StoredBytesRequired => 400,
            RedeemRefusal::TokenUsed
            | RedeemRefusal::ChangeNotDue { .. }
            | RedeemRefusal::DoesNotFit { .. } => 409,
            RedeemRefusal::ArchiveProofRequired => 403,
            RedeemRefusal::TryAgain => 503,
            RedeemRefusal::Unknown => 500,
        }
    }

    /// Whether the token can never buy a month: it is spent, or kept no longer.
    #[must_use]
    pub fn is_useless(&self) -> bool {
        matches!(
            self,
            RedeemRefusal::TokenUsed
                | RedeemRefusal::BadToken
                | RedeemRefusal::TokenExpired
                | RedeemRefusal::UnknownIssuer
        )
    }
}
