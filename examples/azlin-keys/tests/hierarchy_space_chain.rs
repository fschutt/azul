//! The space keys, the epoch chain and the history modes (lane W1-S15c1): C3 (K_seal and K_chunk
//! each depend only on their own seed), C4 (the dedup seed survives a removal rotation and
//! changes on an explicit re-key), C5 (a removed member cannot derive the next epoch), C6 (the
//! chain opens older epochs only, only downwards, only as far as `CHAIN_MAX`, every link verified
//! through the directory), C7 (the two history modes), C10 (a removal costs members + spaces) and
//! the space parts of C11 and C12.
//!
//! Implements planning/architecture.md sections 5.5 and 5.6.
//! Run with `--features test-support` (the test doubles) and, for the hybrid links, `pq`.

#![cfg(feature = "test-support")]
#![allow(dead_code)]

mod hierarchy_common;

use azlin_keys::derive::{
    derive, derive_public, CHAIN_WRAP, CONTEXTS, K_CHUNK, K_SEAL, SPACE_KEY_ID,
};
use azlin_keys::envelope::{Ctx, Envelope, OpenError, RecipientSecret};
use azlin_keys::member::{KeyError, LinkFault};
use azlin_keys::space::{
    chain_open, chain_open_link, chain_wrap, join_grant, open_space_key, removal_wrap_count,
    wrap_space_key, DedupChoice, History, SpaceKey, WrapTarget, CHAIN_MAX,
};
use azlin_keys::{
    ChainSeq, DeviceState, DirectoryEntry, Entropy, EntropyError, Epoch, FailingEntropy,
    MapDirectory, Secret, SeededEntropy, SpaceId, UnixTime, VerifyAt,
};
use hierarchy_common::{directory, pat, pat16, to_hex, Device, Recording, ScriptedEntropy};

fn epoch(n: u32) -> Epoch {
    Epoch::new(n).expect("an epoch from 1")
}

/// A space key made from its three parts: the seal seed byte, the dedup seed byte (each repeated
/// 32 times) and the dedup epoch.
fn space_key(space: &SpaceId, at: u32, seal: u8, dedup: u8, dedup_epoch: u32) -> SpaceKey {
    let plaintext = [
        vec![seal; 32],
        vec![dedup; 32],
        dedup_epoch.to_be_bytes().to_vec(),
    ]
    .concat();
    SpaceKey::from_plaintext(space, epoch(at), &plaintext).expect("a space key")
}

/// The 68 bytes of a space key.
fn bytes_of(key: &SpaceKey) -> Vec<u8> {
    key.to_plaintext().expose().to_vec()
}

fn seal_seed_of(key: &SpaceKey) -> Vec<u8> {
    bytes_of(key).get(..32).expect("the seal seed").to_vec()
}

fn dedup_seed_of(key: &SpaceKey) -> Vec<u8> {
    bytes_of(key).get(32..64).expect("the dedup seed").to_vec()
}

// ---------------------------------------------------------------------------------------------
// The space key: layout, derivations, id
// ---------------------------------------------------------------------------------------------

/// The plaintext of a key is `seal_seed || dedup_seed || dedup_epoch` (big-endian), the two
/// derived keys are BLAKE3 `derive_key` of the two seeds under the contexts of S15a's table, and
/// the key id is the commitment of `space || epoch || plaintext`.
#[test]
fn a_space_key_has_the_documented_layout_derivations_and_id() {
    let space = SpaceId::from_bytes(pat16(0x50));
    let mut ent = ScriptedEntropy::new([pat(0x10, 32), pat(0x30, 32)].concat());
    let key = SpaceKey::generate(&space, epoch(1), DedupChoice::Fresh, None, &mut ent)
        .expect("the first epoch");
    assert_eq!(ent.draws, vec![32, 32]);
    assert_eq!(ent.left(), 0);
    assert_eq!(
        bytes_of(&key),
        [pat(0x10, 32), pat(0x30, 32), vec![0, 0, 0, 1]].concat()
    );
    assert_eq!(key.epoch, Epoch::FIRST);
    assert_eq!(key.dedup_epoch(), Epoch::FIRST);
    assert_eq!(key.space(), &space);

    let seal_seed = Secret::<32>::from_slice(&pat(0x10, 32)).expect("32 bytes");
    let dedup_seed = Secret::<32>::from_slice(&pat(0x30, 32)).expect("32 bytes");
    assert!(key.k_seal().ct_eq(&derive(&K_SEAL, &seal_seed)));
    assert!(key.k_chunk().ct_eq(&derive(&K_CHUNK, &dedup_seed)));
    assert!(!key.k_seal().ct_eq(&key.k_chunk()));

    let material = [
        space.as_bytes().to_vec(),
        1u32.to_be_bytes().to_vec(),
        bytes_of(&key),
    ]
    .concat();
    assert_eq!(
        key.key_id().as_bytes(),
        &derive_public(&SPACE_KEY_ID, &material)
    );

    // The plaintext round-trips, and a copy is the same key.
    let again =
        SpaceKey::from_plaintext(&space, epoch(1), key.to_plaintext().expose()).expect("68 bytes");
    assert_eq!(bytes_of(&again), bytes_of(&key));
    assert_eq!(again.key_id(), key.key_id());
    let copy = key.duplicate();
    assert_eq!(bytes_of(&copy), bytes_of(&key));
    assert_eq!(copy.epoch, key.epoch);
}

/// The key id commits to the space, the epoch, the seal seed, the dedup seed and the dedup epoch:
/// changing any one of them gives another id.
#[test]
fn the_key_id_commits_to_the_whole_material() {
    let space = SpaceId::from_bytes(pat16(0x50));
    let other_space = SpaceId::from_bytes(pat16(0x51));
    let base = space_key(&space, 5, 0x11, 0x22, 3);
    let variants = [
        space_key(&other_space, 5, 0x11, 0x22, 3),
        space_key(&space, 6, 0x11, 0x22, 3),
        space_key(&space, 5, 0x12, 0x22, 3),
        space_key(&space, 5, 0x11, 0x23, 3),
        space_key(&space, 5, 0x11, 0x22, 4),
    ];
    let mut ids = vec![base.key_id()];
    ids.extend(variants.iter().map(SpaceKey::key_id));
    for (i, a) in ids.iter().enumerate() {
        for b in ids.iter().skip(i + 1) {
            assert_ne!(a, b);
        }
    }
    // The same material is the same id.
    assert_eq!(space_key(&space, 5, 0x11, 0x22, 3).key_id(), base.key_id());
}

/// Only 68 bytes are a space key; a dedup epoch is at least 1 and at most the key's own epoch.
#[test]
fn a_space_key_plaintext_is_checked() {
    let space = SpaceId::from_bytes(pat16(0x50));
    let good = [pat(0x10, 32), pat(0x30, 32), 3u32.to_be_bytes().to_vec()].concat();
    assert!(SpaceKey::from_plaintext(&space, epoch(3), &good).is_ok());
    assert!(SpaceKey::from_plaintext(&space, epoch(4), &good).is_ok());
    // Too short, too long, empty.
    for len in [0usize, 1, 32, 64, 67, 69, 100] {
        let bytes = vec![5u8; len];
        assert_eq!(
            SpaceKey::from_plaintext(&space, epoch(9), &bytes).unwrap_err(),
            KeyError::BadPlaintext,
            "{len} bytes"
        );
    }
    // A dedup epoch of 0, and one above the epoch of the key.
    let zero = [pat(0x10, 32), pat(0x30, 32), vec![0, 0, 0, 0]].concat();
    assert_eq!(
        SpaceKey::from_plaintext(&space, epoch(3), &zero).unwrap_err(),
        KeyError::BadPlaintext
    );
    assert_eq!(
        SpaceKey::from_plaintext(&space, epoch(2), &good).unwrap_err(),
        KeyError::BadPlaintext,
        "a dedup epoch above the key's epoch"
    );
}

// ---------------------------------------------------------------------------------------------
// C3
// ---------------------------------------------------------------------------------------------

