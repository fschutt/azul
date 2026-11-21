//! The member and device keys and the group keys (lane W1-S15c1): C1 (a bundle is drawn in a
//! documented order and independently), C2 (the public bundle is the registry's columns), C8 (a
//! member key is wrapped to each device, ONE wrap per added device), C9 (the path member key ->
//! group key -> space key, with real envelopes and a directory), the member and group parts of C11
//! (an entropy failure stops everything) and C12 (no secret is copied or printed).
//!
//! Implements planning/architecture.md section 5.5 "Keys" and section 5.6.
//! Run with `--features test-support` (the test doubles) and, for the hybrid wraps, `pq`.

#![cfg(feature = "test-support")]
#![allow(dead_code)]

mod hierarchy_common;

use azlin_keys::envelope::{Alg, Envelope, OpenError, Recipient, RecipientSecret, WrapError};
use azlin_keys::group::{open_group_key, wrap_group_key_to_member, GroupKey, GroupPublic};
use azlin_keys::member::{
    open_member_key, wrap_member_key_to_device, KeyBundle, KeyError, PublicBundle,
};
use azlin_keys::space::{
    open_space_key, open_space_key_via_group, wrap_space_key, DedupChoice, GroupRoute, SpaceKey,
    WrapTarget,
};
use azlin_keys::{
    Entropy, EntropyError, Epoch, FailingEntropy, GroupId, KeyId, MemberId, Secret, SeededEntropy,
    SignKeypair, SpaceId, VerifyAt,
};
use hierarchy_common::{
    directory, pat, pat16, pat32, pat64, share_a_window, to_hex, Device, Recording, ScriptedEntropy,
};

fn epoch(n: u32) -> Epoch {
    Epoch::new(n).expect("an epoch from 1")
}

/// The route to a space key wrapped to a group: the group, the epoch of its key and the member's
/// own wrap of that group key.
fn route<'a>(group: &'a GroupId, group_wrap: &'a Envelope, group_epoch: u32) -> GroupRoute<'a> {
    GroupRoute {
        group,
        group_epoch: epoch(group_epoch),
        group_wrap,
    }
}

// ---------------------------------------------------------------------------------------------
// C1
// ---------------------------------------------------------------------------------------------

/// The same seed gives the same bundle; the draws are three, in the documented order (32 bytes
/// for Ed25519, 32 for X25519, 64 for ML-KEM: 128 in all); a member bundle and a device bundle
/// from one stream share no key bytes.
#[test]
fn c1_bundle_generation_is_deterministic_and_independent() {
    let a = KeyBundle::generate(&mut SeededEntropy::from_u64(1)).expect("a bundle");
    let b = KeyBundle::generate(&mut SeededEntropy::from_u64(1)).expect("a bundle");
    assert!(a.to_plaintext().ct_eq(&b.to_plaintext()));
    assert_eq!(a.public(), b.public());

    // The size and the order of the draws.
    let mut rec = Recording::new(SeededEntropy::from_u64(1));
    let c = KeyBundle::generate(&mut rec).expect("a bundle");
    assert_eq!(rec.draws, vec![32, 32, 64]);
    assert_eq!(rec.total(), 128);
    assert_eq!(rec.inner.drawn(), 128);
    assert!(c.to_plaintext().ct_eq(&a.to_plaintext()));

    // The bytes served first are the Ed25519 seed, then the X25519 secret, then the ML-KEM seed.
    let script = pat(0x01, 128);
    let mut ent = ScriptedEntropy::new(script.clone());
    let d = KeyBundle::generate(&mut ent).expect("a bundle");
    assert_eq!(ent.draws, vec![32, 32, 64]);
    assert_eq!(ent.left(), 0);
    assert_eq!(d.to_plaintext().expose().as_slice(), script.as_slice());
    let signer = SignKeypair::from_seed(&Secret::from_bytes(pat32(0x01)));
    assert_eq!(d.signer().verify_key(), signer.verify_key());
    assert_eq!(&d.public().sign_pub, signer.verify_key().as_bytes());
    let recipient = RecipientSecret::new(
        &Secret::from_bytes(pat32(0x21)),
        Some(&Secret::from_bytes(pat64(0x41))),
    );
    assert_eq!(&d.public().kx_pub, recipient.kx_public());
    assert_eq!(d.public().pq_pub.as_deref(), recipient.pq_public());
    assert_eq!(d.recipient_secret().x25519_id(), recipient.x25519_id());

    // A member bundle and a device bundle of one stream are independent draws.
    let mut ent = SeededEntropy::from_u64(7);
    let member = KeyBundle::generate(&mut ent).expect("a member key");
    let device = KeyBundle::generate(&mut ent).expect("a device key");
    assert_ne!(member.public().sign_pub, device.public().sign_pub);
    assert_ne!(member.public().kx_pub, device.public().kx_pub);
    assert!(!share_a_window(
        member.to_plaintext().expose(),
        device.to_plaintext().expose(),
        8
    ));
    assert!(!member.to_plaintext().ct_eq(&device.to_plaintext()));
}

