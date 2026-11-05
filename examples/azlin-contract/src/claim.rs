//! The pick-up of a paid drive by its claim ticket (`POST /v1/drives/{id}/claim`, cash by post
//! on another computer than the one that bought it): how the token server refuses it.
//!
//! The two words were sent before this type existed (`claims_used`, `claim_expired`) and the
//! apps compared them as text; they are the contract's now. Added without a golden fixture first
//! (ARCH28 RF51): the fixture and the services' pinned digest move together in one later wave.

use serde::{Deserialize, Serialize};

/// A refused pick-up: the `{"error": ...}` body (its `message` is for logs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum ClaimRefusal {
    /// The drive was picked up as often as it may be (409). No computer can pick it up any
    /// more; the computer that bought it holds its session.
    ClaimsUsed,
    /// The pick-ups of the drive ended 30 days after the first (410).
    ClaimExpired,
    /// A refusal this build does not know: a newer token server sent it.
    #[serde(other)]
    Unknown,
}

impl ClaimRefusal {
    /// Every refusal a token server sends ([`ClaimRefusal::Unknown`] is not one).
    pub const ALL: [ClaimRefusal; 2] = [ClaimRefusal::ClaimsUsed, ClaimRefusal::ClaimExpired];

    /// The `error` word (`claims_used`).
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            ClaimRefusal::ClaimsUsed => "claims_used",
            ClaimRefusal::ClaimExpired => "claim_expired",
            ClaimRefusal::Unknown => "unknown",
        }
    }

    /// The refusal the word `code` names; `None` for one this build does not know.
    #[must_use]
    pub fn parse(code: &str) -> Option<ClaimRefusal> {
        let code = code.trim();
        ClaimRefusal::ALL.into_iter().find(|r| r.as_str() == code)
    }

    /// The HTTP status the token server answers it with.
    #[must_use]
    pub fn status(&self) -> u16 {
        match self {
            ClaimRefusal::ClaimsUsed => 409,
            ClaimRefusal::ClaimExpired => 410,
            ClaimRefusal::Unknown => 500,
        }
    }

    /// Whether asking again can never help: the drive cannot be picked up on this computer.
    /// A refusal this build does not know is not final: an update may read it.
    #[must_use]
    pub fn is_final(&self) -> bool {
        matches!(self, ClaimRefusal::ClaimsUsed | ClaimRefusal::ClaimExpired)
    }
}

impl std::fmt::Display for ClaimRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