/// `K_seal` and `K_chunk` differ, and each changes only with its own seed: not with the other
/// seed, not with the dedup epoch, not with the epoch. (The known answers are pinned in
/// `tests/vectors/space_keys.json`, checked by `c3_k_seal_and_k_chunk_known_answers`.)
#[test]
fn c3_k_seal_and_k_chunk_depend_only_on_their_own_seed() {
    let space = SpaceId::from_bytes(pat16(0x50));
    let base = space_key(&space, 5, 0x11, 0x22, 3);
    assert!(!base.k_seal().ct_eq(&base.k_chunk()));

    let other_seal = space_key(&space, 5, 0x12, 0x22, 3);
    assert!(!base.k_seal().ct_eq(&other_seal.k_seal()));
    assert!(base.k_chunk().ct_eq(&other_seal.k_chunk()));

    let other_dedup = space_key(&space, 5, 0x11, 0x23, 3);
    assert!(base.k_seal().ct_eq(&other_dedup.k_seal()));
    assert!(!base.k_chunk().ct_eq(&other_dedup.k_chunk()));

    // The dedup epoch, the epoch and the space are not inputs of either key.
    for same in [
        space_key(&space, 5, 0x11, 0x22, 4),
        space_key(&space, 6, 0x11, 0x22, 3),
        space_key(&SpaceId::from_bytes(pat16(0x77)), 5, 0x11, 0x22, 3),
    ] {
        assert!(base.k_seal().ct_eq(&same.k_seal()));
        assert!(base.k_chunk().ct_eq(&same.k_chunk()));
    }
    // The keys come from S15a's table: K_seal and K_chunk are its entries, and differ.
    assert!(CONTEXTS.iter().any(|c| c.name() == "K_SEAL"));
    assert!(CONTEXTS.iter().any(|c| c.name() == "K_CHUNK"));
    assert_ne!(K_SEAL.as_str(), K_CHUNK.as_str());
}

// ---------------------------------------------------------------------------------------------
// generate: the rules of a new epoch
// ---------------------------------------------------------------------------------------------

/// The draws of a new epoch: the seal seed, then the dedup seed unless it is carried.
#[test]
fn generate_draws_the_seal_seed_then_the_dedup_seed() {
    let space = SpaceId::from_bytes(pat16(0x50));
    let mut counted = Recording::new(SeededEntropy::from_u64(1));
    let k1 = SpaceKey::generate(&space, epoch(1), DedupChoice::Fresh, None, &mut counted)
        .expect("the first epoch");
    assert_eq!(counted.draws, vec![32, 32]);

    let mut counted = Recording::new(SeededEntropy::from_u64(2));
    let k2 = SpaceKey::generate(
        &space,
        epoch(2),
        DedupChoice::Carry,
        Some(&k1),
        &mut counted,
    )
    .expect("a removal rotation");
    assert_eq!(counted.draws, vec![32], "a Carry draws the seal seed only");

    let mut counted = Recording::new(SeededEntropy::from_u64(3));
    let _k3 = SpaceKey::generate(
        &space,
        epoch(3),
        DedupChoice::Fresh,
        Some(&k2),
        &mut counted,
    )
    .expect("a re-key");
    assert_eq!(counted.draws, vec![32, 32]);
}

/// Only the first epoch has no predecessor; every other epoch follows the key of the same space
/// by exactly one; all of it is refused before anything is drawn.
#[test]
fn generate_refuses_a_wrong_succession_before_drawing() {
    let space = SpaceId::from_bytes(pat16(0x50));
    let other_space = SpaceId::from_bytes(pat16(0x51));
    let k1 = space_key(&space, 1, 1, 2, 1);
    let k4 = space_key(&space, 4, 3, 4, 2);
    let near_max = space_key(&space, u32::MAX, 5, 6, 1);
    let foreign = space_key(&other_space, 1, 7, 8, 1);

    let mut counted = Recording::new(SeededEntropy::from_u64(1));
    let mut try_generate = |at: u32, choice: DedupChoice, prev: Option<&SpaceKey>| {
        SpaceKey::generate(&space, epoch(at), choice, prev, &mut counted).unwrap_err()
    };
    // No predecessor: only the first epoch, and a Carry has nothing to carry.
    assert_eq!(
        try_generate(2, DedupChoice::Fresh, None),
        KeyError::MissingPrevious
    );
    assert_eq!(
        try_generate(1, DedupChoice::Carry, None),
        KeyError::MissingPrevious
    );
    // A predecessor of another space.
    assert_eq!(
        try_generate(2, DedupChoice::Carry, Some(&foreign)),
        KeyError::WrongSpace
    );
    // Not the next epoch: the same, a gap, a step back, the first epoch after one.
    for at in [1, 4, 6] {
        assert_eq!(
            try_generate(at, DedupChoice::Carry, Some(&k4)),
            KeyError::NotNextEpoch,
            "epoch {at} after epoch 4"
        );
    }
    assert_eq!(
        try_generate(1, DedupChoice::Fresh, Some(&k1)),
        KeyError::NotNextEpoch
    );
    // The largest epoch has no next one.
    assert_eq!(
        try_generate(1, DedupChoice::Fresh, Some(&near_max)),
        KeyError::NotNextEpoch
    );
    assert!(counted.draws.is_empty(), "a refused call draws nothing");
}

// ---------------------------------------------------------------------------------------------
// C4
// ---------------------------------------------------------------------------------------------

/// From epoch n to n+1: with `Carry` K_chunk is equal and K_seal differs; with `Fresh` both
/// differ and the dedup epoch is the new epoch (the property that keeps deduplication across
/// removals, section 5.5).
#[test]
fn c4_dedup_survives_a_removal_rotation_and_changes_on_rekey() {
    let space = SpaceId::from_bytes(pat16(0x50));
    let mut ent = SeededEntropy::from_u64(4);
    let k1 =
        SpaceKey::generate(&space, epoch(1), DedupChoice::Fresh, None, &mut ent).expect("epoch 1");
    assert_eq!(
        k1.dedup_epoch(),
        epoch(1),
        "the first epoch draws its dedup seed"
    );

    // Five removals in a row: K_chunk never moves, K_seal moves every time, the dedup epoch stays.
    let mut keys = vec![k1];
    for n in 2..=6 {
        let prev = keys.last().expect("a previous key");
        let next = SpaceKey::generate(&space, epoch(n), DedupChoice::Carry, Some(prev), &mut ent)
            .expect("a removal rotation");
        assert!(next.k_chunk().ct_eq(&prev.k_chunk()), "epoch {n}: K_chunk");
        assert!(!next.k_seal().ct_eq(&prev.k_seal()), "epoch {n}: K_seal");
        assert_eq!(next.dedup_epoch(), epoch(1));
        assert_ne!(next.key_id(), prev.key_id());
        keys.push(next);
    }
    assert!(keys[0].k_chunk().ct_eq(&keys[5].k_chunk()));

    // An explicit re-key: both differ and the dedup epoch is the new epoch.
    let prev = keys.last().expect("the newest key");
    let rekeyed = SpaceKey::generate(&space, epoch(7), DedupChoice::Fresh, Some(prev), &mut ent)
        .expect("a re-key");
    assert!(!rekeyed.k_chunk().ct_eq(&prev.k_chunk()));
    assert!(!rekeyed.k_seal().ct_eq(&prev.k_seal()));
    assert_eq!(rekeyed.dedup_epoch(), epoch(7));
    // After the re-key the new K_chunk is what later removals carry.
    let after = SpaceKey::generate(
        &space,
        epoch(8),
        DedupChoice::Carry,
        Some(&rekeyed),
        &mut ent,
    )
    .expect("a removal after the re-key");
    assert!(after.k_chunk().ct_eq(&rekeyed.k_chunk()));
    assert!(!after.k_chunk().ct_eq(&prev.k_chunk()));
    assert_eq!(after.dedup_epoch(), epoch(7));

    // The carried seed is a copy: the previous key still has its own.
    assert!(prev.k_chunk().ct_eq(&keys[0].k_chunk()));
    assert_eq!(dedup_seed_of(prev), dedup_seed_of(&keys[0]));
    assert_ne!(dedup_seed_of(&rekeyed), dedup_seed_of(prev));
}

