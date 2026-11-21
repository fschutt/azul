//! Tests of the three defects that the escalation lane S47a (`azlin-escalate-e1`) found in this crate
//! (X-DELTA-W1S47A-1, -2 and -3; lane W1-S17c3, decision D-W1S17C3). Each section holds the NAMED ATTACK
//! test of its row (the killer of the replayable patch `mutants/X-DELTA-W1S47A-<n>.patch`) and the tests
//! of the rule around it: the minimised reproductions of S47a's stories 8 and 14 and of its probe of the
//! far restore epoch, run through the production functions (`SignedKit`: `verify_signatures`, `admit`,
//! `apply`).
//!
//! 1. THE BINDING (X-DELTA-W1S47A-1): the typed fields of an entry are the fields of the bytes its
//!    signatures cover (`EntryDecoder`, planning/architecture.md section 5.9, section 2.5).
//! 2. POLICY BY MOVING A GROUP (X-DELTA-W1S47A-2): a move that loosens the effective policy of a group of
//!    the moved subtree is for an owner or an admin with the quorum of a policy change (section 7.2:
//!    "subgroups inherit and may tighten, never loosen").
//! 3. THE RESTORE EPOCH (X-DELTA-W1S47A-3): only a verified restore record moves it; an entry carries
//!    exactly the floor (section 5.13, section 6.10 V3).

use azlin_chain::{
    verify_record, ChainRefusal, RefusalClass, RestoreRecord, RestoreRefusal, RestoreTrust,
    VerifiedRestore,
};
use azlin_policy::{ExternalSharing, PolicyPatch};
use azlin_seams::{
    signing_input, AccountId, AccountKind, ChainSeq, Domain, FakeScheme, Hash32, RestoreEpoch, Span,
};

use super::kit::{account, at, device, group, hash, member};
use super::signed::{invite_op, join_op, sign_with, trusted, Shape, SignedKit};
use crate::admit::{admit, PolicyReason, Refusal};
use crate::decode::{canonical_of, DecodedEntry, EntryDecoder, FakeDecoder, NotAnEntry};
use crate::hash::entry_hash;
use crate::ops::{AppName, Op, OpName, PolicyScope};
use crate::proposal::{RawEntry, Signer, TEAM_LOG_DOMAIN};
use crate::replay::replay;
use crate::roles::Role;
use crate::state::TeamState;
use crate::verify::verify_signatures;

fn team() -> SignedKit {
    SignedKit::new(AccountKind::Team)
}

fn notes() -> Op {
    Op::App {
        name: AppName::new("notes").expect("valid"),
        body: vec![1],
    }
}

fn why(result: Result<(), Refusal>) -> Refusal {
    result.expect_err("the entry must be refused")
}

// ---- 1. the binding of the typed fields to the signed bytes (X-DELTA-W1S47A-1) -----------------------

/// The log of the story: the owner (device 1) makes the department `eng` (group 1); member 2 is invited
/// into it and joins (the inviter cosigns); then the owner signs ENTRY E, `member_role(M -> helpdesk, scope
/// eng)`, which is admitted. Returns the kit and E.
fn story_log() -> (SignedKit, RawEntry) {
    let mut kit = team();
    kit.make_group(1, None);
    kit.enrol(2, Role::Member, &[1], None);
    let helpdesk = Op::MemberRole {
        member: member(2),
        role: Role::Helpdesk,
        admin_scope: Some(group(1)),
    };
    let e = kit.entry_by(1, &[], helpdesk);
    kit.push(e.clone()).expect("E is admitted");
    (kit, e)
}

