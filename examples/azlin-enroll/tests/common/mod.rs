//! Shared helpers of the integration tests: ids and stamps, the two scenes (a personal account and a
//! business), the opener and the complete evidence of each path, the SHIMS that call the crate with
//! an agreed stamp and with the standard facts of the standard scene (so that a test that is not
//! about the stamp or the facts reads as it did), and the plan's path (COMMON-W1R R7).
//!
//! The scenes are the facts the glue would read from the team state: who the person is, which
//! devices are active, who the admins are. Every test builds its own scene, so no test depends on
//! another. A test that IS about the stamp or about other facts calls `azlin_enroll::...` itself.
//!
//! Implements planning/architecture.md section 1.4 rule 2 (a failing seed replays: the property
//! tests draw from `azlin_seams::sim::Rng`) and the plan path rule R7 of COMMON-W1R.

#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

use azlin_enroll::{
    Confirmer, ContactSlot, Contribution, ContributionRefusal, DeviceEvidenceKind, Evidence,
    EvidenceRefusal, KeyAgentChecks, Notices, OpenLimits, OpenRefusal, Opener, Path, PersonFacts,
    ReleaseGrant, ReleaseRefusal, Request, Served, StampCheck,
};
use azlin_policy::{AccountKind, Policy, Signatory};
use azlin_seams::{DeviceId, MemberId, Span, Stamp};

/// The stamp of offset zero: an arbitrary day in 2027.
pub const BASE: u64 = 1_800_000_000;
/// Seconds in a minute.
pub const MINUTE: u64 = 60;
/// Seconds in an hour.
pub const HOUR: u64 = 3_600;
/// Seconds in a day.
pub const DAY: u64 = 86_400;

/// The stamp `offset` seconds after [`BASE`].
pub fn at(offset: u64) -> Stamp {
    Stamp::from_secs(BASE + offset)
}

/// The span of `secs` seconds.
pub fn span(secs: u64) -> Span {
    Span::secs(secs)
}

/// The member with this number.
pub fn member(n: u8) -> MemberId {
    MemberId::from_bytes([n; 16])
}

/// The device with this number.
pub fn device(n: u8) -> DeviceId {
    DeviceId::from_bytes([n; 16])
}

/// The person the requests are about.
pub const PERSON: u8 = 1;
/// The device the key goes to.
pub const NEW_DEVICE: u8 = 100;
/// The key agent's member id.
pub const AGENT: u8 = 200;
/// A helpdesk member.
pub const HELPDESK: u8 = 20;

/// The device the key goes to, as a device.
pub fn nd() -> DeviceId {
    device(NEW_DEVICE)
}

/// The key agent's checks, all passed, for the person and the device of the standard requests.
pub fn all_checks_pass() -> KeyAgentChecks {
    KeyAgentChecks {
        person: member(PERSON),
        device: nd(),
        idp_token: true,
        mdm_certificate: true,
        policy: true,
    }
}

/// The proof of the recovery code, made for the standard new device.
pub fn proof() -> Evidence {
    Evidence::RecoveryCodeProof { device: nd() }
}

/// The share of a contact, released to the standard new device.
pub fn share(slot_number: u8) -> Evidence {
    Evidence::ContactShare {
        slot: slot(slot_number),
        device: nd(),
    }
}

/// A passkey assertion made for the standard new device.
pub fn passkey(user_verified: bool) -> Evidence {
    Evidence::PasskeyAssertion {
        device: nd(),
        user_verified,
    }
}

/// An SSO attestation of `subject` made for the standard new device and issued at `issued_at`.
pub fn sso_of(subject: u8, issued_at: Stamp) -> Evidence {
    Evidence::Sso {
        subject: member(subject),
        device: nd(),
        issued_at,
    }
}

/// Device evidence of the standard new device.
pub fn device_evidence(kind: DeviceEvidenceKind) -> Evidence {
    Evidence::Device { device: nd(), kind }
}

