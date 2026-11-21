//! Check 9 of the brief: the property of the whole machine (planning/architecture.md section 12
//! E2): over 10,000 seeded random orders of {open, evidence, contribute, cancel, a change of the
//! facts, tick, jump} (and serve, expire and release queries), `release` is never before
//! `hold_until`, never after a cancel, never on a disputed stamp, and only with complete evidence;
//! and the key goes out (`mark_released`) only when everybody the request depends on still stands.
//!
//! THE MODEL IS INDEPENDENT of the crate: it is a small oracle written from the plan and the check
//! list of the brief (hold lengths 0, 48 and 72 hours, the life of a request, the evidence of each
//! path and the device it is for, two different people and two shares for an escrow, who may cancel,
//! who must still stand when the key goes out), and it trusts nothing but what the production code
//! ACCEPTED: after every call the model predicts whether the call must be accepted and compares it
//! with the answer, and at every release query it computes whether the release must be granted, and
//! why not. A failing seed is shrunk with `azlin_seams::sim::minimize` and printed.
//!
//! Half of the seeds begin with a STORY (a request on a path, the right evidence, the right
//! contributions, maybe a change of the facts, the hold, a release), so that the paths that need
//! many right steps in a row are granted often enough for the run not to be vacuous.

mod common;

use std::collections::BTreeSet;

use azlin_enroll::{
    add_evidence, cancel, contribute, expire, mark_released, open, pending_for_person, release,
    Canceller, Confirmer, Contribution, ContributionKind, DeviceEvidenceKind, Evidence,
    KeyAgentChecks, OpenLimits, Path, Reason, Release, ReleaseRefusal, RequestState, StampCheck,
    MAX_CONTRIBUTIONS, MAX_EVIDENCE, PATHS,
};
use azlin_seams::sim::{minimize, Rng};
use azlin_seams::{DeviceId, MemberId};
use common::{
    at, device, hold_secs, member, nd, slot, Scene, AGENT, BASE, DAY, HOUR, NEW_DEVICE, PERSON,
};

const SEEDS: u64 = 10_000;
const EVENTS: usize = 40;
/// A device that a fact is wrongly verified for.
const OTHER_DEVICE: u8 = 77;

/// A piece of evidence as the generator makes it: the age of an attestation is relative to the
/// clock of the run, and every piece names the device it was verified for.
#[derive(Clone, Copy, Debug)]
enum Piece {
    Phrase(Confirmer),
    Sso { subject: u8, age: i64, on: u8 },
    RecoveryCode { on: u8 },
    Share { number: u8, on: u8 },
    Passkey { user_verified: bool, on: u8 },
    Device { kind: DeviceEvidenceKind, on: u8 },
}

/// A change of the facts of the person: a device revoked or added back, an admin suspended or
/// reinstated.
#[derive(Clone, Copy, Debug)]
enum Change {
    RevokeDevice(u8),
    AddDevice(u8),
    SuspendAdmin(u8),
    ReinstateAdmin(u8),
}

#[derive(Clone, Debug)]
enum Event {
    Open(Path, StampCheck),
    Evidence(Piece, StampCheck),
    Contribute(Contribution),
    Cancel(Canceller),
    Facts(Change),
    Serve,
    Expire(StampCheck),
    Tick(u64),
    Back(u64),
    Ahead(u64),
    Release(StampCheck),
}

fn pick<T: Clone>(rng: &mut Rng, items: &[T]) -> T {
    rng.pick(items).expect("a non-empty list").clone()
}

fn chance(rng: &mut Rng, percent: u32) -> bool {
    rng.chance(percent, 100)
}

/// A stamp check: agreed nearly always.
fn agreed_mostly(rng: &mut Rng) -> StampCheck {
    if chance(rng, 92) {
        StampCheck::Agreed
    } else {
        StampCheck::Disputed
    }
}

fn all_pass() -> KeyAgentChecks {
    KeyAgentChecks {
        person: member(PERSON),
        device: nd(),
        idp_token: true,
        mdm_certificate: true,
        policy: true,
    }
}

fn checks(rng: &mut Rng) -> KeyAgentChecks {
    if chance(rng, 85) {
        all_pass()
    } else {
        KeyAgentChecks {
            person: if chance(rng, 90) {
                member(PERSON)
            } else {
                member(2)
            },
            device: if chance(rng, 90) {
                nd()
            } else {
                device(OTHER_DEVICE)
            },
            idp_token: chance(rng, 50),
            mdm_certificate: chance(rng, 50),
            policy: chance(rng, 50),
        }
    }
}

