//! Opening someone's data through escrow: who may contribute and when the opening is complete
//! (planning/architecture.md section 7.6 "Escrow and the key agent", section 7.5 "Opening
//! someone's data through escrow", section 12 E2; build unit S17, lane W1-S17e).
//!
//! THE RULES OF 7.6, as code:
//!
//! - **Every opening needs two different people**: the one who asks (the person in a recovery, an
//!   admin in an `escrow_open`) and an admin holder who contributes, never the same person. The
//!   people are counted by `azlin_policy::distinct_people`, the one count of "distinct people,
//!   never one person's devices" (E1).
//! - **The key agent's share never stands in for a person.** It is a share and no person: it
//!   counts toward the shares (two are needed: threshold decryption, "never in one place") and
//!   never toward the people. The key agent never opens escrow alone.
//! - It contributes only when its own checks all pass: the identity provider's signed ID token
//!   for the person, the MDM inventory certificate, and the policy ([`KeyAgentChecks`]), and the
//!   checks name the person and the device they were made for.
//! - The one who asks never contributes to their own opening, and the person whose data is opened
//!   never contributes to the opening of it.
//! - An admin holder must be an active admin: `contribute` takes the facts of the moment, and the
//!   standing of every holder is judged again when the key goes out (`shortfall_for`).
//! - **Single-admin workspaces** (section 7.5): "the owner plus the offline recovery kit, with the
//!   waiting period". When the sole admin asks, one person suffices, the kit
//!   ([`ContributionKind::OwnerKit`]) must be among the contributions, and the sole admin may
//!   then contribute their own share; the full wait and the notices apply (see `hold`). The kit
//!   exists only for the sole admin of a workspace with exactly one admin: in any other workspace
//!   it is refused, so no single admin opens escrow alone by presenting a kit.
//!
//! This module does not touch cryptography: a [`Contribution`] says that a holder's partial
//! decryption was verified (S17f); the crate decides whether the holders add up to an opening.

use azlin_policy::{distinct_people, Signatory};
use azlin_seams::{DeviceId, MemberId, Stamp};

use crate::path::Path;
use crate::request::{sole_admin, FactsRefusal, Liveness, OpenedBy, PersonFacts, Request};

/// The different people an opening needs: two (section 7.6: "the one who asks and an admin holder
/// who contributes").
///
/// Implements planning/architecture.md section 7.6 ("Every opening needs two different people").
///
/// Caller: `shortfall`.
pub const ESCROW_PEOPLE: u8 = 2;

/// The shares an opening needs at least: two (threshold decryption, "for example 2 of 3"; "never
/// in one place"). The escrow's own threshold, if larger, is enforced by the decryption itself.
///
/// Implements planning/architecture.md section 7.6 ("How it is held: threshold, never in one
/// place").
///
/// Caller: `shortfall`.
pub const ESCROW_MIN_SHARES: usize = 2;

/// The most contributions one request keeps (section 1.4 rule 4).
///
/// Implements planning/architecture.md section 1.4 rule 4 (everything has a limit, stated up
/// front).
///
/// Caller: `contribute`.
pub const MAX_CONTRIBUTIONS: usize = 16;

/// One of the key agent's three checks (section 7.6).
///
/// Implements planning/architecture.md section 7.6 ("the identity provider's own signed ID token
/// for the person ... the MDM inventory certificate and the policy").
///
/// Caller: `KeyAgentChecks::first_failure`, `ContributionRefusal`. Seam: S17g (the checks of the
/// ID token).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyAgentCheck {
    /// The identity provider's own signed ID token for the person, checked against its published
    /// keys (never the identity service's attestation).
    IdpToken,
    /// The MDM inventory certificate of the new device.
    MdmCertificate,
    /// The policy allows the release.
    Policy,
}

/// What the key agent checked before it contributed (S17g's answer): for which person and device,
/// and three yes-or-no facts.
///
/// Implements planning/architecture.md section 7.6 ("it contributes automatically when its checks
/// pass").
///
/// Caller: `ContributionKind::KeyAgent`, `contribute`. Seam: S17g (`azlin-idtoken` fills it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyAgentChecks {
    /// The person the checks were made for.
    pub person: MemberId,
    /// The device the checks were made for (the one that is to receive the key).
    pub device: DeviceId,
    /// The ID token of the identity provider named the person and verified.
    pub idp_token: bool,
    /// The MDM inventory certificate verified.
    pub mdm_certificate: bool,
    /// The policy allowed the release.
    pub policy: bool,
}

