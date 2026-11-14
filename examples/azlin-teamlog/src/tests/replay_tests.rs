//! Tests of `replay` and of the equivalence of a device's own check and the metadata group's (check 14
//! of the brief; planning/architecture.md section 5.9 "a new device verifies from the latest checkpoint
//! whose entries it can check", section 12 E1 "devices run their own check, and the test runs both"; lane
//! W1-S17c2). The core is a property test: 5,000 seeded random sequences of entries (valid ones, and ones
//! that are wrong in every way the check knows), each step run twice, as the metadata group runs it
//! (`verify_signatures`, `admit`, `apply` on the state) and as a device runs it (`replay` of the one
//! entry from a copy of the state), and the whole log replayed from the first entry at the end.

use std::collections::{BTreeMap, BTreeSet};

use azlin_chain::{
    verify_from_checkpoint, verify_from_genesis, ChainEntry, ChainId, ChainRefusal, Checkpoint,
    Head, HeadVerifier, SignedHead,
};
use azlin_policy::{AdminQuorum, PolicyPatch};
use azlin_seams::sim::Rng;
use azlin_seams::{
    signing_input, AccountKind, ChainSeq, Domain, FakeScheme, Hash32, KeyId, MemberKind,
    RestoreEpoch, SigAlg, SignatureVerifier, Span, Stamp, VerifyError,
};

use super::kit::{account, at, device, group, hash, key, member};
use super::signed::{dkeys, invite_op, join_op, trusted, SignedKit};
use crate::admit::{Invalid, Refusal};
use crate::apply::apply;
use crate::members::{AppSpec, MemberState};
use crate::ops::{
    AppName, EntitlementState, EntitlementVersion, EscrowReason, InviteSecret, Op, OpName,
    PolicyScope,
};
use crate::proposal::{AssertionVerdict, RawEntry, Signer};
use crate::replay::replay;
use crate::roles::{needs_scope, Role};
use crate::state::TeamState;

fn below(rng: &mut Rng, n: u64) -> u64 {
    rng.below(n).unwrap_or(0)
}

fn chance(rng: &mut Rng, num: u32, den: u32) -> bool {
    rng.chance(num, den)
}

fn good(n: u8) -> AssertionVerdict {
    AssertionVerdict {
        key: device(n),
        user_verified: true,
        challenge_matches: true,
        rp_ok: true,
    }
}

/// A random member number: mostly one that exists, sometimes one that does not.
fn any_of(rng: &mut Rng, existing: &[u8], unknown_base: u8) -> u8 {
    if existing.is_empty() || chance(rng, 1, 14) {
        unknown_base.wrapping_add(below(rng, 4) as u8)
    } else {
        existing
            .get(below(rng, existing.len() as u64) as usize)
            .copied()
            .unwrap_or(unknown_base)
    }
}