#[test]
fn an_old_valid_entry_cannot_be_replayed_at_a_new_position() {
    // The minimised reproduction of X-DELTA-W1S47A-1 (azlin-escalate-e1, story 8): the owner makes M a
    // helpdesk member (E), then takes the role away again; E, with the position fields of a NEW entry
    // and nothing else changed (preimage, hash and every signature untouched), must not be admitted.
    let (mut kit, e) = story_log();
    assert_eq!(
        kit.state.member(&member(2)).map(|m| m.role),
        Some(Role::Helpdesk)
    );
    kit.must(
        1,
        Op::MemberRole {
            member: member(2),
            role: Role::Member,
            admin_scope: None,
        },
    );
    assert_eq!(
        kit.state.member(&member(2)).map(|m| m.role),
        Some(Role::Member)
    );
    // The honest replay of E at its own position is refused: the chain says so.
    assert!(matches!(
        kit.verdict(&e),
        Err(Refusal::Chain(ChainRefusal::Gap { .. }))
    ));
    // E with ONLY the position of the next entry.
    let head = kit.state.head().expect("a head");
    let mut forged = e.clone();
    forged.seq = head.seq.next().expect("a next position");
    forged.prev = head.hash;
    assert_eq!(forged.preimage, e.preimage);
    assert_eq!(forged.hash, e.hash);
    assert_eq!(forged.sig, e.sig);
    assert!(forged.is_intact(), "its hash is the hash of its preimage");
    // The signature holds over the preimage and the position fields say what the chain wants: the only
    // thing that tells it from an entry of the owner is that the signed bytes say another position.
    assert_eq!(kit.verdict(&forged), Err(Refusal::BadSignature));
    assert_eq!(Refusal::BadSignature.class(), RefusalClass::Refetch);
    // The same through a device's own check of the whole log: it stops at the forged entry.
    let mut entries: Vec<RawEntry> = kit.log.iter().map(|(entry, _)| entry.clone()).collect();
    let mut stamps: Vec<_> = kit.log.iter().map(|(_, stamp)| *stamp).collect();
    entries.push(forged.clone());
    stamps.push(kit.stamp());
    let answer = replay(
        TeamState::empty(account(), AccountKind::Team),
        &entries,
        &FakeScheme,
        &trusted(),
        kit.decoder(),
        &stamps,
    );
    assert_eq!(
        answer.err(),
        Some((entries.len() - 1, Refusal::BadSignature)),
        "a device refuses it at the same place and for the same reason as the metadata group"
    );
    // Nothing was applied: M is still a plain member, and no signature of the owner was needed.
    assert_eq!(
        kit.state.member(&member(2)).map(|m| m.role),
        Some(Role::Member)
    );
    // An entry that the owner really signs at that position, with the same op, is admitted: the refusal
    // above is about what the signature covers and not about the op.
    let honest = kit.entry_by(
        1,
        &[],
        Op::MemberRole {
            member: member(2),
            role: Role::Helpdesk,
            admin_scope: Some(group(1)),
        },
    );
    assert_eq!(honest.seq, forged.seq);
    assert_eq!(honest.prev, forged.prev);
    assert_eq!(kit.verdict(&honest), Ok(()));
}