// ---------------------------------------------------------------------------------------------
// C5
// ---------------------------------------------------------------------------------------------

/// Epoch n+1's seal seed is fresh entropy: it is exactly what the seam served, it does not depend
/// on anything of epoch n, and nothing a holder of epoch n can compute equals it. The API has no
/// function from epoch n to the next seal seed except `generate`, which takes entropy.
#[test]
fn c5_a_removed_member_cannot_derive_the_next_epoch() {
    let space = SpaceId::from_bytes(pat16(0x50));
    let k1 = space_key(&space, 1, 0x11, 0x22, 1);
    let other_k1 = space_key(&space, 1, 0x33, 0x44, 1);

    // The seal seed of the new epoch is the first 32 bytes the seam served, whatever the old key.
    let served = pat(0xa0, 32);
    let k2 = SpaceKey::generate(
        &space,
        epoch(2),
        DedupChoice::Carry,
        Some(&k1),
        &mut ScriptedEntropy::new(served.clone()),
    )
    .expect("a removal rotation");
    assert_eq!(seal_seed_of(&k2), served);
    // The same entropy after a different old key gives the same new seal seed and K_seal ...
    let k2_other = SpaceKey::generate(
        &space,
        epoch(2),
        DedupChoice::Carry,
        Some(&other_k1),
        &mut ScriptedEntropy::new(served.clone()),
    )
    .expect("a removal rotation");
    assert_eq!(seal_seed_of(&k2_other), seal_seed_of(&k2));
    assert!(k2_other.k_seal().ct_eq(&k2.k_seal()));
    // ... and another draw gives another one.
    let k2_again = SpaceKey::generate(
        &space,
        epoch(2),
        DedupChoice::Carry,
        Some(&k1),
        &mut ScriptedEntropy::new(pat(0xb0, 32)),
    )
    .expect("a removal rotation");
    assert_ne!(seal_seed_of(&k2_again), seal_seed_of(&k2));
    // A re-key draws both seeds fresh, in order.
    let k2_fresh = SpaceKey::generate(
        &space,
        epoch(2),
        DedupChoice::Fresh,
        Some(&k1),
        &mut ScriptedEntropy::new([pat(0xa0, 32), pat(0xc0, 32)].concat()),
    )
    .expect("a re-key");
    assert_eq!(seal_seed_of(&k2_fresh), served);
    assert_eq!(dedup_seed_of(&k2_fresh), pat(0xc0, 32));

    // Everything a holder of epoch 1 can derive: its seeds and its derived keys, and every key
    // that BLAKE3 derives from any of them under every context of S15a's table. None is the new
    // seal seed.
    let known: Vec<Vec<u8>> = vec![
        seal_seed_of(&k1),
        dedup_seed_of(&k1),
        k1.k_seal().expose().to_vec(),
        k1.k_chunk().expose().to_vec(),
        k1.key_id().as_bytes().to_vec(),
    ];
    let mut derivable = known.clone();
    for context in CONTEXTS {
        for secret in &known {
            if let Some(key) = Secret::<32>::from_slice(secret) {
                derivable.push(derive(context, &key).expose().to_vec());
            }
        }
    }
    let new_seed = seal_seed_of(&k2);
    assert!(derivable.len() > 20);
    for candidate in &derivable {
        assert_ne!(candidate, &new_seed);
    }
    assert!(!k2.k_seal().expose().iter().eq(k1.k_seal().expose().iter()));
}

// ---------------------------------------------------------------------------------------------
// The chain
// ---------------------------------------------------------------------------------------------

/// A space with `epochs` epochs, its chain links, and the device that signed them.
struct Chain {
    space: SpaceId,
    /// `keys[i]` is epoch `i + 1`.
    keys: Vec<SpaceKey>,
    /// `links[i]` seals epoch `i + 1` under epoch `i + 2`.
    links: Vec<Envelope>,
    device: Device,
    dir: MapDirectory,
}

/// Every seventh rotation is an explicit re-key of the space; the others are removals.
fn build_chain(space: SpaceId, epochs: u32, seed: u64) -> Chain {
    let mut ent = SeededEntropy::from_u64(seed);
    let device = Device::new(&mut ent);
    let dir = directory(&[&device]);
    let mut keys: Vec<SpaceKey> = Vec::new();
    let mut links: Vec<Envelope> = Vec::new();
    for n in 1..=epochs {
        let key = match keys.last() {
            None => SpaceKey::generate(&space, epoch(n), DedupChoice::Fresh, None, &mut ent),
            Some(prev) => {
                let choice = if n.is_multiple_of(7) {
                    DedupChoice::Fresh
                } else {
                    DedupChoice::Carry
                };
                SpaceKey::generate(&space, epoch(n), choice, Some(prev), &mut ent)
            }
        }
        .expect("a space key");
        if let Some(prev) = keys.last() {
            links.push(
                chain_wrap(prev, &key, &space, &device.id, &device.signer(), &mut ent)
                    .expect("a chain link"),
            );
        }
        keys.push(key);
    }
    Chain {
        space,
        keys,
        links,
        device,
        dir,
    }
}

fn fault(at: u32, fault: LinkFault) -> KeyError {
    KeyError::ChainLink {
        epoch: epoch(at),
        fault,
    }
}

/// With the newest key a chain of 200 epochs opens fully, in order; the walk goes down and never
/// up; a shorter walk and an empty walk work; the order of the wraps does not matter.
#[test]
fn c6_chain_opens_older_epochs_only_downwards() {
    let space = SpaceId::from_bytes(pat16(0x60));
    let chain = build_chain(space, 201, 6);
    let newest = &chain.keys[200];
    assert_eq!(newest.epoch, epoch(201));
    assert_eq!(chain.links.len(), 200);

    let opened = chain_open(
        newest,
        &chain.links,
        &space,
        Epoch::FIRST,
        &chain.dir,
        VerifyAt::Now,
    )
    .expect("the whole chain opens");
    assert_eq!(opened.len(), 200);
    for (i, key) in opened.iter().enumerate() {
        let original = &chain.keys[199 - i];
        assert_eq!(key.epoch, epoch(200 - i as u32), "in order, newest first");
        assert_eq!(key.epoch, original.epoch);
        assert_eq!(bytes_of(key), bytes_of(original));
        assert_eq!(key.key_id(), original.key_id());
        assert!(key.k_seal().ct_eq(&original.k_seal()));
        assert!(key.k_chunk().ct_eq(&original.k_chunk()));
        assert_eq!(key.dedup_epoch(), original.dedup_epoch());
    }

    // A walk down to epoch 150, and a walk of no steps.
    let some = chain_open(
        newest,
        &chain.links,
        &space,
        epoch(150),
        &chain.dir,
        VerifyAt::Now,
    )
    .expect("a shorter walk");
    assert_eq!(some.len(), 51);
    assert_eq!(some.first().map(|k| k.epoch), Some(epoch(200)));
    assert_eq!(some.last().map(|k| k.epoch), Some(epoch(150)));
    assert!(chain_open(
        newest,
        &chain.links,
        &space,
        epoch(201),
        &chain.dir,
        VerifyAt::Now
    )
    .expect("no step")
    .is_empty());
    // Below the newest key only: asking for an epoch above it is refused.
    assert_eq!(
        chain_open(
            newest,
            &chain.links,
            &space,
            epoch(202),
            &chain.dir,
            VerifyAt::Now
        )
        .unwrap_err(),
        KeyError::UptoAboveNewest
    );

    // The order of the wraps does not matter: the links are found by their context.
    let mut shuffled = chain.links.clone();
    shuffled.reverse();
    shuffled.swap(3, 190);
    let again = chain_open(
        newest,
        &shuffled,
        &space,
        epoch(190),
        &chain.dir,
        VerifyAt::Now,
    )
    .expect("the same links in another order");
    assert_eq!(again.len(), 11);
    for (a, b) in again.iter().zip(&opened) {
        assert_eq!(a.key_id(), b.key_id());
    }

    // A walk that starts lower reads only what is below: the key of epoch 30 opens 29 .. 1 and
    // nothing above, although every link of the space is in `wraps`.
    let from_30 = chain_open(
        &chain.keys[29],
        &chain.links,
        &space,
        Epoch::FIRST,
        &chain.dir,
        VerifyAt::Now,
    )
    .expect("a walk from epoch 30");
    assert_eq!(from_30.len(), 29);
    assert!(from_30.iter().all(|k| k.epoch < epoch(30)));
    assert_eq!(from_30.last().map(|k| k.epoch), Some(Epoch::FIRST));
}