/// The next random entry of a sequence: an op of any kind with arguments that mostly name things that
/// exist, signed by a device (mostly one that exists), cosigned by up to two, and in one case in
/// twenty-five damaged: a bad signature, a wrong `prev` or position, a corrupt copy.
fn random_entry(rng: &mut Rng, kit: &SignedKit) -> RawEntry {
    let devices: Vec<u8> = kit.state.devices().map(|d| d.id.as_bytes()[0]).collect();
    let members: Vec<u8> = kit.state.members().map(|m| m.id.as_bytes()[0]).collect();
    let groups: Vec<u8> = kit
        .state
        .groups()
        .iter()
        .map(|g| g.id.as_bytes()[0])
        .collect();
    let invited: Vec<u8> = kit
        .state
        .members()
        .filter(|m| m.state == MemberState::Invited)
        .map(|m| m.id.as_bytes()[0])
        .collect();
    let signer_device = any_of(rng, &devices, 150);
    let mut cosigners: Vec<u8> = (0..below(rng, 3))
        .map(|_| any_of(rng, &devices, 150))
        .collect();
    let mut assertions = Vec::new();
    let mut signer = Signer::Device(device(signer_device));
    let op = match below(rng, 23) {
        0 | 1 => {
            let n = 100 + below(rng, 40) as u8;
            let app = chance(rng, 1, 12);
            let role = match below(rng, 10) {
                0 => Role::Admin,
                1..=3 => Role::Guest,
                _ => Role::Member,
            };
            let mut groups_of = BTreeSet::new();
            for _ in 0..below(rng, 3) {
                groups_of.insert(group(any_of(rng, &groups, 80)));
            }
            Op::MemberInvite {
                member: member(if chance(rng, 1, 10) {
                    any_of(rng, &members, 160)
                } else {
                    n
                }),
                kind: if app {
                    MemberKind::App
                } else {
                    MemberKind::Person
                },
                role,
                invite_hash: crate::invite::invite_hash(&InviteSecret([n; 32])),
                groups: groups_of,
                app: if app {
                    let mut scopes = BTreeSet::new();
                    scopes.insert(AppName::new("notes").expect("valid"));
                    Some(AppSpec {
                        scopes,
                        expires_at: if chance(rng, 1, 3) {
                            Some(at(kit.stamp().as_secs() + below(rng, 60)))
                        } else {
                            None
                        },
                    })
                } else {
                    None
                },
            }
        }
        2 | 3 => {
            // A join: the new device signs, the inviter's device (usually) confirms.
            let subject = if invited.is_empty() || chance(rng, 1, 8) {
                any_of(rng, &members, 160)
            } else {
                any_of(rng, &invited, 160)
            };
            let new_device = 150 + below(rng, 40) as u8;
            signer = Signer::Device(device(new_device));
            let inviter_device = kit
                .state
                .member(&member(subject))
                .and_then(|m| m.invite.as_ref())
                .and_then(|invite| invite.inviter)
                .and_then(|inviter| {
                    kit.state
                        .devices_of(&inviter)
                        .next()
                        .map(|d| d.id.as_bytes()[0])
                });
            cosigners = match inviter_device {
                Some(n) if !chance(rng, 1, 5) => vec![n],
                _ => cosigners,
            };
            let secret = if chance(rng, 1, 6) {
                200 + below(rng, 5) as u8
            } else {
                subject
            };
            Op::MemberJoin {
                member: member(subject),
                device: device(new_device),
                member_keys: dkeys(subject.wrapping_add(150)),
                device_keys: dkeys(new_device),
                secret: InviteSecret([secret; 32]),
            }
        }
        4 => {
            let n = 190 + below(rng, 40) as u8;
            Op::DeviceAdd {
                member: member(any_of(rng, &members, 160)),
                device: device(n),
                keys: dkeys(n),
            }
        }
        5 => Op::DeviceRemove {
            device: device(any_of(rng, &devices, 150)),
        },
        6 | 7 => {
            let role = match below(rng, 6) {
                0 => Role::Owner,
                1 => Role::Admin,
                2 => Role::GroupAdmin,
                3 => Role::Helpdesk,
                4 => Role::Member,
                _ => Role::Guest,
            };
            let scope = if needs_scope(role) != chance(rng, 1, 10) {
                Some(group(any_of(rng, &groups, 80)))
            } else {
                None
            };
            Op::MemberRole {
                member: member(any_of(rng, &members, 160)),
                role,
                admin_scope: scope,
            }
        }
        8 => Op::MemberSuspend {
            member: member(any_of(rng, &members, 160)),
        },
        9 => Op::MemberRemove {
            member: member(any_of(rng, &members, 160)),
        },
        10 => Op::MemberManaged {
            member: member(any_of(rng, &members, 160)),
            managed: chance(rng, 1, 2),
        },
        11 | 12 => Op::GroupCreate {
            group: group(60 + below(rng, 30) as u8),
            parent: if chance(rng, 1, 5) {
                None
            } else {
                Some(group(any_of(rng, &groups, 80)))
            },
        },
        13 => Op::GroupMember {
            group: group(any_of(rng, &groups, 80)),
            member: member(any_of(rng, &members, 160)),
            present: !chance(rng, 1, 4),
        },
        14 => Op::GroupMove {
            group: group(any_of(rng, &groups, 80)),
            new_parent: group(any_of(rng, &groups, 80)),
        },
        15 => {
            let hours = 1 + below(rng, 20) as u32;
            let mut patch = PolicyPatch::none();
            if chance(rng, 3, 4) {
                patch.credential_ttl = Some(Span::hours(hours));
            }
            if chance(rng, 1, 5) {
                patch.padding = Some(azlin_policy::PaddingPolicy::Full);
            }
            Op::PolicySet {
                scope: if chance(rng, 1, 3) {
                    PolicyScope::Account
                } else {
                    PolicyScope::Group(group(any_of(rng, &groups, 80)))
                },
                patch,
            }
        }
        16 => Op::EscrowSet {
            escrow_pub: [below(rng, 3) as u8; 32],
            holders: (0..below(rng, 3))
                .map(|_| member(any_of(rng, &members, 160)))
                .collect(),
        },
        17 => {
            if chance(rng, 1, 2) {
                Op::AuditKeySet {
                    audit_pub: [below(rng, 3) as u8; 32],
                }
            } else {
                Op::AdminQuorumSet {
                    quorum: AdminQuorum::new(1 + below(rng, 3) as u8).expect("1 to 3"),
                }
            }
        }
        18 => {
            let subject = any_of(rng, &members, 160);
            if chance(rng, 4, 5) {
                assertions.push(good(signer_device));
                for n in &cosigners {
                    if chance(rng, 4, 5) {
                        assertions.push(good(*n));
                    }
                }
            }
            Op::EscrowOpen {
                member: member(subject),
                reason: EscrowReason::LegalRequest,
            }
        }
        19 => {
            let ops: BTreeSet<OpName> = (0..below(rng, 3))
                .map(|_| OpName::ALL[below(rng, OpName::ALL.len() as u64) as usize])
                .collect();
            Op::Delegate {
                key: key(40 + below(rng, 3) as u8),
                ops,
                expires: if chance(rng, 1, 3) {
                    Some(at(kit.stamp().as_secs() + below(rng, 40)))
                } else {
                    None
                },
            }
        }
        20 => {
            if chance(rng, 6, 7) {
                signer = Signer::PaymentsKey;
            }
            Op::Entitlement {
                version: EntitlementVersion(1 + below(rng, 6)),
                state: EntitlementState::Active,
                quota: azlin_seams::ByteCount::gib(1),
                seats: 3,
            }
        }
        _ => Op::App {
            name: AppName::new(if chance(rng, 1, 4) { "mail" } else { "notes" }).expect("valid"),
            body: vec![below(rng, 3) as u8; below(rng, 5) as usize],
        },
    };
    // A delegate key signs a suspension or an invitation now and then.
    if matches!(signer, Signer::Device(_))
        && matches!(op, Op::MemberSuspend { .. } | Op::MemberInvite { .. })
        && chance(rng, 1, 6)
    {
        signer = Signer::DelegateKey(key(40 + below(rng, 3) as u8));
    }
    let mut entry = kit.entry(signer, op, &cosigners, assertions, None);
    match below(rng, 50) {
        0 => entry.sig[0] ^= 1,
        1 => entry.prev = hash(7),
        2 => entry.seq = ChainSeq::new(entry.seq.get() + 2),
        3 => entry.preimage[0] ^= 1,
        4 => entry.hash = hash(9),
        5 => {
            if let Some(cosig) = entry.cosigs.first_mut() {
                cosig.1[0] ^= 1;
            }
        }
        _ => {}
    }
    entry
}