/// A piece that is right for the path (made for the new device).
fn good_piece(path: Path, rng: &mut Rng) -> Piece {
    match path {
        Path::OwnDevice => Piece::Phrase(Confirmer::Device(device(1))),
        Path::QuorumEnrollment => Piece::Phrase(Confirmer::Member(member(11))),
        Path::RecoveryCode => Piece::RecoveryCode { on: NEW_DEVICE },
        Path::ContactShares => Piece::Share {
            number: 1 + u8::try_from(rng.below(3).expect("3")).expect("small"),
            on: NEW_DEVICE,
        },
        Path::PasskeyWrap => Piece::Passkey {
            user_verified: true,
            on: NEW_DEVICE,
        },
        Path::EscrowRecovery | Path::EscrowOpen => Piece::Sso {
            subject: PERSON,
            age: i64::try_from(rng.below(300).expect("300")).expect("small"),
            on: NEW_DEVICE,
        },
    }
}

/// Any piece, right or wrong, for the right device or (rarely) for another.
fn any_piece(rng: &mut Rng) -> Piece {
    let on = if chance(rng, 88) {
        NEW_DEVICE
    } else {
        OTHER_DEVICE
    };
    match rng.below(9).expect("9") {
        0 => Piece::Phrase(Confirmer::Device(pick(
            rng,
            &[device(1), device(2), device(NEW_DEVICE)],
        ))),
        1 => Piece::Phrase(Confirmer::Member(pick(
            rng,
            &[member(10), member(11), member(12), member(PERSON)],
        ))),
        2 => Piece::Sso {
            subject: pick(rng, &[PERSON, PERSON, 2, 11]),
            age: i64::try_from(rng.below(1_000).expect("1000")).expect("small") - 100,
            on,
        },
        3 => Piece::RecoveryCode { on },
        4 => Piece::Share {
            number: 1 + u8::try_from(rng.below(3).expect("3")).expect("small"),
            on,
        },
        5 => Piece::Passkey {
            user_verified: chance(rng, 70),
            on,
        },
        6 => Piece::Device {
            kind: DeviceEvidenceKind::MdmCertificate,
            on,
        },
        7 => Piece::Device {
            kind: DeviceEvidenceKind::TpmAttestation,
            on,
        },
        _ => Piece::Device {
            kind: DeviceEvidenceKind::SecureEnclaveAttestation,
            on,
        },
    }
}

fn any_contribution(rng: &mut Rng) -> Contribution {
    match rng.below(4).expect("4") {
        0 => Contribution::admin(member(pick(rng, &[PERSON, 10, 11, 12, 13]))),
        1 => Contribution::admin(member(pick(rng, &[11, 12]))),
        2 => Contribution::key_agent(member(pick(rng, &[AGENT, 201, 11])), checks(rng)),
        _ => Contribution::owner_kit(member(pick(rng, &[10, 11, PERSON]))),
    }
}

fn any_canceller(rng: &mut Rng) -> Canceller {
    match rng.below(7).expect("7") {
        0 | 1 => Canceller::Admin(member(pick(rng, &[10, 11, 12, 13, 30]))),
        2 | 3 => Canceller::PersonDevice(device(pick(rng, &[1, 2, 100, 77]))),
        4 => Canceller::Helpdesk(member(20)),
        _ => Canceller::PendingDevice(device(pick(rng, &[100, 101]))),
    }
}

fn any_change(rng: &mut Rng) -> Change {
    match rng.below(4).expect("4") {
        0 => Change::RevokeDevice(pick(rng, &[1, 1, 2])),
        1 => Change::AddDevice(pick(rng, &[1, 2])),
        2 => Change::SuspendAdmin(pick(rng, &[10, 11, 11, 12])),
        _ => Change::ReinstateAdmin(pick(rng, &[10, 11, 12])),
    }
}

