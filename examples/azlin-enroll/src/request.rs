//! The request: one attempt to get a key onto a new device, and the state machine around it
//! (planning/architecture.md section 7.5 "Enrollment and recovery that an attacker cannot use",
//! section 5.3 `reg_device` (`state` pending, `enrolled_by`, `hold_until`), section 5.9 "The group
//! enforces holds by withholding the wrap until `hold_until`"; build unit S17, lane W1-S17e).
//!
//! A [`Request`] is made only by [`open`]: its fields are private, its hold (`hold_until`) is
//! computed from the policy ONCE, at the opening, and everything else about it changes only
//! through the functions of this crate (`add_evidence`, `contribute`, `cancel`, `mark_released`,
//! `expire`), each of which refuses what the plan forbids and changes nothing when it refuses.
//!
//! ```text
//!   open --> Open --cancel-----------> Cancelled   (any admin, any ACTIVE device of the person)
//!               \--mark_released----> Released     (only with a grant of `release`; answers `Served`)
//!                \--expire---------> Expired      (after the hold: 7 days; own device: 15 minutes)
//! ```
//!
//! THE FACTS. The crate takes plain facts ([`PersonFacts`]) that the glue unit builds from the team
//! state and the registry: it depends on neither, so it stays a leaf. The facts say who the person
//! is, whether the person is managed, which devices are active, who the active admins are and how
//! many admins the account has. `open` judges the opener against them, `cancel` and `contribute`
//! judge who acts, and `mark_released` judges the standing of the opener, of the one who confirmed
//! the phrase and of every escrow holder AGAIN at the moment the key goes out: an approving device
//! that was revoked, or admins that were suspended, since the opening do not finish a request.
//!
//! THE CLOCK. Every function takes the leader's `Stamp` of the call and reads no clock; `open`
//! and `add_evidence`, which fix time-dependent facts (the hold, the age of an attestation), and
//! `release`, `mark_released` and `expire`, which decide on time, also take the `StampCheck` of the
//! leader's stamp and refuse a disputed one. A request never judges itself at a time before it was
//! opened.

use core::fmt;
use std::collections::BTreeSet;

use azlin_policy::{
    quorum_met, required_people, AdminQuorum, DeviceApproval, Field, Policy, QuorumClass,
    QuorumShort, Signatory,
};
use azlin_seams::{hash32, DeviceId, Hash32, MemberId, MemberKind, Span, Stamp};

use crate::cancel::Canceller;
use crate::escrow_open::Contribution;
use crate::evidence::{Evidence, SSO_FRESH};
use crate::hold::{hold_for, life_after};
use crate::limits::{LimitRefusal, OpenLimits};
use crate::notices::{NoticeKind, Notices};
use crate::path::{OpenerKind, Path, PolicyGate};
use crate::release::StampCheck;

/// The longest list of admins or of devices that the facts may hold (a `u8` count: the policy's
/// `max_devices_per_member` is a `u8` too).
///
/// Implements planning/architecture.md section 1.4 rule 4 (everything has a limit, stated up
/// front).
///
/// Caller: `PersonFacts::check`.
pub const MAX_LIST: usize = 255;

/// The most signers of an opening entry that are looked at (the policy's quorum is at most 8).
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 5.10 (`admin_quorum`).
///
/// Caller: `open`.
pub const MAX_SIGNERS: usize = 16;

/// The BLAKE3 `derive_key` context of a request id (a hash context of this crate; the style of
/// the keys family).
///
/// Implements planning/architecture.md section 5.9 (a request is named by a hash of the facts of
/// its opening, so every voter of the group computes the same id).
///
/// Caller: `open`.
pub const REQUEST_CONTEXT: &str = "azlin 2026-11 enrollment request id v1";

/// The id of a request: a hash of the facts of its opening (the person, the new device, the path,
/// the one who asked, the kind of opener, the stamp and the number of the opening in its ledger),
/// so every replica of the group computes the same id without any entropy, and two requests never
/// have the same one.
///
/// Implements planning/architecture.md section 5.9 (the metadata group applies entries
/// deterministically).
///
/// Caller: `open`, `ReleaseGrant`, `OpenLimits`. Seam: S9c/ENROLL-WIRE (the key of the request in
/// the registry).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId(Hash32);