/// Every path that waits a hold, in the order of the table.
pub const HOLD_PATHS: [Path; 6] = [
    Path::QuorumEnrollment,
    Path::RecoveryCode,
    Path::ContactShares,
    Path::PasskeyWrap,
    Path::EscrowRecovery,
    Path::EscrowOpen,
];

/// The hold of a path under the default policy of a business with an active subject, in seconds
/// (the oracle of the tests: 0, 48 hours, 72 hours: written from the plan, not from the crate).
pub fn hold_secs(path: Path) -> u64 {
    match path {
        Path::OwnDevice => 0,
        Path::EscrowOpen => 72 * HOUR,
        _ => 48 * HOUR,
    }
}

/// The facts, the policy and the ledger of one account.
pub struct Scene {
    /// The facts about the person.
    pub facts: PersonFacts,
    /// The account policy.
    pub policy: Policy,
    /// The ledger of the openings.
    pub limits: OpenLimits,
}

impl Scene {
    /// A personal account: the person is their own and only admin, has one active device and no
    /// escrow.
    pub fn personal() -> Scene {
        Scene {
            facts: PersonFacts {
                person: member(PERSON),
                managed: false,
                active_devices: vec![device(1)],
                admins: vec![member(PERSON)],
                admin_count: 1,
                escrow_configured: false,
                offboarded_by: 0,
            },
            policy: Policy::defaults(AccountKind::Personal),
            limits: OpenLimits::standard(),
        }
    }

    /// A family account: like a personal one, under the family defaults.
    pub fn family() -> Scene {
        let mut scene = Scene::personal();
        scene.policy = Policy::defaults(AccountKind::Family);
        scene
    }

    /// A business: the person is a managed member with two active devices, three admins (10, 11,
    /// 12), an escrow, and the business defaults (a quorum of two, the passkey wrap off).
    pub fn business() -> Scene {
        Scene {
            facts: PersonFacts {
                person: member(PERSON),
                managed: true,
                active_devices: vec![device(1), device(2)],
                admins: vec![member(10), member(11), member(12)],
                admin_count: 3,
                escrow_configured: true,
                offboarded_by: 0,
            },
            policy: Policy::defaults(AccountKind::Business),
            limits: OpenLimits::standard(),
        }
    }

    /// A business that also allows the passkey wrap: every path can be opened.
    pub fn full() -> Scene {
        let mut scene = Scene::business();
        scene.policy.passkey_recovery = true;
        scene
    }

    /// The opener a path expects in these scenes.
    pub fn opener(&self, path: Path) -> Opener {
        match path {
            Path::OwnDevice => Opener::person_device(device(1), nd()),
            Path::QuorumEnrollment => Opener::admins(
                vec![Signatory::person(member(11)), Signatory::person(member(12))],
                nd(),
            ),
            Path::RecoveryCode | Path::ContactShares | Path::PasskeyWrap | Path::EscrowRecovery => {
                Opener::person(nd())
            }
            Path::EscrowOpen => Opener::admins(
                vec![Signatory::person(member(10)), Signatory::person(member(11))],
                nd(),
            ),
        }
    }

    /// Opens a request, with the opener the path expects.
    pub fn try_open(&mut self, path: Path, now: Stamp) -> Result<(Request, Notices), OpenRefusal> {
        let opener = self.opener(path);
        open(
            &self.facts,
            &self.policy,
            path,
            opener,
            now,
            &mut self.limits,
        )
    }

    /// Opens a request that must open, with a fresh ledger (so a test that opens many requests does
    /// not meet the limits).
    pub fn open(&mut self, path: Path, now: Stamp) -> Request {
        self.limits = OpenLimits::standard();
        match self.try_open(path, now) {
            Ok((request, _notices)) => request,
            Err(refusal) => panic!("{path} must open: {refusal}"),
        }
    }