#[test]
fn every_typed_field_of_an_entry_is_bound_to_its_signed_bytes() {
    let mut kit = team();
    kit.enrol(2, Role::Member, &[], None);
    let entry = kit.entry_by(2, &[], notes());
    assert_eq!(kit.verdict(&entry), Ok(()));
    let refused = |what: &str, change: &dyn Fn(&mut RawEntry)| {
        let mut copy = entry.clone();
        change(&mut copy);
        assert_eq!(kit.verdict(&copy), Err(Refusal::BadSignature), "{what}");
        // The same through `verify_signatures` alone: the binding is the signature check's.
        let alone = verify_signatures(&kit.state, &copy, &FakeScheme, &trusted(), kit.decoder());
        assert_eq!(alone.err(), Some(Refusal::BadSignature), "{what}");
    };
    refused("seq", &|e| e.seq = ChainSeq::new(e.seq.get() + 1));
    refused("prev", &|e| e.prev = hash(9));
    refused("at", &|e| e.at = at(1));
    refused("epoch", &|e| e.epoch = RestoreEpoch::new(1));
    // The signer: the owner's device instead of the member's, with the owner's own signature over the
    // same bytes (it verifies under the owner's key): only the binding says that the bytes name the
    // member's device.
    refused("signer", &|e| {
        e.signer = Signer::Device(device(1));
        e.sig = sign_with([1; 32], &e.preimage);
    });
    refused("op", &|e| {
        e.op = Op::MemberSuspend { member: member(1) };
    });
    // The account: an entry signed for another account and typed as this one's is not what was signed;
    // typed as the other account's it is not this log's (the chain says so first).
    let foreign = kit.entry_shaped(
        Signer::Device(device(2)),
        notes(),
        &[],
        Vec::new(),
        None,
        Shape {
            account: Some(AccountId::from_bytes([0x44; 16])),
            ..Shape::default()
        },
    );
    let mut typed_as_ours = foreign.clone();
    typed_as_ours.account = account();
    assert_eq!(
        kit.verdict(&typed_as_ours),
        Err(Refusal::BadSignature),
        "account"
    );
    assert_eq!(
        kit.verdict(&foreign),
        Err(Refusal::Chain(ChainRefusal::WrongChain))
    );
    // What is NOT signed is not bound: the signatures and the sealed field may differ from nothing.
    let mut sealed = entry.clone();
    sealed.sealed = Some(vec![1, 2, 3]);
    assert_eq!(kit.verdict(&sealed), Ok(()));
    // And an unchanged entry stays admitted after all of the above (nothing was applied or registered).
    assert_eq!(kit.verdict(&entry), Ok(()));
}

#[test]
fn a_preimage_of_another_domain_or_of_no_entry_is_no_signed_entry() {
    let mut kit = team();
    kit.enrol(2, Role::Member, &[], None);
    let entry = kit.entry_by(2, &[], notes());
    let canonical = canonical_of(&entry.preimage)
        .expect("the team-log domain")
        .to_vec();
    // The very same canonical bytes under another domain: the decoder knows them (they are registered),
    // the signature over them holds, and still it is no entry of the team log (section 2.5).
    let resign = |entry: &RawEntry, preimage: Vec<u8>| {
        let mut copy = entry.clone();
        copy.hash = entry_hash(&preimage);
        copy.sig = sign_with([2; 32], &preimage);
        copy.preimage = preimage;
        copy
    };
    for other in ["azlin-team-log-v2", "azlin-entitlement-v2"] {
        let other_domain = resign(&entry, signing_input(&Domain::new(other), &canonical));
        assert!(canonical_of(&other_domain.preimage).is_none(), "{other}");
        assert_eq!(
            kit.verdict(&other_domain),
            Err(Refusal::BadSignature),
            "{other}"
        );
    }
    // No domain at all.
    let bare = resign(&entry, canonical.clone());
    assert_eq!(kit.verdict(&bare), Err(Refusal::BadSignature));
    // The right domain over bytes that are no entry (the decoder cannot decode them): `NotAnEntry`.
    let nonsense = resign(&entry, signing_input(&TEAM_LOG_DOMAIN, b"not an entry"));
    assert_eq!(kit.decoder().decode(b"not an entry"), Err(NotAnEntry));
    assert_eq!(kit.verdict(&nonsense), Err(Refusal::BadSignature));
    // The control: the right domain over the registered bytes is the entry, admitted.
    let control = resign(&entry, signing_input(&TEAM_LOG_DOMAIN, &canonical));
    assert_eq!(control.preimage, entry.preimage);
    assert_eq!(kit.verdict(&control), Ok(()));
}

#[test]
fn canonical_bytes_are_what_follows_the_domain_and_its_separator() {
    let preimage = signing_input(&TEAM_LOG_DOMAIN, b"abc");
    assert_eq!(canonical_of(&preimage), Some(&b"abc"[..]));
    let empty = signing_input(&TEAM_LOG_DOMAIN, b"");
    assert_eq!(canonical_of(&empty), Some(&b""[..]));
    // The separator is part of the prefix; a domain that merely starts the same is another domain.
    assert_eq!(canonical_of(TEAM_LOG_DOMAIN.as_str().as_bytes()), None);
    assert_eq!(canonical_of(b""), None);
    let longer = signing_input(&Domain::new("azlin-team-log-v1x"), b"abc");
    assert_eq!(canonical_of(&longer), None);
    let other = signing_input(&Domain::new("azlin-team-log-v2"), b"abc");
    assert_eq!(canonical_of(&other), None);
}