impl RequestId {
    /// The hash that is the id.
    pub const fn as_hash(&self) -> &Hash32 {
        &self.0
    }
}

/// 64 lowercase hex digits.
impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RequestId({})", self.0)
    }
}

/// What is true about the person a request is about, as the glue reads it from the team state and
/// the registry at the moment of the call.
///
/// - `person`: the member the request is about (the subject).
/// - `managed`: the member is managed (the company can open the member's keys through escrow;
///   private and family members never are).
/// - `active_devices`: the member's devices in state active (not pending, not revoked).
/// - `admins`: the owners and admins of the account who are active (a suspended or removed admin is
///   not listed). This is the audience of every notice, the list of who may cancel and the list of
///   who may hold a share of an escrow opening.
/// - `admin_count`: how many owners and admins the account has, suspended ones included, so that a
///   suspended second admin does not make a workspace look like a single-admin workspace. It is
///   never below the number of listed admins.
/// - `escrow_configured`: the account has an escrow key (`escrow_set`).
/// - `offboarded_by`: how many DIFFERENT admins signed the member's offboarding; 0 when the member
///   is not offboarded. Two or more are a quorum, and only a quorum shortens the wait of an escrow
///   opening (section 7.5: "no wait for a member already offboarded by a quorum"), and only for a
///   member who has no active device left (a member with an active device can still object). The
///   glue counts the DIFFERENT admins (a person once, however many devices signed).
///
/// Implements planning/architecture.md section 7.5 and section 7.6 (who may do what, and for whom).
///
/// Caller: `open`, `cancel`, `contribute`, `mark_released`. Seam: TEAMLOG-APPLY (builds it from
/// `TeamState`: R5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PersonFacts {
    /// The member the request is about.
    pub person: MemberId,
    /// Whether the member is managed (escrow-recoverable).
    pub managed: bool,
    /// The member's active devices.
    pub active_devices: Vec<DeviceId>,
    /// The active admins of the account.
    pub admins: Vec<MemberId>,
    /// How many admins the account has, suspended ones included.
    pub admin_count: u8,
    /// Whether the account has an escrow key.
    pub escrow_configured: bool,
    /// How many different admins signed the member's offboarding (0: not offboarded).
    pub offboarded_by: u8,
}

/// Why the facts are refused.
///
/// Implements planning/architecture.md section 1.4 rule 4 and rule 11 (invariants asserted: a
/// fact that cannot be true is an error, never a guess).
///
/// Caller: `open`, `cancel`, `contribute`, `mark_released` (as a part of their refusals).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FactsRefusal {
    /// A list of the facts is longer than [`MAX_LIST`].
    #[error("a list of the facts is longer than {}", MAX_LIST)]
    TooLong,
    /// A member or a device is listed twice.
    #[error("a member or a device is listed twice in the facts")]
    Duplicate,
    /// `admin_count` is below the number of admins listed.
    #[error("admin_count is below the number of listed admins")]
    AdminCountBelowList,
    /// The facts are those of another person than the request.
    #[error("the facts are those of another person than the request")]
    AnotherPerson,
}

/// Whether a slice holds one value twice.
fn has_duplicates<T: Ord + Copy>(items: &[T]) -> bool {
    let mut seen = BTreeSet::new();
    items.iter().any(|item| !seen.insert(*item))
}

impl PersonFacts {
    /// The facts hold together: bounded lists, no repeats, a count that is not below the list.
    /// `open` asks for it; the functions that only look something up in the lists (`cancel`,
    /// `contribute`, `mark_released`) do not, so that a veto is never refused for a mistake of the
    /// bookkeeping.
    pub(crate) fn check(&self) -> Result<(), FactsRefusal> {
        if self.admins.len() > MAX_LIST || self.active_devices.len() > MAX_LIST {
            return Err(FactsRefusal::TooLong);
        }
        if usize::from(self.admin_count) < self.admins.len() {
            return Err(FactsRefusal::AdminCountBelowList);
        }
        if has_duplicates(&self.admins) || has_duplicates(&self.active_devices) {
            return Err(FactsRefusal::Duplicate);
        }
        Ok(())
    }