/// One event of the random walk; `path` is the path of the request the walk is about.
fn any_event(rng: &mut Rng, path: &mut Path) -> Event {
    let roll = rng.below(100).expect("100");
    match roll {
        0..=9 => {
            *path = pick(rng, &Path::ALL);
            let check = if chance(rng, 95) {
                StampCheck::Agreed
            } else {
                StampCheck::Disputed
            };
            Event::Open(*path, check)
        }
        10..=33 => {
            let check = agreed_mostly(rng);
            if chance(rng, 70) {
                Event::Evidence(good_piece(*path, rng), check)
            } else {
                Event::Evidence(any_piece(rng), check)
            }
        }
        34..=48 => {
            if chance(rng, 60) {
                let holder = pick(rng, &[11u8, 12]);
                if chance(rng, 60) {
                    Event::Contribute(Contribution::admin(member(holder)))
                } else {
                    Event::Contribute(Contribution::key_agent(member(AGENT), all_pass()))
                }
            } else {
                Event::Contribute(any_contribution(rng))
            }
        }
        49..=53 => Event::Cancel(any_canceller(rng)),
        54..=57 => Event::Facts(any_change(rng)),
        58..=61 => Event::Serve,
        62..=64 => Event::Expire(if chance(rng, 80) {
            StampCheck::Agreed
        } else {
            StampCheck::Disputed
        }),
        65..=75 => Event::Tick(pick(
            rng,
            &[1, 60, HOUR, 12 * HOUR, 48 * HOUR, 72 * HOUR, DAY, 3 * DAY],
        )),
        76..=81 => Event::Back(pick(rng, &[1, 600, DAY, 10 * DAY])),
        82..=84 => Event::Ahead(pick(rng, &[DAY, 10 * DAY, 365 * DAY])),
        _ => Event::Release(if chance(rng, 75) {
            StampCheck::Agreed
        } else {
            StampCheck::Disputed
        }),
    }
}

/// A story: a request on a path, its right evidence and contributions, maybe a change of the facts,
/// some noise, the end of the hold, and a release.
fn story(rng: &mut Rng, path: &mut Path) -> Vec<Event> {
    *path = pick(rng, &Path::ALL);
    let mut events = vec![Event::Open(*path, StampCheck::Agreed)];
    if *path == Path::ContactShares {
        for number in [1, 2] {
            events.push(Event::Evidence(
                Piece::Share {
                    number,
                    on: NEW_DEVICE,
                },
                StampCheck::Agreed,
            ));
        }
    } else {
        events.push(Event::Evidence(good_piece(*path, rng), StampCheck::Agreed));
    }
    if path.is_escrow() {
        events.push(Event::Contribute(Contribution::admin(member(11))));
        events.push(Event::Contribute(Contribution::key_agent(
            member(AGENT),
            all_pass(),
        )));
    }
    if chance(rng, 50) {
        events.push(Event::Facts(any_change(rng)));
    }
    for _ in 0..rng.below(4).expect("4") {
        events.push(any_event(rng, path));
    }
    events.push(Event::Tick(hold_secs(*path) + pick(rng, &[0, 1, HOUR])));
    events.push(if chance(rng, 70) {
        Event::Serve
    } else {
        Event::Release(StampCheck::Agreed)
    });
    events
}

fn generate(seed: u64) -> Vec<Event> {
    let mut rng = Rng::new(seed);
    let mut path = Path::RecoveryCode;
    let mut events = if seed % 2 == 0 {
        story(&mut rng, &mut path)
    } else {
        Vec::new()
    };
    while events.len() < EVENTS {
        events.push(any_event(&mut rng, &mut path));
    }
    events
}

/// The facts of the person as the oracle knows them.
struct World {
    devices: BTreeSet<DeviceId>,
    admins: BTreeSet<MemberId>,
}

impl World {
    fn new() -> World {
        World {
            devices: [1, 2].into_iter().map(device).collect(),
            admins: [10, 11, 12].into_iter().map(member).collect(),
        }
    }

    /// Whether the opener of the path (the one of the scenes) still stands: the approving device is
    /// active, the signers are active admins.
    fn allows_opening(&self, path: Path) -> bool {
        match path {
            Path::OwnDevice => self.devices.contains(&device(1)),
            Path::QuorumEnrollment => {
                self.admins.contains(&member(11)) && self.admins.contains(&member(12))
            }
            Path::EscrowOpen => {
                self.admins.contains(&member(10)) && self.admins.contains(&member(11))
            }
            _ => true,
        }
    }
}

/// Applies a change to the facts of the scene and to the oracle's.
fn apply(change: Change, scene: &mut Scene, world: &mut World) {
    match change {
        Change::RevokeDevice(n) => {
            scene.facts.active_devices.retain(|d| *d != device(n));
            world.devices.remove(&device(n));
        }
        Change::AddDevice(n) => {
            if world.devices.insert(device(n)) {
                scene.facts.active_devices.push(device(n));
            }
        }
        Change::SuspendAdmin(n) => {
            scene.facts.admins.retain(|a| *a != member(n));
            world.admins.remove(&member(n));
        }
        Change::ReinstateAdmin(n) => {
            if world.admins.insert(member(n)) {
                scene.facts.admins.push(member(n));
            }
        }
    }
}