#[test]
fn the_fake_decoder_decodes_what_was_registered_and_nothing_else() {
    let decoder = FakeDecoder::new();
    assert!(decoder.is_empty());
    let entry = DecodedEntry {
        account: account(),
        seq: ChainSeq::MIN,
        prev: Hash32::ZERO,
        at: at(10),
        epoch: RestoreEpoch::ZERO,
        signer: Signer::Device(device(1)),
        op: notes(),
    };
    assert_eq!(decoder.register(b"one".to_vec(), entry.clone()), None);
    assert_eq!(decoder.len(), 1);
    assert_eq!(decoder.decode(b"one"), Ok(entry.clone()));
    assert_eq!(decoder.decode(b"two"), Err(NotAnEntry));
    assert_eq!(decoder.decode(b""), Err(NotAnEntry));
    // Registering the same bytes again gives the first meaning back, so that a test can see a clash.
    let other = DecodedEntry {
        seq: ChainSeq::new(1),
        ..entry.clone()
    };
    assert_eq!(
        decoder.register(b"one".to_vec(), other.clone()),
        Some(entry)
    );
    assert_eq!(decoder.decode(b"one"), Ok(other));
    // A copy is its own table.
    let copy = decoder.clone();
    copy.register(b"three".to_vec(), copy.decode(b"one").expect("known"));
    assert_eq!(copy.len(), 2);
    assert_eq!(decoder.len(), 1);
    assert_eq!(
        NotAnEntry.to_string(),
        "the signed bytes are not a team-log entry"
    );
}

// ---- 2. a group admin loosens an admin-set policy by moving a group (X-DELTA-W1S47A-2) ---------------

/// What an admin restricts a department to: no external sharing and one-hour credentials.
fn restriction() -> PolicyPatch {
    PolicyPatch {
        external_sharing: Some(ExternalSharing::SameWorkspaceOnly),
        credential_ttl: Some(Span::hours(1)),
        ..PolicyPatch::none()
    }
}

/// A team account (quorum 1, so the owner builds it alone) with the departments `eng` (group 1) >
/// `hr` (2) > `sub` (3), `eng` > `loose` (4) and `eng` > `free` (6), an admin (member 3, device 3), the
/// group admin of `eng` (member 5, device 5), and `hr` restricted BY THE ADMIN.
fn departments() -> SignedKit {
    let mut kit = team();
    kit.make_group(1, None);
    kit.make_group(2, Some(1));
    kit.make_group(3, Some(2));
    kit.make_group(4, Some(1));
    kit.make_group(6, Some(1));
    kit.enrol(3, Role::Admin, &[], None);
    kit.enrol(5, Role::GroupAdmin, &[1], Some(1));
    kit.must(
        3,
        Op::PolicySet {
            scope: PolicyScope::Group(group(2)),
            patch: restriction(),
        },
    );
    kit
}

fn move_group(n: u8, new_parent: u8) -> Op {
    Op::GroupMove {
        group: group(n),
        new_parent: group(new_parent),
    }
}

