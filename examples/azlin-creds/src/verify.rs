//! Who judges time, and how much skew a verifier tolerates (planning/architecture.md section 2.4:
//! "Time is the leader's stamp, checked ... A node whose clock differs from the median by more than 10 s
//! uses no clock for decisions and says so"; DRIFT D-12: "the node's clock, checked against the median,
//! judges, with 30 s of skew; assertions last at most 10 minutes").
//!
//! A credential's `issued_at` and `expires_at` are LEADER stamps; the verifying node has its OWN clock.
//! [`VerifierNow`] is the node's reading with the verdict of its clock check ([`ClockTrust`]): an
//! untrusted clock refuses every verification with its own error, never "valid" and never "expired".
//! The ONE place a leader stamp meets a node time is `crossing`, so a typed clock later (a node time
//! that is not a leader stamp) is a change of this file alone.
//!
//! Implements planning/architecture.md section 2.4 and section 5.11 (credential and assertion
//! lifetimes); the numbers are the defaults of [`VerifyRules`] and move to the signed `node_config`
//! limits (S5's limits table) when that table names them.

use azlin_keys::{Span, UnixTime};
use azlin_regbase::LeaderTime;

use crate::error::Chain;

/// The time a verifying node reads on its own clock (whole seconds since 1970). It is the same type as
/// a leader stamp today; the NAME says which clock a value came from, and `crossing` is the only
/// function that compares the two.
///
/// Caller: `VerifierNow`, S9g (`replica_usable`).
pub type NodeTime = UnixTime;

/// Whether the node's clock can be believed (planning/architecture.md section 2.4: the node compares
/// its clock with the median of its peers').
///
/// Caller: the nodes' clock check (S40b's `timeguard`) sets it; every verifier reads it first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockTrust {
    /// The clock agrees with the median: decisions by time are made.
    Trusted,
    /// The clock differs from the median by more than 10 s: no decision by time is made, and the node
    /// says so.
    Untrusted,
}

/// What a verifier knows about the time: the node's reading and whether to believe it.
///
/// Implements planning/architecture.md section 2.4 (a node with a bad clock decides nothing by time).
///
/// Caller: `verify_credential`, `verify_assertion`; S9g's `check_connection` and `replica_usable`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerifierNow {
    /// The node's clock reading.
    pub time: NodeTime,
    /// Whether the reading may be used.
    pub trust: ClockTrust,
}

impl VerifierNow {
    /// A reading of a clock that agrees with the median.
    pub const fn trusted(time: NodeTime) -> VerifierNow {
        VerifierNow {
            time,
            trust: ClockTrust::Trusted,
        }
    }

    /// A reading of a clock that does NOT agree with the median: every verification refuses it.
    pub const fn untrusted(time: NodeTime) -> VerifierNow {
        VerifierNow {
            time,
            trust: ClockTrust::Untrusted,
        }
    }

    /// The time to decide by, or why there is none.
    pub(crate) fn decide(self) -> Result<NodeTime, Chain> {
        match self.trust {
            ClockTrust::Trusted => Ok(self.time),
            ClockTrust::Untrusted if azlin_seams::mutant!("creds_untrusted_clock_ignored") => {
                Ok(self.time)
            }
            ClockTrust::Untrusted => Err(Chain::ClockUntrusted),
        }
    }
}

/// The numbers a verifier judges by (DRIFT D-12): the skew it tolerates before a credential's
/// `issued_at`, the longest credential it accepts, the longest assertion.
///
/// Implements planning/architecture.md section 5.10 (`credential_ttl_s`: 12 h, 1 h for business) and
/// section 5.11 (assertions expire in "minutes").
///
/// Caller: `verify_credential`, `verify_assertion`, the issuers; the nodes read the numbers from the
/// signed `node_config` once at start (rule 4), S9g and the services pass the default until then.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerifyRules {
    /// How far before `issued_at` a node's clock may read and the credential still be valid (30 s).
    pub skew: Span,
    /// The longest `expires_at - issued_at` of a credential (12 h).
    pub max_credential_lifetime: Span,
    /// The longest `expires - issued` of a member assertion (10 minutes).
    pub max_assertion_lifetime: Span,
}

impl VerifyRules {
    /// 30 s of skew, 12 h of credential, 10 minutes of assertion.
    pub const DEFAULT: VerifyRules = VerifyRules {
        skew: Span::secs(30),
        max_credential_lifetime: Span::hours(12),
        max_assertion_lifetime: Span::minutes(10),
    };
}

impl Default for VerifyRules {
    fn default() -> VerifyRules {
        VerifyRules::DEFAULT
    }
}

/// THE ONE PLACE a leader stamp meets a node clock: the node-clock time of a leader stamp. Both are
/// whole seconds since 1970 today (the stamps are compared as they stand, within the skew of
/// [`VerifyRules`]); a typed clock later changes this function and nothing else.
pub(crate) const fn crossing(t: LeaderTime) -> NodeTime {
    t
}

/// Whether the node's `now` is inside the window `not_before <= now < not_after` (`None` never ends)
/// of two leader stamps.
pub(crate) fn window_holds(
    not_before: LeaderTime,
    not_after: Option<LeaderTime>,
    now: NodeTime,
) -> bool {
    crossing(not_before) <= now && not_after.is_none_or(|end| now < crossing(end))
}

/// The lifetime rules of a signed claim set: `expires` after `issued` (else `Malformed`), and the
/// distance at most `max` (else `LifetimeTooLong`).
pub(crate) fn check_lifetime(
    issued: LeaderTime,
    expires: LeaderTime,
    max: Span,
) -> Result<(), Chain> {
    match expires.elapsed_since(issued) {
        Some(span) if span > Span::ZERO => {
            if span > max {
                Err(Chain::LifetimeTooLong)
            } else {
                Ok(())
            }
        }
        _ => Err(Chain::Malformed),
    }
}

/// The validity of a claim set at the node's `now`: not before `issued - skew` (else `NotYetValid`),
/// and strictly before `expires` (else `Expired`: the expiry second belongs to the expired side).
pub(crate) fn check_times(
    issued: LeaderTime,
    expires: LeaderTime,
    now: NodeTime,
    skew: Span,
) -> Result<(), Chain> {
    if let Some(start) = crossing(issued).checked_sub(skew) {
        if now < start {
            return Err(Chain::NotYetValid);
        }
    }
    let expired = if azlin_seams::mutant!("creds_expiry_inclusive") {
        now > crossing(expires)
    } else {
        now >= crossing(expires)
    };
    if expired {
        return Err(Chain::Expired);
    }
    Ok(())
}
