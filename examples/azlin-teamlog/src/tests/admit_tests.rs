//! Tests of `verify_signatures` and `admit` through the production functions (checks 5 to 8, 10 and 11
//! of the brief; lane W1-S17c2): the chain, the signatures, the preconditions of every entry, the
//! quorum of distinct people, the groups, the roles, the policy, the entitlement and the app ops. Every
//! entry is made and signed by `SignedKit` and checked by `verify_signatures` then `admit`.

use azlin_chain::{ChainRefusal, RefusalClass};
use azlin_policy::{AdminQuorum, PolicyPatch};
use azlin_seams::{AccountKind, ByteCount, ChainSeq, Span};

use super::kit::{account, at, device, group, hash, key, member};
use super::signed::{dkeys, invite_op, join_op, sign_with, universe, Shape, SignedKit};
use crate::admit::{admit, Invalid, PolicyReason, Refusal};
use crate::devices::DeviceState;
use crate::members::MemberState;
use crate::ops::{
    AppName, EntitlementState, EntitlementVersion, EscrowReason, Op, OpName, PolicyScope,
    APP_BODY_MAX,
};
use crate::proposal::{AssertionVerdict, Signer};
use crate::roles::Role;
use crate::state::{
    MAX_ESCROW_HOLDERS, MAX_GROUPS_PER_MEMBER, MAX_GROUP_DEPTH, MAX_PENDING_ESCROW_OPENS,
};
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

/// A passkey assertion that holds, made by device `n`.
fn av(n: u8) -> AssertionVerdict {
    AssertionVerdict {
        key: device(n),
        user_verified: true,
        challenge_matches: true,
        rp_ok: true,
    }
}

fn why(result: Result<(), Refusal>) -> Refusal {
    result.expect_err("the entry must be refused")
}

/// The entry an honest device 1 would have signed with the signed fields of `shape`: an app op, validly
/// signed, whose position, claimed time, restore epoch or account is what the shape says.
fn shaped(kit: &SignedKit, shape: Shape) -> crate::proposal::RawEntry {
    kit.entry_shaped(
        Signer::Device(device(1)),
        notes(),
        &[],
        Vec::new(),
        None,
        shape,
    )
}

// ---- check 5: the chain, the signatures, the signer ----------------------------------------------

#[test]
fn wrong_prev_refused_with_reason() {
    let mut kit = team();
    kit.make_group(1, None);
    let head = kit.state.head().expect("a head");
    // (Entries SIGNED with another position: a typed field changed after the signing is not what was
    // signed and is `BadSignature`, see `escalation_tests`.)
    let entry = shaped(
        &kit,
        Shape {
            prev: Some(hash(0x99)),
            ..Shape::default()
        },
    );
    assert_eq!(
        kit.verdict(&entry),
        Err(Refusal::Chain(ChainRefusal::WrongPrev {
            expected: head.hash,
            got: hash(0x99),
        }))
    );
    // A position that is not the next one: a gap.
    let gap = shaped(
        &kit,
        Shape {
            seq: Some(ChainSeq::new(head.seq.get() + 5)),
            ..Shape::default()
        },
    );
    assert_eq!(
        kit.verdict(&gap),
        Err(Refusal::Chain(ChainRefusal::Gap {
            expected: ChainSeq::new(head.seq.get() + 1),
            got: gap.seq,
        }))
    );
    // The entry that was applied is no longer the next one.
    let applied = kit.entry_by(1, &[], notes());
    kit.push(applied.clone()).expect("admitted");
    let next = kit.state.head().expect("a head").seq.next().expect("next");
    assert_eq!(
        kit.verdict(&applied),
        Err(Refusal::Chain(ChainRefusal::Gap {
            expected: next,
            got: applied.seq,
        }))
    );
    // An entry signed for another account does not belong to this log.
    let foreign = shaped(
        &kit,
        Shape {
            account: Some(azlin_seams::AccountId::from_bytes([0x44; 16])),
            ..Shape::default()
        },
    );
    assert_eq!(
        kit.verdict(&foreign),
        Err(Refusal::Chain(ChainRefusal::WrongChain))
    );
    assert_ne!(foreign.account, account());
    // The reason is a refusal, not an alarm; a fork would be one.
    assert_eq!(
        ChainRefusal::WrongPrev {
            expected: head.hash,
            got: hash(1)
        }
        .class(),
        RefusalClass::Refuse
    );
}

#[test]
fn a_stamp_that_runs_backwards_is_refused_whatever_the_entry_says_of_itself() {
    let kit = team();
    let entry = kit.entry_by(1, &[], notes());
    let proposal = verify_signatures(
        &kit.state,
        &entry,
        &azlin_seams::FakeScheme,
        &super::signed::trusted(),
        kit.decoder(),
    )
    .expect("the signatures hold");
    // The leader's stamp of the genesis was 1,010: an earlier stamp is a leader clock fault.
    assert_eq!(
        admit(&kit.state, &proposal, at(5)).err(),
        Some(Refusal::Chain(ChainRefusal::StampBackwards))
    );
    // The entry's own `at` is informational: any value of it, signed, is admitted at a good stamp.
    for claimed in [0u64, 5, 2_000_000_000] {
        let other = shaped(
            &kit,
            Shape {
                at: Some(at(claimed)),
                ..Shape::default()
            },
        );
        assert_eq!(kit.verdict(&other), Ok(()));
    }
}

#[test]
fn bad_signature_refused() {
    let kit = team();
    let mut flipped = kit.entry_by(1, &[], notes());
    flipped.sig[0] ^= 0x01;
    assert_eq!(kit.verdict(&flipped), Err(Refusal::BadSignature));
    // A signature of another key.
    let mut other_key = kit.entry_by(1, &[], notes());
    other_key.sig = sign_with([9; 32], &other_key.preimage);
    assert_eq!(kit.verdict(&other_key), Err(Refusal::BadSignature));
    // A signature that is too short, one over another preimage, and none at all.
    let mut short = kit.entry_by(1, &[], notes());
    short.sig.truncate(10);
    assert_eq!(kit.verdict(&short), Err(Refusal::BadSignature));
    let mut moved = kit.entry_by(1, &[], notes());
    moved.sig = sign_with([1; 32], b"the preimage of another entry");
    assert_eq!(kit.verdict(&moved), Err(Refusal::BadSignature));
    let mut empty = kit.entry_by(1, &[], notes());
    empty.sig.clear();
    assert_eq!(kit.verdict(&empty), Err(Refusal::BadSignature));
    // The good entry is admitted.
    assert_eq!(kit.verdict(&kit.entry_by(1, &[], notes())), Ok(()));
}

#[test]
fn a_corrupt_copy_is_fetched_again_and_is_no_fork() {
    let kit = team();
    let mut entry = kit.entry_by(1, &[], notes());
    entry.preimage[0] ^= 0x01;
    let refusal = why(kit.verdict(&entry));
    assert_eq!(refusal, Refusal::Chain(ChainRefusal::Corrupt));
    match refusal {
        Refusal::Chain(chain) => assert_eq!(chain.class(), RefusalClass::Refetch),
        other => panic!("a chain refusal expected, was {other:?}"),
    }
    // A changed hash with the preimage untouched is corrupt too.
    let mut moved = kit.entry_by(1, &[], notes());
    moved.hash = hash(0x13);
    assert_eq!(
        kit.verdict(&moved),
        Err(Refusal::Chain(ChainRefusal::Corrupt))
    );
    assert!(kit.entry_by(1, &[], notes()).is_intact());
}

#[test]
fn every_part_of_an_entry_has_a_limit() {
    let kit = team();
    let mut many = kit.entry_by(1, &[], notes());
    many.cosigs = (0..17u8).map(|n| (device(n), vec![0; 64])).collect();
    assert!(matches!(
        kit.verdict(&many),
        Err(Refusal::Invalid(Invalid::Limit(_)))
    ));
    let mut long_sig = kit.entry_by(1, &[], notes());
    long_sig.sig = vec![0; 4097];
    assert!(matches!(
        kit.verdict(&long_sig),
        Err(Refusal::Invalid(Invalid::Limit(_)))
    ));
    let mut assertions = kit.entry_by(1, &[], notes());
    assertions.assertions = (0..17u8).map(av).collect();
    assert!(matches!(
        kit.verdict(&assertions),
        Err(Refusal::Invalid(Invalid::Limit(_)))
    ));
    let mut sealed = kit.entry_by(1, &[], notes());
    sealed.sealed = Some(vec![0; 4097]);
    assert!(matches!(
        kit.verdict(&sealed),
        Err(Refusal::Invalid(Invalid::Limit(_)))
    ));
    let mut preimage = kit.entry_by(1, &[], notes());
    preimage.preimage = vec![0; 65_537];
    assert!(matches!(
        kit.verdict(&preimage),
        Err(Refusal::Invalid(Invalid::Limit(_)))
    ));
    // At the limit they are fine (the sealed field is opaque bytes: `admit` never reads it).
    let mut at_limit = kit.entry_by(1, &[], notes());
    at_limit.sealed = Some(vec![0; 4096]);
    assert_eq!(kit.verdict(&at_limit), Ok(()));
}

#[test]
fn unknown_signer_refused() {
    let kit = team();
    let stranger = kit.entry(Signer::Device(device(99)), notes(), &[], Vec::new(), None);
    assert_eq!(kit.verdict(&stranger), Err(Refusal::SignerUnknown));
    // A device that exists in no state of this account, signing a genesis-looking join, is no one either.
    let join = kit.entry(
        Signer::Device(device(98)),
        join_op(2, 2),
        &[1],
        Vec::new(),
        None,
    );
    assert!(kit.verdict(&join).is_err());
}

