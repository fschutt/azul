//! The quorum: how many DIFFERENT PEOPLE a change needs, and whether the signatures at hand are
//! enough. "A quorum counts distinct people, never one person's devices" (planning/architecture.md
//! section 12, E1: "no entry is accepted unless its signer may sign it; a quorum counts distinct
//! people, never one person's devices").
//!
//! The classes of change that the policy's `admin_quorum` guards are the five of section 5.10
//! ("business: 2 for admin changes, policy, escrow, escrow_open, quorum enrollments"); every other
//! change needs one person (the signer). The signer counts toward the quorum with the cosigners
//! (`cosigs`, section 5.9: "further admin signatures for a quorum"): the caller hands in the signer
//! and the cosigners as ONE list of [`Signatory`]s, one entry for each signature, and the count is
//! the number of different people in it. Devices never reach this file: the caller names the MEMBER
//! of each signing device, so five devices of one person are five entries with one member and count
//! once, and an app (a member of kind `App`) never counts.
//!
//! Which op is which class (an "admin change" is a role to or from admin or owner, the removal or
//! suspension of an admin or owner, a delegation, a device added by quorum) is the authority table
//! of the team log (lane S17c1); this file names the classes and counts.
//!
//! Implements planning/architecture.md section 5.10 (row `admin_quorum`), section 5.9 ("`policy_set`,
//! `escrow_set`, `audit_key_set`, `admin_quorum_set` | admin, with the policy's quorum";
//! "`escrow_open` | quorum"; `cosigs`) and section 12 (E1).

use std::collections::BTreeSet;

use azlin_seams::MemberId;

use crate::fields::Policy;
use crate::kind::MemberKind;

/// The class of a change, for the quorum: the five classes of planning/architecture.md section 5.10
/// that `admin_quorum` guards, and `Other` for every change that needs one person only.
///
/// Implements planning/architecture.md section 5.10 (row `admin_quorum`: "admin changes, policy,
/// escrow, escrow_open, quorum enrollments").
///
/// Caller: `required_people`, `quorum_met`; Seam: S17c1 (every op of the authority table names its
/// class), S17e (`escrow_open` and quorum enrollments).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QuorumClass {
    /// A change of an admin or an owner: a role to or from admin or owner, the removal or
    /// suspension of one, a delegation.
    AdminChange,
    /// A change of policy (`policy_set`, `audit_key_set`, `admin_quorum_set`).
    PolicyChange,
    /// A change of escrow (`escrow_set`, a managed member made private).
    EscrowChange,
    /// The opening of a member's escrow (`escrow_open`).
    EscrowOpen,
    /// A device enrolled by a quorum (`device_add` without the person's own device).
    QuorumEnrollment,
    /// Any other change: one person, the signer.
    Other,
}

words!(
    QuorumClass,
    "quorum class",
    [
        AdminChange => "admin_change",
        PolicyChange => "policy_change",
        EscrowChange => "escrow_change",
        EscrowOpen => "escrow_open",
        QuorumEnrollment => "quorum_enrollment",
        Other => "other",
    ]
);

/// How many different people a change of this class needs under this policy: the policy's
/// `admin_quorum` for the five guarded classes (1 for personal, family and team accounts, 2 for a
/// business), and 1 for `Other`. Never 0.
///
/// Implements planning/architecture.md section 5.10 (row `admin_quorum`) and section 5.9
/// ("with the policy's quorum").
///
/// Caller: `quorum_met`; Seam: S17c1 (`azlin-teamlog` `QuorumShort{have, need}`), S17e
/// (`azlin-enroll` asks for the people of an escrow opening).
pub fn required_people(p: &Policy, class: QuorumClass) -> u8 {
    if azlin_seams::mutant!("policy_quorum_needs_one") {
        return 1;
    }
    match class {
        QuorumClass::AdminChange
        | QuorumClass::PolicyChange
        | QuorumClass::EscrowChange
        | QuorumClass::EscrowOpen
        | QuorumClass::QuorumEnrollment => p.admin_quorum.get(),
        QuorumClass::Other => 1,
    }
}