impl KeyAgentChecks {
    /// The first check that did not pass, if any.
    pub const fn first_failure(&self) -> Option<KeyAgentCheck> {
        if !self.idp_token {
            Some(KeyAgentCheck::IdpToken)
        } else if !self.mdm_certificate {
            Some(KeyAgentCheck::MdmCertificate)
        } else if !self.policy {
            Some(KeyAgentCheck::Policy)
        } else {
            None
        }
    }
}

/// Whose share a contribution is.
///
/// Implements planning/architecture.md section 7.6 (admin holders, the key agent) and section 7.5
/// (single-admin workspaces: the owner plus the offline recovery kit).
///
/// Caller: `Contribution`, `contribute`, `shortfall`. Seam: S17f (`azlin-escrow` says which kind a
/// verified partial is).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContributionKind {
    /// An admin holder's share: the holder is a person.
    Admin,
    /// The key agent's share, with the outcome of its own checks: a share, never a person.
    KeyAgent(KeyAgentChecks),
    /// The sole admin's offline recovery kit: a share of the one person who holds the kit.
    OwnerKit,
}

/// A verified contribution to an escrow opening: whose it is and what kind.
///
/// Implements planning/architecture.md section 7.6 ("each holder contributes a partial result").
///
/// Caller: `contribute`, `Request::contributions`. Seam: S17f (builds one from a verified
/// partial), S47b.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Contribution {
    /// The member who holds the share: the admin, the owner, or the key agent's own member id.
    pub holder: MemberId,
    /// The kind of share.
    pub kind: ContributionKind,
}

impl Contribution {
    /// An admin holder's contribution.
    pub const fn admin(holder: MemberId) -> Contribution {
        Contribution {
            holder,
            kind: ContributionKind::Admin,
        }
    }

    /// The key agent's contribution (the agent is an app principal with its own member id).
    pub const fn key_agent(agent: MemberId, checks: KeyAgentChecks) -> Contribution {
        Contribution {
            holder: agent,
            kind: ContributionKind::KeyAgent(checks),
        }
    }

    /// The sole admin's recovery-kit contribution.
    pub const fn owner_kit(owner: MemberId) -> Contribution {
        Contribution {
            holder: owner,
            kind: ContributionKind::OwnerKit,
        }
    }
}

/// Why a contribution is refused. A refusal changes nothing.
///
/// Implements planning/architecture.md section 7.6 (the rules of an opening).
///
/// Caller: `contribute`. Seam: S17f/ENROLL-WIRE (maps it to a refusal of the entry).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ContributionRefusal {
    /// The facts are those of another person than the request.
    #[error(transparent)]
    Facts(#[from] FactsRefusal),
    /// The request is cancelled, released or expired.
    #[error("the request is not open any more")]
    NotOpen,
    /// The request is past its expiry.
    #[error("the request has expired")]
    Expired,
    /// The path of the request does not open escrow.
    #[error("the path {0} does not open escrow")]
    NotAnEscrowPath(Path),
    /// The holder of an admin's share or of a kit is not an active admin.
    #[error("{member} is not an active admin: no share of the escrow is theirs to give")]
    NotAnAdmin {
        /// The holder.
        member: MemberId,
    },
    /// The one who asks cannot contribute to their own opening.
    #[error("the one who asks cannot contribute to their own opening")]
    AskerCannotContribute,
    /// The person whose data is opened cannot contribute to the opening.
    #[error("the person whose data is opened cannot contribute to the opening")]
    SubjectCannotContribute,
    /// The helpdesk member who opened the request cannot contribute to it: helpdesk cannot approve
    /// alone.
    #[error("the helpdesk member who opened the request cannot contribute to it")]
    OpenerCannotContribute,
    /// The key agent is an app principal: an active admin or the person is no key agent.
    #[error("the key agent is an app principal, not an admin or the person")]
    AgentIsNoApp,
    /// The key agent's checks were made for another person or device than the request's.
    #[error("the key agent's checks were made for another person or device")]
    AgentCheckedAnother,
    /// One of the key agent's checks did not pass.
    #[error("the key agent's check {0:?} did not pass")]
    KeyAgentCheckFailed(KeyAgentCheck),
    /// The kit belongs to the sole admin of a workspace with one admin, and to nobody else.
    #[error("the recovery kit is the sole admin's: no other workspace has one")]
    KitOnlyForTheSoleAdmin,
    /// The request holds the most contributions it may.
    #[error(
        "the request holds the most contributions it may ({})",
        MAX_CONTRIBUTIONS
    )]
    TooMany,
}

