//! The refresh of a drive's credentials (`POST /v1/drives/{id}/credentials`): how the token
//! server refuses it, and whether the drive token was spent.

use serde::{Deserialize, Serialize};

use crate::time::Rfc3339;

/// A refused refresh: the `{"error": ...}` body the token server answers with (its `message`
/// is for logs, not part of the contract).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum RefreshRefusal {
    /// No token, a malformed one, or one of no family of this drive (401).
    Unauthorized,
    /// A token rotated away was presented again: it leaked, the family is revoked (401).
    TokenReuse,
    /// The family was revoked, or ran out idle (401).
    CredentialsRevoked,
    /// The device is the one a recovery lockdown hands the drive to, when its notice ends (403).
    LockdownPending { pending_until: Option<Rfc3339> },
    /// The drive is banned and its grace is over (403).
    DriveBanned(BanWire),
    /// The token family is not verified yet (an upgrade); nothing was spent (503).
    NotVerified,
    /// The family changed between the check and the rotation; nothing was spent (503).
    TryAgain,
    /// The token server could not finish (503). `spent: false`: before the token was spent;
    /// missing: it may have been spent - the same token asks again as a lost answer.
    Unavailable {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spent: Option<bool>,
    },
    /// A refusal this build does not know: a newer token server sent it.
    #[serde(other)]
    Unknown,
}

/// What a banned drive's refusal says about its ban.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BanWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ban_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ban_until: Option<Rfc3339>,
}

impl RefreshRefusal {
    /// The HTTP status the token server answers it with.
    #[must_use]
    pub fn status(&self) -> u16 {
        match self {
            RefreshRefusal::Unauthorized
            | RefreshRefusal::TokenReuse
            | RefreshRefusal::CredentialsRevoked => 401,
            RefreshRefusal::LockdownPending { .. } | RefreshRefusal::DriveBanned(_) => 403,
            RefreshRefusal::NotVerified
            | RefreshRefusal::TryAgain
            | RefreshRefusal::Unavailable { .. } => 503,
            RefreshRefusal::Unknown => 500,
        }
    }

    /// Whether the same token may be sent again at once: the token server says it spent
    /// nothing.
    #[must_use]
    pub fn spent_nothing(&self) -> bool {
        matches!(
            self,
            RefreshRefusal::NotVerified
                | RefreshRefusal::TryAgain
                | RefreshRefusal::Unavailable { spent: Some(false) }
        )
    }
}