/// A small account for a sequence to work on: some members and groups, made by valid entries where the
/// account's quorum allows them (the others are simply refused: the sequences start from whatever the
/// account's kind lets the owner do alone).
fn start(rng: &mut Rng) -> SignedKit {
    let kinds = [
        AccountKind::Personal,
        AccountKind::Family,
        AccountKind::Team,
        AccountKind::Business,
    ];
    let kind = kinds[below(rng, 4) as usize];
    let mut kit = SignedKit::new(kind);
    let _ = kit.send(
        1,
        Op::GroupCreate {
            group: group(1),
            parent: None,
        },
    );
    let _ = kit.send(
        1,
        Op::GroupCreate {
            group: group(2),
            parent: Some(group(1)),
        },
    );
    for n in 2..=4u8 {
        let _ = kit.send(1, invite_op(n, &[1]));
        let join = kit.entry(
            Signer::Device(device(n)),
            join_op(n, n),
            &[1],
            Vec::new(),
            None,
        );
        let _ = kit.push(join);
    }
    let _ = kit.send(
        1,
        Op::MemberRole {
            member: member(2),
            role: Role::Admin,
            admin_scope: None,
        },
    );
    let _ = kit.send(
        1,
        Op::MemberRole {
            member: member(3),
            role: Role::GroupAdmin,
            admin_scope: Some(group(1)),
        },
    );
    kit
}