/// What the oracle says a release query must answer.
#[derive(Debug)]
enum Expected {
    Grant,
    Withheld(&'static str),
    AlreadyReleased(u64),
}

/// What the oracle knows of the request: only what it accepted.
struct Model {
    path: Path,
    /// The one who asks: the person, or the first admin who signed.
    asker: MemberId,
    /// The admins who signed the opening entry.
    signers: Vec<MemberId>,
    /// How many different admins the opening needed.
    quorum_need: usize,
    /// The device that approved (an own-device request).
    approver: Option<DeviceId>,
    opened_at: u64,
    hold_until: u64,
    expires_at: u64,
    cancelled: bool,
    released: Option<u64>,
    expired: bool,
    evidence: Vec<Evidence>,
    contributions: Vec<Contribution>,
}

/// The device a fact was verified for, if it names one.
fn device_of(piece: &Evidence) -> Option<DeviceId> {
    match piece {
        Evidence::PhraseConfirmed(_) => None,
        Evidence::Sso { device, .. }
        | Evidence::RecoveryCodeProof { device }
        | Evidence::ContactShare { device, .. }
        | Evidence::PasskeyAssertion { device, .. }
        | Evidence::Device { device, .. } => Some(*device),
    }
}

impl Model {
    /// A request opened on `path` at `now` by the opener of the scenes.
    fn opened(path: Path, now: u64) -> Model {
        let hold = hold_secs(path);
        // The own-device flow lives fifteen minutes; every other request seven days after its hold.
        let life = if path == Path::OwnDevice {
            15 * 60
        } else {
            7 * DAY
        };
        let (asker, signers, quorum_need, approver) = match path {
            Path::OwnDevice => (member(PERSON), Vec::new(), 0, Some(device(1))),
            Path::QuorumEnrollment => (member(11), vec![member(11), member(12)], 2, None),
            Path::EscrowOpen => (member(10), vec![member(10), member(11)], 2, None),
            _ => (member(PERSON), Vec::new(), 0, None),
        };
        Model {
            path,
            asker,
            signers,
            quorum_need,
            approver,
            opened_at: now,
            hold_until: now + hold,
            expires_at: now + hold + life,
            cancelled: false,
            released: None,
            expired: false,
            evidence: Vec::new(),
            contributions: Vec::new(),
        }
    }

    fn eff(&self, now: u64) -> u64 {
        now.max(self.opened_at)
    }

    fn open_state(&self) -> bool {
        !self.cancelled && self.released.is_none() && !self.expired
    }

    fn live(&self, now: u64) -> bool {
        self.open_state() && self.eff(now) <= self.expires_at
    }

    fn accepts_evidence(&self, now: u64, piece: &Evidence, check: StampCheck) -> bool {
        if check != StampCheck::Agreed || !self.live(now) {
            return false;
        }
        if let Some(on) = device_of(piece) {
            if on != nd() {
                return false;
            }
        }
        let eff = self.eff(now);
        match piece {
            Evidence::PhraseConfirmed(who) => match (self.path, who) {
                (Path::OwnDevice, Confirmer::Device(d)) => *d == device(1),
                (Path::QuorumEnrollment, Confirmer::Member(m)) => {
                    *m == member(11) || *m == member(12)
                }
                _ => false,
            },
            Evidence::Sso {
                subject, issued_at, ..
            } => {
                let issued = issued_at.as_secs() - BASE;
                self.path == Path::EscrowRecovery
                    && *subject == member(PERSON)
                    && issued <= eff
                    && eff - issued <= 600
            }
            Evidence::RecoveryCodeProof { .. } => self.path == Path::RecoveryCode,
            Evidence::ContactShare { .. } => self.path == Path::ContactShares,
            Evidence::PasskeyAssertion { user_verified, .. } => {
                self.path == Path::PasskeyWrap && *user_verified
            }
            Evidence::Device { kind, .. } => *kind == DeviceEvidenceKind::MdmCertificate,
        }
    }