#[test]
fn pending_device_signs_nothing() {
    let mut kit = team();
    kit.enrol(2, Role::Member, &[], None);
    // The owner, who is not member 2, adds a device for it: the quorum path, so it starts in its hold.
    kit.must(
        1,
        Op::DeviceAdd {
            member: member(2),
            device: device(70),
            keys: dkeys(70),
        },
    );
    let hold_until = match kit.state.device(&device(70)).expect("a device").state {
        DeviceState::Pending { hold_until } => hold_until,
        other => panic!("pending expected, was {other:?}"),
    };
    let signed_by_70 = |kit: &SignedKit| kit.entry_by(70, &[], notes());
    assert_eq!(
        kit.verdict(&signed_by_70(&kit)),
        Err(Refusal::SignerNotActive {
            state: DeviceState::Pending { hold_until }
        })
    );
    // The hold ends AT hold_until: one second before, nothing; at it, the device signs.
    let before_end = hold_until.as_secs() - kit.stamp().as_secs() - 1;
    kit.advance(before_end);
    assert_eq!(kit.stamp().as_secs() + 1, hold_until.as_secs());
    assert!(matches!(
        kit.verdict(&signed_by_70(&kit)),
        Err(Refusal::SignerNotActive { .. })
    ));
    kit.advance(1);
    assert_eq!(kit.stamp(), hold_until);
    assert_eq!(kit.verdict(&signed_by_70(&kit)), Ok(()));
}

#[test]
fn revoked_device_signs_nothing() {
    let mut kit = team();
    kit.enrol(2, Role::Member, &[], None);
    assert_eq!(kit.verdict(&kit.entry_by(2, &[], notes())), Ok(()));
    kit.must(1, Op::DeviceRemove { device: device(2) });
    assert_eq!(
        kit.verdict(&kit.entry_by(2, &[], notes())),
        Err(Refusal::SignerNotActive {
            state: DeviceState::Revoked
        })
    );
}

#[test]
fn suspended_member_signs_nothing() {
    let mut kit = team();
    kit.enrol(2, Role::Member, &[], None);
    kit.must(1, Op::MemberSuspend { member: member(2) });
    assert_eq!(
        kit.verdict(&kit.entry_by(2, &[], notes())),
        Err(Refusal::MemberNotActive {
            state: MemberState::Suspended
        })
    );
}

#[test]
fn removed_member_signs_nothing() {
    let mut kit = team();
    kit.enrol(2, Role::Member, &[], None);
    kit.must(1, Op::MemberRemove { member: member(2) });
    // The removal revoked its devices, so the device is the first thing that fails.
    assert_eq!(
        kit.verdict(&kit.entry_by(2, &[], notes())),
        Err(Refusal::SignerNotActive {
            state: DeviceState::Revoked
        })
    );
    // A state in which the device is somehow Active still does not let a removed member sign.
    let mut damaged = kit.clone();
    damaged
        .state
        .devices
        .get_mut(&device(2))
        .expect("a device")
        .state = DeviceState::Active;
    assert_eq!(
        damaged.verdict(&damaged.entry_by(2, &[], notes())),
        Err(Refusal::MemberNotActive {
            state: MemberState::Removed
        })
    );
    // An invited member has no device; give it one and it still signs nothing.
    let mut invited = kit.clone();
    invited.must(1, invite_op(3, &[]));
    let mut with_device = invited.clone();
    let mut row = with_device
        .state
        .device(&device(1))
        .expect("a device")
        .clone();
    row.id = device(3);
    row.member = member(3);
    row.keys = dkeys(3);
    with_device.state.devices.insert(device(3), row);
    assert_eq!(
        with_device.verdict(&with_device.entry_by(3, &[], notes())),
        Err(Refusal::MemberNotActive {
            state: MemberState::Invited
        })
    );
}

#[test]
fn an_app_principal_signs_nothing_after_it_expires() {
    let mut kit = team();
    let mut scopes = std::collections::BTreeSet::new();
    scopes.insert(AppName::new("notes").expect("valid"));
    let expires = at(kit.stamp().as_secs() + 1_000);
    kit.must(
        1,
        Op::MemberInvite {
            member: member(5),
            kind: azlin_seams::MemberKind::App,
            role: Role::Member,
            invite_hash: crate::invite::invite_hash(&crate::ops::InviteSecret([5; 32])),
            groups: Default::default(),
            app: Some(crate::members::AppSpec {
                scopes,
                expires_at: Some(expires),
            }),
        },
    );
    let join = kit.entry(
        Signer::Device(device(5)),
        join_op(5, 5),
        &[1],
        Vec::new(),
        None,
    );
    kit.push(join).expect("the app principal joins");
    assert_eq!(kit.verdict(&kit.entry_by(5, &[], notes())), Ok(()));
    // Its scope names `notes` only.
    let mail = Op::App {
        name: AppName::new("mail").expect("valid"),
        body: vec![1],
    };
    assert_eq!(
        why(kit.verdict(&kit.entry_by(5, &[], mail))),
        Refusal::OutOfScope { op: OpName::App }
    );
    kit.advance(1_000);
    assert_eq!(
        kit.verdict(&kit.entry_by(5, &[], notes())),
        Err(Refusal::Invalid(Invalid::AppExpired))
    );
}

// ---- check 6: quorum of distinct people ------------------------------------------------------------

/// An admin change that needs the quorum of the universe: member 24 becomes an admin.
fn make_admin() -> Op {
    Op::MemberRole {
        member: member(24),
        role: Role::Admin,
        admin_scope: None,
    }
}

#[test]
fn quorum_of_two_devices_of_one_person_refused() {
    let kit = universe();
    // The owner has two devices, 1 and 11: both sign, and they are one person.
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[11], make_admin())),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
    // The signer's own device as the only cosigner is no second person either.
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[1], make_admin())),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
}

#[test]
fn quorum_of_two_people_admitted() {
    let kit = universe();
    for cosigner in [2u8, 3, 4] {
        assert_eq!(
            kit.verdict(&kit.entry_by(1, &[cosigner], make_admin())),
            Ok(()),
            "cosigner {cosigner}"
        );
    }
    // Both of the owner's devices and an admin: still counted as two people, enough.
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[11, 4], make_admin())),
        Ok(())
    );
    // An admin signs, an owner cosigns: the signer need not be the owner.
    let by_admin = Op::MemberRole {
        member: member(24),
        role: Role::GroupAdmin,
        admin_scope: Some(group(3)),
    };
    assert_eq!(
        kit.verdict(&kit.entry_by(3, &[], by_admin)),
        Ok(()),
        "no admin change, no quorum"
    );
    // A cosigner who is no admin counts for nothing.
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[7], make_admin())),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
}

#[test]
fn cosig_from_suspended_admin_not_counted() {
    let mut kit = universe();
    // Suspending an admin is an admin change: the owner and the second owner.
    kit.must_with(1, &[2], Op::MemberSuspend { member: member(4) });
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[4], make_admin())),
        Err(Refusal::QuorumShort { have: 1, need: 2 }),
        "a suspended admin's cosignature counts for nothing"
    );
    assert_eq!(kit.verdict(&kit.entry_by(1, &[3], make_admin())), Ok(()));
    // The same for a cosigner whose device is revoked.
    kit.must_with(1, &[2], Op::DeviceRemove { device: device(3) });
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[3], make_admin())),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
}