#[test]
fn moving_a_group_cannot_loosen_its_effective_policy() {
    // The minimised reproduction of X-DELTA-W1S47A-2 (azlin-escalate-e1, story 14): the admin restricted
    // `hr`; `sub` below it inherits the restriction; the group admin whose scope is `eng` has scope over
    // both groups and moves `sub` out from under `hr`, to where the restriction does not apply.
    let kit = departments();
    let before = kit
        .state
        .effective_policy_of(&group(3))
        .expect("a policy for sub");
    assert_eq!(before.external_sharing, ExternalSharing::SameWorkspaceOnly);
    assert_eq!(before.credential_ttl, Span::hours(1));
    let loosening = kit.entry_by(5, &[], move_group(3, 1));
    let refusal = why(kit.verdict(&loosening));
    match &refusal {
        Refusal::PolicyForbids(PolicyReason::Loosens(fields)) => {
            let mut named: Vec<(String, String, String)> = fields
                .iter()
                .map(|f| (f.field.to_string(), f.parent.clone(), f.child.clone()))
                .collect();
            named.sort();
            assert_eq!(
                named,
                vec![
                    (
                        "credential_ttl".to_string(),
                        "3600 s".to_string(),
                        "43200 s".to_string()
                    ),
                    (
                        "external_sharing".to_string(),
                        "same_workspace_only".to_string(),
                        "allowed".to_string()
                    ),
                ]
            );
        }
        other => panic!("a loosening expected, was {other:?}"),
    }
    assert_eq!(refusal.class(), RefusalClass::Refuse);
    // To the sibling that has no restriction: the same. With an admin's cosignature on top (the quorum
    // is met, and still a group admin only tightens).
    assert!(matches!(
        kit.verdict(&kit.entry_by(5, &[], move_group(3, 4))),
        Err(Refusal::PolicyForbids(PolicyReason::Loosens(_)))
    ));
    assert!(matches!(
        kit.verdict(&kit.entry_by(5, &[3], move_group(3, 1))),
        Err(Refusal::PolicyForbids(PolicyReason::Loosens(_)))
    ));
    // Nothing changed: `sub` is still restricted.
    assert_eq!(
        kit.state
            .effective_policy_of(&group(3))
            .expect("a policy")
            .credential_ttl,
        Span::hours(1)
    );
}

#[test]
fn a_move_that_keeps_or_tightens_every_policy_is_the_group_admins_to_make() {
    let kit = departments();
    // `hr` carries its own restriction: moved under `loose` it is as restricted as before, and so is
    // `sub` below it.
    assert_eq!(kit.verdict(&kit.entry_by(5, &[], move_group(2, 4))), Ok(()));
    // A free group moved under the restricted `hr` gets TIGHTER.
    assert_eq!(kit.verdict(&kit.entry_by(5, &[], move_group(6, 2))), Ok(()));
    // A move between parents that give the same policy changes nothing.
    assert_eq!(kit.verdict(&kit.entry_by(5, &[], move_group(6, 4))), Ok(()));
    // The tightening move is applied and the policy of the group is the restricted one.
    let mut moved = kit.clone();
    moved.must(5, move_group(6, 2));
    let after = moved
        .state
        .effective_policy_of(&group(6))
        .expect("a policy for the moved group");
    assert_eq!(after.credential_ttl, Span::hours(1));
    assert_eq!(after.external_sharing, ExternalSharing::SameWorkspaceOnly);
}

#[test]
fn a_move_that_loosens_is_an_owner_or_an_admin_with_the_quorum_of_a_policy_change() {
    let mut kit = departments();
    // In a team account the quorum is one person: the admin, or the owner, moves alone.
    assert_eq!(kit.verdict(&kit.entry_by(3, &[], move_group(3, 1))), Ok(()));
    assert_eq!(kit.verdict(&kit.entry_by(1, &[], move_group(3, 1))), Ok(()));
    // Raise the quorum to two: a loosening move is then the quorum's.
    kit.must(
        1,
        Op::AdminQuorumSet {
            quorum: azlin_policy::AdminQuorum::TWO,
        },
    );
    for device_n in [1u8, 3] {
        assert_eq!(
            kit.verdict(&kit.entry_by(device_n, &[], move_group(3, 1))),
            Err(Refusal::QuorumShort { have: 1, need: 2 }),
            "device {device_n} alone"
        );
    }
    assert_eq!(
        kit.verdict(&kit.entry_by(3, &[1], move_group(3, 1))),
        Ok(())
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[3], move_group(3, 1))),
        Ok(())
    );
    // The quorum is for the loosening only: a move that loosens nothing asks for none.
    assert_eq!(kit.verdict(&kit.entry_by(3, &[], move_group(6, 2))), Ok(()));
    assert_eq!(kit.verdict(&kit.entry_by(5, &[], move_group(2, 4))), Ok(()));
    // The group admin never loosens, quorum or none.
    assert!(matches!(
        kit.verdict(&kit.entry_by(5, &[1], move_group(3, 1))),
        Err(Refusal::PolicyForbids(PolicyReason::Loosens(_)))
    ));
    // Applied by the quorum, the move does loosen: the restriction is gone from `sub`.
    kit.must_with(3, &[1], move_group(3, 1));
    let after = kit
        .state
        .effective_policy_of(&group(3))
        .expect("a policy for sub");
    assert_eq!(after.external_sharing, ExternalSharing::Allowed);
    assert_eq!(after.credential_ttl, Span::hours(12));
}