/// One signature of an entry, by the MEMBER who made it (never the device): the member and what it
/// is. The caller maps the signing device to its member before it asks.
///
/// Implements planning/architecture.md section 12 (E1: "a quorum counts distinct people, never one
/// person's devices") and section 5.3 (`reg_member.kind`: person or app).
///
/// Caller: `distinct_people`, `quorum_met`; Seam: S17c1 (the signer's and the cosigners' members),
/// S17e (the holders who contribute to an escrow opening).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Signatory {
    /// The member whose device made the signature.
    pub member: MemberId,
    /// A person, or an app.
    pub kind: MemberKind,
}

impl Signatory {
    /// A signature of a person.
    ///
    /// Implements planning/architecture.md section 12 (E1).
    ///
    /// Seam: S17c1, S17e, S47a (the stories build signatories).
    pub const fn person(member: MemberId) -> Signatory {
        Signatory {
            member,
            kind: MemberKind::Person,
        }
    }

    /// A signature of an app (an app principal: it never counts toward a quorum).
    ///
    /// Implements planning/architecture.md section 12 (E1) and section 5.3 (`reg_member.kind`).
    ///
    /// Seam: S17c1, S47a (the stories build signatories).
    pub const fn app(member: MemberId) -> Signatory {
        Signatory {
            member,
            kind: MemberKind::App,
        }
    }
}

/// The number of different people among the signatures: each member counts once however many of its
/// devices signed, and a member of kind `App` never counts. A member id that is listed as a person
/// in one entry and as an app in another is no consistent fact of the registry, so it counts for
/// nobody (a quorum is never over-counted). The count stops at 255: more than that is 255.
///
/// Implements planning/architecture.md section 12 (E1: "a quorum counts distinct people, never one
/// person's devices").
///
/// Caller: `quorum_met`; Seam: S17e (`azlin-enroll` counts the holders of an escrow opening).
pub fn distinct_people(signers: &[Signatory]) -> u8 {
    if azlin_seams::mutant!("policy_quorum_counts_devices") {
        return saturating_u8(signers.len());
    }
    let mut people = BTreeSet::new();
    let mut apps = BTreeSet::new();
    for signer in signers {
        let is_person = match signer.kind {
            MemberKind::Person => true,
            MemberKind::App => azlin_seams::mutant!("policy_quorum_counts_apps"),
        };
        if is_person {
            people.insert(signer.member);
        } else {
            apps.insert(signer.member);
        }
    }
    saturating_u8(people.difference(&apps).count())
}

/// `count` as a `u8`, at most 255 (a saturating conversion: a count above 255 is 255).
fn saturating_u8(count: usize) -> u8 {
    u8::try_from(count).unwrap_or(u8::MAX)
}

/// The signatures are not enough: how many different people there are, and how many are needed.
///
/// Implements planning/architecture.md section 5.9 ("The metadata group rejects an entry with a
/// reason for ... a missing quorum").
///
/// Caller: `quorum_met`; Seam: S17c1 (`Refusal::QuorumShort{have, need}`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("quorum short: {have} different people signed, {need} are needed")]
pub struct QuorumShort {
    /// The number of different people among the signatures.
    pub have: u8,
    /// The number of different people the change needs.
    pub need: u8,
}

/// Whether the signatures are enough for a change of this class under this policy: the number of
/// different people among them is at least [`required_people`].
///
/// Implements planning/architecture.md section 5.9 ("with the policy's quorum", "a missing quorum")
/// and section 12 (E1).
///
/// Seam: S17c1 (`azlin-teamlog` checks every entry that a quorum guards), S16b (`azlin-spaces`
/// proposals that ask for a quorum).
pub fn quorum_met(
    p: &Policy,
    class: QuorumClass,
    signers: &[Signatory],
) -> Result<(), QuorumShort> {
    let need = required_people(p, class);
    let have = distinct_people(signers);
    let enough = if azlin_seams::mutant!("policy_quorum_off_by_one") {
        have > need
    } else {
        have >= need
    };
    if enough {
        Ok(())
    } else {
        Err(QuorumShort { have, need })
    }
}