#[test]
fn cosig_over_another_hash_not_counted() {
    let kit = universe();
    // A cosignature of the second owner over ANOTHER entry (copied from it) is a signature that does not
    // verify here: it is left out, so the quorum is short.
    let other = kit.entry_by(1, &[2], Op::AuditKeySet { audit_pub: [1; 32] });
    let copied = other.cosigs.clone();
    assert_eq!(copied.len(), 1);
    let mut entry = kit.entry_by(1, &[], make_admin());
    entry.cosigs = copied;
    assert_eq!(
        kit.verdict(&entry),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
    // The same cosigner, signing this entry, counts.
    assert_eq!(kit.verdict(&kit.entry_by(1, &[2], make_admin())), Ok(()));
    // A cosignature of an unknown device is left out too, and so is a damaged one.
    let mut unknown = kit.entry_by(1, &[], make_admin());
    unknown.cosigs = vec![(device(99), sign_with([99; 32], &unknown.preimage))];
    assert_eq!(
        kit.verdict(&unknown),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
    let mut damaged = kit.entry_by(1, &[2], make_admin());
    damaged.cosigs[0].1[0] ^= 1;
    assert_eq!(
        kit.verdict(&damaged),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
}

#[test]
fn escrow_open_needs_two_different_admins_with_uv_assertions() {
    let kit = universe();
    let open = |subject: u8| Op::EscrowOpen {
        member: member(subject),
        reason: EscrowReason::LegalRequest,
    };
    let entry = |signer: u8, cosigners: &[u8], assertions: Vec<AssertionVerdict>| {
        kit.entry(
            Signer::Device(device(signer)),
            open(24),
            cosigners,
            assertions,
            None,
        )
    };
    // Two different admins, each with an assertion with user verification over this entry: admitted.
    assert_eq!(kit.verdict(&entry(1, &[3], vec![av(1), av(3)])), Ok(()));
    // One admin alone.
    assert_eq!(
        kit.verdict(&entry(1, &[], vec![av(1)])),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
    // Two devices of one person, each with an assertion: one person.
    assert_eq!(
        kit.verdict(&entry(1, &[11], vec![av(1), av(11)])),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
    // A cosigner who is no admin.
    assert_eq!(
        kit.verdict(&entry(1, &[7], vec![av(1), av(7)])),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
    // Two admins, but the second has no assertion: the quorum would be met without it, so the missing
    // assertion is the reason.
    assert_eq!(
        kit.verdict(&entry(1, &[3], vec![av(1)])),
        Err(Refusal::AssertionMissing)
    );
    assert_eq!(
        kit.verdict(&entry(1, &[3], Vec::new())),
        Err(Refusal::AssertionMissing)
    );
    // An assertion by a device that did not sign the entry is no assertion of a signer.
    assert_eq!(
        kit.verdict(&entry(1, &[3], vec![av(1), av(4)])),
        Err(Refusal::AssertionMissing)
    );
    // Only an owner or an admin signs it; a group admin with a quorum of admins does not.
    assert_eq!(
        why(kit.verdict(&kit.entry(
            Signer::Device(device(5)),
            open(24),
            &[3],
            vec![av(5), av(3)],
            None
        ))),
        Refusal::RoleMaySignNo {
            op: OpName::EscrowOpen,
            role: Some(Role::GroupAdmin)
        }
    );
    // The subject must be managed, and must be a member that has something to open.
    let mut private = kit.clone();
    private
        .state
        .members
        .get_mut(&member(24))
        .expect("a member")
        .managed = false;
    assert_eq!(
        private.verdict(&private.entry(
            Signer::Device(device(1)),
            open(24),
            &[3],
            vec![av(1), av(3)],
            None
        )),
        Err(Refusal::PolicyForbids(PolicyReason::NotManaged))
    );
    let unknown = kit.entry(
        Signer::Device(device(1)),
        open(77),
        &[3],
        vec![av(1), av(3)],
        None,
    );
    assert_eq!(kit.verdict(&unknown), Err(Refusal::SubjectUnknown));
    // A member who never joined has nothing to open.
    let mut invited = kit.clone();
    invited.must(1, invite_op(60, &[]));
    let open_invited = invited.entry(
        Signer::Device(device(1)),
        open(60),
        &[3],
        vec![av(1), av(3)],
        None,
    );
    assert_eq!(
        invited.verdict(&open_invited),
        Err(Refusal::PolicyForbids(PolicyReason::NotManaged)),
        "an invited member is not managed in a team account"
    );
}

#[test]
fn assertion_over_another_entry_hash_refused() {
    let kit = universe();
    let open = Op::EscrowOpen {
        member: member(24),
        reason: EscrowReason::LeaverHandover,
    };
    let mut other_challenge = av(3);
    other_challenge.challenge_matches = false;
    let entry = kit.entry(
        Signer::Device(device(1)),
        open,
        &[3],
        vec![av(1), other_challenge],
        None,
    );
    assert_eq!(kit.verdict(&entry), Err(Refusal::AssertionMissing));
}

#[test]
fn assertion_without_user_verification_refused() {
    let kit = universe();
    let open = Op::EscrowOpen {
        member: member(24),
        reason: EscrowReason::LeaverHandover,
    };
    let mut not_verified = av(3);
    not_verified.user_verified = false;
    let entry = kit.entry(
        Signer::Device(device(1)),
        open.clone(),
        &[3],
        vec![av(1), not_verified],
        None,
    );
    assert_eq!(kit.verdict(&entry), Err(Refusal::AssertionMissing));
    // Nor does an assertion for another relying party count.
    let mut wrong_rp = av(3);
    wrong_rp.rp_ok = false;
    let entry = kit.entry(
        Signer::Device(device(1)),
        open,
        &[3],
        vec![av(1), wrong_rp],
        None,
    );
    assert_eq!(kit.verdict(&entry), Err(Refusal::AssertionMissing));
}

// ---- check 7: groups -------------------------------------------------------------------------------

#[test]
fn group_move_cycle_refused() {
    let kit = universe();
    // Group 2 into its own child 3, and into itself.
    for new_parent in [3u8, 2] {
        assert_eq!(
            kit.verdict(&kit.entry_by(
                1,
                &[],
                Op::GroupMove {
                    group: group(2),
                    new_parent: group(new_parent),
                },
            )),
            Err(Refusal::GroupCycle),
            "group 2 under group {new_parent}"
        );
    }
    // A top-level group never moves, and a move that is no cycle is admitted.
    assert_eq!(
        kit.verdict(&kit.entry_by(
            1,
            &[],
            Op::GroupMove {
                group: group(1),
                new_parent: group(4),
            },
        )),
        Err(Refusal::Invalid(Invalid::RootGroup))
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(
            1,
            &[],
            Op::GroupMove {
                group: group(3),
                new_parent: group(4),
            },
        )),
        Ok(())
    );
    // Unknown groups.
    assert_eq!(
        kit.verdict(&kit.entry_by(
            1,
            &[],
            Op::GroupMove {
                group: group(99),
                new_parent: group(4),
            },
        )),
        Err(Refusal::SubjectUnknown)
    );
}

#[test]
fn group_move_needs_scope_over_both_ends() {
    let kit = universe();
    let mv = |from: u8, to: u8| Op::GroupMove {
        group: group(from),
        new_parent: group(to),
    };
    // The group admin of group 2: group 3 is below it, group 4 is not.
    assert_eq!(kit.verdict(&kit.entry_by(5, &[], mv(3, 2))), Ok(()));
    assert_eq!(
        kit.verdict(&kit.entry_by(5, &[], mv(3, 4))),
        Err(Refusal::OutOfScope {
            op: OpName::GroupMove
        })
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(5, &[], mv(4, 3))),
        Err(Refusal::OutOfScope {
            op: OpName::GroupMove
        })
    );
    // Helpdesk moves no group at all.
    assert_eq!(
        why(kit.verdict(&kit.entry_by(6, &[], mv(3, 2)))),
        Refusal::RoleMaySignNo {
            op: OpName::GroupMove,
            role: Some(Role::Helpdesk)
        }
    );
    // An admin moves anywhere that makes no cycle.
    assert_eq!(kit.verdict(&kit.entry_by(3, &[], mv(3, 4))), Ok(()));
}

#[test]
fn group_admin_cannot_leave_its_subtree() {
    let kit = universe();
    // It moves its own group (and its subtree) out under a sibling department.
    assert_eq!(
        kit.verdict(&kit.entry_by(
            5,
            &[],
            Op::GroupMove {
                group: group(2),
                new_parent: group(4),
            },
        )),
        Err(Refusal::OutOfScope {
            op: OpName::GroupMove
        })
    );
    // It makes no top-level group, and none under another department.
    for parent in [None, Some(group(4)), Some(group(1))] {
        assert_eq!(
            kit.verdict(&kit.entry_by(
                5,
                &[],
                Op::GroupCreate {
                    group: group(60),
                    parent,
                },
            )),
            Err(Refusal::OutOfScope {
                op: OpName::GroupCreate
            }),
            "parent {parent:?}"
        );
    }
    assert_eq!(
        kit.verdict(&kit.entry_by(
            5,
            &[],
            Op::GroupCreate {
                group: group(60),
                parent: Some(group(3)),
            },
        )),
        Ok(())
    );
    // It takes into its group nobody who belongs to another department, and nobody into another
    // department's group; a member of no group, or of its own subtree, it takes.
    let add = |group_n: u8, member_n: u8| Op::GroupMember {
        group: group(group_n),
        member: member(member_n),
        present: true,
    };
    assert_eq!(
        kit.verdict(&kit.entry_by(5, &[], add(2, 34))),
        Err(Refusal::OutOfScope {
            op: OpName::GroupMember
        })
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(5, &[], add(4, 40))),
        Err(Refusal::OutOfScope {
            op: OpName::GroupMember
        })
    );
    assert_eq!(kit.verdict(&kit.entry_by(5, &[], add(2, 40))), Ok(()));
    assert_eq!(kit.verdict(&kit.entry_by(5, &[], add(2, 24))), Ok(()));
    // It takes nobody out of another department's group.
    let remove = Op::GroupMember {
        group: group(4),
        member: member(34),
        present: false,
    };
    assert_eq!(
        kit.verdict(&kit.entry_by(5, &[], remove)),
        Err(Refusal::OutOfScope {
            op: OpName::GroupMember
        })
    );
}

#[test]
fn scope_flows_down_nested_groups_only() {
    let mut kit = universe();
    // A member of the parent group (1), a member of a sibling department (4), a member of nothing.
    kit.enrol(50, Role::Member, &[1], None);
    let suspend = |n: u8| Op::MemberSuspend { member: member(n) };
    // Group admin 5 has the scope of group 2: group 3 is below it, so its members are in scope.
    assert_eq!(kit.verdict(&kit.entry_by(5, &[], suspend(24))), Ok(()));
    assert_eq!(
        kit.verdict(&kit.entry_by(5, &[], suspend(7))),
        Ok(()),
        "a member of group 3 too"
    );
    for outside in [34u8, 40, 50] {
        assert_eq!(
            kit.verdict(&kit.entry_by(5, &[], suspend(outside))),
            Err(Refusal::OutOfScope {
                op: OpName::MemberSuspend
            }),
            "member {outside}"
        );
    }
    // The scope follows the tree as it is now: group 3 moves under group 4, and its members leave the
    // scope of group 2 with it.
    kit.must(
        1,
        Op::GroupMove {
            group: group(3),
            new_parent: group(4),
        },
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(5, &[], suspend(24))),
        Err(Refusal::OutOfScope {
            op: OpName::MemberSuspend
        })
    );
    // And the other way: group 4 under group 2 brings its members in.
    kit.must(
        1,
        Op::GroupMove {
            group: group(4),
            new_parent: group(2),
        },
    );
    assert_eq!(kit.verdict(&kit.entry_by(5, &[], suspend(34))), Ok(()));
    assert_eq!(
        kit.verdict(&kit.entry_by(5, &[], suspend(24))),
        Ok(()),
        "group 3 is below group 4, below group 2"
    );
    // Helpdesk has the same scope.
    assert_eq!(kit.verdict(&kit.entry_by(6, &[], suspend(34))), Ok(()));
    assert_eq!(
        kit.verdict(&kit.entry_by(6, &[], suspend(50))),
        Err(Refusal::OutOfScope {
            op: OpName::MemberSuspend
        })
    );
}

#[test]
fn the_group_tree_has_a_depth_limit() {
    let mut kit = team();
    kit.make_group(1, None);
    for n in 2..=(MAX_GROUP_DEPTH as u8 + 1) {
        kit.make_group(n, Some(n - 1));
    }
    // Group 17 is at depth 16, the deepest there may be; a child of it would be at depth 17.
    let deeper = Op::GroupCreate {
        group: group(100),
        parent: Some(group(MAX_GROUP_DEPTH as u8 + 1)),
    };
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], deeper)),
        Err(Refusal::Invalid(Invalid::Limit(
            "the depth of the group tree"
        )))
    );
    // A move that would make the tree deeper is refused: a chain of two groups (103 > 101 > 102) cannot
    // go under the deepest group, one level at the top would.
    kit.make_group(103, None);
    kit.make_group(101, Some(103));
    kit.make_group(102, Some(101));
    assert_eq!(
        kit.verdict(&kit.entry_by(
            1,
            &[],
            Op::GroupMove {
                group: group(101),
                new_parent: group(MAX_GROUP_DEPTH as u8 + 1),
            },
        )),
        Err(Refusal::Invalid(Invalid::Limit(
            "the depth of the group tree"
        )))
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(
            1,
            &[],
            Op::GroupMove {
                group: group(102),
                new_parent: group(MAX_GROUP_DEPTH as u8),
            },
        )),
        Ok(())
    );
}