#[test]
fn the_group_move_row_of_the_table_asks_the_policy_quorum_for_a_loosening_move_only() {
    use crate::authority::{row, QuorumFacts, QuorumRule};
    let rule = row(OpName::GroupMove).quorum;
    assert_eq!(
        rule,
        QuorumRule::MoveLoosensPolicy(azlin_policy::QuorumClass::PolicyChange)
    );
    assert_eq!(rule.class(QuorumFacts::default()), None);
    assert_eq!(
        rule.class(QuorumFacts {
            move_loosens_policy: true,
            ..QuorumFacts::default()
        }),
        Some(azlin_policy::QuorumClass::PolicyChange)
    );
}

// ---- 3. any member's entry can raise the restore epoch (X-DELTA-W1S47A-3) ---------------------------

/// A trust that accepts every record: the tests build the witness of a restore without a map key.
struct AcceptAll;

impl RestoreTrust for AcceptAll {
    fn check(&self, _record: &RestoreRecord) -> Result<(), RestoreRefusal> {
        Ok(())
    }
}

/// The witness of a restore of this epoch (a record that lists no position: only the epoch matters here).
fn restore(epoch: u64) -> VerifiedRestore {
    let record = RestoreRecord {
        epoch: RestoreEpoch::new(epoch),
        at: at(5_000),
        positions: Vec::new(),
        signatures: Vec::new(),
    };
    verify_record(&record, &AcceptAll).expect("a record that lists nothing is sound")
}

#[test]
fn an_entry_cannot_raise_the_restore_epoch() {
    // The minimised reproduction of X-DELTA-W1S47A-3 (azlin-escalate-e1, probe of the far epoch): a plain
    // member's device signs an ordinary app op that claims the largest restore epoch there is.
    let mut kit = team();
    kit.enrol(2, Role::Member, &[], None);
    let far = RestoreEpoch::new(u64::MAX);
    let claim = kit.entry_shaped(
        Signer::Device(device(2)),
        notes(),
        &[],
        Vec::new(),
        None,
        Shape {
            epoch: Some(far),
            ..Shape::default()
        },
    );
    let refusal = why(kit.verdict(&claim));
    assert_eq!(
        refusal,
        Refusal::EpochNotAnnounced {
            got: far,
            floor: RestoreEpoch::ZERO
        }
    );
    assert_eq!(refusal.class(), RefusalClass::Alarm);
    assert_eq!(
        refusal.to_string(),
        "the entry claims restore epoch 18446744073709551615 and the chain stands in 0: only an announced \
         restore moves it"
    );
    // The chain stands where it stood, and the owner's next entry (epoch 0) is admitted.
    assert_eq!(kit.state.tip().epoch(), RestoreEpoch::ZERO);
    kit.must(
        1,
        Op::MemberRole {
            member: member(2),
            role: Role::Guest,
            admin_scope: None,
        },
    );
    assert_eq!(kit.state.tip().epoch(), RestoreEpoch::ZERO);
    // Not even one epoch above the floor is an entry's to claim; the same for a device's own check.
    for epoch in [1u64, 2, 1_000] {
        let entry = kit.entry_shaped(
            Signer::Device(device(1)),
            notes(),
            &[],
            Vec::new(),
            None,
            Shape {
                epoch: Some(RestoreEpoch::new(epoch)),
                ..Shape::default()
            },
        );
        let expected = Refusal::EpochNotAnnounced {
            got: RestoreEpoch::new(epoch),
            floor: RestoreEpoch::ZERO,
        };
        assert_eq!(why(kit.verdict(&entry)), expected, "epoch {epoch}");
        let device_answer = replay(
            kit.state.clone(),
            std::slice::from_ref(&entry),
            &FakeScheme,
            &trusted(),
            kit.decoder(),
            &[kit.stamp()],
        );
        assert_eq!(device_answer.err(), Some((0, expected)), "epoch {epoch}");
    }
}