/// A missing or tampered link (a flipped bit, a swapped or relabelled link, a link of another
/// space, a doubled link) stops the walk with a typed error AT THAT LINK.
#[test]
fn c6_a_missing_or_tampered_link_stops_the_walk_at_that_link() {
    let space = SpaceId::from_bytes(pat16(0x61));
    let chain = build_chain(space, 12, 61);
    let newest = &chain.keys[11];
    let walk = |links: &[Envelope]| {
        chain_open(
            newest,
            links,
            &space,
            Epoch::FIRST,
            &chain.dir,
            VerifyAt::Now,
        )
    };
    // The untouched chain opens: 11 older epochs.
    assert_eq!(walk(&chain.links).expect("the whole chain").len(), 11);

    // A missing link: the walk reaches it and stops.
    let mut links = chain.links.clone();
    links.remove(8); // the link that seals epoch 9
    assert_eq!(walk(&links).unwrap_err(), fault(9, LinkFault::Missing));
    // The keys above the break are still reachable: ask only for them.
    let above = chain_open(newest, &links, &space, epoch(10), &chain.dir, VerifyAt::Now)
        .expect("the epochs above the break");
    assert_eq!(above.len(), 2);

    // Every field of the link is covered: each flipped bit is refused at the link, with the
    // error of the first check it fails.
    struct Case {
        name: &'static str,
        tamper: Box<dyn Fn(&mut Envelope)>,
        refused: OpenError,
    }
    #[cfg_attr(not(feature = "pq"), allow(unused_mut))]
    let mut cases = vec![
        Case {
            name: "ct",
            tamper: Box::new(|e| e.ct[0] ^= 1),
            refused: OpenError::BadSignature,
        },
        Case {
            name: "nonce",
            tamper: Box::new(|e| e.nonce[0] ^= 1),
            refused: OpenError::BadSignature,
        },
        Case {
            name: "epk",
            tamper: Box::new(|e| {
                if let Some(epk) = e.epk.as_mut() {
                    epk[0] ^= 1;
                }
            }),
            refused: OpenError::BadSignature,
        },
        Case {
            name: "sig",
            tamper: Box::new(|e| {
                let mut bytes = *e.sig.as_bytes();
                bytes[10] ^= 1;
                e.sig = azlin_keys::Signature::from_bytes(bytes);
            }),
            refused: OpenError::BadSignature,
        },
        Case {
            name: "rcpt",
            tamper: Box::new(|e| {
                let mut bytes = *e.rcpt.as_bytes();
                bytes[0] ^= 1;
                e.rcpt = azlin_keys::KeyId::from_bytes(bytes);
            }),
            refused: OpenError::NotForMe,
        },
        Case {
            name: "signer",
            tamper: Box::new(|e| {
                let mut bytes = *e.signer.as_bytes();
                bytes[0] ^= 1;
                e.signer = azlin_keys::DeviceId::from_bytes(bytes);
            }),
            refused: OpenError::UnknownSigner,
        },
    ];
    #[cfg(feature = "pq")]
    cases.push(Case {
        name: "kem_ct",
        tamper: Box::new(|e| {
            if let Some(kem_ct) = e.kem_ct.as_mut() {
                kem_ct[0] ^= 1;
            }
        }),
        refused: OpenError::BadSignature,
    });
    for case in &cases {
        let mut links = chain.links.clone();
        (case.tamper)(&mut links[8]);
        assert_eq!(
            walk(&links).unwrap_err(),
            fault(9, LinkFault::Open(case.refused)),
            "a flipped bit in {}",
            case.name
        );
    }

    // Two links swapped in the list: the same chain (they are found by context) ...
    let mut swapped = chain.links.clone();
    swapped.swap(2, 7);
    assert_eq!(walk(&swapped).expect("swapped positions").len(), 11);
    // ... but a link relabelled for another epoch and re-signed by an authorized device (so that
    // only the cipher can see it): the link that seals epoch 10 given the context of epoch 9,
    // and the recipient id the walk expects there. The signature verifies, the cipher refuses.
    let mut forged = chain.links[9].clone();
    forged.ctx = Ctx::chain(&space, epoch(9));
    forged.rcpt = chain.keys[9].chain_recipient().key_id();
    forged.sign(&chain.device.signer()).expect("re-signed");
    let mut relabelled = chain.links.clone();
    relabelled[8] = forged;
    assert_eq!(
        walk(&relabelled).unwrap_err(),
        fault(9, LinkFault::Open(OpenError::Auth))
    );
    // A link given the context of the epoch above, with its own recipient: not for the key
    // that walk holds.
    let mut shifted = chain.links[8].clone();
    shifted.ctx = Ctx::chain(&space, epoch(10));
    shifted.sign(&chain.device.signer()).expect("re-signed");
    let mut shifted_links = chain.links.clone();
    shifted_links[9] = shifted;
    assert_eq!(
        walk(&shifted_links).unwrap_err(),
        fault(10, LinkFault::Open(OpenError::NotForMe))
    );

    // A link of another space: the walk does not find a link for this space and stops there.
    let other = build_chain(SpaceId::from_bytes(pat16(0x62)), 12, 62);
    let mut foreign = chain.links.clone();
    foreign[8] = other.links[8].clone();
    assert_eq!(walk(&foreign).unwrap_err(), fault(9, LinkFault::Missing));

    // Two different envelopes for one epoch are a fault of the registry, never a choice; an
    // identical copy counts once.
    let another = chain_wrap(
        &chain.keys[8],
        &chain.keys[9],
        &space,
        &chain.device.id,
        &chain.device.signer(),
        &mut SeededEntropy::from_u64(999),
    )
    .expect("another link for epoch 9");
    assert_ne!(another, chain.links[8]);
    let mut doubled = chain.links.clone();
    doubled.push(another);
    assert_eq!(walk(&doubled).unwrap_err(), fault(9, LinkFault::Duplicate));
    let mut copied = chain.links.clone();
    copied.push(chain.links[8].clone());
    assert_eq!(walk(&copied).expect("an identical copy").len(), 11);
}