// ---- check 8: roles and the last owner ------------------------------------------------------------

#[test]
fn last_owner_cannot_be_demoted_removed_or_suspended() {
    let mut kit = team();
    kit.enrol(2, Role::Admin, &[], None);
    let owner_gone = |kit: &SignedKit| kit.state.active_owner_count(None);
    assert_eq!(owner_gone(&kit), 1);
    // The sole owner removes itself: the account would have no owner.
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], Op::MemberRemove { member: member(1) })),
        Err(Refusal::LastOwner)
    );
    // It demotes itself, or suspends itself: refused as well (nobody changes its own role, and an owner
    // is suspended by another owner only).
    assert!(kit
        .verdict(&kit.entry_by(
            1,
            &[],
            Op::MemberRole {
                member: member(1),
                role: Role::Admin,
                admin_scope: None,
            },
        ))
        .is_err());
    assert!(kit
        .verdict(&kit.entry_by(1, &[], Op::MemberSuspend { member: member(1) }))
        .is_err());
    // An admin cannot take the owner away.
    for (name, op) in [
        (OpName::MemberRemove, Op::MemberRemove { member: member(1) }),
        (
            OpName::MemberSuspend,
            Op::MemberSuspend { member: member(1) },
        ),
        (
            OpName::MemberRole,
            Op::MemberRole {
                member: member(1),
                role: Role::Member,
                admin_scope: None,
            },
        ),
    ] {
        assert_eq!(
            why(kit.verdict(&kit.entry_by(2, &[], op))),
            Refusal::RoleMaySignNo {
                op: name,
                role: Some(Role::Admin)
            }
        );
    }
    assert_eq!(owner_gone(&kit), 1, "nothing was applied");
    // With a second owner the first can go; then the second is the last.
    kit.must(
        1,
        Op::MemberRole {
            member: member(2),
            role: Role::Owner,
            admin_scope: None,
        },
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(2, &[], Op::MemberRemove { member: member(1) })),
        Ok(())
    );
    kit.must(2, Op::MemberRemove { member: member(1) });
    assert_eq!(
        kit.verdict(&kit.entry_by(2, &[], Op::MemberRemove { member: member(2) })),
        Err(Refusal::LastOwner)
    );
}

#[test]
fn admin_cannot_make_or_touch_an_owner() {
    let kit = universe();
    // Admin 3, with the quorum met by admin 4, does what an admin may not.
    let tries = [
        (
            OpName::MemberRole,
            Op::MemberRole {
                member: member(2),
                role: Role::Member,
                admin_scope: None,
            },
        ),
        (
            OpName::MemberRole,
            Op::MemberRole {
                member: member(24),
                role: Role::Owner,
                admin_scope: None,
            },
        ),
        (
            OpName::MemberSuspend,
            Op::MemberSuspend { member: member(2) },
        ),
        (OpName::MemberRemove, Op::MemberRemove { member: member(2) }),
        (OpName::DeviceRemove, Op::DeviceRemove { device: device(2) }),
        (
            OpName::DeviceAdd,
            Op::DeviceAdd {
                member: member(2),
                device: device(100),
                keys: dkeys(100),
            },
        ),
    ];
    for (name, op) in tries {
        assert_eq!(
            why(kit.verdict(&kit.entry_by(3, &[4], op.clone()))),
            Refusal::RoleMaySignNo {
                op: name,
                role: Some(Role::Admin)
            },
            "{op:?}"
        );
    }
    // The owner may, with the quorum of an admin change.
    assert_eq!(
        kit.verdict(&kit.entry_by(
            1,
            &[4],
            Op::MemberRole {
                member: member(24),
                role: Role::Owner,
                admin_scope: None,
            },
        )),
        Ok(())
    );
    // An admin does not make an admin either (it assigns group admin and below).
    assert_eq!(
        why(kit.verdict(&kit.entry_by(
            3,
            &[4],
            Op::MemberRole {
                member: member(24),
                role: Role::Admin,
                admin_scope: None,
            },
        ))),
        Refusal::RoleMaySignNo {
            op: OpName::MemberRole,
            role: Some(Role::Admin)
        }
    );
}

#[test]
fn group_admin_cannot_make_a_group_admin() {
    let kit = universe();
    let to = |subject: u8, role: Role, scope: Option<u8>| Op::MemberRole {
        member: member(subject),
        role,
        admin_scope: scope.map(group),
    };
    for role in [Role::GroupAdmin, Role::Admin, Role::Owner, Role::Helpdesk] {
        assert_eq!(
            why(kit.verdict(&kit.entry_by(5, &[4], to(24, role, Some(3))))),
            Refusal::RoleMaySignNo {
                op: OpName::MemberRole,
                role: Some(Role::GroupAdmin)
            },
            "{role:?}"
        );
    }
    // It makes a member of its subtree a guest, and a guest a member.
    assert_eq!(
        kit.verdict(&kit.entry_by(5, &[], to(24, Role::Guest, None))),
        Ok(())
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(5, &[], to(25, Role::Member, None))),
        Ok(())
    );
    // It does not touch another group admin of its subtree, nor anybody outside it.
    assert_eq!(
        why(kit.verdict(&kit.entry_by(5, &[], to(22, Role::Member, None)))),
        Refusal::RoleMaySignNo {
            op: OpName::MemberRole,
            role: Some(Role::GroupAdmin)
        }
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(5, &[], to(34, Role::Guest, None))),
        Err(Refusal::OutOfScope {
            op: OpName::MemberRole
        })
    );
}

#[test]
fn no_self_role_change() {
    let kit = universe();
    for (device_n, role) in [
        (1u8, Role::Admin),
        (3, Role::Owner),
        (5, Role::Member),
        (7, Role::Guest),
    ] {
        // Device n belongs to member n in this universe.
        let own = Op::MemberRole {
            member: member(device_n),
            role,
            admin_scope: None,
        };
        assert!(
            matches!(
                kit.verdict(&kit.entry_by(device_n, &[4], own)),
                Err(Refusal::RoleMaySignNo {
                    op: OpName::MemberRole,
                    ..
                })
            ),
            "member {device_n}"
        );
    }
}

#[test]
fn helpdesk_cannot_approve_a_device_alone() {
    let mut kit = team();
    kit.make_group(1, None);
    kit.enrol(2, Role::Helpdesk, &[1], Some(1));
    kit.enrol(3, Role::Member, &[1], None);
    let add = Op::DeviceAdd {
        member: member(3),
        device: device(70),
        keys: dkeys(70),
    };
    assert_eq!(
        why(kit.verdict(&kit.entry_by(2, &[], add.clone()))),
        Refusal::RoleMaySignNo {
            op: OpName::DeviceAdd,
            role: Some(Role::Helpdesk)
        }
    );
    // Not with a cosigner either, and not for a member of its own scope.
    assert_eq!(
        why(kit.verdict(&kit.entry_by(2, &[1], add))),
        Refusal::RoleMaySignNo {
            op: OpName::DeviceAdd,
            role: Some(Role::Helpdesk)
        }
    );
    // The owner may (a team account: one person), and the member's own device may.
    assert_eq!(
        kit.verdict(&kit.entry_by(
            1,
            &[],
            Op::DeviceAdd {
                member: member(3),
                device: device(70),
                keys: dkeys(70),
            },
        )),
        Ok(())
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(
            3,
            &[],
            Op::DeviceAdd {
                member: member(3),
                device: device(70),
                keys: dkeys(70),
            },
        )),
        Ok(())
    );
    // Helpdesk opens no escrow, changes no role, makes no group.
    for op in [
        Op::GroupCreate {
            group: group(9),
            parent: Some(group(1)),
        },
        Op::MemberRole {
            member: member(3),
            role: Role::Guest,
            admin_scope: None,
        },
    ] {
        assert!(matches!(
            kit.verdict(&kit.entry_by(2, &[], op)),
            Err(Refusal::RoleMaySignNo { .. })
        ));
    }
}