/// What a sequence did, for the coverage assertions.
#[derive(Default)]
struct Coverage {
    attempts: usize,
    admitted: usize,
    admitted_ops: BTreeMap<String, usize>,
    refusals: BTreeMap<String, usize>,
}

/// The word of a refusal, for the coverage table.
fn word(refusal: &Refusal) -> &'static str {
    match refusal {
        Refusal::Chain(_) => "chain",
        Refusal::BadSignature => "bad_signature",
        Refusal::SignerUnknown => "signer_unknown",
        Refusal::SignerNotActive { .. } => "signer_not_active",
        Refusal::MemberNotActive { .. } => "member_not_active",
        Refusal::RoleMaySignNo { .. } => "role",
        Refusal::OutOfScope { .. } => "scope",
        Refusal::QuorumShort { .. } => "quorum",
        Refusal::AssertionMissing => "assertion",
        Refusal::PolicyForbids(_) => "policy",
        Refusal::LastOwner => "last_owner",
        Refusal::GroupCycle => "cycle",
        Refusal::DelegationDoesNotCover { .. } => "delegation_covers",
        Refusal::DelegationExpired => "delegation_expired",
        Refusal::InviteInvalid => "invite",
        Refusal::UnknownOp => "unknown_op",
        Refusal::SubjectUnknown => "subject_unknown",
        Refusal::ConsentMissing => "consent",
        Refusal::Invalid(_) => "invalid",
    }
}

/// Runs one seeded sequence: every step as the metadata group runs it and as a device runs it, and at the
/// end the whole log from its first entry.
fn run_sequence(seed: u64, steps: usize, coverage: &mut Coverage) {
    let mut rng = Rng::new(seed);
    let mut kit = start(&mut rng);
    let kind = kit.state.kind();
    for step in 0..steps {
        let entry = random_entry(&mut rng, &kit);
        let stamp = kit.stamp();
        let bytes_before = kit.state.canonical_bytes();
        // The metadata group's answer, on the state, without applying yet.
        let server = kit.check(&entry);
        assert_eq!(
            bytes_before,
            kit.state.canonical_bytes(),
            "seed {seed} step {step}: admit changed the state"
        );
        // A device's answer: its own check of this one entry from a copy of the state.
        let device_answer = replay(
            kit.state.clone(),
            std::slice::from_ref(&entry),
            &FakeScheme,
            &trusted(),
            &[stamp],
        );
        coverage.attempts += 1;
        match (server, device_answer) {
            (Ok(admitted), Ok(device_state)) => {
                let mut server_state = kit.state.clone();
                let effects = apply(&mut server_state, admitted);
                assert!(effects.applied, "seed {seed} step {step}");
                assert_eq!(
                    server_state.state_hash(),
                    device_state.state_hash(),
                    "seed {seed} step {step}: the state hashes differ"
                );
                assert_eq!(server_state, device_state, "seed {seed} step {step}");
                *coverage.admitted_ops.entry(entry.op_name().as_str().to_string()).or_default() += 1;
                coverage.admitted += 1;
                kit.push(entry).expect("an admitted entry is admitted again");
                assert_eq!(kit.state, server_state, "seed {seed} step {step}");
            }
            (Err(on_server), Err((position, on_device))) => {
                assert_eq!(position, 0, "seed {seed} step {step}");
                assert_eq!(on_server, on_device, "seed {seed} step {step}: another reason");
                *coverage.refusals.entry(word(&on_server).to_string()).or_default() += 1;
                // A refused entry leaves the state byte for byte as it was.
                assert_eq!(
                    bytes_before,
                    kit.state.canonical_bytes(),
                    "seed {seed} step {step}: a refused entry changed the state"
                );
            }
            (server, device_answer) => panic!(
                "seed {seed} step {step}: the metadata group says {:?}, a device says {:?} for {:?}",
                server.map(|_| ()),
                device_answer.map(|_| ()),
                entry
            ),
        }
    }
    // The whole log, from its first entry, on a device: the same state, the same hash.
    let entries: Vec<RawEntry> = kit.log.iter().map(|(entry, _)| entry.clone()).collect();
    let stamps: Vec<Stamp> = kit.log.iter().map(|(_, stamp)| *stamp).collect();
    let replayed = replay(
        TeamState::empty(account(), kind),
        &entries,
        &FakeScheme,
        &trusted(),
        &stamps,
    )
    .unwrap_or_else(|(position, why)| {
        panic!("seed {seed}: replay refused entry {position}: {why}")
    });
    assert_eq!(replayed.state_hash(), kit.state.state_hash(), "seed {seed}");
    assert_eq!(replayed, kit.state, "seed {seed}");
}