    /// Adds every piece of evidence and every contribution the path needs, at `now`.
    pub fn complete(&self, request: &mut Request, now: Stamp) {
        let path = request.path();
        let evidence: Vec<Evidence> = match path {
            Path::OwnDevice => vec![Evidence::PhraseConfirmed(Confirmer::Device(device(1)))],
            Path::QuorumEnrollment => {
                vec![Evidence::PhraseConfirmed(Confirmer::Member(member(11)))]
            }
            Path::RecoveryCode => vec![proof()],
            Path::ContactShares => vec![share(1), share(2)],
            Path::PasskeyWrap => vec![passkey(true)],
            Path::EscrowRecovery => vec![sso_of(PERSON, now)],
            Path::EscrowOpen => Vec::new(),
        };
        for piece in evidence {
            if let Err(refusal) = add_evidence(request, piece, now) {
                panic!("{path}: {piece:?} refused: {refusal}");
            }
        }
        if path.is_escrow() {
            for c in [
                Contribution::key_agent(member(AGENT), all_checks_pass()),
                Contribution::admin(member(11)),
            ] {
                if let Err(refusal) = azlin_enroll::contribute(request, &self.facts, c, now) {
                    panic!("{path}: {c:?} refused: {refusal}");
                }
            }
        }
    }
}

/// The standard facts of the standard scene, for another person: what the shims use.
pub fn standard_facts(person: MemberId) -> PersonFacts {
    let mut facts = Scene::business().facts;
    facts.person = person;
    facts
}

/// `azlin_enroll::open` with an agreed stamp.
pub fn open(
    facts: &PersonFacts,
    policy: &Policy,
    path: Path,
    opener: Opener,
    now: Stamp,
    limits: &mut OpenLimits,
) -> Result<(Request, Notices), OpenRefusal> {
    azlin_enroll::open(facts, policy, path, opener, now, StampCheck::Agreed, limits)
}

/// `azlin_enroll::add_evidence` with an agreed stamp.
pub fn add_evidence(r: &mut Request, e: Evidence, now: Stamp) -> Result<(), EvidenceRefusal> {
    azlin_enroll::add_evidence(r, e, now, StampCheck::Agreed)
}

/// `azlin_enroll::contribute` with the standard facts of the standard scene (for the person of the
/// request).
pub fn contribute(r: &mut Request, c: Contribution, now: Stamp) -> Result<(), ContributionRefusal> {
    let facts = standard_facts(r.person());
    azlin_enroll::contribute(r, &facts, c, now)
}

/// `azlin_enroll::mark_released` at the stamp of the grant, agreed.
pub fn mark_released(
    r: &mut Request,
    facts: &PersonFacts,
    grant: ReleaseGrant,
) -> Result<Served, ReleaseRefusal> {
    let now = grant.at();
    azlin_enroll::mark_released(r, facts, grant, now, StampCheck::Agreed)
}

/// The contact slot with this number.
pub fn slot(n: u8) -> ContactSlot {
    match ContactSlot::new(n) {
        Ok(slot) => slot,
        Err(error) => panic!("{error}"),
    }
}

/// The folder of this crate.
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The plan, by the rule R7: `AZLIN_PLAN_MD`; else (an azul crate)
/// `CARGO_MANIFEST_DIR/../../../azul-apps/planning/architecture.md`. `None` (and a printed
/// `SKIPPED: plan not present`) when the file is absent and `AZLIN_PLAN_MD` is unset; a failure when
/// it is set and the file cannot be read.
pub fn plan_text() -> Option<String> {
    let from_env = std::env::var_os("AZLIN_PLAN_MD");
    let path = match &from_env {
        Some(path) => PathBuf::from(path),
        None => root().join("../../../azul-apps/planning/architecture.md"),
    };
    match fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(error) => {
            assert!(
                from_env.is_none(),
                "AZLIN_PLAN_MD is set but {} cannot be read: {error}",
                path.display()
            );
            eprintln!("SKIPPED: plan not present");
            None
        }
    }
}