    /// The facts are the facts of the person of this request (and of nobody else).
    pub(crate) fn check_for(&self, request: &Request) -> Result<(), FactsRefusal> {
        if self.person != request.person() {
            return Err(FactsRefusal::AnotherPerson);
        }
        Ok(())
    }
}

/// Who opens a request, and what the glue knows about them.
///
/// Implements planning/architecture.md section 7.5 (the person's own device approves; recovery
/// from the new device; quorum enrollment and escrow opening by admins; "Helpdesk may only open a
/// recovery request") and section 7.6 (the one who asks).
///
/// Caller: `Opener`, `open`. Seam: TEAMLOG-APPLY and S9c (map the signer of the opening entry).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenedBy {
    /// An ACTIVE device of the person approves the new device (checked against the facts).
    PersonDevice(DeviceId),
    /// The person themself, from the new device (the evidence is added to the request).
    Person,
    /// Admins signed the opening entry: the first is the one who asks, the others co-signed. Each
    /// must be an active admin and a person, and together they must meet the policy's quorum.
    Admins(Vec<Signatory>),
    /// A helpdesk member opens a recovery request for the person (and nothing else).
    Helpdesk(MemberId),
}

impl OpenedBy {
    /// The kind of opener.
    pub fn kind(&self) -> OpenerKind {
        match self {
            OpenedBy::PersonDevice(_) => OpenerKind::PersonDevice,
            OpenedBy::Person => OpenerKind::Person,
            OpenedBy::Admins(_) => OpenerKind::Admins,
            OpenedBy::Helpdesk(_) => OpenerKind::Helpdesk,
        }
    }
}

/// The opener of a request and the device that is to receive the key.
///
/// Implements planning/architecture.md section 7.5 and section 5.3 (`reg_device`).
///
/// Caller: `open`. Seam: TEAMLOG-APPLY and S9c.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opener {
    /// Who opens.
    pub by: OpenedBy,
    /// The device that is to receive the key: the new device of the person, or, for an escrow
    /// opening, the recipient's device.
    pub new_device: DeviceId,
}

impl Opener {
    /// An active device of the person approves `new_device`.
    pub const fn person_device(approver: DeviceId, new_device: DeviceId) -> Opener {
        Opener {
            by: OpenedBy::PersonDevice(approver),
            new_device,
        }
    }

    /// The person opens from `new_device`.
    pub const fn person(new_device: DeviceId) -> Opener {
        Opener {
            by: OpenedBy::Person,
            new_device,
        }
    }

    /// Admins signed the opening entry (the first signer is the one who asks).
    pub fn admins(signers: Vec<Signatory>, new_device: DeviceId) -> Opener {
        Opener {
            by: OpenedBy::Admins(signers),
            new_device,
        }
    }

    /// A helpdesk member opens a recovery request for `new_device`.
    pub const fn helpdesk(member: MemberId, new_device: DeviceId) -> Opener {
        Opener {
            by: OpenedBy::Helpdesk(member),
            new_device,
        }
    }
}

/// Where a request stands.
///
/// Implements planning/architecture.md section 7.5 (a request is open, cancelled, released) and
/// section 5.3 (`reg_device.state`).
///
/// Caller: `Request::state`, `release`, `cancel`. Seam: S9c/ENROLL-WIRE (maps it to the device
/// row), S47b.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestState {
    /// Waiting: evidence and contributions may come, any admin or active device may cancel.
    Open,
    /// Cancelled by an admin or an active device of the person at this stamp: never releases.
    Cancelled {
        /// Who cancelled.
        by: Canceller,
        /// The leader's stamp of the cancel entry.
        at: Stamp,
    },
    /// The key was released at this stamp.
    Released {
        /// The leader's stamp of the release.
        at: Stamp,
    },
    /// Nobody finished the request in time.
    Expired,
}

/// What the request fixed from the policy and the facts at its opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Terms {
    /// How old an SSO attestation may be when it is presented.
    pub(crate) sso_max_age: Span,
    /// The only admin of the account, when it has exactly one.
    pub(crate) sole_admin: Option<MemberId>,
    /// How many different admins the opening entry needed (0 when no admins opened it).
    pub(crate) quorum_need: u8,
}