#[test]
fn replay_equals_incremental_for_5000_seeded_sequences() {
    let mut coverage = Coverage::default();
    for seed in 0..5_000u64 {
        run_sequence(seed, 10, &mut coverage);
    }
    // The property is only worth something if the sequences reach the interesting branches.
    assert_eq!(coverage.attempts, 50_000);
    assert!(
        coverage.admitted * 25 >= coverage.attempts,
        "{} of {} entries admitted: the generator is too wild",
        coverage.admitted,
        coverage.attempts
    );
    assert!(
        coverage.admitted_ops.len() >= 12,
        "ops admitted: {:?}",
        coverage.admitted_ops
    );
    assert!(
        coverage.refusals.len() >= 8,
        "refusals seen: {:?}",
        coverage.refusals
    );
    for word in [
        "chain",
        "bad_signature",
        "role",
        "scope",
        "quorum",
        "subject_unknown",
    ] {
        assert!(
            coverage.refusals.contains_key(word),
            "no {word} refusal in 5,000 sequences: {:?}",
            coverage.refusals
        );
    }
}

#[test]
fn admit_does_not_mutate_state() {
    // `admit` takes the state by shared reference; this shows what that means: whatever it answers, the
    // canonical bytes of the state, its hash and its chain are what they were.
    let mut kit = super::signed::universe();
    for n in 0..30u64 {
        let mut rng = Rng::new(1_000 + n);
        let entry = random_entry(&mut rng, &kit);
        let before = kit.state.clone();
        let bytes = kit.state.canonical_bytes();
        let _ = kit.check(&entry);
        let _ = kit.check(&entry);
        assert_eq!(kit.state, before);
        assert_eq!(kit.state.canonical_bytes(), bytes);
        assert_eq!(kit.state.state_hash(), before.state_hash());
        let _ = kit.push(entry);
    }
}

#[test]
fn refused_entry_leaves_state_byte_identical() {
    let mut kit = super::signed::universe();
    let bytes = kit.state.canonical_bytes();
    let hash_before = kit.state.state_hash();
    let head = kit.state.head();
    let refused = [
        // A member signs a role change.
        kit.entry_by(
            7,
            &[],
            Op::MemberRole {
                member: member(24),
                role: Role::Guest,
                admin_scope: None,
            },
        ),
        // A quorum of one.
        kit.entry_by(
            1,
            &[],
            Op::MemberRole {
                member: member(24),
                role: Role::Admin,
                admin_scope: None,
            },
        ),
        // A signer nobody knows.
        kit.entry(
            Signer::Device(device(199)),
            Op::App {
                name: AppName::new("notes").expect("valid"),
                body: vec![],
            },
            &[],
            Vec::new(),
            None,
        ),
    ];
    for entry in refused {
        assert!(kit.push(entry).is_err());
        assert_eq!(kit.state.canonical_bytes(), bytes);
        assert_eq!(kit.state.state_hash(), hash_before);
        assert_eq!(kit.state.head(), head);
    }
    // And a refused entry is refused again, with the same reason, once something else was admitted.
    let again = kit.entry_by(
        7,
        &[],
        Op::MemberRole {
            member: member(24),
            role: Role::Guest,
            admin_scope: None,
        },
    );
    let first = kit.verdict(&again);
    kit.must(
        1,
        Op::GroupCreate {
            group: group(70),
            parent: None,
        },
    );
    let moved = kit.entry_by(
        7,
        &[],
        Op::MemberRole {
            member: member(24),
            role: Role::Guest,
            admin_scope: None,
        },
    );
    assert_eq!(first.is_err(), kit.verdict(&moved).is_err());
}