/// Every link's signature is checked through the directory, at the point the caller names: an
/// unknown signer, a signer not yet active, a revoked one and a directory that cannot answer are
/// each refused with their own error at the first link; a signer who was valid then and revoked
/// since opens at the earlier point.
#[test]
fn c6_every_link_is_verified_through_the_directory_at_the_given_point() {
    let space = SpaceId::from_bytes(pat16(0x63));
    let chain = build_chain(space, 6, 63);
    let newest = &chain.keys[5];
    let key = chain.device.keys.public().verify_key();
    let walk = |dir: &MapDirectory, at: VerifyAt| {
        chain_open(newest, &chain.links, &space, Epoch::FIRST, dir, at)
    };
    let refused = |error: OpenError| fault(5, LinkFault::Open(error));

    assert_eq!(walk(&chain.dir, VerifyAt::Now).expect("known").len(), 5);
    assert_eq!(
        walk(&MapDirectory::new(), VerifyAt::Now).unwrap_err(),
        refused(OpenError::UnknownSigner)
    );
    let mut unavailable = chain.dir.clone();
    unavailable.set_unavailable(true);
    assert_eq!(
        walk(&unavailable, VerifyAt::Now).unwrap_err(),
        refused(OpenError::DirectoryUnavailable)
    );
    let entry = |now: DeviceState| DirectoryEntry {
        key,
        now,
        added_at: UnixTime::from_secs(10),
        removed_at: Some(UnixTime::from_secs(100)),
        added_seq: ChainSeq::new(2),
        removed_seq: Some(ChainSeq::new(10)),
    };
    let mut pending = MapDirectory::new();
    pending.insert(chain.device.id, entry(DeviceState::Pending));
    assert_eq!(
        walk(&pending, VerifyAt::Now).unwrap_err(),
        refused(OpenError::SignerNotYetActive)
    );
    // Revoked now, valid at the points before the removal: the caller's point decides.
    let mut removed = MapDirectory::new();
    removed.insert(chain.device.id, entry(DeviceState::Revoked));
    assert_eq!(
        walk(&removed, VerifyAt::Now).unwrap_err(),
        refused(OpenError::SignerRevoked)
    );
    assert_eq!(
        walk(&removed, VerifyAt::ChainSeq(ChainSeq::new(5)))
            .expect("valid at position 5")
            .len(),
        5
    );
    assert_eq!(
        walk(&removed, VerifyAt::ChainSeq(ChainSeq::new(10))).unwrap_err(),
        refused(OpenError::SignerRevoked)
    );
    assert_eq!(
        walk(&removed, VerifyAt::ChainSeq(ChainSeq::new(1))).unwrap_err(),
        refused(OpenError::SignerNotYetActive)
    );
    assert_eq!(
        walk(&removed, VerifyAt::Time(UnixTime::from_secs(50)))
            .expect("valid at time 50")
            .len(),
        5
    );
    assert_eq!(
        walk(&removed, VerifyAt::Time(UnixTime::from_secs(100))).unwrap_err(),
        refused(OpenError::SignerRevoked)
    );
}

/// A link that opens to something that is no space key, or to a key that no rotation could have
/// made, is refused at its epoch; `chain_wrap` refuses such a pair too, and a pair of the wrong
/// epochs or spaces.
#[test]
fn c6_a_link_must_hold_a_space_key_that_fits_the_epoch_above() {
    let space = SpaceId::from_bytes(pat16(0x64));
    let chain = build_chain(space, 12, 64);
    let newest = &chain.keys[11];
    let mut ent = SeededEntropy::from_u64(640);
    let walk = |links: &[Envelope]| {
        chain_open(newest, links, &space, epoch(11), &chain.dir, VerifyAt::Now)
    };
    let signer = chain.device.signer();

    // Not 68 bytes.
    let garbage = newest
        .chain_recipient()
        .wrap(
            &Ctx::chain(&space, epoch(11)),
            &[7u8; 10],
            &chain.device.id,
            &signer,
            &mut ent,
        )
        .expect("a wrap of 10 bytes");
    let mut links = chain.links.clone();
    links[10] = garbage;
    assert_eq!(walk(&links).unwrap_err(), fault(11, LinkFault::Plaintext));

    // A key whose dedup material does not follow from the epoch above: epoch 12 carries the
    // dedup seed of epoch 7, and this key has another dedup seed with the same dedup epoch.
    assert_eq!(newest.dedup_epoch(), epoch(7));
    let bad_older = space_key(&space, 11, 0x77, 0x88, 7);
    assert_eq!(
        chain_wrap(
            &bad_older,
            newest,
            &space,
            &chain.device.id,
            &signer,
            &mut ent
        )
        .unwrap_err(),
        fault(11, LinkFault::Inconsistent)
    );
    let forged = newest
        .chain_recipient()
        .wrap(
            &Ctx::chain(&space, epoch(11)),
            bad_older.to_plaintext().expose(),
            &chain.device.id,
            &signer,
            &mut ent,
        )
        .expect("a forged link");
    let mut links = chain.links.clone();
    links[10] = forged;
    assert_eq!(
        walk(&links).unwrap_err(),
        fault(11, LinkFault::Inconsistent)
    );

    // The pair of a real rotation is accepted in both of its shapes: carried and re-keyed.
    assert!(chain_wrap(
        &chain.keys[10],
        newest,
        &space,
        &chain.device.id,
        &signer,
        &mut ent
    )
    .is_ok());
    let fresh = &chain.keys[6]; // epoch 7: the re-key
    assert_eq!(fresh.dedup_epoch(), epoch(7));
    assert!(chain_wrap(
        &chain.keys[5],
        fresh,
        &space,
        &chain.device.id,
        &signer,
        &mut ent
    )
    .is_ok());

    // Not the epoch above, not the same space.
    assert_eq!(
        chain_wrap(
            &chain.keys[3],
            newest,
            &space,
            &chain.device.id,
            &signer,
            &mut ent
        )
        .unwrap_err(),
        KeyError::NotNextEpoch
    );
    assert_eq!(
        chain_wrap(
            newest,
            &chain.keys[10],
            &space,
            &chain.device.id,
            &signer,
            &mut ent
        )
        .unwrap_err(),
        KeyError::NotNextEpoch
    );
    let other_space = SpaceId::from_bytes(pat16(0x65));
    assert_eq!(
        chain_wrap(
            &chain.keys[10],
            newest,
            &other_space,
            &chain.device.id,
            &signer,
            &mut ent
        )
        .unwrap_err(),
        KeyError::WrongSpace
    );
    assert_eq!(
        chain_open(
            newest,
            &chain.links,
            &other_space,
            epoch(11),
            &chain.dir,
            VerifyAt::Now
        )
        .unwrap_err(),
        KeyError::WrongSpace
    );
    // The first epoch has no older epoch to open.
    assert_eq!(
        chain_open_link(
            &chain.keys[0],
            &chain.links[0],
            &space,
            &chain.dir,
            VerifyAt::Now
        )
        .unwrap_err(),
        KeyError::NotNextEpoch
    );
}

/// The key of epoch n cannot open epoch n+1: not by walking (the walk goes down), not by opening
/// the link that seals n+1, and not with a key that has the dedup seed of epoch n and the label
/// of its epoch but not its seal seed (the chain key derives from the seal seed alone).
#[test]
fn c6_epoch_n_cannot_open_epoch_n_plus_1() {
    let space = SpaceId::from_bytes(pat16(0x66));
    let chain = build_chain(space, 12, 66);
    let key_10 = &chain.keys[9];
    let link_sealing_11 = &chain.links[10];
    let link_sealing_9 = &chain.links[8];

    // The link that seals epoch 11 is for the context of epoch 11, not of the epoch below epoch
    // 10: epoch 10's key does not even get to try its cipher.
    assert_eq!(
        chain_open_link(key_10, link_sealing_11, &space, &chain.dir, VerifyAt::Now).unwrap_err(),
        fault(9, LinkFault::Open(OpenError::CtxMismatch))
    );
    // The walk from epoch 10 never returns epoch 11 or 12, whatever it is given.
    let below = chain_open(
        key_10,
        &chain.links,
        &space,
        Epoch::FIRST,
        &chain.dir,
        VerifyAt::Now,
    )
    .expect("a walk from epoch 10");
    assert!(below.iter().all(|k| k.epoch < epoch(10)));
    assert_eq!(below.len(), 9);
    // The key that has epoch 10's dedup seed and epoch, and another epoch's seal seed, is not
    // epoch 10: the link that seals epoch 9 is not for it.
    let forged_bytes = [
        seal_seed_of(&chain.keys[11]),
        dedup_seed_of(key_10),
        key_10.dedup_epoch().get().to_be_bytes().to_vec(),
    ]
    .concat();
    let forged = SpaceKey::from_plaintext(&space, epoch(10), &forged_bytes).expect("a space key");
    assert_eq!(forged.dedup_epoch(), key_10.dedup_epoch());
    assert_eq!(
        chain_open_link(&forged, link_sealing_9, &space, &chain.dir, VerifyAt::Now).unwrap_err(),
        fault(9, LinkFault::Open(OpenError::NotForMe))
    );
    // The real key opens it.
    assert!(chain_open_link(key_10, link_sealing_9, &space, &chain.dir, VerifyAt::Now).is_ok());
}