/// Whether a request still takes part in anything at a given time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Liveness {
    /// Open and not past its expiry.
    Live,
    /// Cancelled, released or expired.
    Closed,
    /// Still open but past its expiry.
    Expired,
}

/// One attempt to get a key onto a new device. See the module.
///
/// Implements planning/architecture.md section 7.5, section 5.3 (`reg_device`: `state`,
/// `enrolled_by`, `hold_until`) and section 12 E2.
///
/// Caller: `open` (the only constructor), every function of the crate. Seam: S9c/ENROLL-WIRE (the
/// group's state holds the requests), S47b.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub(crate) id: RequestId,
    pub(crate) person: MemberId,
    pub(crate) new_device: DeviceId,
    pub(crate) path: Path,
    pub(crate) opened_by: OpenedBy,
    pub(crate) asker: MemberId,
    pub(crate) opened_at: Stamp,
    pub(crate) hold_until: Stamp,
    pub(crate) expires_at: Stamp,
    pub(crate) terms: Terms,
    pub(crate) evidence: Vec<Evidence>,
    pub(crate) contributions: Vec<Contribution>,
    pub(crate) state: RequestState,
}

impl Request {
    /// The id of the request.
    pub fn id(&self) -> RequestId {
        self.id
    }

    /// The person the request is about.
    pub fn person(&self) -> MemberId {
        self.person
    }

    /// The device that is to receive the key.
    pub fn new_device(&self) -> DeviceId {
        self.new_device
    }

    /// The path of the request.
    pub fn path(&self) -> Path {
        self.path
    }

    /// Who opened the request.
    pub fn opened_by(&self) -> &OpenedBy {
        &self.opened_by
    }

    /// The one who asks: the person for a recovery and an own-device approval, the first signer
    /// for an opening by admins.
    pub fn asker(&self) -> MemberId {
        self.asker
    }

    /// The leader's stamp of the opening.
    pub fn opened_at(&self) -> Stamp {
        self.opened_at
    }

    /// The instant the hold ends: fixed when the request was opened, from the policy of that
    /// moment. The wrap is withheld before it, on every path.
    pub fn hold_until(&self) -> Stamp {
        self.hold_until
    }

    /// The instant the request expires if nobody released it: seven days after the hold (fifteen
    /// minutes after the opening for the path without a hold).
    pub fn expires_at(&self) -> Stamp {
        self.expires_at
    }

    /// Where the request stands.
    pub fn state(&self) -> RequestState {
        self.state
    }

    /// Whether the request is open.
    pub fn is_open(&self) -> bool {
        self.state == RequestState::Open
    }

    /// The pieces of evidence the request holds.
    pub fn evidence(&self) -> &[Evidence] {
        &self.evidence
    }

    /// The contributions to an escrow opening the request holds.
    pub fn contributions(&self) -> &[Contribution] {
        &self.contributions
    }

    /// Whether the request holds device evidence that counts: an MDM certificate tied to the
    /// company's inventory (a TPM or Secure Enclave attestation is refused as device evidence).
    pub fn company_device(&self) -> bool {
        self.evidence.iter().any(
            |held| matches!(held, Evidence::Device { kind, .. } if kind.counts_as_company_device()),
        )
    }

    /// Whether the request is open and not past its expiry at `now`.
    pub fn is_pending_at(&self, now: Stamp) -> bool {
        self.state == RequestState::Open && now <= self.expires_at
    }

    /// How old an SSO attestation may be when it is presented to this request.
    pub(crate) fn sso_max_age(&self) -> Span {
        self.terms.sso_max_age
    }

    /// Whether `member` is the only admin of the account.
    pub(crate) fn is_sole_admin(&self, member: MemberId) -> bool {
        self.terms.sole_admin == Some(member)
    }

    /// The time a call is judged at: its stamp, but never before the request was opened.
    pub(crate) fn anchored(&self, now: Stamp) -> Stamp {
        if now < self.opened_at {
            self.opened_at
        } else {
            now
        }
    }