#[test]
fn helpdesk_may_suspend_in_scope() {
    let kit = universe();
    let suspend = |n: u8| Op::MemberSuspend { member: member(n) };
    assert_eq!(kit.verdict(&kit.entry_by(6, &[], suspend(24))), Ok(()));
    assert_eq!(
        kit.verdict(&kit.entry_by(6, &[], suspend(34))),
        Err(Refusal::OutOfScope {
            op: OpName::MemberSuspend
        })
    );
    // Never somebody above it, even with a quorum.
    assert_eq!(
        why(kit.verdict(&kit.entry_by(6, &[4], suspend(21)))),
        Refusal::RoleMaySignNo {
            op: OpName::MemberSuspend,
            role: Some(Role::Helpdesk)
        }
    );
    // Helpdesk removes nobody.
    assert_eq!(
        why(kit.verdict(&kit.entry_by(6, &[], Op::MemberRemove { member: member(24) }))),
        Refusal::RoleMaySignNo {
            op: OpName::MemberRemove,
            role: Some(Role::Helpdesk)
        }
    );
}

#[test]
fn business_role_change_to_admin_needs_two_people() {
    // A business account asks two different people for an admin change.
    let mut business = SignedKit::new(AccountKind::Business);
    business.must(1, invite_op(2, &[]));
    let join = business.entry(
        Signer::Device(device(2)),
        join_op(2, 2),
        &[1],
        Vec::new(),
        None,
    );
    business.push(join).expect("the join is admitted");
    let promote = Op::MemberRole {
        member: member(2),
        role: Role::Admin,
        admin_scope: None,
    };
    assert_eq!(
        business.verdict(&business.entry_by(1, &[], promote.clone())),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
    assert_eq!(
        business.verdict(&business.entry_by(1, &[1], promote)),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
    // With the quorum of the universe (two) a second person makes it.
    let kit = universe();
    assert_eq!(kit.verdict(&kit.entry_by(1, &[4], make_admin())), Ok(()));
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], make_admin())),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
}

// ---- the other ops of the table -------------------------------------------------------------------

#[test]
fn device_add_by_a_quorum_needs_the_policy_to_allow_it_and_respects_the_limit() {
    // A personal account's device_approval is `self`: no quorum enrols a device.
    let mut personal = SignedKit::new(AccountKind::Personal);
    personal.must(1, invite_op(2, &[]));
    let join = personal.entry(
        Signer::Device(device(2)),
        join_op(2, 2),
        &[1],
        Vec::new(),
        None,
    );
    personal.push(join).expect("the join is admitted");
    let add = |new: u8| Op::DeviceAdd {
        member: member(2),
        device: device(new),
        keys: dkeys(new),
    };
    assert_eq!(
        personal.verdict(&personal.entry_by(1, &[], add(70))),
        Err(Refusal::PolicyForbids(PolicyReason::DeviceApprovalSelfOnly))
    );
    assert_eq!(
        personal.verdict(&personal.entry_by(2, &[], add(70))),
        Ok(()),
        "its own device may"
    );
    // The most devices a member may have (10): the member has one; nine more, then the eleventh.
    for n in 0..9u8 {
        personal.must(2, add(70 + n));
    }
    assert_eq!(
        personal.verdict(&personal.entry_by(2, &[], add(90))),
        Err(Refusal::PolicyForbids(PolicyReason::MaxDevices { max: 10 }))
    );
    // A revoked device frees its place, but its id is never used again.
    personal.must(2, Op::DeviceRemove { device: device(70) });
    assert_eq!(
        personal.verdict(&personal.entry_by(2, &[], add(90))),
        Ok(())
    );
    assert_eq!(
        personal.verdict(&personal.entry_by(2, &[], add(70))),
        Err(Refusal::Invalid(Invalid::IdTaken))
    );
}

#[test]
fn device_remove_is_the_members_own_or_an_admins() {
    let kit = universe();
    // The device itself, another device of the same member, an admin.
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], Op::DeviceRemove { device: device(1) })),
        Ok(())
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(11, &[], Op::DeviceRemove { device: device(1) })),
        Ok(())
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(3, &[], Op::DeviceRemove { device: device(24) })),
        Ok(())
    );
    // Another member's device, by a member who is no admin.
    assert_eq!(
        why(kit.verdict(&kit.entry_by(7, &[], Op::DeviceRemove { device: device(8) }))),
        Refusal::RoleMaySignNo {
            op: OpName::DeviceRemove,
            role: Some(Role::Member)
        }
    );
    // An admin's device by someone else: the quorum of an admin change; an unknown device; a revoked one.
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], Op::DeviceRemove { device: device(3) })),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[4], Op::DeviceRemove { device: device(3) })),
        Ok(())
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], Op::DeviceRemove { device: device(99) })),
        Err(Refusal::SubjectUnknown)
    );
    let mut gone = kit.clone();
    gone.must(1, Op::DeviceRemove { device: device(11) });
    assert_eq!(
        gone.verdict(&gone.entry_by(1, &[], Op::DeviceRemove { device: device(11) })),
        Err(Refusal::Invalid(Invalid::DeviceRevoked))
    );
}

#[test]
fn member_invite_rules() {
    let kit = universe();
    let invite =
        |n: u8, role: Role, kind: azlin_seams::MemberKind, groups: &[u8]| Op::MemberInvite {
            member: member(n),
            kind,
            role,
            invite_hash: hash(n),
            groups: groups.iter().map(|g| group(*g)).collect(),
            app: None,
        };
    let person = azlin_seams::MemberKind::Person;
    // Only member and guest are invited; a role comes later, by `member_role`.
    for role in [Role::Owner, Role::Admin, Role::GroupAdmin, Role::Helpdesk] {
        assert_eq!(
            why(kit.verdict(&kit.entry_by(1, &[], invite(70, role, person, &[])))),
            Refusal::RoleMaySignNo {
                op: OpName::MemberInvite,
                role: Some(Role::Owner)
            },
            "{role:?}"
        );
    }
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], invite(70, Role::Guest, person, &[]))),
        Ok(())
    );
    // The id is the inviter's choice and is never reused.
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], invite(24, Role::Member, person, &[]))),
        Err(Refusal::Invalid(Invalid::IdTaken))
    );
    // An unknown group.
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], invite(70, Role::Member, person, &[99]))),
        Err(Refusal::SubjectUnknown)
    );
    // An app principal has app columns and a person has none.
    assert_eq!(
        kit.verdict(&kit.entry_by(
            1,
            &[],
            invite(70, Role::Member, azlin_seams::MemberKind::App, &[])
        )),
        Err(Refusal::Invalid(Invalid::Args(
            "an app principal has app columns and a person has none"
        )))
    );
    // A group admin invites into its subtree, and into nothing else.
    assert_eq!(
        kit.verdict(&kit.entry_by(5, &[], invite(70, Role::Member, person, &[3]))),
        Ok(())
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(5, &[], invite(70, Role::Member, person, &[4]))),
        Err(Refusal::OutOfScope {
            op: OpName::MemberInvite
        })
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(5, &[], invite(70, Role::Member, person, &[]))),
        Err(Refusal::OutOfScope {
            op: OpName::MemberInvite
        })
    );
    // A member invites nobody.
    assert!(matches!(
        kit.verdict(&kit.entry_by(7, &[], invite(70, Role::Member, person, &[3]))),
        Err(Refusal::RoleMaySignNo { .. })
    ));
}

#[test]
fn member_managed_rules() {
    let mut kit = universe();
    let managed = |n: u8, value: bool| Op::MemberManaged {
        member: member(n),
        managed: value,
    };
    // Managed to private asks for the quorum (EscrowChange).
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], managed(24, false))),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[4], managed(24, false))),
        Ok(())
    );
    // Private to managed asks for the member's own device as a cosigner, and for no quorum.
    kit.must_with(1, &[4], managed(24, false));
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], managed(24, true))),
        Err(Refusal::ConsentMissing)
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[4], managed(24, true))),
        Err(Refusal::ConsentMissing),
        "an admin's cosignature is no consent"
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[24], managed(24, true))),
        Ok(())
    );
    // No change needs nothing.
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], managed(25, true))),
        Ok(())
    );
    // A group admin does not set it (it is the admin's); an unknown member.
    assert!(matches!(
        kit.verdict(&kit.entry_by(5, &[4], managed(24, true))),
        Err(Refusal::RoleMaySignNo { .. })
    ));
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[4], managed(77, false))),
        Err(Refusal::SubjectUnknown)
    );
}

#[test]
fn escrow_set_and_audit_key_and_admin_quorum_ask_the_quorum_of_their_class() {
    let kit = universe();
    let escrow = |holders: &[u8]| Op::EscrowSet {
        escrow_pub: [3; 32],
        holders: holders.iter().map(|n| member(*n)).collect(),
    };
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], escrow(&[1, 2]))),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
    assert_eq!(kit.verdict(&kit.entry_by(1, &[4], escrow(&[1, 2]))), Ok(()));
    // Every holder is a member who is a person; no more than the limit.
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[4], escrow(&[1, 99]))),
        Err(Refusal::SubjectUnknown)
    );
    let many: Vec<u8> = (20..(20 + MAX_ESCROW_HOLDERS as u8 + 1)).collect();
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[4], escrow(&many))),
        Err(Refusal::Invalid(Invalid::Limit("escrow holders")))
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], Op::AuditKeySet { audit_pub: [4; 32] })),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(3, &[4], Op::AuditKeySet { audit_pub: [4; 32] })),
        Ok(())
    );
    // Lowering the quorum needs the CURRENT quorum.
    let lower = Op::AdminQuorumSet {
        quorum: AdminQuorum::ONE,
    };
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], lower.clone())),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
    assert_eq!(kit.verdict(&kit.entry_by(1, &[2], lower)), Ok(()));
    // Once lowered, one person acts alone.
    let mut lowered = kit.clone();
    lowered.must_with(
        1,
        &[2],
        Op::AdminQuorumSet {
            quorum: AdminQuorum::ONE,
        },
    );
    assert_eq!(
        lowered.verdict(&lowered.entry_by(1, &[], make_admin())),
        Ok(())
    );
}