/// Two accounts' member keys are independent: no column of the public bundle is shared, so two
/// accounts cannot be linked by a key (section 5.5).
#[test]
fn member_keys_are_independent_per_account() {
    let mut ent = SeededEntropy::from_u64(11);
    let account_a = KeyBundle::generate(&mut ent).expect("a member key");
    let account_b = KeyBundle::generate(&mut ent).expect("a member key");
    let (pa, pb) = (account_a.public(), account_b.public());
    assert_ne!(pa.sign_pub, pb.sign_pub);
    assert_ne!(pa.kx_pub, pb.kx_pub);
    assert_ne!(pa.key_id(), pb.key_id());
    assert!(!share_a_window(
        account_a.to_plaintext().expose(),
        account_b.to_plaintext().expose(),
        8
    ));
    #[cfg(feature = "pq")]
    assert_ne!(pa.pq_pub, pb.pq_pub);
}

/// The plaintext of a bundle round-trips, and only 128 bytes are a bundle.
#[test]
fn a_bundle_round_trips_through_its_plaintext() {
    let a = KeyBundle::generate(&mut SeededEntropy::from_u64(12)).expect("a bundle");
    let plaintext = a.to_plaintext();
    let b = KeyBundle::from_plaintext(plaintext.expose()).expect("128 bytes");
    assert_eq!(b.public(), a.public());
    assert!(b.to_plaintext().ct_eq(&plaintext));
    let copy = a.duplicate();
    assert!(copy.to_plaintext().ct_eq(&plaintext));
    assert_eq!(copy.public(), a.public());
    for len in [0usize, 1, 31, 32, 127, 129, 160] {
        assert_eq!(
            KeyBundle::from_plaintext(&vec![7u8; len]).unwrap_err(),
            KeyError::BadPlaintext,
            "{len} bytes are no bundle"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// C2
// ---------------------------------------------------------------------------------------------

/// `PublicBundle` has the lengths of the columns `sign_pub`, `kx_pub`, `pq_pub` of `reg_member`
/// and `reg_device` (32, 32, 1,184) and its `key_id` is S15a's `KeyId::of_bundle`.
#[test]
fn c2_public_bundle_matches_columns() {
    let bundle = KeyBundle::generate(&mut SeededEntropy::from_u64(2)).expect("a bundle");
    let public = bundle.public();
    assert_eq!(public.sign_pub.len(), 32);
    assert_eq!(public.kx_pub.len(), 32);
    #[cfg(feature = "pq")]
    {
        assert_eq!(public.pq_pub.as_ref().map(Vec::len), Some(1184));
        assert_eq!(azlin_keys::envelope::PQ_KEY_LEN, 1184);
    }
    #[cfg(not(feature = "pq"))]
    assert_eq!(public.pq_pub, None);
    assert_eq!(
        public.key_id(),
        KeyId::of_bundle(&public.kx_pub, public.pq_pub.as_deref())
    );
    assert_eq!(public.verify_key().as_bytes(), &public.sign_pub);
    assert_eq!(public.sign_pub, *bundle.signer().verify_key().as_bytes());

    // The recipient is hybrid exactly when there is an ML-KEM key; its id is the bundle's.
    let recipient = public.recipient();
    assert_eq!(recipient.key_id(), public.key_id());
    match (&recipient, &public.pq_pub) {
        (Recipient::Hybrid(h), Some(pq)) => {
            assert_eq!(h.kx, public.kx_pub);
            assert_eq!(&h.pq, pq);
        }
        (Recipient::X25519(x), None) => assert_eq!(x.kx, public.kx_pub),
        other => panic!("the recipient does not follow the ML-KEM key: {other:?}"),
    }
    // What the recipient secret of the bundle says is the same.
    let secret = bundle.recipient_secret();
    assert_eq!(secret.kx_public(), &public.kx_pub);
    assert_eq!(secret.pq_public(), public.pq_pub.as_deref());
}

// ---------------------------------------------------------------------------------------------
// C8
// ---------------------------------------------------------------------------------------------

/// The member key wrapped to each of three devices opens on each and on no other; a wrap of
/// another context, for another key or from an unknown signer is refused.
#[test]
fn c8_member_to_device_wrap_roundtrip() {
    let mut ent = SeededEntropy::from_u64(8);
    let member_id = MemberId::generate(&mut ent).expect("a member id");
    let member = KeyBundle::generate(&mut ent).expect("a member key");
    let wrapper = Device::new(&mut ent);
    let devices: Vec<Device> = (0..3).map(|_| Device::new(&mut ent)).collect();
    let stranger = Device::new(&mut ent);
    let dir = directory(&[&wrapper]);

    let wraps: Vec<Envelope> = devices
        .iter()
        .map(|d| {
            wrap_member_key_to_device(
                &member,
                &d.keys.public(),
                &member_id,
                &wrapper.id,
                &wrapper.signer(),
                &mut ent,
            )
            .expect("a wrap")
        })
        .collect();
    for env in &wraps {
        assert_eq!(env.ctx.as_str(), format!("member/{member_id}/key"));
        assert_eq!(env.signer, wrapper.id);
    }
    let open_as = |env: &Envelope, who: &Device| {
        open_member_key(
            env,
            &who.keys,
            &member_id,
            &member.public(),
            &dir,
            VerifyAt::Now,
        )
    };
    for (i, device) in devices.iter().enumerate() {
        assert_eq!(wraps[i].rcpt, device.keys.public().key_id());
        let opened = open_as(&wraps[i], device).expect("the device opens its wrap");
        assert!(opened.to_plaintext().ct_eq(&member.to_plaintext()));
        assert_eq!(opened.public(), member.public());
        for (j, other) in devices.iter().enumerate() {
            if i != j {
                assert_eq!(
                    open_as(&wraps[i], other).unwrap_err(),
                    KeyError::Open(OpenError::NotForMe),
                    "device {j} opened the wrap for device {i}"
                );
            }
        }
        assert_eq!(
            open_as(&wraps[i], &stranger).unwrap_err(),
            KeyError::Open(OpenError::NotForMe)
        );
    }

    // Another member's context, another member's registered keys, a signer the directory
    // does not know: each is refused, each with its own error.
    let other_member = MemberId::generate(&mut ent).expect("a member id");
    assert_eq!(
        open_member_key(
            &wraps[0],
            &devices[0].keys,
            &other_member,
            &member.public(),
            &dir,
            VerifyAt::Now
        )
        .unwrap_err(),
        KeyError::Open(OpenError::CtxMismatch)
    );
    let impostor = KeyBundle::generate(&mut ent).expect("another member key");
    assert_eq!(
        open_member_key(
            &wraps[0],
            &devices[0].keys,
            &member_id,
            &impostor.public(),
            &dir,
            VerifyAt::Now
        )
        .unwrap_err(),
        KeyError::BundleMismatch
    );
    assert_eq!(
        open_member_key(
            &wraps[0],
            &devices[0].keys,
            &member_id,
            &member.public(),
            &directory(&[&stranger]),
            VerifyAt::Now
        )
        .unwrap_err(),
        KeyError::Open(OpenError::UnknownSigner)
    );
}

/// Adding a device wraps ONE key, however many spaces the member has (section 5.5): the spaces
/// are wrapped to the member key, the new device gets the member key, and the one wrap is all the
/// entropy and all the envelopes that adding a device costs.
#[test]
fn c8_adding_a_device_wraps_one_key_not_every_space() {
    let mut ent = SeededEntropy::from_u64(80);
    let member_id = MemberId::generate(&mut ent).expect("a member id");
    let member = KeyBundle::generate(&mut ent).expect("a member key");
    let wrapper = Device::new(&mut ent);
    let dir = directory(&[&wrapper]);

    let mut draws_per_added_device = Vec::new();
    for spaces in [1usize, 5, 100] {
        let space_wraps: Vec<(SpaceId, Envelope)> = (0..spaces)
            .map(|_| {
                let space = SpaceId::generate(&mut ent).expect("a space id");
                let key =
                    SpaceKey::generate(&space, Epoch::FIRST, DedupChoice::Fresh, None, &mut ent)
                        .expect("a space key");
                let env = wrap_space_key(
                    &key,
                    &space,
                    &WrapTarget::Member(member.public()),
                    &wrapper.id,
                    &wrapper.signer(),
                    &mut ent,
                )
                .expect("a wrap to the member");
                (space, env)
            })
            .collect();

        // The new device: one wrap, counted.
        let new_device = Device::new(&mut ent);
        let mut counted = Recording::new(SeededEntropy::from_u64(81));
        let wrap = wrap_member_key_to_device(
            &member,
            &new_device.keys.public(),
            &member_id,
            &wrapper.id,
            &wrapper.signer(),
            &mut counted,
        )
        .expect("the one wrap");
        draws_per_added_device.push(counted.draws.clone());

        // It reaches every space through the one member key.
        let adopted = open_member_key(
            &wrap,
            &new_device.keys,
            &member_id,
            &member.public(),
            &dir,
            VerifyAt::Now,
        )
        .expect("the device adopts the member key");
        for (space, env) in &space_wraps {
            let key = open_space_key(env, space, Epoch::FIRST, &adopted, &dir, VerifyAt::Now)
                .expect("the new device opens the space key");
            assert_eq!(key.epoch, Epoch::FIRST);
            assert_eq!(key.space(), space);
        }
    }
    // The same draws for 1, 5 and 100 spaces: the ephemeral key, the ML-KEM message for a
    // hybrid wrap, and the nonce.
    assert!(draws_per_added_device.windows(2).all(|w| w[0] == w[1]));
    #[cfg(feature = "pq")]
    assert_eq!(draws_per_added_device[0], vec![32, 32, 24]);
    #[cfg(not(feature = "pq"))]
    assert_eq!(draws_per_added_device[0], vec![32, 24]);
}

// ---------------------------------------------------------------------------------------------
// C9
// ---------------------------------------------------------------------------------------------

/// Member key -> group key (epoch g) -> space key (epoch n), with real envelopes and a directory;
/// a member who is not in the group cannot open; a group key of another epoch is refused
/// (`CtxMismatch`).
#[test]
fn c9_group_path_end_to_end() {
    let mut ent = SeededEntropy::from_u64(9);
    let group = GroupId::generate(&mut ent).expect("a group id");
    let space = SpaceId::generate(&mut ent).expect("a space id");
    let manager = Device::new(&mut ent);
    let m1 = Device::new(&mut ent);
    let m2 = Device::new(&mut ent);
    let outsider = Device::new(&mut ent);
    let dir = directory(&[&manager]);

    // Group key epochs 1 and 2, each wrapped to the members of the group only.
    let g1 = GroupKey::generate(&group, epoch(1), &mut ent).expect("group key 1");
    let g2 = GroupKey::generate(&group, epoch(2), &mut ent).expect("group key 2");
    let wrap_to = |g: &GroupKey, who: &Device, ent: &mut SeededEntropy| {
        wrap_group_key_to_member(
            g,
            &group,
            &who.keys.public(),
            &manager.id,
            &manager.signer(),
            ent,
        )
        .expect("a group key wrap")
    };
    let g1_for_m1 = wrap_to(&g1, &m1, &mut ent);
    let g1_for_m2 = wrap_to(&g1, &m2, &mut ent);
    let g2_for_m1 = wrap_to(&g2, &m1, &mut ent);
    assert_eq!(g1_for_m1.ctx.as_str(), format!("group/{group}/epoch/1"));
    assert_eq!(g2_for_m1.ctx.as_str(), format!("group/{group}/epoch/2"));
    assert_eq!(g1_for_m1.rcpt, m1.keys.public().key_id());

    // A space key of epoch 3, granted to the group: wrapped to the group key of epoch 1.
    let k1 = SpaceKey::generate(&space, epoch(1), DedupChoice::Fresh, None, &mut ent).expect("k1");
    let k2 =
        SpaceKey::generate(&space, epoch(2), DedupChoice::Carry, Some(&k1), &mut ent).expect("k2");
    let k3 =
        SpaceKey::generate(&space, epoch(3), DedupChoice::Carry, Some(&k2), &mut ent).expect("k3");
    let target = WrapTarget::Group(g1.public());
    let space_env = wrap_space_key(
        &k3,
        &space,
        &target,
        &manager.id,
        &manager.signer(),
        &mut ent,
    )
    .expect("a wrap to the group");
    assert_eq!(space_env.alg, Alg::X25519SealedBox);
    assert_eq!(space_env.rcpt, target.key_id());
    assert_eq!(space_env.rcpt, g1.public().recipient_id());
    assert_eq!(space_env.ctx.as_str(), format!("space/{space}/epoch/3"));

    // Each member of the group reaches the space key through their own wrap of the group key.
    for (who, wrap) in [(&m1, &g1_for_m1), (&m2, &g1_for_m2)] {
        let opened = open_space_key_via_group(
            &space_env,
            &space,
            epoch(3),
            &route(&group, wrap, 1),
            &who.keys,
            &dir,
            VerifyAt::Now,
        )
        .expect("a member of the group opens the space key");
        assert_eq!(opened.epoch, epoch(3));
        assert!(opened.to_plaintext().ct_eq(&k3.to_plaintext()));
        assert_eq!(opened.key_id(), k3.key_id());
    }

    // A member who is not in the group cannot open the group key, so cannot open the space key.
    assert_eq!(
        open_space_key_via_group(
            &space_env,
            &space,
            epoch(3),
            &route(&group, &g1_for_m1, 1),
            &outsider.keys,
            &dir,
            VerifyAt::Now
        )
        .unwrap_err(),
        KeyError::Open(OpenError::NotForMe)
    );
    // A group key of another epoch is refused: epoch 2 asked for, the wrap is epoch 1's ...
    assert_eq!(
        open_space_key_via_group(
            &space_env,
            &space,
            epoch(3),
            &route(&group, &g1_for_m1, 2),
            &m1.keys,
            &dir,
            VerifyAt::Now
        )
        .unwrap_err(),
        KeyError::Open(OpenError::CtxMismatch)
    );
    // ... and the right group key of epoch 2 does not open a space key wrapped to epoch 1.
    assert_eq!(
        open_space_key_via_group(
            &space_env,
            &space,
            epoch(3),
            &route(&group, &g2_for_m1, 2),
            &m1.keys,
            &dir,
            VerifyAt::Now
        )
        .unwrap_err(),
        KeyError::Open(OpenError::NotForMe)
    );
    // The space key of another epoch is refused by its context.
    assert_eq!(
        open_space_key_via_group(
            &space_env,
            &space,
            epoch(2),
            &route(&group, &g1_for_m1, 1),
            &m1.keys,
            &dir,
            VerifyAt::Now
        )
        .unwrap_err(),
        KeyError::Open(OpenError::CtxMismatch)
    );

    // The group key opened by a member is the group's: same id, same public half.
    let opened_g1 = open_group_key(&g1_for_m1, &group, epoch(1), &m1.keys, &dir, VerifyAt::Now)
        .expect("the member opens the group key");
    assert_eq!(opened_g1.id(), g1.id());
    assert_eq!(opened_g1.public(), g1.public());
    assert_ne!(g1.id(), g2.id());
    assert_eq!(opened_g1.group(), &group);
}

/// A wrap names its own group and its own space: a key of another group or space is refused
/// before anything is drawn.
#[test]
fn group_and_space_wraps_name_the_right_owner() {
    let mut ent = SeededEntropy::from_u64(19);
    let group = GroupId::generate(&mut ent).expect("a group id");
    let other_group = GroupId::generate(&mut ent).expect("a group id");
    let space = SpaceId::generate(&mut ent).expect("a space id");
    let other_space = SpaceId::generate(&mut ent).expect("a space id");
    let device = Device::new(&mut ent);
    let g = GroupKey::generate(&group, Epoch::FIRST, &mut ent).expect("a group key");
    let k = SpaceKey::generate(&space, Epoch::FIRST, DedupChoice::Fresh, None, &mut ent)
        .expect("a space key");

    let mut counted = Recording::new(SeededEntropy::from_u64(1));
    assert_eq!(
        wrap_group_key_to_member(
            &g,
            &other_group,
            &device.keys.public(),
            &device.id,
            &device.signer(),
            &mut counted
        )
        .unwrap_err(),
        KeyError::WrongGroup
    );
    assert_eq!(
        wrap_space_key(
            &k,
            &other_space,
            &WrapTarget::Member(device.keys.public()),
            &device.id,
            &device.signer(),
            &mut counted
        )
        .unwrap_err(),
        KeyError::WrongSpace
    );
    assert!(
        counted.draws.is_empty(),
        "nothing is drawn for a refused wrap"
    );
    // The group key's own accessors agree with what it was made for.
    assert_eq!(g.group(), &group);
    assert_eq!(g.epoch, Epoch::FIRST);
    assert_eq!(g.as_recipient().kx, g.public().kx_pub);
    assert_eq!(g.as_recipient().key_id(), g.public().recipient_id());
    let copy = g.duplicate();
    assert_eq!(copy.public(), g.public());
    assert!(copy.to_plaintext().ct_eq(&g.to_plaintext()));
    assert_eq!(
        GroupKey::from_plaintext(&group, Epoch::FIRST, &[0u8; 31]).unwrap_err(),
        KeyError::BadPlaintext
    );
    assert_eq!(
        GroupKey::from_plaintext(&group, Epoch::FIRST, &[0u8; 33]).unwrap_err(),
        KeyError::BadPlaintext
    );
    let again = GroupKey::from_plaintext(&group, Epoch::FIRST, g.to_plaintext().expose())
        .expect("32 bytes");
    assert_eq!(again.public(), g.public());
}

// ---------------------------------------------------------------------------------------------
// C11
// ---------------------------------------------------------------------------------------------

/// Runs `op` with an entropy source that fails after `k` bytes, for every `k` below the `total`
/// the operation draws: each must return an entropy error (a key or an envelope is never made
/// from a failed draw), and the run with exactly `total` bytes succeeds.
fn sweep_entropy_failures(
    total: usize,
    mut op: impl FnMut(&mut dyn Entropy) -> Result<(), KeyError>,
) {
    for k in 0..total {
        match op(&mut FailingEntropy::after(k)) {
            Err(KeyError::Entropy(EntropyError::Exhausted))
            | Err(KeyError::Wrap(WrapError::Entropy(EntropyError::Exhausted))) => {}
            other => panic!("after {k} of {total} bytes the operation answered {other:?}"),
        }
    }
    op(&mut FailingEntropy::after(total)).expect("the operation succeeds with all its bytes");
}

/// A `FailingEntropy` in each generator and wrap returns the error and no partial value.
#[test]
fn c11_entropy_failure_stops_everything_in_member_and_group_code() {
    let mut ent = SeededEntropy::from_u64(5);
    let group = GroupId::generate(&mut ent).expect("a group id");
    let space = SpaceId::generate(&mut ent).expect("a space id");
    let member_id = MemberId::generate(&mut ent).expect("a member id");
    let member = KeyBundle::generate(&mut ent).expect("a member key");
    let device = Device::new(&mut ent);
    let peer = Device::new(&mut ent);
    let g = GroupKey::generate(&group, Epoch::FIRST, &mut ent).expect("a group key");
    let k1 = SpaceKey::generate(&space, epoch(1), DedupChoice::Fresh, None, &mut ent).expect("k1");

    // How many bytes does each operation draw?
    fn draws(op: impl FnOnce(&mut dyn Entropy) -> Result<(), KeyError>) -> usize {
        let mut counted = Recording::new(SeededEntropy::from_u64(3));
        op(&mut counted).expect("the operation succeeds");
        counted.total()
    }
    let bundle_total = draws(|e| KeyBundle::generate(e).map(drop));
    assert_eq!(bundle_total, 128);
    sweep_entropy_failures(bundle_total, |e| KeyBundle::generate(e).map(drop));

    let group_total = draws(|e| GroupKey::generate(&group, Epoch::FIRST, e).map(drop));
    assert_eq!(group_total, 32);
    sweep_entropy_failures(group_total, |e| {
        GroupKey::generate(&group, Epoch::FIRST, e).map(drop)
    });

    let wrap_member = |e: &mut dyn Entropy| {
        wrap_member_key_to_device(
            &member,
            &peer.keys.public(),
            &member_id,
            &device.id,
            &device.signer(),
            e,
        )
        .map(drop)
    };
    let wrap_total = draws(wrap_member);
    #[cfg(feature = "pq")]
    assert_eq!(wrap_total, 32 + 32 + 24);
    #[cfg(not(feature = "pq"))]
    assert_eq!(wrap_total, 32 + 24);
    sweep_entropy_failures(wrap_total, wrap_member);

    let wrap_group = |e: &mut dyn Entropy| {
        wrap_group_key_to_member(
            &g,
            &group,
            &peer.keys.public(),
            &device.id,
            &device.signer(),
            e,
        )
        .map(drop)
    };
    let wrap_group_total = draws(wrap_group);
    sweep_entropy_failures(wrap_group_total, wrap_group);

    let wrap_space_to_group = |e: &mut dyn Entropy| {
        wrap_space_key(
            &k1,
            &space,
            &WrapTarget::Group(g.public()),
            &device.id,
            &device.signer(),
            e,
        )
        .map(drop)
    };
    let group_target_total = draws(wrap_space_to_group);
    assert_eq!(
        group_target_total,
        32 + 24,
        "a group is a classical recipient"
    );
    sweep_entropy_failures(group_target_total, wrap_space_to_group);

    // No key is ever zeroes: a successful draw gives key material, never an all-zero key.
    assert!(member.to_plaintext().expose().iter().any(|b| *b != 0));
    assert!(g.to_plaintext().expose().iter().any(|b| *b != 0));
}

// ---------------------------------------------------------------------------------------------
// C12 (the part that runs: the compile_fail examples are the doc tests of the three modules)
// ---------------------------------------------------------------------------------------------

/// `Debug` of a key shows no byte of it, and the accessors that hand out key bytes return
/// `Secret`s.
#[test]
fn c12_secrets_never_print_in_member_and_group_code() {
    let script = pat(0x01, 128);
    let bundle = KeyBundle::generate(&mut ScriptedEntropy::new(script.clone())).expect("a bundle");
    let group = GroupId::from_bytes(pat16(0xa0));
    let g = GroupKey::generate(&group, epoch(4), &mut ScriptedEntropy::new(pat(0x61, 32)))
        .expect("a group key");

    let shown = [format!("{bundle:?}"), format!("{g:?}")];
    let secrets = [script, pat(0x61, 32)];
    for (text, secret) in shown.iter().zip(&secrets) {
        assert!(text.contains("***"), "{text}");
        // No window of four secret bytes appears as hex, in either case.
        for window in secret.windows(4) {
            let hex = to_hex(window);
            assert!(!text.to_lowercase().contains(&hex), "{text} shows {hex}");
        }
    }
    // The public data is not secret and the key id is shown to tell keys apart.
    assert!(shown[0].contains(&bundle.public().key_id().to_string()));
    assert!(shown[1].contains(&group.to_string()));

    let _plaintext: Secret<128> = bundle.to_plaintext();
    let _group_plaintext: Secret<32> = g.to_plaintext();
    assert_eq!(format!("{:?}", bundle.to_plaintext()), "Secret<128>(***)");
    assert_eq!(format!("{:?}", g.to_plaintext()), "Secret<32>(***)");
    // The recipient secret of a bundle prints nothing either.
    assert_eq!(
        format!("{:?}", bundle.recipient_secret()),
        "RecipientSecret(***)"
    );
    // The public half prints what it is.
    let public: PublicBundle = bundle.public();
    assert!(format!("{public:?}").contains("PublicBundle"));
    let gp: GroupPublic = g.public();
    assert_eq!(gp.group, group);
    assert_eq!(gp.epoch, epoch(4));
}