    fn accepts_contribution(&self, now: u64, c: &Contribution, world: &World) -> bool {
        if !self.live(now) || !self.path.is_escrow() {
            return false;
        }
        match c.kind {
            ContributionKind::Admin => {
                world.admins.contains(&c.holder)
                    && c.holder != self.asker
                    && c.holder != member(PERSON)
            }
            ContributionKind::KeyAgent(k) => {
                !world.admins.contains(&c.holder)
                    && c.holder != member(PERSON)
                    && k.person == member(PERSON)
                    && k.device == nd()
                    && k.idp_token
                    && k.mdm_certificate
                    && k.policy
            }
            // Three admins: no sole admin, no kit.
            ContributionKind::OwnerKit => false,
        }
    }

    fn allows_cancel(&self, by: &Canceller, world: &World) -> bool {
        self.open_state()
            && match by {
                Canceller::Admin(m) => world.admins.contains(m),
                Canceller::PersonDevice(d) => world.devices.contains(d),
                Canceller::Helpdesk(_) | Canceller::PendingDevice(_) => false,
            }
    }

    /// Two different people and two shares: the people are the one who asks and the admins who
    /// contributed, the shares every contribution. With `world`, only the admins who still are
    /// active count (the key agent has no standing of the team's to lose). A member that is listed
    /// both as a person (an admin's share) and as an app (the key agent) is no consistent fact of
    /// the registry and counts for nobody: the people are never over-counted (the rule of the
    /// policy's count of distinct people, written here from its documentation).
    fn escrow_complete(&self, world: Option<&World>) -> bool {
        let stands = |c: &Contribution| match (world, c.kind) {
            (Some(w), ContributionKind::Admin) => w.admins.contains(&c.holder),
            _ => true,
        };
        let held: Vec<&Contribution> = self.contributions.iter().filter(|c| stands(*c)).collect();
        let mut people: BTreeSet<MemberId> = BTreeSet::new();
        let mut apps: BTreeSet<MemberId> = BTreeSet::new();
        // An admin who asked (an opening that admins signed) is a person only while they still are
        // an active admin; the person who asks for their own recovery needs no standing.
        let asker_stands = match (world, self.path) {
            (Some(w), Path::EscrowOpen) => w.admins.contains(&self.asker),
            _ => true,
        };
        if asker_stands {
            people.insert(self.asker);
        }
        for c in &held {
            match c.kind {
                ContributionKind::Admin => {
                    people.insert(c.holder);
                }
                ContributionKind::KeyAgent(_) => {
                    apps.insert(c.holder);
                }
                // Three admins: no sole admin, no kit: the kit is never accepted.
                ContributionKind::OwnerKit => {}
            }
        }
        people.difference(&apps).count() >= 2 && held.len() >= 2
    }

    fn evidence_complete(&self) -> bool {
        let has = |test: fn(&Evidence) -> bool| self.evidence.iter().any(test);
        match self.path {
            Path::OwnDevice | Path::QuorumEnrollment => {
                has(|e| matches!(e, Evidence::PhraseConfirmed(_)))
            }
            Path::RecoveryCode => has(|e| matches!(e, Evidence::RecoveryCodeProof { .. })),
            Path::ContactShares => {
                let slots: BTreeSet<_> = self
                    .evidence
                    .iter()
                    .filter_map(|e| match e {
                        Evidence::ContactShare { slot, .. } => Some(slot.get()),
                        _ => None,
                    })
                    .collect();
                slots.len() >= 2
            }
            Path::PasskeyWrap => has(|e| matches!(e, Evidence::PasskeyAssertion { .. })),
            Path::EscrowRecovery => {
                has(|e| matches!(e, Evidence::Sso { .. })) && self.escrow_complete(None)
            }
            Path::EscrowOpen => self.escrow_complete(None),
        }
    }

    /// What `release` must answer.
    fn expected_answer(&self, now: u64, check: StampCheck) -> Expected {
        if self.cancelled {
            Expected::Withheld("cancelled")
        } else if self.expired {
            Expected::Withheld("expired")
        } else if let Some(when) = self.released {
            Expected::AlreadyReleased(when)
        } else if check != StampCheck::Agreed {
            Expected::Withheld("disputed")
        } else if now > self.expires_at {
            Expected::Withheld("expired")
        } else if now < self.hold_until {
            Expected::Withheld("hold")
        } else if !self.evidence_complete() {
            Expected::Withheld("evidence")
        } else {
            Expected::Grant
        }
    }