#[test]
fn only_a_verified_restore_moves_the_epoch_and_then_the_entries_of_the_new_epoch_are_the_ones() {
    let mut kit = team();
    kit.enrol(2, Role::Member, &[], None);
    let at_epoch = |kit: &SignedKit, epoch: u64| {
        kit.entry_shaped(
            Signer::Device(device(2)),
            notes(),
            &[],
            Vec::new(),
            None,
            Shape {
                epoch: Some(RestoreEpoch::new(epoch)),
                ..Shape::default()
            },
        )
    };
    // The restore of epoch 1 is announced and verified: the floor moves.
    let before = kit.state.state_hash();
    kit.state.raise_floor(&restore(1)).expect("above the floor");
    assert_eq!(kit.state.tip().epoch(), RestoreEpoch::new(1));
    assert_ne!(
        kit.state.state_hash(),
        before,
        "the epoch is part of the state hash"
    );
    // The entries of the lost timeline (epoch 0) are an alarm of the chain; the ones of the new epoch are
    // admitted; the ones of an epoch nobody announced are an alarm of ours.
    let lost = at_epoch(&kit, 0);
    let refusal = why(kit.verdict(&lost));
    assert_eq!(
        refusal,
        Refusal::Chain(ChainRefusal::BelowRestoreEpoch {
            got: RestoreEpoch::ZERO,
            floor: RestoreEpoch::new(1)
        })
    );
    assert_eq!(refusal.class(), RefusalClass::Alarm);
    assert_eq!(kit.verdict(&at_epoch(&kit, 1)), Ok(()));
    assert_eq!(
        why(kit.verdict(&at_epoch(&kit, 2))),
        Refusal::EpochNotAnnounced {
            got: RestoreEpoch::new(2),
            floor: RestoreEpoch::new(1)
        }
    );
    // An admitted entry of the new epoch leaves the floor where it was (it does not "move with" it).
    kit.push(at_epoch(&kit, 1)).expect("an entry of the epoch");
    assert_eq!(kit.state.tip().epoch(), RestoreEpoch::new(1));
    // A record that is not above the floor has been applied already; nothing changes, nothing lowers it.
    let state = kit.state.clone();
    assert_eq!(
        kit.state.raise_floor(&restore(1)),
        Err(RestoreRefusal::Replayed {
            record: RestoreEpoch::new(1),
            pinned: RestoreEpoch::new(1)
        })
    );
    assert_eq!(
        kit.state.raise_floor(&restore(0)),
        Err(RestoreRefusal::Replayed {
            record: RestoreEpoch::ZERO,
            pinned: RestoreEpoch::new(1)
        })
    );
    assert_eq!(kit.state, state);
    // A later restore moves it again, and a jump of several epochs is a restore like any other.
    kit.state.raise_floor(&restore(4)).expect("above the floor");
    assert_eq!(kit.state.tip().epoch(), RestoreEpoch::new(4));
    assert_eq!(kit.verdict(&at_epoch(&kit, 4)), Ok(()));
}