    /// Whether the request takes part in anything at `at`.
    pub(crate) fn liveness(&self, at: Stamp) -> Liveness {
        if self.state != RequestState::Open {
            Liveness::Closed
        } else if at > self.expires_at {
            Liveness::Expired
        } else {
            Liveness::Live
        }
    }
}

/// The requests of any person that are open: what an admin lists when it syncs. No stamp is
/// asked for: the list shows every request that has not been closed, so that a stamp that jumped
/// cannot hide one (the sweep, `expire`, closes the requests whose life is over).
///
/// Implements planning/architecture.md section 12 E2 ("an existing device sees every request and
/// can veto it": a notice that was lost changes nothing, the request is still listed).
///
/// Caller: none yet. Seam: S9c/ENROLL-WIRE (the registry's listing of pending enrollments), S47b.
pub fn pending(requests: &[Request]) -> Vec<&Request> {
    requests
        .iter()
        .filter(|request| request.is_open())
        .collect()
}

/// The requests about this person that are open: what a device of the person lists when it syncs,
/// whatever notices it received. No stamp is asked for (see [`pending`]).
///
/// Implements planning/architecture.md section 12 E2 ("an existing device sees every request and
/// can veto it") and section 5.9 (notices come from the group; the state is what is true).
///
/// Caller: none yet. Seam: S9c/ENROLL-WIRE (the device's resync after a missed notice), S47b.
pub fn pending_for_person(requests: &[Request], person: MemberId) -> Vec<&Request> {
    requests
        .iter()
        .filter(|request| request.person == person && request.is_open())
        .collect()
}