#[test]
fn replay_stops_at_the_first_refusal_and_says_where() {
    let kit = super::signed::universe();
    let mut entries: Vec<RawEntry> = kit.log.iter().map(|(entry, _)| entry.clone()).collect();
    let stamps: Vec<Stamp> = kit.log.iter().map(|(_, stamp)| *stamp).collect();
    let empty = || TeamState::empty(account(), AccountKind::Team);
    assert!(replay(empty(), &entries, &FakeScheme, &trusted(), &stamps).is_ok());
    // A damaged signature at position 17: the first 17 are fine, the 18th is where it stops.
    entries[17].sig[0] ^= 1;
    let answer = replay(empty(), &entries, &FakeScheme, &trusted(), &stamps);
    assert_eq!(answer.err(), Some((17, Refusal::BadSignature)));
    entries[17].sig[0] ^= 1;
    // A wrong `prev` at position 5.
    entries[5].prev = hash(3);
    let answer = replay(empty(), &entries, &FakeScheme, &trusted(), &stamps);
    match answer {
        Err((5, Refusal::Chain(ChainRefusal::WrongPrev { .. }))) => {}
        other => panic!("a wrong prev at 5 expected, was {:?}", other.map(|_| ())),
    }
    entries[5].prev = kit.log[4].0.hash;
    // A missing entry is a gap; a stamp that runs backwards is the leader's fault.
    let mut without = entries.clone();
    without.remove(9);
    let mut fewer = stamps.clone();
    fewer.remove(9);
    match replay(empty(), &without, &FakeScheme, &trusted(), &fewer) {
        Err((9, Refusal::Chain(ChainRefusal::Gap { .. }))) => {}
        other => panic!("a gap at 9 expected, was {:?}", other.map(|_| ())),
    }
    let mut backwards = stamps.clone();
    backwards[12] = at(1);
    match replay(empty(), &entries, &FakeScheme, &trusted(), &backwards) {
        Err((12, Refusal::Chain(ChainRefusal::StampBackwards))) => {}
        other => panic!(
            "a stamp that runs backwards at 12 expected, was {:?}",
            other.map(|_| ())
        ),
    }
    // One stamp for each entry.
    let short = &stamps[..stamps.len() - 1];
    assert_eq!(
        replay(empty(), &entries, &FakeScheme, &trusted(), short).err(),
        Some((
            short.len(),
            Refusal::Invalid(Invalid::Args("one leader stamp for each entry"))
        ))
    );
    // No entries: the state it started from.
    let untouched = replay(empty(), &[], &FakeScheme, &trusted(), &[]).expect("nothing to do");
    assert_eq!(untouched, empty());
}

/// The domain of the test head's signature.
const HEAD_DOMAIN: Domain = Domain::new("azlin-teamlog-test-head-v1");

/// The bytes a test head's signature covers: every field but the signature.
fn head_preimage(head: &SignedHead) -> Vec<u8> {
    let body = format!(
        "{:?}|{:?}|{:?}|{:?}",
        head.head, head.epoch, head.signed_at, head.signer
    );
    signing_input(&HEAD_DOMAIN, body.as_bytes())
}

/// A verifier of test heads over the fake scheme.
struct HeadChecker;

impl HeadVerifier for HeadChecker {
    fn verify_head(&self, h: &SignedHead) -> Result<(), VerifyError> {
        FakeScheme.verify(
            SigAlg::Ed25519,
            &FakeScheme::public_key(&h.signer),
            &head_preimage(h),
            &h.sig,
        )
    }
}

fn signed_head(head: Head, epoch: RestoreEpoch, signed_at: Stamp) -> SignedHead {
    let mut signed = SignedHead {
        head,
        epoch,
        signed_at,
        signer: KeyId::from_bytes([0xCC; 32]),
        sig: Vec::new(),
    };
    signed.sig = FakeScheme::sign(&signed.signer, &head_preimage(&signed));
    signed
}

/// The chain machinery's view of an entry at the leader's stamp (what the glue implements).
struct Linked<'a> {
    entry: &'a RawEntry,
    stamp: Stamp,
}

impl ChainEntry for Linked<'_> {
    fn chain(&self) -> ChainId {
        ChainId::team_log(self.entry.account)
    }

    fn seq(&self) -> ChainSeq {
        self.entry.seq
    }

    fn prev(&self) -> Hash32 {
        self.entry.prev
    }

    fn hash(&self) -> Hash32 {
        self.entry.hash
    }

    fn at(&self) -> Stamp {
        self.stamp
    }

    fn epoch(&self) -> RestoreEpoch {
        self.entry.epoch
    }

    fn is_intact(&self) -> bool {
        self.entry.is_intact()
    }
}