    /// Whether everybody the request depends on still stands: the device that approved, the admins
    /// who signed (enough of them), the one who confirmed the phrase, the holders of the escrow.
    fn standing_ok(&self, world: &World) -> bool {
        let approver_ok = match self.approver {
            Some(d) => world.devices.contains(&d),
            None => true,
        };
        let standing_signers: BTreeSet<MemberId> = self
            .signers
            .iter()
            .filter(|s| world.admins.contains(*s))
            .copied()
            .collect();
        let signers_ok = standing_signers.len() >= self.quorum_need;
        let phrase_ok = match self.path {
            Path::OwnDevice | Path::QuorumEnrollment => self.evidence.iter().any(|e| match e {
                Evidence::PhraseConfirmed(Confirmer::Device(d)) => world.devices.contains(d),
                Evidence::PhraseConfirmed(Confirmer::Member(m)) => world.admins.contains(m),
                _ => false,
            }),
            _ => true,
        };
        let escrow_ok = !self.path.is_escrow() || self.escrow_complete(Some(world));
        approver_ok && signers_ok && phrase_ok && escrow_ok
    }
}

fn class_of(reason: &Reason) -> &'static str {
    match reason {
        Reason::Cancelled => "cancelled",
        Reason::Expired => "expired",
        Reason::StampDisputed => "disputed",
        Reason::HoldNotElapsed { .. } => "hold",
        Reason::EvidenceMissing(_) => "evidence",
    }
}

/// Whether the crate's answer is the oracle's.
fn verdict(answer: &Release, expected: &Expected) -> Result<(), String> {
    match (answer, expected) {
        (Release::Granted(_), Expected::Grant) => Ok(()),
        (Release::Withheld(reason), Expected::Withheld(class)) => {
            if class_of(reason) == *class {
                Ok(())
            } else {
                Err(format!("withheld for {reason:?}, the model says {class}"))
            }
        }
        (Release::AlreadyReleased { at: got }, Expected::AlreadyReleased(want)) => {
            if *got == at(*want) {
                Ok(())
            } else {
                Err(format!("released at {got:?}, the model says +{want} s"))
            }
        }
        (answer, expected) => Err(format!(
            "the crate says {answer:?}, the model says {expected:?}"
        )),
    }
}

fn realize(piece: &Piece, now: u64) -> Evidence {
    match *piece {
        Piece::Phrase(who) => Evidence::PhraseConfirmed(who),
        Piece::Sso { subject, age, on } => {
            let issued = (i64::try_from(now).expect("small") - age).max(0);
            Evidence::Sso {
                subject: member(subject),
                device: device(on),
                issued_at: at(u64::try_from(issued).expect("not negative")),
            }
        }
        Piece::RecoveryCode { on } => Evidence::RecoveryCodeProof { device: device(on) },
        Piece::Share { number, on } => Evidence::ContactShare {
            slot: slot(number),
            device: device(on),
        },
        Piece::Passkey { user_verified, on } => Evidence::PasskeyAssertion {
            device: device(on),
            user_verified,
        },
        Piece::Device { kind, on } => Evidence::Device {
            device: device(on),
            kind,
        },
    }
}

#[derive(Debug, Default)]
struct Stats {
    granted_by_path: [u32; 7],
    withheld: u32,
    already_released: u32,
    cancelled: u32,
    evidence_accepted: u32,
    evidence_refused: u32,
    contributions_accepted: u32,
    contributions_refused: u32,
    served: u32,
    standing_lost: u32,
    expired: u32,
    opened: u32,
    open_refused: u32,
    facts_changes: u32,
}

fn path_index(path: Path) -> usize {
    Path::ALL.iter().position(|p| *p == path).expect("a path")
}