/// Why a request is not opened. A refusal changes nothing, not even the ledger of the limits.
///
/// Implements planning/architecture.md section 7.5, section 7.6 and section 5.10 (each refusal is a
/// rule of the plan).
///
/// Caller: `open`. Seam: S9c/ENROLL-WIRE (maps it to the answer of the entry).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OpenRefusal {
    /// The leader's stamp is disputed: the hold would be fixed on a time nobody vouches for.
    #[error("the leader's stamp is disputed: no request is opened on it")]
    StampDisputed,
    /// The facts are inconsistent.
    #[error(transparent)]
    Facts(#[from] FactsRefusal),
    /// This kind of opener may not open this path.
    #[error("{opener:?} may not open the path {path}")]
    OpenerNotAllowed {
        /// The path.
        path: Path,
        /// The kind of opener.
        opener: OpenerKind,
    },
    /// The approving device is not an active device of the person.
    #[error("the approving device is not an active device of the person")]
    NotAnActiveDevice,
    /// A signer of the opening entry is not an active admin (or is not a person).
    #[error("{member} is not an active admin")]
    NotAnAdmin {
        /// The signer.
        member: MemberId,
    },
    /// The opening entry has no signer.
    #[error("the opening entry has no signer")]
    NoSigners,
    /// The opening entry has more signers than are looked at.
    #[error("the opening entry has more than {} signers", MAX_SIGNERS)]
    TooManySigners,
    /// The person is a signer of the opening of their own request.
    #[error("the person cannot sign the opening of their own request")]
    SubjectIsASigner,
    /// The signers are not enough for the policy's quorum.
    #[error(transparent)]
    QuorumShort(#[from] QuorumShort),
    /// The policy forbids the path.
    #[error("the policy forbids this path ({field})")]
    PolicyForbids {
        /// The policy field that forbids it.
        field: Field,
    },
    /// The member is not managed: there is no escrow path.
    #[error("the member is not managed: there is no escrow path")]
    NotManaged,
    /// The account has no escrow key.
    #[error("the account has no escrow")]
    EscrowNotConfigured,
    /// The new device is already an active device of the person.
    #[error("the device is already an active device of the person")]
    DeviceAlreadyActive,
    /// The member has the most devices the policy allows.
    #[error("the member has {have} devices: the policy allows {max}")]
    DevicesFull {
        /// The active devices.
        have: usize,
        /// The most the policy allows.
        max: u8,
    },
    /// The hold does not fit the stamp (an overflow).
    #[error("the hold does not fit the stamp")]
    TimeOverflow,
    /// A rate limit stops the opening.
    #[error(transparent)]
    Limit(#[from] LimitRefusal),
}

/// The id of a request: a hash of the facts of its opening.
fn request_id(
    person: MemberId,
    new_device: DeviceId,
    path: Path,
    asker: MemberId,
    kind: OpenerKind,
    opened_at: Stamp,
    seq: u64,
) -> RequestId {
    let secs = opened_at.as_secs().to_le_bytes();
    let seq = seq.to_le_bytes();
    RequestId(hash32(
        REQUEST_CONTEXT,
        &[
            person.as_bytes().as_slice(),
            new_device.as_bytes().as_slice(),
            path.as_str().as_bytes(),
            asker.as_bytes().as_slice(),
            kind.as_str().as_bytes(),
            secs.as_slice(),
            seq.as_slice(),
        ],
    ))
}

/// The gate that policy and facts put before a path.
fn check_gate(gate: PolicyGate, facts: &PersonFacts, policy: &Policy) -> Result<(), OpenRefusal> {
    match gate {
        PolicyGate::Always => Ok(()),
        PolicyGate::DeviceApprovalAllowsQuorum => match policy.device_approval {
            DeviceApproval::SelfOrQuorum => Ok(()),
            DeviceApproval::SelfOnly => Err(OpenRefusal::PolicyForbids {
                field: Field::DeviceApproval,
            }),
        },
        PolicyGate::PasskeyRecoveryOn => {
            if policy.passkey_recovery {
                Ok(())
            } else {
                Err(OpenRefusal::PolicyForbids {
                    field: Field::PasskeyRecovery,
                })
            }
        }
        PolicyGate::ManagedMemberWithEscrow => {
            if !facts.managed {
                Err(OpenRefusal::NotManaged)
            } else if !facts.escrow_configured {
                Err(OpenRefusal::EscrowNotConfigured)
            } else {
                Ok(())
            }
        }
    }
}

/// The signers of an opening by admins: each an active admin and a person, none of them the
/// person of the request, together the policy's quorum. The first one is the one who asks. The
/// answer is the one who asks and the number of different admins the entry needed.
///
/// In a workspace with exactly one admin the quorum of an escrow opening is that one admin:
/// section 7.5 allows "the owner plus the offline recovery kit, with the waiting period" there, and
/// a quorum of two people cannot exist. No other path is relaxed.
fn verify_signers(
    signers: &[Signatory],
    facts: &PersonFacts,
    policy: &Policy,
    path: Path,
) -> Result<(MemberId, u8), OpenRefusal> {
    let first = match signers.first() {
        Some(first) => first,
        None => return Err(OpenRefusal::NoSigners),
    };
    if signers.len() > MAX_SIGNERS {
        return Err(OpenRefusal::TooManySigners);
    }
    for signer in signers {
        if signer.kind != MemberKind::Person || !facts.admins.contains(&signer.member) {
            return Err(OpenRefusal::NotAnAdmin {
                member: signer.member,
            });
        }
        if signer.member == facts.person {
            return Err(OpenRefusal::SubjectIsASigner);
        }
    }
    let class = match path {
        Path::QuorumEnrollment => QuorumClass::QuorumEnrollment,
        Path::EscrowOpen => QuorumClass::EscrowOpen,
        _ => QuorumClass::Other,
    };
    let relaxed;
    let effective: &Policy = if path == Path::EscrowOpen && facts.admin_count == 1 {
        let mut single = policy.clone();
        single.admin_quorum = AdminQuorum::ONE;
        relaxed = single;
        &relaxed
    } else {
        policy
    };
    quorum_met(effective, class, signers)?;
    Ok((first.member, required_people(effective, class)))
}

/// Checks the opener against the facts and returns the one who asks and the number of different
/// admins the opening entry needed.
fn verify_opener(
    opener: &Opener,
    facts: &PersonFacts,
    policy: &Policy,
    path: Path,
) -> Result<(MemberId, u8), OpenRefusal> {
    match &opener.by {
        OpenedBy::PersonDevice(approver) => {
            if !facts.active_devices.contains(approver) {
                return Err(OpenRefusal::NotAnActiveDevice);
            }
            Ok((facts.person, 0))
        }
        OpenedBy::Person | OpenedBy::Helpdesk(_) => Ok((facts.person, 0)),
        OpenedBy::Admins(signers) => verify_signers(signers, facts, policy, path),
    }
}

/// The only admin of the account, when it has exactly one (and that one is active).
pub(crate) fn sole_admin(facts: &PersonFacts) -> Option<MemberId> {
    if facts.admin_count != 1 {
        return None;
    }
    match facts.admins.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

/// How old an SSO attestation may be when presented to a request opened under this policy: the
/// fixed freshness of a recovery, or the policy's maximum age when that is shorter.
fn sso_age(policy: &Policy) -> Span {
    match policy.sso_max_age {
        Some(max) if max < SSO_FRESH => max,
        _ => SSO_FRESH,
    }
}

/// Opens a request on a path: checks who opens and what the policy and the facts allow, fixes the
/// hold from the policy NOW (`hold_until = now + hold`, never changed afterwards), counts the
/// opening against the person's limits, and returns the request with the notices the group must
/// emit to every admin and every active device of the person.
///
/// A disputed stamp opens nothing: the hold would be fixed on a time that nobody vouches for (a
/// clock that lags shortens it in real time). Nothing is written (the ledger `limits` included)
/// when it refuses. The guard `enroll_hold_skipped_on_recovery` turns the hold of the recovery
/// paths off (in `hold_for`).
///
/// Implements planning/architecture.md section 7.5 (paths, notice and hold, helpdesk, rate limits
/// per person, device evidence), section 5.10 (`enroll_hold_s`, `device_approval`,
/// `passkey_recovery`, `max_devices_per_member`, `admin_quorum`), section 7.6 (private members are
/// never escrowed), section 2.4 (the leader's stamp is checked) and section 14 D42.
///
/// Caller: none yet. Seam: ENROLL-WIRE (S9c's `device_enroll_begin` and the team log's
/// `device_add`, `escrow_open`), S47b.
pub fn open(
    facts: &PersonFacts,
    policy: &Policy,
    path: Path,
    opener: Opener,
    now: Stamp,
    stamp: StampCheck,
    limits: &mut OpenLimits,
) -> Result<(Request, Notices), OpenRefusal> {
    if stamp != StampCheck::Agreed {
        return Err(OpenRefusal::StampDisputed);
    }
    facts.check()?;
    let rule = path.rule();
    let kind = opener.by.kind();
    if !rule.openers.contains(&kind) {
        return Err(OpenRefusal::OpenerNotAllowed { path, opener: kind });
    }
    let (asker, quorum_need) = verify_opener(&opener, facts, policy, path)?;
    check_gate(rule.policy_gate, facts, policy)?;
    if path.enrolls_a_device() {
        if facts.active_devices.contains(&opener.new_device) {
            return Err(OpenRefusal::DeviceAlreadyActive);
        }
        if facts.active_devices.len() >= usize::from(policy.max_devices_per_member) {
            return Err(OpenRefusal::DevicesFull {
                have: facts.active_devices.len(),
                max: policy.max_devices_per_member,
            });
        }
    }
    let length = hold_for(path, policy, facts);
    let hold_until = now.checked_add(length).ok_or(OpenRefusal::TimeOverflow)?;
    let expires_at = hold_until
        .checked_add(life_after(rule.hold))
        .ok_or(OpenRefusal::TimeOverflow)?;
    let seq = limits.peek_seq(facts.person, kind);
    let id = request_id(facts.person, opener.new_device, path, asker, kind, now, seq);
    let request = Request {
        id,
        person: facts.person,
        new_device: opener.new_device,
        path,
        opened_by: opener.by,
        asker,
        opened_at: now,
        hold_until,
        expires_at,
        terms: Terms {
            sso_max_age: sso_age(policy),
            sole_admin: sole_admin(facts),
            quorum_need,
        },
        evidence: Vec::new(),
        contributions: Vec::new(),
        state: RequestState::Open,
    };
    let notices = Notices::one(NoticeKind::Opened { path }, &request, facts);
    // The last step: the opening is counted only when every other check passed.
    limits.admit(facts.person, kind, id, now, expires_at)?;
    Ok((request, notices))
}