/// What an opening still lacks.
///
/// Implements planning/architecture.md section 7.6 (two different people, threshold shares) and
/// section 7.5 (single-admin workspaces).
///
/// Caller: `shortfall`, `shortfall_for`, `release` (as `EvidenceMissing(EscrowThreshold)`), the
/// standing check of `mark_released`. Seam: the AzWorkspace dialog that says what is still missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortfall {
    /// Fewer different people than the opening needs.
    People {
        /// The different people so far (the one who asks, and the admin holders).
        have: u8,
        /// The different people needed.
        need: u8,
    },
    /// Fewer shares than the opening needs.
    Shares {
        /// The shares so far.
        have: usize,
        /// The shares needed.
        need: usize,
    },
    /// The sole admin asks, and the offline recovery kit is not among the contributions.
    Kit,
}

/// Adds a verified contribution to an open escrow request. A contribution the same as one the
/// request holds is accepted and changes nothing. The request is judged at `now`, never earlier
/// than its own `opened_at`. The facts are those of the moment: the holder of an admin's share or
/// of a kit must be an active admin in them, and the key agent must be neither an admin nor the
/// person.
///
/// Implements planning/architecture.md section 7.6 ("The key agent's share never stands in for a
/// person"; "the one who asks ... and an admin holder who contributes, never the same person").
///
/// Caller: none yet. Seam: ENROLL-WIRE and S17f (the group's `escrow_contribute` handler), S47b.
pub fn contribute(
    r: &mut Request,
    facts: &PersonFacts,
    c: Contribution,
    now: Stamp,
) -> Result<(), ContributionRefusal> {
    facts.check_for(r)?;
    let at = r.anchored(now);
    match r.liveness(at) {
        Liveness::Live => {}
        Liveness::Closed => return Err(ContributionRefusal::NotOpen),
        Liveness::Expired => return Err(ContributionRefusal::Expired),
    }
    if !r.path().is_escrow() {
        return Err(ContributionRefusal::NotAnEscrowPath(r.path()));
    }
    // "Helpdesk ... cannot approve alone": the helpdesk member who opened the request is no holder
    // of it (no admin's share, no kit, not the agent's share either), whatever the glue believes of
    // that member's role.
    if let OpenedBy::Helpdesk(helpdesk) = r.opened_by() {
        if *helpdesk == c.holder {
            return Err(ContributionRefusal::OpenerCannotContribute);
        }
    }
    match c.kind {
        ContributionKind::Admin => {
            if !facts.admins.contains(&c.holder) {
                return Err(ContributionRefusal::NotAnAdmin { member: c.holder });
            }
            // The sole admin of the workspace is the one exception: "the owner plus the offline
            // recovery kit" (section 7.5).
            let sole = r.is_sole_admin(c.holder);
            if c.holder == r.asker() && !sole {
                return Err(ContributionRefusal::AskerCannotContribute);
            }
            if c.holder == r.person() && !sole {
                return Err(ContributionRefusal::SubjectCannotContribute);
            }
        }
        ContributionKind::KeyAgent(checks) => {
            if facts.admins.contains(&c.holder) || c.holder == r.person() {
                return Err(ContributionRefusal::AgentIsNoApp);
            }
            if checks.person != r.person() || checks.device != r.new_device() {
                return Err(ContributionRefusal::AgentCheckedAnother);
            }
            if let Some(failed) = checks.first_failure() {
                return Err(ContributionRefusal::KeyAgentCheckFailed(failed));
            }
        }
        ContributionKind::OwnerKit => {
            if !facts.admins.contains(&c.holder) {
                return Err(ContributionRefusal::NotAnAdmin { member: c.holder });
            }
            if !r.is_sole_admin(c.holder) {
                return Err(ContributionRefusal::KitOnlyForTheSoleAdmin);
            }
        }
    }
    if r.contributions.contains(&c) {
        return Ok(());
    }
    if r.contributions.len() >= MAX_CONTRIBUTIONS {
        return Err(ContributionRefusal::TooMany);
    }
    r.contributions.push(c);
    Ok(())
}