/// Runs the events against the crate and the model; the first disagreement is the error.
fn run(events: &[Event], stats: &mut Stats) -> Result<(), String> {
    let mut scene = Scene::full();
    let mut world = World::new();
    let mut request: Option<azlin_enroll::Request> = None;
    let mut model: Option<Model> = None;
    let mut now: u64 = 0;
    for (step, event) in events.iter().enumerate() {
        // The clock at the start of the step (a copy: the step itself may move the clock).
        let clock = now;
        let here = |what: &str| format!("step {step} ({event:?}) at +{clock} s: {what}");
        match event {
            Event::Tick(secs) => now += secs,
            Event::Ahead(secs) => now += secs,
            Event::Back(secs) => now = now.saturating_sub(*secs),
            Event::Facts(change) => {
                apply(*change, &mut scene, &mut world);
                stats.facts_changes += 1;
            }
            Event::Open(path, check) => {
                if let Some(m) = &model {
                    if m.open_state() {
                        continue;
                    }
                }
                scene.limits = OpenLimits::standard();
                let predicted = *check == StampCheck::Agreed && world.allows_opening(*path);
                let opener = scene.opener(*path);
                let opened = open(
                    &scene.facts,
                    &scene.policy,
                    *path,
                    opener,
                    at(now),
                    *check,
                    &mut scene.limits,
                );
                match (opened, predicted) {
                    (Ok((r, notices)), true) => {
                        let m = Model::opened(*path, now);
                        if r.hold_until() != at(m.hold_until) {
                            return Err(here("the hold of the request is not the plan's"));
                        }
                        if r.expires_at() != at(m.expires_at) {
                            return Err(here("the life of the request is not the model's"));
                        }
                        if r.opened_at() != at(now) || !r.is_open() {
                            return Err(here("a new request is open and opened now"));
                        }
                        if notices.admins() != scene.facts.admins
                            || notices.devices() != scene.facts.active_devices
                        {
                            return Err(here(
                                "the notices do not go to every admin and every device",
                            ));
                        }
                        model = Some(m);
                        request = Some(r);
                        stats.opened += 1;
                    }
                    (Err(_), false) => stats.open_refused += 1,
                    (Ok(_), false) => {
                        return Err(here("a request was opened, the model refuses the opening"));
                    }
                    (Err(refusal), true) => {
                        return Err(here(&format!("a refused opening: {refusal}")));
                    }
                }
            }
            Event::Evidence(piece, check) => {
                let (Some(r), Some(m)) = (request.as_mut(), model.as_mut()) else {
                    continue;
                };
                let evidence = realize(piece, now);
                let predicted = m.accepts_evidence(now, &evidence, *check);
                let result = add_evidence(r, evidence, at(now), *check);
                if result.is_ok() != predicted {
                    return Err(here(&format!(
                        "evidence {evidence:?}: the crate says {result:?}, the model says {predicted}"
                    )));
                }
                if result.is_ok() {
                    let again = m.evidence.contains(&evidence);
                    let second_of_a_kind = m.evidence.iter().any(|e| e.kind() == evidence.kind())
                        && !matches!(evidence, Evidence::ContactShare { .. });
                    if !again && !second_of_a_kind {
                        m.evidence.push(evidence);
                    }
                    stats.evidence_accepted += 1;
                } else {
                    stats.evidence_refused += 1;
                }
            }
            Event::Contribute(c) => {
                let (Some(r), Some(m)) = (request.as_mut(), model.as_mut()) else {
                    continue;
                };
                let predicted = m.accepts_contribution(now, c, &world);
                let result = contribute(r, &scene.facts, *c, at(now));
                if result.is_ok() != predicted {
                    return Err(here(&format!(
                        "contribution {c:?}: the crate says {result:?}, the model says {predicted}"
                    )));
                }
                if result.is_ok() {
                    if !m.contributions.contains(c) {
                        m.contributions.push(*c);
                    }
                    stats.contributions_accepted += 1;
                } else {
                    stats.contributions_refused += 1;
                }
            }
            Event::Cancel(by) => {
                let (Some(r), Some(m)) = (request.as_mut(), model.as_mut()) else {
                    continue;
                };
                let predicted = m.allows_cancel(by, &world);
                let result = cancel(r, &scene.facts, *by, at(now));
                if result.is_ok() != predicted {
                    return Err(here(&format!(
                        "cancel by {by:?}: the crate says {result:?}, the model says {predicted}"
                    )));
                }
                if result.is_ok() {
                    m.cancelled = true;
                    stats.cancelled += 1;
                }
            }
            Event::Serve => {
                let (Some(r), Some(m)) = (request.as_mut(), model.as_mut()) else {
                    continue;
                };
                let answer = release(r, at(now), StampCheck::Agreed);
                let expected = m.expected_answer(now, StampCheck::Agreed);
                verdict(&answer, &expected).map_err(|what| here(&what))?;
                if let Release::Granted(grant) = answer {
                    let taken = mark_released(r, &scene.facts, grant, at(now), StampCheck::Agreed);
                    match (taken, m.standing_ok(&world)) {
                        (Ok(served), true) => {
                            if served.notices().len() != 1 || served.new_device() != nd() {
                                return Err(here("a release makes one notice"));
                            }
                            m.released = Some(now);
                            stats.served += 1;
                        }
                        (Err(ReleaseRefusal::NoLongerStands(_)), false) => {
                            stats.standing_lost += 1;
                        }
                        (taken, stands) => {
                            return Err(here(&format!(
                                "mark_released says {taken:?}, the model says the standing is {stands}"
                            )));
                        }
                    }
                }
            }
            Event::Expire(check) => {
                let (Some(r), Some(m)) = (request.as_mut(), model.as_mut()) else {
                    continue;
                };
                let predicted =
                    *check == StampCheck::Agreed && m.open_state() && now > m.expires_at;
                let got = expire(r, at(now), *check);
                if got != predicted {
                    return Err(here(&format!(
                        "expire: the crate says {got}, the model says {predicted}"
                    )));
                }
                if got {
                    m.expired = true;
                    stats.expired += 1;
                }
            }
            Event::Release(check) => {
                let (Some(r), Some(m)) = (request.as_ref(), model.as_ref()) else {
                    continue;
                };
                let answer = release(r, at(now), *check);
                let expected = m.expected_answer(now, *check);
                verdict(&answer, &expected).map_err(|what| here(&what))?;
                match &answer {
                    Release::Granted(_) => {
                        // The laws of E2, stated directly on the crate's own answer.
                        if r.hold_until() > at(now) {
                            return Err(here("released before hold_until"));
                        }
                        if matches!(r.state(), RequestState::Cancelled { .. }) || m.cancelled {
                            return Err(here("released after a cancel"));
                        }
                        if *check != StampCheck::Agreed {
                            return Err(here("released on a disputed stamp"));
                        }
                        if !m.evidence_complete() {
                            return Err(here("released without complete evidence"));
                        }
                        stats.granted_by_path[path_index(m.path)] += 1;
                    }
                    Release::Withheld(_) => stats.withheld += 1,
                    Release::AlreadyReleased { .. } => stats.already_released += 1,
                }
            }
        }
        // The invariants of every state.
        if let (Some(r), Some(m)) = (&request, &model) {
            if r.hold_until() != at(m.hold_until)
                || r.opened_at() != at(m.opened_at)
                || r.expires_at() != at(m.expires_at)
            {
                return Err(here(
                    "the hold, the opening or the life of a request changed",
                ));
            }
            if r.evidence().len() > MAX_EVIDENCE || r.contributions().len() > MAX_CONTRIBUTIONS {
                return Err(here("a list grew past its bound"));
            }
            let stored_cancelled = matches!(r.state(), RequestState::Cancelled { .. });
            let stored_released = matches!(r.state(), RequestState::Released { .. });
            let stored_expired = r.state() == RequestState::Expired;
            if stored_cancelled != m.cancelled
                || stored_released != m.released.is_some()
                || stored_expired != m.expired
            {
                return Err(here("the state of the request is not the model's"));
            }
            // The listing of a device that missed every notice: exactly the open requests.
            let listed = pending_for_person(std::slice::from_ref(r), member(PERSON)).len();
            if listed != usize::from(m.open_state()) {
                return Err(here(
                    "the listing of the pending requests is not the model's",
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn release_is_never_before_hold_until_never_after_cancel_and_only_with_complete_evidence() {
    let mut stats = Stats::default();
    for seed in 0..SEEDS {
        let events = generate(seed);
        if let Err(message) = run(&events, &mut stats) {
            let shrunk = minimize(events.clone(), |candidate| {
                run(candidate, &mut Stats::default()).is_err()
            });
            panic!(
                "seed {seed}: {message}\nshrunk from {} to {} events:\n{shrunk:#?}",
                events.len(),
                shrunk.len()
            );
        }
    }
    eprintln!("{stats:?}");
    // The run was not vacuous: every path was granted, and every kind of event happened.
    for (path, _) in PATHS {
        assert!(
            stats.granted_by_path[path_index(path)] >= 5,
            "{path} was granted only {} times in {SEEDS} runs: {stats:?}",
            stats.granted_by_path[path_index(path)]
        );
    }
    assert!(stats.withheld > 3_000, "{stats:?}");
    assert!(stats.already_released >= 3, "{stats:?}");
    assert!(stats.cancelled > 150, "{stats:?}");
    assert!(
        stats.evidence_accepted > 2_000 && stats.evidence_refused > 2_000,
        "{stats:?}"
    );
    assert!(
        stats.contributions_accepted > 400 && stats.contributions_refused > 400,
        "{stats:?}"
    );
    assert!(stats.served > 100, "{stats:?}");
    assert!(stats.standing_lost >= 3, "{stats:?}");
    assert!(stats.expired > 20, "{stats:?}");
    assert!(stats.opened > 2_000, "{stats:?}");
    assert!(stats.open_refused >= 3, "{stats:?}");
    assert!(stats.facts_changes > 2_000, "{stats:?}");
}
