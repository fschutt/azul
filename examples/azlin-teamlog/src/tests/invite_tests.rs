//! Tests of invitations (check 12 of the brief; planning/architecture.md section 5.11 "a `member_invite`
//! entry carries the hash of a secret ... the joiner's device shows its fingerprint phrase and the
//! inviter's device confirms it ... before the `member_join` entry takes effect"; lane W1-S17c2). Every
//! entry goes through `verify_signatures` and `admit` (`SignedKit`).

use azlin_seams::{hash32, AccountKind, Hash32};

use super::kit::{device, hash, key, member};
use super::signed::{dkeys, invite_op, join_op, universe, SignedKit};
use crate::admit::{Invalid, Refusal};
use crate::hash::entry_hash;
use crate::invite::{invite_hash, INVITE_HASH_DOMAIN};
use crate::members::MemberState;
use crate::ops::{InviteSecret, Op, OpName};
use crate::proposal::Signer;
use crate::roles::Role;

/// A team account (an admin quorum of one) whose owner (member 1) invited member 2, who has not joined.
fn invited() -> SignedKit {
    let mut kit = SignedKit::new(AccountKind::Team);
    kit.must(1, invite_op(2, &[]));
    kit
}

/// The join of member `n` with device `n`, signed by that device, with `cosigners`.
fn join_by(kit: &SignedKit, n: u8, secret: u8, cosigners: &[u8]) -> crate::proposal::RawEntry {
    kit.entry(
        Signer::Device(device(n)),
        join_op(n, secret),
        cosigners,
        Vec::new(),
        None,
    )
}

#[test]
fn member_join_needs_the_invite_preimage() {
    let kit = invited();
    // The secret whose hash the invitation carries opens it.
    assert_eq!(kit.verdict(&join_by(&kit, 2, 2, &[1])), Ok(()));
    // Another secret does not: a guess, a copy of another invitation's, all zeroes.
    for wrong in [0u8, 1, 3, 9, 255] {
        assert_eq!(
            kit.verdict(&join_by(&kit, 2, wrong, &[1])),
            Err(Refusal::InviteInvalid),
            "secret {wrong}"
        );
    }
    // The hash is not the secret: the hash itself, as a "secret", does not open it.
    let hash_as_secret = Op::MemberJoin {
        member: member(2),
        device: device(2),
        member_keys: dkeys(152),
        device_keys: dkeys(2),
        secret: InviteSecret(*invite_hash(&InviteSecret([2; 32])).as_bytes()),
    };
    let entry = kit.entry(
        Signer::Device(device(2)),
        hash_as_secret,
        &[1],
        Vec::new(),
        None,
    );
    assert_eq!(kit.verdict(&entry), Err(Refusal::InviteInvalid));
    // A member nobody invited, and one the invitation is not for.
    assert_eq!(
        kit.verdict(&join_by(&kit, 9, 9, &[1])),
        Err(Refusal::InviteInvalid)
    );
}

#[test]
fn member_join_needs_the_inviters_cosignature() {
    // The admin 3 invites member 4; the owner is a different member.
    let mut kit = SignedKit::new(AccountKind::Team);
    kit.enrol(3, Role::Admin, &[], None);
    kit.enrol(5, Role::Member, &[], None);
    kit.must(3, invite_op(4, &[]));
    // No cosignature, a cosignature of an owner who is not the inviter, of a member, of the joiner
    // itself: the inviter's device has not confirmed.
    let cases: [&[u8]; 4] = [&[], &[1], &[5], &[4]];
    for cosigners in cases {
        assert_eq!(
            kit.verdict(&join_by(&kit, 4, 4, cosigners)),
            Err(Refusal::InviteInvalid),
            "cosigners {cosigners:?}"
        );
    }
    // The inviter's device confirms.
    assert_eq!(kit.verdict(&join_by(&kit, 4, 4, &[3])), Ok(()));
    // So does another device of the inviter.
    let mut two_devices = kit.clone();
    two_devices.must(
        3,
        Op::DeviceAdd {
            member: member(3),
            device: device(33),
            keys: dkeys(33),
        },
    );
    assert_eq!(kit.verdict(&join_by(&kit, 4, 4, &[3, 1])), Ok(()));
    assert_eq!(
        two_devices.verdict(&join_by(&two_devices, 4, 4, &[33])),
        Ok(())
    );
    // A cosignature that does not verify is no confirmation.
    let mut damaged = join_by(&kit, 4, 4, &[3]);
    damaged.cosigs[0].1[0] ^= 1;
    assert_eq!(kit.verdict(&damaged), Err(Refusal::InviteInvalid));
    // The inviter must still be an Active member: a suspended inviter confirms nothing.
    let mut suspended = kit.clone();
    suspended.must(1, Op::MemberSuspend { member: member(3) });
    assert_eq!(
        suspended.verdict(&join_by(&suspended, 4, 4, &[3])),
        Err(Refusal::InviteInvalid)
    );
}