// ---- check 10: policy and entitlement --------------------------------------------------------------

#[test]
fn policy_set_loosening_a_group_refused() {
    let mut kit = universe();
    let ttl = |hours: u32| PolicyPatch {
        credential_ttl: Some(Span::hours(hours)),
        ..PolicyPatch::none()
    };
    // Group 2 tightens the credential lifetime to 6 hours (the account's is 12).
    kit.must_with(
        1,
        &[4],
        Op::PolicySet {
            scope: PolicyScope::Group(group(2)),
            patch: ttl(6),
        },
    );
    // Group 3, below it, asks for 10 hours: looser than what it inherits.
    let loosen = Op::PolicySet {
        scope: PolicyScope::Group(group(3)),
        patch: ttl(10),
    };
    let refusal = why(kit.verdict(&kit.entry_by(1, &[4], loosen)));
    match refusal {
        Refusal::PolicyForbids(PolicyReason::Loosens(fields)) => {
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].field, azlin_policy::Field::CredentialTtl);
            assert_eq!(fields[0].parent, "21600 s");
            assert_eq!(fields[0].child, "36000 s");
        }
        other => panic!("a loosening expected, was {other:?}"),
    }
    // Tightening further, or asking for the same, is fine; the account's own policy may change anything.
    let tighter = Op::PolicySet {
        scope: PolicyScope::Group(group(3)),
        patch: ttl(2),
    };
    assert_eq!(kit.verdict(&kit.entry_by(1, &[4], tighter)), Ok(()));
    let same = Op::PolicySet {
        scope: PolicyScope::Group(group(3)),
        patch: ttl(6),
    };
    assert_eq!(kit.verdict(&kit.entry_by(1, &[4], same)), Ok(()));
    let account_looser = Op::PolicySet {
        scope: PolicyScope::Account,
        patch: ttl(48),
    };
    assert_eq!(kit.verdict(&kit.entry_by(1, &[4], account_looser)), Ok(()));
    // A group may not change what only the account sets.
    let managed = Op::PolicySet {
        scope: PolicyScope::Group(group(3)),
        patch: PolicyPatch {
            padding: Some(azlin_policy::PaddingPolicy::Full),
            ..PolicyPatch::none()
        },
    };
    assert!(matches!(
        kit.verdict(&kit.entry_by(1, &[4], managed)),
        Err(Refusal::PolicyForbids(PolicyReason::Loosens(_)))
    ));
    // The quorum applies to a policy change.
    let plain = Op::PolicySet {
        scope: PolicyScope::Group(group(3)),
        patch: ttl(2),
    };
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], plain)),
        Err(Refusal::QuorumShort { have: 1, need: 2 })
    );
}

#[test]
fn policy_set_by_group_admin_outside_subtree_refused() {
    let kit = universe();
    let tight = PolicyPatch {
        credential_ttl: Some(Span::hours(2)),
        ..PolicyPatch::none()
    };
    let group_policy = |g: u8| Op::PolicySet {
        scope: PolicyScope::Group(group(g)),
        patch: tight.clone(),
    };
    // Group admin 5 (scope group 2), with the quorum met by an admin.
    assert_eq!(kit.verdict(&kit.entry_by(5, &[4], group_policy(3))), Ok(()));
    assert_eq!(kit.verdict(&kit.entry_by(5, &[4], group_policy(2))), Ok(()));
    assert_eq!(
        kit.verdict(&kit.entry_by(5, &[4], group_policy(4))),
        Err(Refusal::OutOfScope {
            op: OpName::PolicySet
        })
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(5, &[4], group_policy(1))),
        Err(Refusal::OutOfScope {
            op: OpName::PolicySet
        })
    );
    // It does not set the account's policy at all.
    let account_policy = Op::PolicySet {
        scope: PolicyScope::Account,
        patch: tight.clone(),
    };
    assert_eq!(
        why(kit.verdict(&kit.entry_by(5, &[4], account_policy))),
        Refusal::RoleMaySignNo {
            op: OpName::PolicySet,
            role: Some(Role::GroupAdmin)
        }
    );
    // An unknown group.
    assert_eq!(
        kit.verdict(&kit.entry_by(3, &[4], group_policy(99))),
        Err(Refusal::SubjectUnknown)
    );
}

#[test]
fn policy_epoch_exhaustion_refuses_a_change_and_not_a_no_op() {
    let mut kit = team();
    kit.state.policy_epoch = azlin_seams::PolicyEpoch::new(u32::MAX);
    let change = Op::PolicySet {
        scope: PolicyScope::Account,
        patch: PolicyPatch {
            credential_ttl: Some(Span::hours(1)),
            ..PolicyPatch::none()
        },
    };
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], change)),
        Err(Refusal::PolicyForbids(PolicyReason::EpochExhausted))
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(
            1,
            &[],
            Op::PolicySet {
                scope: PolicyScope::Account,
                patch: PolicyPatch::none(),
            },
        )),
        Ok(())
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(
            1,
            &[],
            Op::AdminQuorumSet {
                quorum: AdminQuorum::new(4).expect("four"),
            },
        )),
        Err(Refusal::PolicyForbids(PolicyReason::EpochExhausted))
    );
}

fn entitlement(version: u64) -> Op {
    Op::Entitlement {
        version: EntitlementVersion(version),
        state: EntitlementState::Active,
        quota: ByteCount::gib(10),
        seats: 5,
    }
}

#[test]
fn entitlement_only_by_the_payments_key() {
    let mut kit = team();
    let by_payments = |kit: &SignedKit, version: u64| {
        kit.entry(
            Signer::PaymentsKey,
            entitlement(version),
            &[],
            Vec::new(),
            None,
        )
    };
    assert_eq!(kit.verdict(&by_payments(&kit, 1)), Ok(()));
    // A device, even the owner's, may not sign it.
    assert_eq!(
        why(kit.verdict(&kit.entry_by(1, &[], entitlement(1)))),
        Refusal::RoleMaySignNo {
            op: OpName::Entitlement,
            role: Some(Role::Owner)
        }
    );
    // A delegate key may not: nothing delegates an entitlement.
    let delegate = kit.entry(
        SignedKit::delegate_signer(40),
        entitlement(1),
        &[],
        Vec::new(),
        None,
    );
    assert_eq!(
        kit.verdict(&delegate),
        Err(Refusal::DelegationDoesNotCover {
            op: OpName::Entitlement
        })
    );
    // A key that is not the payments key does not verify; a signature of the owner's key neither.
    let mut forged = by_payments(&kit, 1);
    forged.sig = sign_with([0xFB; 32], &forged.preimage);
    assert_eq!(kit.verdict(&forged), Err(Refusal::BadSignature));
    forged.sig = sign_with([1; 32], &forged.preimage);
    assert_eq!(kit.verdict(&forged), Err(Refusal::BadSignature));
    // With no trusted payments key no entitlement is admitted.
    let proposal = verify_signatures(
        &kit.state,
        &by_payments(&kit, 1),
        &azlin_seams::FakeScheme,
        &crate::proposal::TrustedKeys::none(),
        kit.decoder(),
    );
    assert_eq!(proposal.err(), Some(Refusal::BadSignature));
    // The payments key signs an entitlement and nothing else.
    let invite = kit.entry(
        Signer::PaymentsKey,
        invite_op(2, &[]),
        &[],
        Vec::new(),
        None,
    );
    assert_eq!(
        kit.verdict(&invite),
        Err(Refusal::RoleMaySignNo {
            op: OpName::MemberInvite,
            role: None
        })
    );
    // The higher version wins: the same or an older one is refused.
    let first = by_payments(&kit, 3);
    kit.push(first).expect("admitted");
    for version in [3u64, 2, 1] {
        assert_eq!(
            kit.verdict(&by_payments(&kit, version)),
            Err(Refusal::PolicyForbids(PolicyReason::EntitlementNotNewer)),
            "version {version}"
        );
    }
    assert_eq!(kit.verdict(&by_payments(&kit, 4)), Ok(()));
}

#[test]
fn entitlement_cannot_add_a_member_or_change_a_role() {
    let mut kit = universe();
    let members_before: Vec<_> = kit.state.members().cloned().collect();
    // The payments key cannot sign a member_invite, a member_role or a group op.
    let attempts = [
        invite_op(70, &[]),
        make_admin(),
        Op::GroupCreate {
            group: group(60),
            parent: None,
        },
        Op::MemberSuspend { member: member(24) },
    ];
    for op in attempts {
        let entry = kit.entry(Signer::PaymentsKey, op.clone(), &[], Vec::new(), None);
        assert!(
            matches!(
                kit.verdict(&entry),
                Err(Refusal::RoleMaySignNo { role: None, .. })
            ),
            "{op:?}"
        );
    }
    // And an entitlement changes no member and no role.
    let entitlement_entry = kit.entry(Signer::PaymentsKey, entitlement(9), &[], Vec::new(), None);
    kit.push(entitlement_entry).expect("admitted");
    let members_after: Vec<_> = kit.state.members().cloned().collect();
    assert_eq!(members_before, members_after);
    assert_eq!(kit.state.entitlement().map(|e| e.seats), Some(5));
}

// ---- check 11: app ops and unknown ops ---------------------------------------------------------------

