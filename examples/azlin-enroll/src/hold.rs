//! The hold of a path: how long the group withholds the wrap, from the policy and the facts
//! (planning/architecture.md section 7.5 "Notice and hold", section 5.10 `enroll_hold_s` and
//! `escrow_open_wait_s`, section 14 D42; build unit S17, lane W1-S17e).
//!
//! "No key is released before its hold ends on any path" (section 12 E2). The length is read from
//! the policy ONCE, when the request is opened, and the instant `hold_until` is stored in the
//! request: a later change of the policy never shortens (or lengthens) a hold that is running.
//!
//! - [`HoldKind::None`]: the person's own device approves; no hold (section 7.5, first row).
//! - [`HoldKind::EnrollHold`]: `enroll_hold_s` of the policy (48 h by default, D42): "any
//!   enrollment without the person's own device".
//! - [`HoldKind::EscrowOpenWait`]: `escrow_open_wait_s` (72 h) for an active member, during which
//!   the person or any other admin can object; NO wait for a member already offboarded by a
//!   quorum. A quorum is at least two different admins ([`OFFBOARDING_QUORUM_MIN`]): one admin is
//!   never a quorum, whatever the policy's own number is, and in a workspace with one admin the
//!   wait is full whatever the facts say (section 7.5, "Single-admin workspaces"). A member who
//!   still has an active device is still an active member: the wait is full for them too.

use azlin_policy::Policy;
use azlin_seams::Span;

use crate::path::Path;
use crate::request::PersonFacts;

/// How long a request lives after its hold ended, when nobody releases it: seven days, then it is
/// `Expired` and the person opens a new one. The plan has no expiry rule: an estimate (drift row
/// of the lane).
///
/// Implements planning/architecture.md section 7.5 (a request that nobody finishes does not stay
/// open for ever; section 1.4 rule 4: everything has a limit).
///
/// Caller: `life_after` (it fixes `expires_at` of a request that waits a hold). Seam: S9c
/// (`sweep_holds` calls `expire`).
pub const EXPIRES_AFTER_HOLD: Span = Span::days(7);

/// How long an own-device request lives: fifteen minutes. The request has no hold, so its whole
/// life is the QR code flow (the new device shows the code, the approving device compares the
/// phrase); a request that nobody finishes frees the person's slot soon and cannot be finished by
/// an approver whose standing was lost days ago. An estimate (drift row of the lane).
///
/// Implements planning/architecture.md section 7.4 (the QR code and the fingerprint phrase are one
/// flow of two devices) and section 1.4 rule 4.
///
/// Caller: `life_after`.
pub const OWN_DEVICE_LIFE: Span = Span::minutes(15);

/// The fewest different admins that make a quorum of an offboarding: two. The policy's own number
/// of a team is one, but one person is never "a quorum" (section 12 E1: a quorum counts distinct
/// people), and an offboarding by one admin leaves the full wait to object.
///
/// Implements planning/architecture.md section 7.5 ("no wait for a member already offboarded by a
/// quorum") and section 12 E1.
///
/// Caller: `hold_length`.
pub const OFFBOARDING_QUORUM_MIN: u8 = 2;

/// The kind of hold a path waits.
///
/// Implements planning/architecture.md section 7.5 (the table of paths) and section 5.10
/// (`enroll_hold_s`, `escrow_open_wait_s`).
///
/// Caller: `PathRule::hold`, `hold_length`, `life_after`. Seam: S47b (reads the hold of each path).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HoldKind {
    /// No hold: the person's own device approves.
    None,
    /// The enrollment hold of the policy.
    EnrollHold,
    /// The escrow-opening wait of the policy.
    EscrowOpenWait,
}

/// The length of a kind of hold under this policy for this person.
///
/// Implements planning/architecture.md section 5.10 (`enroll_hold_s`: 48 h for any enrollment
/// without the person's own device; `escrow_open_wait_s`: 72 h for an active member) and section
/// 7.5 ("no wait for a member already offboarded by a quorum").
///
/// Caller: `hold_for`. Seam: S47b (the oracle computes the same length from the plan).
pub fn hold_length(kind: HoldKind, policy: &Policy, facts: &PersonFacts) -> Span {
    match kind {
        HoldKind::None => Span::ZERO,
        HoldKind::EnrollHold => policy.enroll_hold,
        HoldKind::EscrowOpenWait => {
            // A quorum needs at least two people, and with one admin in the workspace nobody
            // offboarded the member "by a quorum", whatever the fact says. A member who still has
            // an active device is still an active member, whatever the offboarding says: that
            // device can object, so the wait is the full one.
            let no_wait = facts.offboarded_by >= OFFBOARDING_QUORUM_MIN
                && facts.admin_count >= OFFBOARDING_QUORUM_MIN
                && facts.active_devices.is_empty();
            if no_wait {
                Span::ZERO
            } else {
                policy.escrow_open_wait
            }
        }
    }
}

/// The length of the hold of a path: its row of `PATHS` read under the policy and the facts. The
/// guard `enroll_hold_skipped_on_recovery` turns the hold of every recovery path off.
///
/// Implements planning/architecture.md section 7.5 ("The same hold applies to every recovery") and
/// section 14 D42.
///
/// Caller: `open`.
pub fn hold_for(path: Path, policy: &Policy, facts: &PersonFacts) -> Span {
    if path.is_recovery() && azlin_seams::mutant!("enroll_hold_skipped_on_recovery") {
        return Span::ZERO;
    }
    hold_length(path.rule().hold, policy, facts)
}

/// How long a request lives after its hold ended: [`OWN_DEVICE_LIFE`] for the path without a hold,
/// [`EXPIRES_AFTER_HOLD`] for the others.
///
/// Implements planning/architecture.md section 7.5 and section 1.4 rule 4 (a request lives for a
/// bounded time; the plan names no expiry: drift row of the lane).
///
/// Caller: `open`.
pub fn life_after(kind: HoldKind) -> Span {
    match kind {
        HoldKind::None => OWN_DEVICE_LIFE,
        HoldKind::EnrollHold | HoldKind::EscrowOpenWait => EXPIRES_AFTER_HOLD,
    }
}