#[test]
fn an_invitation_made_by_a_delegate_key_is_confirmed_by_an_owner_or_an_admin() {
    let mut kit = universe();
    let mut ops = std::collections::BTreeSet::new();
    ops.insert(OpName::MemberInvite);
    kit.must_with(
        1,
        &[4],
        Op::Delegate {
            key: key(40),
            ops,
            expires: None,
        },
    );
    // The identity service invites member 70; nobody invited it, so no member's device is "the inviter".
    let by_service = kit.entry(
        Signer::DelegateKey(key(40)),
        invite_op(70, &[3]),
        &[],
        Vec::new(),
        None,
    );
    kit.push(by_service).expect("a delegate key invites");
    assert_eq!(
        kit.state
            .member(&member(70))
            .expect("invited")
            .invite
            .as_ref()
            .map(|i| i.inviter),
        Some(None)
    );
    // A plain member's device does not confirm it; an owner's, an admin's do.
    assert_eq!(
        kit.verdict(&join_by(&kit, 70, 70, &[7])),
        Err(Refusal::InviteInvalid)
    );
    assert_eq!(
        kit.verdict(&join_by(&kit, 70, 70, &[5])),
        Err(Refusal::InviteInvalid),
        "a group admin neither"
    );
    assert_eq!(kit.verdict(&join_by(&kit, 70, 70, &[1])), Ok(()));
    assert_eq!(kit.verdict(&join_by(&kit, 70, 70, &[3])), Ok(()));
}

#[test]
fn invite_cannot_be_used_twice() {
    let mut kit = invited();
    kit.must(1, invite_op(3, &[]));
    let first = join_by(&kit, 2, 2, &[1]);
    kit.push(first).expect("the first join is admitted");
    assert_eq!(
        kit.state.member(&member(2)).map(|m| m.state),
        Some(MemberState::Active)
    );
    assert!(
        kit.state
            .member(&member(2))
            .expect("a member")
            .invite
            .is_none(),
        "used up"
    );
    // The same secret again, from a new device of the same member: the invitation is gone.
    let again = Op::MemberJoin {
        member: member(2),
        device: device(90),
        member_keys: dkeys(152),
        device_keys: dkeys(90),
        secret: InviteSecret([2; 32]),
    };
    let entry = kit.entry(Signer::Device(device(90)), again, &[1], Vec::new(), None);
    assert_eq!(kit.verdict(&entry), Err(Refusal::InviteInvalid));
    // That secret opens no other member's invitation either: member 3's hash is another secret's.
    let borrowed = Op::MemberJoin {
        member: member(3),
        device: device(91),
        member_keys: dkeys(153),
        device_keys: dkeys(91),
        secret: InviteSecret([2; 32]),
    };
    let entry = kit.entry(Signer::Device(device(91)), borrowed, &[1], Vec::new(), None);
    assert_eq!(kit.verdict(&entry), Err(Refusal::InviteInvalid));
    // Member 3 joins with its own.
    assert_eq!(kit.verdict(&join_by(&kit, 3, 3, &[1])), Ok(()));
}

#[test]
fn join_by_a_wrong_secret_refused() {
    let kit = invited();
    let wrong = join_by(&kit, 2, 7, &[1]);
    assert_eq!(kit.verdict(&wrong), Err(Refusal::InviteInvalid));
    // A device id that is already taken cannot be joined with, even with the right secret and a
    // confirmation: the device would be a second device with that id.
    let taken = Op::MemberJoin {
        member: member(2),
        device: device(1),
        member_keys: dkeys(152),
        device_keys: dkeys(1),
        secret: InviteSecret([2; 32]),
    };
    let entry = kit.entry(Signer::Device(device(1)), taken, &[1], Vec::new(), None);
    assert_eq!(kit.verdict(&entry), Err(Refusal::Invalid(Invalid::IdTaken)));
    // The new device signs with the key it registers: another signature does not do.
    let mut forged = join_by(&kit, 2, 2, &[1]);
    forged.sig = super::signed::sign_with([77; 32], &forged.preimage);
    assert_eq!(kit.verdict(&forged), Err(Refusal::BadSignature));
}

#[test]
fn the_invite_hash_is_a_domain_separated_hash_of_the_secret() {
    let secret = InviteSecret([7; 32]);
    assert_eq!(invite_hash(&secret), invite_hash(&InviteSecret([7; 32])));
    assert_ne!(invite_hash(&secret), invite_hash(&InviteSecret([8; 32])));
    assert_eq!(
        invite_hash(&secret),
        hash32(INVITE_HASH_DOMAIN, &[&[7u8; 32]])
    );
    // No other hash of the crate is this one, and it is not the secret.
    assert_ne!(invite_hash(&secret), entry_hash(&[7u8; 32]));
    assert_ne!(invite_hash(&secret), Hash32::from_bytes([7; 32]));
    assert_ne!(invite_hash(&secret), hash(7));
    assert_eq!(INVITE_HASH_DOMAIN, "azlin-teamlog-invite-v1");
}
