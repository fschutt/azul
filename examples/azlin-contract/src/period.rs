//! A paid checkout's period tokens (`POST /v1/tokens/issue`): the instalments a prepay's months
//! are issued in, what the issue answers and how it is refused.

use serde::{Deserialize, Serialize};

use crate::time::Rfc3339;

/// A prepay's months are issued one a month from its approval: the k-th (counted from 0) from
/// `approved_at + k * INSTALMENT_SECS` on. So a refund or a chargeback stops the months that
/// were not issued yet, while the drive never waits for one (a month is redeemed a week before
/// the period ends, three weeks after its instalment is due).
pub const INSTALMENT_SECS: i64 = 30 * 86_400;

/// Where a checkout's period tokens stand: of its `months`, `issued` are signed; the next may
/// be asked for from `next_issue_at` on (none: every month is issued).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueSchedule {
    pub months: u32,
    pub issued: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_issue_at: Option<Rfc3339>,
}

impl IssueSchedule {
    /// The schedule of a checkout of `months` approved at `approved_at` (seconds since 1970)
    /// with `issued` of them signed.
    #[must_use]
    pub fn at(months: u32, issued: u32, approved_at: i64) -> IssueSchedule {
        let next_issue_at = (issued < months)
            .then(|| Rfc3339::from_unix(approved_at + i64::from(issued) * INSTALMENT_SECS));
        IssueSchedule {
            months,
            issued,
            next_issue_at,
        }
    }

    /// How many of `months` may be issued in all at `now`: the instalments due since
    /// `approved_at`, the first at once (also for a clock behind the approval).
    #[must_use]
    pub fn due_by(months: u32, approved_at: i64, now: i64) -> u32 {
        let elapsed = (now - approved_at).max(0) / INSTALMENT_SECS;
        let due = u32::try_from(elapsed.saturating_add(1)).unwrap_or(u32::MAX);
        due.min(months)
    }

    /// The schedule of a running monthly plan: like [`IssueSchedule::at`] while a paid month is
    /// not issued; paid up, the next month is asked for when its payment is due (a payment that
    /// is late: a day from `now`), so the plan never reads as complete.
    #[must_use]
    pub fn recurring_at(months: u32, issued: u32, approved_at: i64, now: i64) -> IssueSchedule {
        if issued < months {
            return IssueSchedule::at(months, issued, approved_at);
        }
        let due = approved_at + i64::from(issued) * INSTALMENT_SECS;
        IssueSchedule {
            months,
            issued,
            next_issue_at: Some(Rfc3339::from_unix(due.max(now + 86_400))),
        }
    }

    /// Every month is issued and no more come.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.issued >= self.months && self.next_issue_at.is_none()
    }

    /// Whether the next instalment may be asked for at `now`.
    #[must_use]
    pub fn may_ask(&self, now: i64) -> bool {
        !self.is_complete() && self.next_issue_at.as_ref().is_none_or(|at| at.unix() <= now)
    }
}

/// What a successful issue answers: one blind signature per blinded message, by the issuer key
/// `key_id` (`<tier>/<year>`), and the checkout's schedule after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueAnswer {
    pub tier: String,
    pub key_id: String,
    /// The issuer's public key (SPKI PEM), when the server holds it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_key_pem: Option<String>,
    /// Standard base64, in the order of the blinded messages.
    pub blind_signatures: Vec<String>,
    pub schedule: IssueSchedule,
}

/// A refused issue: the `{"error": ...}` body (its `message` is for logs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum IssueRefusal {
    /// The request does not read: no checkout id, no or too many blinded messages (400).
    BadRequest,
    /// No issue key: the sealed sign-up's is required (400).
    IssueKeyRequired,
    /// No `key_id`: the key the messages are blinded for (400).
    KeyIdRequired,
    /// Not this checkout's issue key (403).
    IssueKeyWrong,
    /// The token server has no such checkout (404).
    NoSuchCheckout,
    /// The checkout is not paid, or its payment went back after the approval (409).
    NotPaid,
    /// The payment mandate was stopped after repeated reversals (409).
    MandateStopped,
    /// Every month of the checkout is issued (409).
    AlreadyIssued,
    /// The next instalment is not due yet: asked again from `schedule.next_issue_at` (409).
    NotDue { schedule: IssueSchedule },
    /// The messages were blinded for a key the server no longer signs with (the year turned):
    /// blind them anew for `key_id` (409). Nothing was signed or counted.
    KeyChanged {
        key_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        public_key_pem: Option<String>,
    },
    /// The server lacks the tier's issuer key for now (503).
    IssuerKeyMissing,
    /// An identical request came at the same moment (503).
    TryAgain,
    /// The server could not finish; nothing was counted (503).
    Unavailable,
    /// A refusal this build does not know: a newer token server sent it.
    #[serde(other)]
    Unknown,
}

impl IssueRefusal {
    /// The `error` word (`not_due`).
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            IssueRefusal::BadRequest => "bad_request",
            IssueRefusal::IssueKeyRequired => "issue_key_required",
            IssueRefusal::KeyIdRequired => "key_id_required",
            IssueRefusal::IssueKeyWrong => "issue_key_wrong",
            IssueRefusal::NoSuchCheckout => "no_such_checkout",
            IssueRefusal::NotPaid => "not_paid",
            IssueRefusal::MandateStopped => "mandate_stopped",
            IssueRefusal::AlreadyIssued => "already_issued",
            IssueRefusal::NotDue { .. } => "not_due",
            IssueRefusal::KeyChanged { .. } => "key_changed",
            IssueRefusal::IssuerKeyMissing => "issuer_key_missing",
            IssueRefusal::TryAgain => "try_again",
            IssueRefusal::Unavailable => "unavailable",
            IssueRefusal::Unknown => "unknown",
        }
    }

    /// The HTTP status the token server answers it with.
    #[must_use]
    pub fn status(&self) -> u16 {
        match self {
            IssueRefusal::BadRequest | IssueRefusal::IssueKeyRequired | IssueRefusal::KeyIdRequired => {
                400
            }
            IssueRefusal::IssueKeyWrong => 403,
            IssueRefusal::NoSuchCheckout => 404,
            IssueRefusal::NotPaid
            | IssueRefusal::MandateStopped
            | IssueRefusal::AlreadyIssued
            | IssueRefusal::NotDue { .. }
            | IssueRefusal::KeyChanged { .. } => 409,
            IssueRefusal::IssuerKeyMissing | IssueRefusal::TryAgain | IssueRefusal::Unavailable => 503,
            IssueRefusal::Unknown => 500,
        }
    }

    /// Whether asking again can never help: the grant's months that are not issued are lost to
    /// this checkout (a reversed payment, a wrong key, a checkout gone, every month issued). A
    /// refusal this build does not know is not final: an update may read it.
    #[must_use]
    pub fn is_final(&self) -> bool {
        matches!(
            self,
            IssueRefusal::BadRequest
                | IssueRefusal::IssueKeyRequired
                | IssueRefusal::KeyIdRequired
                | IssueRefusal::IssueKeyWrong
                | IssueRefusal::NoSuchCheckout
                | IssueRefusal::NotPaid
                | IssueRefusal::MandateStopped
                | IssueRefusal::AlreadyIssued
        )
    }
}