#[test]
fn app_op_stored_never_interpreted_and_changes_no_row() {
    let mut kit = universe();
    let before = kit.state.clone();
    let entry = kit.entry_by(
        7,
        &[],
        Op::App {
            name: AppName::new("notes").expect("valid"),
            body: vec![9, 9, 9],
        },
    );
    let effects = kit.push(entry).expect("a member signs an app op");
    assert!(effects.registry.is_empty(), "no row changes");
    assert!(!effects.credentials_invalidated);
    assert_eq!(effects.notices.len(), 1);
    // Nothing but the chain moved.
    let mut after = kit.state.clone();
    after.tip = before.tip.clone();
    assert_eq!(after, before);
    // The body is whatever bytes the app wants: it is not read.
    for body in [
        vec![],
        vec![0xFF; 32],
        b"{\"op\":\"member_role\",\"role\":\"owner\"}".to_vec(),
    ] {
        let entry = kit.entry_by(
            7,
            &[],
            Op::App {
                name: AppName::new("notes").expect("valid"),
                body,
            },
        );
        assert_eq!(kit.verdict(&entry), Ok(()));
    }
    // Every role but a guest signs one: an owner, an admin, a group admin, helpdesk, a member.
    for signer in [1u8, 3, 5, 6, 7] {
        assert_eq!(
            kit.verdict(&kit.entry_by(signer, &[], notes())),
            Ok(()),
            "device {signer}"
        );
    }
}

#[test]
fn app_op_by_guest_refused() {
    let kit = universe();
    assert_eq!(
        why(kit.verdict(&kit.entry_by(8, &[], notes()))),
        Refusal::RoleMaySignNo {
            op: OpName::App,
            role: Some(Role::Guest)
        }
    );
    // A delegate key signs no app op either.
    let delegate = kit.entry(
        SignedKit::delegate_signer(40),
        notes(),
        &[],
        Vec::new(),
        None,
    );
    assert!(matches!(
        kit.verdict(&delegate),
        Err(Refusal::DelegationDoesNotCover { .. })
    ));
}

#[test]
fn app_op_over_size_cap_refused() {
    let kit = team();
    let sized = |len: usize| Op::App {
        name: AppName::new("notes").expect("valid"),
        body: vec![0; len],
    };
    let at_cap = APP_BODY_MAX.get() as usize;
    assert_eq!(kit.verdict(&kit.entry_by(1, &[], sized(at_cap))), Ok(()));
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], sized(at_cap + 1))),
        Err(Refusal::Invalid(Invalid::Limit("the body of an app op")))
    );
}

#[test]
fn unknown_op_refused_with_reason() {
    use crate::ops::UnknownOp;
    for word in [
        "space_grant",
        "account_delete",
        "grant_everything",
        "",
        "MEMBER_ROLE",
    ] {
        let refusal = OpName::from_wire(word).map_err(Refusal::from);
        assert_eq!(refusal, Err(Refusal::UnknownOp), "{word:?}");
    }
    assert_eq!(
        Refusal::from(UnknownOp).to_string(),
        "not an op of the team log"
    );
    assert_eq!(OpName::from_wire("member_role"), Ok(OpName::MemberRole));
}

#[test]
fn the_first_entry_is_the_genesis_and_only_the_first() {
    // On a started log a genesis is a second genesis.
    let kit = team();
    let genesis = Op::Genesis {
        owner: member(9),
        device: device(9),
        member_keys: dkeys(9),
        device_keys: dkeys(9),
    };
    let again = kit.entry(Signer::Device(device(9)), genesis, &[], Vec::new(), None);
    match kit.verdict(&again) {
        Err(Refusal::Chain(ChainRefusal::GenesisAgain { have, got })) => {
            assert_eq!(Some(have), kit.state.genesis_hash());
            assert_eq!(got, again.hash);
        }
        other => panic!("a second genesis expected, was {other:?}"),
    }
    // On an empty log nothing but a genesis is the first entry: a join of a new device (which signs with
    // the key it registers, so its signature holds) is not.
    let empty = SignedKit::empty(AccountKind::Team);
    let join = empty.entry(
        Signer::Device(device(2)),
        join_op(2, 2),
        &[],
        Vec::new(),
        None,
    );
    assert_eq!(
        empty.verdict(&join),
        Err(Refusal::Chain(ChainRefusal::NotGenesis))
    );
    // A genesis is signed with the key it registers, and by the device it names.
    let genesis = |device_n: u8, key_n: u8| Op::Genesis {
        owner: member(1),
        device: device(device_n),
        member_keys: dkeys(101),
        device_keys: dkeys(key_n),
    };
    assert_eq!(
        empty.verdict(&empty.entry_by(1, &[], genesis(1, 1))),
        Ok(())
    );
    assert_eq!(
        empty.verdict(&empty.entry_by(2, &[], genesis(1, 1))),
        Err(Refusal::SignerUnknown),
        "another device than the one the genesis names"
    );
    assert_eq!(
        empty.verdict(&empty.entry_by(1, &[], genesis(1, 2))),
        Err(Refusal::BadSignature),
        "a signature that is not by the key the genesis registers"
    );
    // The seq of a genesis is 0 and its prev is zero.
    let signed_with = |shape: Shape| {
        empty.entry_shaped(
            Signer::Device(device(1)),
            genesis(1, 1),
            &[],
            Vec::new(),
            None,
            shape,
        )
    };
    let late = signed_with(Shape {
        seq: Some(ChainSeq::new(1)),
        ..Shape::default()
    });
    assert_eq!(
        empty.verdict(&late),
        Err(Refusal::Chain(ChainRefusal::NotGenesis))
    );
    let wrong_prev = signed_with(Shape {
        prev: Some(hash(5)),
        ..Shape::default()
    });
    assert_eq!(
        empty.verdict(&wrong_prev),
        Err(Refusal::Chain(ChainRefusal::NotGenesis))
    );
}

#[test]
fn admit_returns_the_witness_of_the_entry_it_admitted() {
    let kit = team();
    let entry = kit.entry_by(1, &[], notes());
    let admitted = kit.check(&entry).expect("admitted");
    assert_eq!(admitted.seq(), entry.seq);
    assert_eq!(admitted.hash(), entry.hash);
    assert_eq!(admitted.stamp(), kit.stamp());
    assert_eq!(admitted.op(), &entry.op);
}

// ---- the decisions the first mutant round showed unguarded -----------------------------------------
//
// Each test below turns one surviving hand mutant red (the mutant round of lane W1-S17c: 153 exact-text
// mutants, 15 of which survived the first 462 tests).

#[test]
fn verify_signatures_alone_refuses_an_entry_of_another_account() {
    let kit = team();
    // Signed for another account, in every part: the copy is intact, the typed fields are the signed
    // ones and the signature holds. Only the account says that it is not this log's.
    let foreign = shaped(
        &kit,
        Shape {
            account: Some(azlin_seams::AccountId::from_bytes([0x44; 16])),
            ..Shape::default()
        },
    );
    assert!(foreign.is_intact());
    let proposal = verify_signatures(
        &kit.state,
        &foreign,
        &azlin_seams::FakeScheme,
        &super::signed::trusted(),
        kit.decoder(),
    );
    assert_eq!(
        proposal.err(),
        Some(Refusal::Chain(ChainRefusal::WrongChain))
    );
}

#[test]
fn a_delegate_keys_signature_must_verify() {
    let mut kit = team();
    kit.must(
        1,
        Op::Delegate {
            key: key(40),
            ops: [OpName::MemberInvite].into_iter().collect(),
            expires: None,
        },
    );
    let mut entry = kit.entry(
        SignedKit::delegate_signer(40),
        invite_op(70, &[]),
        &[],
        Vec::new(),
        None,
    );
    assert_eq!(kit.verdict(&entry), Ok(()));
    // The signature of another key is no signature of the delegate key; nor is a flipped bit of its own.
    let right = entry.sig.clone();
    entry.sig = sign_with([41; 32], &entry.preimage);
    assert_eq!(kit.verdict(&entry), Err(Refusal::BadSignature));
    entry.sig = right;
    entry.sig[0] ^= 0x01;
    assert_eq!(kit.verdict(&entry), Err(Refusal::BadSignature));
}

#[test]
fn the_payments_key_signs_an_entitlement_and_nothing_else() {
    // It creates no account: a genesis it signs is refused.
    let empty = SignedKit::empty(AccountKind::Team);
    let genesis = Op::Genesis {
        owner: member(1),
        device: device(1),
        member_keys: dkeys(101),
        device_keys: dkeys(1),
    };
    let created = empty.entry(Signer::PaymentsKey, genesis, &[], Vec::new(), None);
    assert_eq!(
        why(empty.verdict(&created)),
        Refusal::RoleMaySignNo {
            op: OpName::Genesis,
            role: None
        }
    );
    // It joins nobody: no role signs a join, and the role gate never sees it.
    let mut kit = team();
    kit.must(1, invite_op(2, &[]));
    let join = kit.entry(Signer::PaymentsKey, join_op(2, 2), &[1], Vec::new(), None);
    assert_eq!(
        why(kit.verdict(&join)),
        Refusal::RoleMaySignNo {
            op: OpName::MemberJoin,
            role: None
        }
    );
    // And it signs no other op.
    for op in [
        notes(),
        Op::MemberSuspend { member: member(1) },
        Op::GroupCreate {
            group: group(9),
            parent: None,
        },
        Op::AuditKeySet { audit_pub: [4; 32] },
    ] {
        let name = op.name();
        let entry = kit.entry(Signer::PaymentsKey, op, &[], Vec::new(), None);
        assert_eq!(
            why(kit.verdict(&entry)),
            Refusal::RoleMaySignNo {
                op: name,
                role: None
            }
        );
    }
}