/// The signature of one contribution for the policy's count of people: an admin holder and the
/// holder of a kit as a person, the key agent as an app, which never counts. The guard
/// `enroll_agent_counts_as_person` makes the agent a person.
fn signatory_of(c: &Contribution) -> Signatory {
    match c.kind {
        ContributionKind::Admin | ContributionKind::OwnerKit => Signatory::person(c.holder),
        ContributionKind::KeyAgent(_) => {
            if azlin_seams::mutant!("enroll_agent_counts_as_person") {
                Signatory::person(c.holder)
            } else {
                Signatory::app(c.holder)
            }
        }
    }
}

/// What the opening still lacks when only the contributions for which `stands` is true are
/// counted, the one who asks counts as a person only when `asker_stands`, and `sole` is the sole
/// admin that the exception of the single-admin workspace may name. The one derivation behind
/// `shortfall` and `shortfall_for`.
fn assess(
    r: &Request,
    sole: Option<MemberId>,
    asker_stands: bool,
    stands: &dyn Fn(&Contribution) -> bool,
) -> Option<Shortfall> {
    if !r.path().is_escrow() {
        return None;
    }
    let held: Vec<&Contribution> = r.contributions.iter().filter(|c| stands(c)).collect();
    let sole_admin_asks = sole == Some(r.asker());
    let has_kit = held
        .iter()
        .any(|c| matches!(c.kind, ContributionKind::OwnerKit));
    if sole_admin_asks && !has_kit {
        return Some(Shortfall::Kit);
    }
    let need_people = if sole_admin_asks { 1 } else { ESCROW_PEOPLE };
    let mut signers: Vec<Signatory> = Vec::new();
    if asker_stands {
        signers.push(Signatory::person(r.asker()));
    }
    signers.extend(held.iter().map(|c| signatory_of(c)));
    let people = distinct_people(&signers);
    if people < need_people {
        return Some(Shortfall::People {
            have: people,
            need: need_people,
        });
    }
    let shares = held.len();
    if shares < ESCROW_MIN_SHARES {
        return Some(Shortfall::Shares {
            have: shares,
            need: ESCROW_MIN_SHARES,
        });
    }
    None
}

/// What the escrow opening of this request still lacks, or `None` when it is complete: two
/// different people (one when the sole admin asks) and at least two shares, and, when the sole
/// admin asks, the offline recovery kit. A request of a path that does not open escrow lacks
/// nothing. This counts every contribution the request holds, as the request's own record has it;
/// `shortfall_for` counts only the holders who still stand.
///
/// Implements planning/architecture.md section 7.6 ("Every opening needs two different people";
/// "The key agent's share never stands in for a person") and section 7.5 ("Single-admin
/// workspaces").
///
/// Caller: `satisfied` (the `EscrowThreshold` evidence of `release`). Seam: the AzWorkspace dialog,
/// S47b.
pub fn shortfall(r: &Request) -> Option<Shortfall> {
    assess(r, r.terms.sole_admin, true, &|_| true)
}

/// The same as `shortfall`, but counting only the contributions of the holders who still stand in
/// these facts: an admin or the holder of a kit must still be an active admin (the key agent has
/// no standing of the team's to lose), an admin who asked (an opening that admins signed) counts
/// as a person only while they still are an active admin, and the exception of the single-admin
/// workspace holds only while the workspace still has that one admin.
///
/// Implements planning/architecture.md section 7.6 (the holders are admins) and section 7.5 ("a
/// single admin" only in a workspace with one admin).
///
/// Caller: `mark_released` (the standing of the opening when the key goes out). Seam: the
/// AzWorkspace dialog.
pub fn shortfall_for(r: &Request, facts: &PersonFacts) -> Option<Shortfall> {
    let sole_now = sole_admin(facts);
    let sole = if r.terms.sole_admin == sole_now {
        sole_now
    } else {
        None
    };
    // The person who asks for their own recovery needs no standing of the team's; an admin who
    // asks does.
    let asker_stands = match r.opened_by() {
        OpenedBy::Admins(_) => facts.admins.contains(&r.asker()),
        OpenedBy::PersonDevice(_) | OpenedBy::Person | OpenedBy::Helpdesk(_) => true,
    };
    assess(r, sole, asker_stands, &|c| match c.kind {
        ContributionKind::Admin | ContributionKind::OwnerKit => facts.admins.contains(&c.holder),
        ContributionKind::KeyAgent(_) => true,
    })
}