/// A removed member who knows epoch n and holds every public link cannot read epoch n+1: no link
/// is sealed to a key they hold, and the new epoch's key is wrapped only to the remaining.
#[test]
fn a_removed_member_with_every_link_reads_nothing_new() {
    let space = SpaceId::from_bytes(pat16(0x67));
    let chain = build_chain(space, 8, 67);
    let mut ent = SeededEntropy::from_u64(670);
    let removed = Device::new(&mut ent);
    let remaining = Device::new(&mut ent);
    // Epoch 8 is wrapped to the remaining member only.
    let env8 = wrap_space_key(
        &chain.keys[7],
        &space,
        &WrapTarget::Member(remaining.keys.public()),
        &chain.device.id,
        &chain.device.signer(),
        &mut ent,
    )
    .expect("a wrap to the remaining member");
    assert!(open_space_key(
        &env8,
        &space,
        epoch(8),
        &remaining.keys,
        &chain.dir,
        VerifyAt::Now
    )
    .is_ok());
    assert_eq!(
        open_space_key(
            &env8,
            &space,
            epoch(8),
            &removed.keys,
            &chain.dir,
            VerifyAt::Now
        )
        .unwrap_err(),
        KeyError::Open(OpenError::NotForMe)
    );
    // The removed member's best walk, from epoch 7 with all links, ends at epoch 1.
    let walk = chain_open(
        &chain.keys[6],
        &chain.links,
        &space,
        Epoch::FIRST,
        &chain.dir,
        VerifyAt::Now,
    )
    .expect("the removed member's walk");
    assert!(walk.iter().all(|k| k.epoch < epoch(7)));
}

/// The walk is bounded by `CHAIN_MAX`: exactly that many epochs may be asked for and one more is
/// refused before any link is opened (the boundary is checked without 4,096 links: the walk of
/// exactly the limit begins and stops at its first link).
#[test]
fn chain_depth_is_bounded() {
    assert_eq!(CHAIN_MAX, 4096);
    let space = SpaceId::from_bytes(pat16(0x68));
    let dir = MapDirectory::new();
    let plaintext = [pat(0x10, 64), 1u32.to_be_bytes().to_vec()].concat();
    let key = |at: u32| SpaceKey::from_plaintext(&space, epoch(at), &plaintext).expect("a key");

    // 4,096 steps (epoch 4,097 down to epoch 1): allowed, and it stops at the first missing link.
    assert_eq!(
        chain_open(&key(4097), &[], &space, Epoch::FIRST, &dir, VerifyAt::Now).unwrap_err(),
        fault(4096, LinkFault::Missing)
    );
    // 4,097 steps: refused before anything is opened, whatever `wraps` holds.
    assert_eq!(
        chain_open(&key(4098), &[], &space, Epoch::FIRST, &dir, VerifyAt::Now).unwrap_err(),
        KeyError::ChainTooLong
    );
    // The same limit from the other end: 4,097 steps down to epoch 2 is 4,096 and allowed.
    assert_eq!(
        chain_open(&key(4098), &[], &space, epoch(2), &dir, VerifyAt::Now).unwrap_err(),
        fault(4097, LinkFault::Missing)
    );
    assert_eq!(
        chain_open(
            &key(u32::MAX),
            &[],
            &space,
            Epoch::FIRST,
            &dir,
            VerifyAt::Now
        )
        .unwrap_err(),
        KeyError::ChainTooLong
    );
}

// ---------------------------------------------------------------------------------------------
// The chain recipient
// ---------------------------------------------------------------------------------------------

/// The chain recipient of an epoch is the BLAKE3 `derive_key` stream of its seal seed under the
/// context `CHAIN_WRAP`: the first 32 bytes are the X25519 secret (exactly what the registry
/// function `derive` gives), the next 64 the ML-KEM seed. It depends on the seal seed alone, so
/// the holder of `K_seal`, of `K_chunk` or of the dedup seed that the epochs of a space share
/// cannot open the chain.
#[test]
fn the_chain_recipient_is_the_derive_key_stream_of_the_seal_seed() {
    let space = SpaceId::from_bytes(pat16(0x90));
    let key = space_key(&space, 6, 0x41, 0x42, 3);
    let seal = Secret::<32>::from_slice(&seal_seed_of(&key)).expect("32 bytes");

    // The stream, read by hand: 32 bytes of X25519 secret, then 64 of ML-KEM seed.
    let mut hasher = blake3::Hasher::new_derive_key(CHAIN_WRAP.as_str());
    hasher.update(seal.expose());
    let mut stream = hasher.finalize_xof();
    let mut kx = [0u8; 32];
    let mut pq = [0u8; 64];
    stream.fill(&mut kx);
    stream.fill(&mut pq);
    assert_eq!(derive(&CHAIN_WRAP, &seal).expose(), &kx);
    let by_hand = RecipientSecret::new(&Secret::from_bytes(kx), Some(&Secret::from_bytes(pq)));
    assert_eq!(key.chain_recipient(), by_hand.recipient());
    assert_eq!(key.chain_recipient().key_id(), by_hand.recipient().key_id());

    // The same seal seed with another dedup seed, dedup epoch or epoch: the same recipient.
    for same in [
        space_key(&space, 6, 0x41, 0x99, 3),
        space_key(&space, 9, 0x41, 0x42, 5),
    ] {
        assert_eq!(same.chain_recipient(), key.chain_recipient());
    }
    // Another seal seed: another recipient.
    assert_ne!(
        space_key(&space, 6, 0x43, 0x42, 3).chain_recipient(),
        key.chain_recipient()
    );
    // Not the recipient of any other key the epoch has: K_seal, K_chunk and the dedup seed used
    // as X25519 secrets name other recipients.
    let dedup = Secret::<32>::from_slice(&dedup_seed_of(&key)).expect("32 bytes");
    for other in [key.k_seal(), key.k_chunk(), dedup] {
        assert_ne!(
            RecipientSecret::new(&other, None).x25519_id(),
            key.chain_recipient().key_id()
        );
    }
}

// ---------------------------------------------------------------------------------------------
// C7
// ---------------------------------------------------------------------------------------------