#[test]
fn an_app_principal_signs_until_the_second_before_its_expiry_and_not_at_it() {
    let mut kit = team();
    let mut scopes = std::collections::BTreeSet::new();
    scopes.insert(AppName::new("notes").expect("valid"));
    let expires = at(kit.stamp().as_secs() + 1_000);
    kit.must(
        1,
        Op::MemberInvite {
            member: member(5),
            kind: azlin_seams::MemberKind::App,
            role: Role::Member,
            invite_hash: crate::invite::invite_hash(&crate::ops::InviteSecret([5; 32])),
            groups: Default::default(),
            app: Some(crate::members::AppSpec {
                scopes,
                expires_at: Some(expires),
            }),
        },
    );
    let join = kit.entry(
        Signer::Device(device(5)),
        join_op(5, 5),
        &[1],
        Vec::new(),
        None,
    );
    kit.push(join).expect("the app principal joins");
    // One second before its `expires_at` it signs; at it, it is over.
    kit.advance(expires.as_secs() - kit.stamp().as_secs() - 1);
    assert_eq!(kit.stamp().as_secs() + 1, expires.as_secs());
    assert_eq!(kit.verdict(&kit.entry_by(5, &[], notes())), Ok(()));
    kit.advance(1);
    assert_eq!(kit.stamp(), expires);
    assert_eq!(
        kit.verdict(&kit.entry_by(5, &[], notes())),
        Err(Refusal::Invalid(Invalid::AppExpired))
    );
}

#[test]
fn a_proposal_is_checked_against_the_device_key_of_the_state_that_admits_it() {
    let mut kit = team();
    kit.enrol(2, Role::Member, &[], None);
    let entry = kit.entry_by(2, &[], notes());
    let proposal = verify_signatures(
        &kit.state,
        &entry,
        &azlin_seams::FakeScheme,
        &super::signed::trusted(),
        kit.decoder(),
    )
    .expect("the signatures hold");
    assert!(admit(&kit.state, &proposal, kit.stamp()).is_ok());
    // Another state of the same log in which device 2 has another key: the signature that verified is
    // not a signature of that device.
    let mut other = kit.clone();
    other
        .state
        .devices
        .get_mut(&device(2))
        .expect("a device")
        .keys
        .sign = [77; 32];
    assert_eq!(
        admit(&other.state, &proposal, other.stamp()).err(),
        Some(Refusal::BadSignature)
    );
}

#[test]
fn device_add_is_for_an_active_member_only() {
    let mut kit = team();
    kit.enrol(2, Role::Member, &[], None);
    kit.enrol(3, Role::Member, &[], None);
    kit.enrol(5, Role::Member, &[], None);
    kit.must(1, Op::MemberSuspend { member: member(2) });
    kit.must(1, Op::MemberRemove { member: member(3) });
    kit.must(1, invite_op(4, &[]));
    let add = |n: u8| Op::DeviceAdd {
        member: member(n),
        device: device(70),
        keys: dkeys(70),
    };
    for (n, state) in [
        (2u8, MemberState::Suspended),
        (3, MemberState::Removed),
        (4, MemberState::Invited),
    ] {
        assert_eq!(
            kit.verdict(&kit.entry_by(1, &[], add(n))),
            Err(Refusal::Invalid(Invalid::WrongMemberState(state))),
            "member {n}"
        );
    }
    assert_eq!(kit.verdict(&kit.entry_by(1, &[], add(5))), Ok(()));
}

#[test]
fn a_scoped_role_needs_a_scope_group_and_the_others_have_none() {
    let kit = universe();
    let role_op = |role: Role, scope: Option<u8>| Op::MemberRole {
        member: member(40),
        role,
        admin_scope: scope.map(group),
    };
    for role in [Role::GroupAdmin, Role::Helpdesk] {
        assert_eq!(
            kit.verdict(&kit.entry_by(1, &[], role_op(role, None))),
            Err(Refusal::Invalid(Invalid::Args(
                "a group admin or helpdesk member needs a scope group"
            ))),
            "{role:?}"
        );
        assert_eq!(
            kit.verdict(&kit.entry_by(1, &[], role_op(role, Some(2)))),
            Ok(()),
            "{role:?}"
        );
        assert_eq!(
            kit.verdict(&kit.entry_by(1, &[], role_op(role, Some(99)))),
            Err(Refusal::SubjectUnknown),
            "{role:?}"
        );
    }
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], role_op(Role::Guest, Some(2)))),
        Err(Refusal::Invalid(Invalid::Args(
            "only a group admin or a helpdesk member has a scope group"
        )))
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], role_op(Role::Guest, None))),
        Ok(())
    );
}

#[test]
fn an_app_principal_is_never_managed_has_no_escrow_and_holds_none() {
    let mut kit = universe();
    kit.must(
        1,
        Op::MemberInvite {
            member: member(70),
            kind: azlin_seams::MemberKind::App,
            role: Role::Member,
            invite_hash: hash(70),
            groups: Default::default(),
            app: Some(crate::members::AppSpec {
                scopes: Default::default(),
                expires_at: None,
            }),
        },
    );
    assert_eq!(
        kit.verdict(&kit.entry_by(
            1,
            &[],
            Op::MemberManaged {
                member: member(70),
                managed: true
            }
        )),
        Err(Refusal::Invalid(Invalid::Args(
            "an app principal is never managed"
        )))
    );
    let open = kit.entry(
        Signer::Device(device(1)),
        Op::EscrowOpen {
            member: member(70),
            reason: EscrowReason::LegalRequest,
        },
        &[3],
        vec![av(1), av(3)],
        None,
    );
    assert_eq!(
        kit.verdict(&open),
        Err(Refusal::Invalid(Invalid::Args(
            "an app principal has no escrow"
        )))
    );
    let holder = Op::EscrowSet {
        escrow_pub: [3; 32],
        holders: [member(1), member(70)].into_iter().collect(),
    };
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[4], holder)),
        Err(Refusal::Invalid(Invalid::Args(
            "an escrow holder is a person"
        )))
    );
}

#[test]
fn a_member_is_in_at_most_the_most_groups_one_member_may_be_in() {
    let mut kit = team();
    kit.enrol(2, Role::Member, &[], None);
    let first = 100u8;
    for n in 0..MAX_GROUPS_PER_MEMBER as u8 {
        kit.make_group(first + n, None);
        kit.must(
            1,
            Op::GroupMember {
                group: group(first + n),
                member: member(2),
                present: true,
            },
        );
    }
    kit.make_group(200, None);
    let one_more = Op::GroupMember {
        group: group(200),
        member: member(2),
        present: true,
    };
    assert_eq!(
        kit.verdict(&kit.entry_by(1, &[], one_more.clone())),
        Err(Refusal::Invalid(Invalid::Limit("groups of one member")))
    );
    // Leaving a group frees a place.
    kit.must(
        1,
        Op::GroupMember {
            group: group(first),
            member: member(2),
            present: false,
        },
    );
    assert_eq!(kit.verdict(&kit.entry_by(1, &[], one_more)), Ok(()));
}

#[test]
fn escrow_openings_that_wait_are_bounded() {
    let mut kit = universe();
    let open = |kit: &SignedKit| {
        kit.entry(
            Signer::Device(device(1)),
            Op::EscrowOpen {
                member: member(24),
                reason: EscrowReason::LegalRequest,
            },
            &[3],
            vec![av(1), av(3)],
            None,
        )
    };
    for _ in 0..MAX_PENDING_ESCROW_OPENS {
        let entry = open(&kit);
        kit.push(entry).expect("an opening that waits is admitted");
    }
    assert_eq!(
        kit.state.pending_escrow_opens().count(),
        MAX_PENDING_ESCROW_OPENS
    );
    assert_eq!(
        kit.verdict(&open(&kit)),
        Err(Refusal::Invalid(Invalid::Limit(
            "escrow openings that wait"
        )))
    );
}

#[test]
fn a_group_may_replace_its_own_patch_with_a_looser_one_that_still_tightens_what_it_inherits() {
    let mut kit = universe();
    let ttl = |hours: u32| PolicyPatch {
        credential_ttl: Some(Span::hours(hours)),
        ..PolicyPatch::none()
    };
    let set = |hours: u32| Op::PolicySet {
        scope: PolicyScope::Group(group(3)),
        patch: ttl(hours),
    };
    kit.must_with(1, &[4], set(2));
    // Looser than its own earlier patch and still tighter than the 12 hours it inherits: a new patch is
    // measured against the level above, not against the patch it replaces.
    assert_eq!(kit.verdict(&kit.entry_by(1, &[4], set(6))), Ok(()));
    // Looser than what it inherits: refused, whatever it set before.
    assert!(matches!(
        kit.verdict(&kit.entry_by(1, &[4], set(24))),
        Err(Refusal::PolicyForbids(PolicyReason::Loosens(_)))
    ));
}

#[test]
fn an_assertion_holds_only_when_all_three_of_its_facts_do() {
    for bits in 0..8u8 {
        let verdict = AssertionVerdict {
            key: device(1),
            user_verified: bits & 1 != 0,
            challenge_matches: bits & 2 != 0,
            rp_ok: bits & 4 != 0,
        };
        assert_eq!(verdict.holds(), bits == 7, "{verdict:?}");
    }
}

#[test]
fn an_entry_is_intact_only_when_its_hash_is_the_hash_of_its_preimage() {
    let kit = team();
    let entry = kit.entry_by(1, &[], notes());
    assert!(entry.is_intact());
    let mut other_hash = entry.clone();
    other_hash.hash = hash(5);
    assert!(!other_hash.is_intact());
    let mut other_preimage = entry.clone();
    other_preimage.preimage.push(0);
    assert!(!other_preimage.is_intact());
}