#[test]
fn replay_from_checkpoint_state_equals_from_genesis() {
    let kit = super::signed::universe();
    let entries: Vec<RawEntry> = kit.log.iter().map(|(entry, _)| entry.clone()).collect();
    let stamps: Vec<Stamp> = kit.log.iter().map(|(_, stamp)| *stamp).collect();
    assert!(entries.len() > 40, "{} entries", entries.len());
    let empty = || TeamState::empty(account(), AccountKind::Team);
    let from_genesis =
        replay(empty(), &entries, &FakeScheme, &trusted(), &stamps).expect("verifies");
    assert_eq!(from_genesis, kit.state);
    // The checkpoint at position k: the head the group signed and the hash of the state the entries up to
    // it produced.
    for k in [0usize, 1, 7, 20, entries.len() - 2, entries.len() - 1] {
        let at_k = replay(
            empty(),
            &entries[..=k],
            &FakeScheme,
            &trusted(),
            &stamps[..=k],
        )
        .expect("verifies");
        let head = at_k.head().expect("a head");
        assert_eq!(head.seq, ChainSeq::new(k as u64));
        let checkpoint = Checkpoint::new(
            signed_head(head, RestoreEpoch::ZERO, stamps[k]),
            stamps[k],
            at_k.state_hash(),
        );
        // A device that fetched the registry state of the checkpoint compares its hash with the claim...
        let fetched = at_k.clone();
        assert_eq!(fetched.state_hash(), checkpoint.state_hash());
        // ...checks the entries after it (the chain's rules over the leader's stamps)...
        let rest = &entries[k + 1..];
        let linked: Vec<Linked<'_>> = rest
            .iter()
            .zip(&stamps[k + 1..])
            .map(|(entry, stamp)| Linked {
                entry,
                stamp: *stamp,
            })
            .collect();
        let end = verify_from_checkpoint(&checkpoint, &HeadChecker, &linked)
            .expect("the chain continues");
        assert_eq!(Some(end), from_genesis.head());
        // ...and replays them: the same state as from the first entry.
        let from_checkpoint = replay(fetched, rest, &FakeScheme, &trusted(), &stamps[k + 1..])
            .expect("the entries after the checkpoint verify");
        assert_eq!(from_checkpoint, from_genesis, "checkpoint at {k}");
        assert_eq!(from_checkpoint.state_hash(), from_genesis.state_hash());
    }
    // The chain checks of azlin-chain agree with the check of this crate on the whole log.
    let all_linked: Vec<Linked<'_>> = entries
        .iter()
        .zip(&stamps)
        .map(|(entry, stamp)| Linked {
            entry,
            stamp: *stamp,
        })
        .collect();
    let end = verify_from_genesis(ChainId::team_log(account()), &all_linked).expect("a chain");
    assert_eq!(end, from_genesis.head());
    // A checkpoint whose head signature does not hold is a corrupt checkpoint, and a state that is not
    // the checkpoint's is told apart by the hash.
    let at_k = replay(
        empty(),
        &entries[..=10],
        &FakeScheme,
        &trusted(),
        &stamps[..=10],
    )
    .expect("verifies");
    let mut forged = signed_head(at_k.head().expect("a head"), RestoreEpoch::ZERO, stamps[10]);
    forged.sig[0] ^= 1;
    let checkpoint = Checkpoint::new(forged, stamps[10], at_k.state_hash());
    assert_eq!(
        verify_from_checkpoint(&checkpoint, &HeadChecker, &all_linked[11..]),
        Err(ChainRefusal::Corrupt)
    );
    let mut other_state = at_k.clone();
    other_state.audit_pub = Some([1; 32]);
    assert_ne!(other_state.state_hash(), at_k.state_hash());
    // A device that starts from a state that is not the one the entries follow refuses at once.
    let wrong_start = replay(
        other_state,
        &entries[11..],
        &FakeScheme,
        &trusted(),
        &stamps[11..],
    );
    assert!(
        wrong_start.is_ok(),
        "a different audit key does not stop the chain: the hash comparison does"
    );
    assert_ne!(
        wrong_start.expect("replayed").state_hash(),
        from_genesis.state_hash()
    );
}