/// `Full`: a joiner given the newest key and the chain reads all epochs. `Forward`: a joiner
/// given only the new epoch's key cannot open any older epoch, yet can compute the current
/// `K_chunk`.
#[test]
fn c7_history_modes() {
    let space = SpaceId::from_bytes(pat16(0x70));
    let mut ent = SeededEntropy::from_u64(7);

    // ---- full: five epochs and their four links; the joiner is wrapped the newest key.
    let chain = build_chain(space, 5, 71);
    let joiner = Device::new(&mut ent);
    let newest = &chain.keys[4];
    let wrap = wrap_space_key(
        newest,
        &space,
        &WrapTarget::Member(joiner.keys.public()),
        &chain.device.id,
        &chain.device.signer(),
        &mut ent,
    )
    .expect("a wrap to the joiner");
    let opened = open_space_key(
        &wrap,
        &space,
        epoch(5),
        &joiner.keys,
        &chain.dir,
        VerifyAt::Now,
    )
    .expect("the joiner opens the newest key");
    let older = chain_open(
        &opened,
        &chain.links,
        &space,
        Epoch::FIRST,
        &chain.dir,
        VerifyAt::Now,
    )
    .expect("the joiner walks the chain");
    let grant = join_grant(History::Full, epoch(5), true);
    assert_eq!(
        grant.epochs,
        vec![epoch(5), epoch(4), epoch(3), epoch(2), epoch(1)]
    );
    assert_eq!(older.len(), grant.epochs.len() - 1);
    for key in &older {
        let original = &chain.keys[key.epoch.get() as usize - 1];
        assert!(
            key.k_seal().ct_eq(&original.k_seal()),
            "K_seal of epoch {}",
            key.epoch
        );
        assert!(key.k_chunk().ct_eq(&original.k_chunk()));
    }

    // ---- forward: the same five epochs, but the space writes no chain wrap for the epoch made on
    // add. The joiner is added: the key rotates to epoch 6 (dedup seed carried) and is wrapped to
    // the veteran and the joiner; no link of epoch 5 under epoch 6 exists.
    let veteran = Device::new(&mut ent);
    let old_wrap = wrap_space_key(
        newest,
        &space,
        &WrapTarget::Member(veteran.keys.public()),
        &chain.device.id,
        &chain.device.signer(),
        &mut ent,
    )
    .expect("the veteran's wrap of epoch 5");
    let k6 = SpaceKey::generate(&space, epoch(6), DedupChoice::Carry, Some(newest), &mut ent)
        .expect("the rotation on add");
    let wrap6 = wrap_space_key(
        &k6,
        &space,
        &WrapTarget::Member(joiner.keys.public()),
        &chain.device.id,
        &chain.device.signer(),
        &mut ent,
    )
    .expect("a wrap of epoch 6 to the joiner");
    let held = open_space_key(
        &wrap6,
        &space,
        epoch(6),
        &joiner.keys,
        &chain.dir,
        VerifyAt::Now,
    )
    .expect("the joiner opens epoch 6");
    // Epoch 5 is out of reach: through the veteran's wrap (not for the joiner) ...
    assert_eq!(
        open_space_key(
            &old_wrap,
            &space,
            epoch(5),
            &joiner.keys,
            &chain.dir,
            VerifyAt::Now
        )
        .unwrap_err(),
        KeyError::Open(OpenError::NotForMe)
    );
    // ... and through the chain: all four links of the full era are public, and none seals epoch
    // 5 under epoch 6.
    assert_eq!(
        chain_open(
            &held,
            &chain.links,
            &space,
            epoch(5),
            &chain.dir,
            VerifyAt::Now
        )
        .unwrap_err(),
        fault(5, LinkFault::Missing)
    );
    assert_eq!(
        chain_open(
            &held,
            &chain.links,
            &space,
            Epoch::FIRST,
            &chain.dir,
            VerifyAt::Now
        )
        .unwrap_err(),
        fault(5, LinkFault::Missing)
    );
    // The joiner still computes the current K_chunk (the dedup seed is carried), and has a K_seal
    // of its own epoch only.
    assert!(held.k_chunk().ct_eq(&newest.k_chunk()));
    assert!(!held.k_seal().ct_eq(&newest.k_seal()));
    assert_eq!(
        join_grant(History::Forward, epoch(6), true).epochs,
        vec![epoch(6)]
    );
    assert_eq!(
        join_grant(History::Forward, epoch(6), false).epochs,
        vec![epoch(6)]
    );
}

/// `forward_history_joiner_cannot_read_older`: the joiner of a forward space holds only the new
/// epoch's key. Every older epoch's wrap and every public link is useless to them.
#[test]
fn forward_history_joiner_cannot_read_older() {
    let space = SpaceId::from_bytes(pat16(0x71));
    let chain = build_chain(space, 4, 72);
    let mut ent = SeededEntropy::from_u64(720);
    let joiner = Device::new(&mut ent);
    // The older epochs' wraps went to the members of their time; each is refused for the joiner.
    let member = Device::new(&mut ent);
    for key in &chain.keys {
        let env = wrap_space_key(
            key,
            &space,
            &WrapTarget::Member(member.keys.public()),
            &chain.device.id,
            &chain.device.signer(),
            &mut ent,
        )
        .expect("an old wrap");
        assert_eq!(
            open_space_key(
                &env,
                &space,
                key.epoch,
                &joiner.keys,
                &chain.dir,
                VerifyAt::Now
            )
            .unwrap_err(),
            KeyError::Open(OpenError::NotForMe)
        );
    }
    // The new epoch's key is the joiner's only key; no walk reads below it.
    let k5 = SpaceKey::generate(
        &space,
        epoch(5),
        DedupChoice::Carry,
        Some(&chain.keys[3]),
        &mut ent,
    )
    .expect("the rotation on add");
    assert_eq!(
        chain_open(
            &k5,
            &chain.links,
            &space,
            Epoch::FIRST,
            &chain.dir,
            VerifyAt::Now
        )
        .unwrap_err(),
        fault(4, LinkFault::Missing)
    );
    assert!(k5.k_chunk().ct_eq(&chain.keys[3].k_chunk()));
}