#[test]
fn an_account_born_after_a_restore_starts_in_that_epoch() {
    // The metadata group verifies the last restore record again and raises the floor of the empty state
    // before the genesis (TeamState::raise_floor documents it): the genesis carries exactly the floor.
    let mut kit = SignedKit::empty(AccountKind::Team);
    kit.state.raise_floor(&restore(3)).expect("above the floor");
    let genesis = |kit: &SignedKit, epoch: Option<u64>| {
        kit.entry_shaped(
            Signer::Device(device(1)),
            Op::Genesis {
                owner: member(1),
                device: device(1),
                member_keys: super::signed::dkeys(101),
                device_keys: super::signed::dkeys(1),
            },
            &[],
            Vec::new(),
            None,
            Shape {
                epoch: epoch.map(RestoreEpoch::new),
                ..Shape::default()
            },
        )
    };
    assert!(matches!(
        kit.verdict(&genesis(&kit, Some(0))),
        Err(Refusal::Chain(ChainRefusal::BelowRestoreEpoch { .. }))
    ));
    assert!(matches!(
        kit.verdict(&genesis(&kit, Some(4))),
        Err(Refusal::EpochNotAnnounced { .. })
    ));
    let entry = genesis(&kit, None);
    assert_eq!(entry.epoch, RestoreEpoch::new(3));
    kit.push(entry).expect("the genesis of the epoch");
    assert_eq!(kit.state.tip().epoch(), RestoreEpoch::new(3));
}

#[test]
fn every_refusal_has_a_class() {
    use RefusalClass::{Alarm, Refetch, Refuse};
    // The chain's own classes carry through.
    assert_eq!(Refusal::Chain(ChainRefusal::Corrupt).class(), Refetch);
    assert_eq!(
        Refusal::Chain(ChainRefusal::Fork {
            at: ChainSeq::MIN,
            have: hash(1),
            got: hash(2)
        })
        .class(),
        Alarm
    );
    assert_eq!(
        Refusal::Chain(ChainRefusal::BelowRestoreEpoch {
            got: RestoreEpoch::ZERO,
            floor: RestoreEpoch::new(1)
        })
        .class(),
        Alarm
    );
    assert_eq!(Refusal::Chain(ChainRefusal::StampBackwards).class(), Refuse);
    // The ones of this crate: a copy whose signature does not hold is fetched again, an epoch nobody
    // announced is an alarm, the rest is a refusal with a reason.
    assert_eq!(Refusal::BadSignature.class(), Refetch);
    assert_eq!(
        Refusal::EpochNotAnnounced {
            got: RestoreEpoch::new(2),
            floor: RestoreEpoch::ZERO
        }
        .class(),
        Alarm
    );
    for plain in [
        Refusal::SignerUnknown,
        Refusal::AssertionMissing,
        Refusal::LastOwner,
        Refusal::GroupCycle,
        Refusal::DelegationExpired,
        Refusal::InviteInvalid,
        Refusal::UnknownOp,
        Refusal::SubjectUnknown,
        Refusal::ConsentMissing,
        Refusal::QuorumShort { have: 1, need: 2 },
        Refusal::OutOfScope { op: OpName::App },
        Refusal::PolicyForbids(PolicyReason::NotManaged),
    ] {
        assert_eq!(plain.class(), Refuse, "{plain:?}");
    }
}

#[test]
fn join_and_invite_still_work_through_the_binding() {
    // The two ops whose signer is not yet in the state (`genesis` is covered by the kit itself): the new
    // device signs its own join, with the key it registers, and the decoder says what that entry is.
    let mut kit = team();
    kit.must(1, invite_op(2, &[]));
    let join = kit.entry(
        Signer::Device(device(2)),
        join_op(2, 2),
        &[1],
        Vec::new(),
        None,
    );
    assert_eq!(kit.verdict(&join), Ok(()));
    // The proposal carries the entry as signed.
    let proposal = verify_signatures(&kit.state, &join, &FakeScheme, &trusted(), kit.decoder())
        .expect("the signatures hold");
    assert_eq!(proposal.entry(), &join);
    assert!(admit(&kit.state, &proposal, kit.stamp()).is_ok());
}