/// The grant: the newest first, the chain only for a `Full` space whose chain is available, never
/// further than `CHAIN_MAX` below the newest; the history words of the registry column.
#[test]
fn join_grant_and_history_words() {
    let all = |n: u32| (1..=n).rev().map(epoch).collect::<Vec<_>>();
    assert_eq!(join_grant(History::Full, epoch(1), true).epochs, all(1));
    assert_eq!(join_grant(History::Full, epoch(3), true).epochs, all(3));
    assert_eq!(
        join_grant(History::Full, epoch(3), false).epochs,
        vec![epoch(3)]
    );
    assert_eq!(
        join_grant(History::Forward, epoch(3), true).epochs,
        vec![epoch(3)]
    );
    assert_eq!(
        join_grant(History::Forward, epoch(3), false).epochs,
        vec![epoch(3)]
    );
    // Never more than CHAIN_MAX below the newest.
    let long = join_grant(History::Full, epoch(10_000), true).epochs;
    assert_eq!(long.len(), CHAIN_MAX as usize + 1);
    assert_eq!(long.first(), Some(&epoch(10_000)));
    assert_eq!(long.last(), Some(&epoch(10_000 - CHAIN_MAX)));
    let exact = join_grant(History::Full, epoch(CHAIN_MAX + 1), true).epochs;
    assert_eq!(exact.len(), CHAIN_MAX as usize + 1);
    assert_eq!(exact.last(), Some(&Epoch::FIRST));

    assert_eq!(History::Full.word(), "full");
    assert_eq!(History::Forward.word(), "forward");
    assert_eq!(History::from_word("full"), Some(History::Full));
    assert_eq!(History::from_word("forward"), Some(History::Forward));
    for word in ["", "Full", "FORWARD", "full ", "none", "fwd"] {
        assert_eq!(History::from_word(word), None, "{word:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// C10
// ---------------------------------------------------------------------------------------------

/// `removal_wrap_count(remaining, spaces)` is exactly `remaining + spaces` (section 5.6), for the
/// sizes of the brief, and never the product; and the wraps a removal really makes are that many.
#[test]
fn c10_removal_cost_is_members_plus_spaces() {
    for remaining in [1usize, 10, 1000] {
        for spaces in [1usize, 5, 100] {
            let cost = removal_wrap_count(remaining, spaces);
            assert_eq!(
                cost,
                Some(remaining + spaces),
                "{remaining} members, {spaces} spaces"
            );
            assert_ne!(cost, remaining.checked_mul(spaces));
        }
    }
    assert_eq!(removal_wrap_count(0, 0), Some(0));
    assert_eq!(removal_wrap_count(usize::MAX, 1), None);
    assert_eq!(removal_wrap_count(1, usize::MAX), None);

    // A real removal: one new group key epoch wrapped to each remaining member, one new epoch per
    // space wrapped to the new group key: the envelopes made are members + spaces.
    let mut ent = SeededEntropy::from_u64(10);
    let group = azlin_keys::GroupId::generate(&mut ent).expect("a group id");
    let manager = Device::new(&mut ent);
    let members: Vec<Device> = (0..10).map(|_| Device::new(&mut ent)).collect();
    let spaces: Vec<(SpaceId, SpaceKey)> = (0..5)
        .map(|_| {
            let id = SpaceId::generate(&mut ent).expect("a space id");
            let key = SpaceKey::generate(&id, epoch(1), DedupChoice::Fresh, None, &mut ent)
                .expect("a space key");
            (id, key)
        })
        .collect();
    let new_group_key = azlin_keys::group::GroupKey::generate(&group, epoch(2), &mut ent)
        .expect("the new group key");
    let mut envelopes = 0usize;
    for member in &members {
        azlin_keys::group::wrap_group_key_to_member(
            &new_group_key,
            &group,
            &member.keys.public(),
            &manager.id,
            &manager.signer(),
            &mut ent,
        )
        .expect("a group key wrap");
        envelopes += 1;
    }
    for (id, old) in &spaces {
        let next = SpaceKey::generate(id, epoch(2), DedupChoice::Carry, Some(old), &mut ent)
            .expect("the next epoch");
        wrap_space_key(
            &next,
            id,
            &WrapTarget::Group(new_group_key.public()),
            &manager.id,
            &manager.signer(),
            &mut ent,
        )
        .expect("a wrap to the group");
        envelopes += 1;
    }
    assert_eq!(
        Some(envelopes),
        removal_wrap_count(members.len(), spaces.len())
    );
    assert_ne!(envelopes, members.len() * spaces.len());
}

// ---------------------------------------------------------------------------------------------
// WrapTarget
// ---------------------------------------------------------------------------------------------

/// A target names the `rcpt` of its wraps; a member and a group are different recipients.
#[test]
fn a_wrap_target_names_the_rcpt_of_its_wraps() {
    let mut ent = SeededEntropy::from_u64(14);
    let space = SpaceId::generate(&mut ent).expect("a space id");
    let group = azlin_keys::GroupId::generate(&mut ent).expect("a group id");
    let signer = Device::new(&mut ent);
    let member = Device::new(&mut ent);
    let g =
        azlin_keys::group::GroupKey::generate(&group, Epoch::FIRST, &mut ent).expect("a group key");
    let key = SpaceKey::generate(&space, Epoch::FIRST, DedupChoice::Fresh, None, &mut ent)
        .expect("a space key");
    let dir = directory(&[&signer]);

    let to_member = WrapTarget::Member(member.keys.public());
    let to_group = WrapTarget::Group(g.public());
    assert_ne!(to_member.key_id(), to_group.key_id());
    assert_eq!(to_member.key_id(), member.keys.public().key_id());
    assert_eq!(to_group.key_id(), g.public().recipient_id());
    for target in [&to_member, &to_group] {
        let env = wrap_space_key(&key, &space, target, &signer.id, &signer.signer(), &mut ent)
            .expect("a wrap");
        assert_eq!(env.rcpt, target.key_id());
        assert_eq!(env.ctx.as_str(), format!("space/{space}/epoch/1"));
    }
    // The member opens theirs; the group's key opens the group's.
    let env = wrap_space_key(
        &key,
        &space,
        &to_member,
        &signer.id,
        &signer.signer(),
        &mut ent,
    )
    .expect("a wrap");
    assert_eq!(
        bytes_of(
            &open_space_key(
                &env,
                &space,
                Epoch::FIRST,
                &member.keys,
                &dir,
                VerifyAt::Now
            )
            .expect("opens")
        ),
        bytes_of(&key)
    );
    let env = wrap_space_key(
        &key,
        &space,
        &to_group,
        &signer.id,
        &signer.signer(),
        &mut ent,
    )
    .expect("a wrap");
    assert_eq!(
        bytes_of(
            &azlin_keys::space::open_space_key_with_group(
                &env,
                &space,
                Epoch::FIRST,
                &g,
                &dir,
                VerifyAt::Now
            )
            .expect("opens")
        ),
        bytes_of(&key)
    );
}

// ---------------------------------------------------------------------------------------------
// C11 and C12 for the space code
// ---------------------------------------------------------------------------------------------

fn sweep(total: usize, mut op: impl FnMut(&mut dyn Entropy) -> Result<(), KeyError>) {
    for k in 0..total {
        match op(&mut FailingEntropy::after(k)) {
            Err(KeyError::Entropy(EntropyError::Exhausted))
            | Err(KeyError::Wrap(azlin_keys::envelope::WrapError::Entropy(
                EntropyError::Exhausted,
            ))) => {}
            other => panic!("after {k} of {total} bytes the operation answered {other:?}"),
        }
    }
    op(&mut FailingEntropy::after(total)).expect("the operation succeeds with all its bytes");
}

/// A `FailingEntropy` in the space key generator, the space key wraps and the chain wrap returns
/// the error and no partial value.
#[test]
fn c11_entropy_failure_stops_everything_in_space_code() {
    let chain = build_chain(SpaceId::from_bytes(pat16(0x80)), 3, 11);
    let space = chain.space;
    let mut ent = SeededEntropy::from_u64(110);
    let member = Device::new(&mut ent);
    let signer = chain.device.signer();

    let total_of = |op: &dyn Fn(&mut dyn Entropy) -> Result<(), KeyError>| {
        let mut counted = Recording::new(SeededEntropy::from_u64(3));
        op(&mut counted).expect("the operation succeeds");
        counted.total()
    };
    // generate: 64 bytes for a first or re-keyed epoch, 32 for a carried one.
    let first = |e: &mut dyn Entropy| {
        SpaceKey::generate(&space, epoch(1), DedupChoice::Fresh, None, e).map(drop)
    };
    assert_eq!(total_of(&first), 64);
    sweep(64, first);
    let carried = |e: &mut dyn Entropy| {
        SpaceKey::generate(
            &space,
            epoch(4),
            DedupChoice::Carry,
            Some(&chain.keys[2]),
            e,
        )
        .map(drop)
    };
    assert_eq!(total_of(&carried), 32);
    sweep(32, carried);
    let rekeyed = |e: &mut dyn Entropy| {
        SpaceKey::generate(
            &space,
            epoch(4),
            DedupChoice::Fresh,
            Some(&chain.keys[2]),
            e,
        )
        .map(drop)
    };
    assert_eq!(total_of(&rekeyed), 64);
    sweep(64, rekeyed);

    // The wraps: ephemeral key, (ML-KEM message), nonce.
    let to_member = |e: &mut dyn Entropy| {
        wrap_space_key(
            &chain.keys[2],
            &space,
            &WrapTarget::Member(member.keys.public()),
            &chain.device.id,
            &signer,
            e,
        )
        .map(drop)
    };
    let member_total = total_of(&to_member);
    #[cfg(feature = "pq")]
    assert_eq!(member_total, 32 + 32 + 24);
    #[cfg(not(feature = "pq"))]
    assert_eq!(member_total, 32 + 24);
    sweep(member_total, to_member);

    let link = |e: &mut dyn Entropy| {
        chain_wrap(
            &chain.keys[1],
            &chain.keys[2],
            &space,
            &chain.device.id,
            &signer,
            e,
        )
        .map(drop)
    };
    let link_total = total_of(&link);
    #[cfg(feature = "pq")]
    assert_eq!(link_total, 32 + 32 + 24);
    #[cfg(not(feature = "pq"))]
    assert_eq!(link_total, 32 + 24);
    sweep(link_total, link);
}

/// `Debug` of a space key shows no byte of it; `k_seal`, `k_chunk` and `to_plaintext` return
/// `Secret`s, which print as `Secret<N>(***)`.
#[test]
fn c12_secrets_never_print_in_space_code() {
    let space = SpaceId::from_bytes(pat16(0xa0));
    let key = SpaceKey::generate(
        &space,
        epoch(1),
        DedupChoice::Fresh,
        None,
        &mut ScriptedEntropy::new([pat(0x10, 32), pat(0x30, 32)].concat()),
    )
    .expect("a space key");
    let shown = format!("{key:?}");
    assert!(shown.contains("***"), "{shown}");
    assert!(shown.contains(&space.to_string()));
    assert!(shown.contains("epoch 1"));
    let secret = bytes_of(&key);
    for window in secret.windows(4) {
        let hex = to_hex(window);
        assert!(!shown.to_lowercase().contains(&hex), "{shown} shows {hex}");
    }
    let plaintext: Secret<68> = key.to_plaintext();
    let k_seal: Secret<32> = key.k_seal();
    let k_chunk: Secret<32> = key.k_chunk();
    assert_eq!(format!("{plaintext:?}"), "Secret<68>(***)");
    assert_eq!(format!("{k_seal:?}"), "Secret<32>(***)");
    assert_eq!(format!("{k_chunk:?}"), "Secret<32>(***)");
    // The recipient a link is wrapped to is public data and prints as such; its secret does not.
    assert!(!format!("{:?}", key.chain_recipient()).is_empty());
}
