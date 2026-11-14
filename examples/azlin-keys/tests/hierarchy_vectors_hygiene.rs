//! C3 (the known answers of the key hierarchy: `tests/vectors/space_keys.json`), C6's known
//! answers (`tests/vectors/epoch_chain.json`) and C13 (crate hygiene of the three modules).
//!
//! The positive vectors are made from fixed inputs: the entropy the generators draw (patterns:
//! `pat(start, n)` counts up from `start`, so an independent program rebuilds every byte; the
//! program of lane W1-S15c1 is `scratch/W1-S15c1/hier_ref.py`, with a BLAKE3, an X25519, an
//! XChaCha20-Poly1305, an Ed25519 and a deterministic CBOR written from the specifications and
//! OpenSSL 3.5's ML-KEM-768 for the hybrid half). They pin the draw order, the plaintext layouts,
//! `K_seal`, `K_chunk`, the key ids, the derivation of the chain recipient and the bytes of the
//! chain links. The negative vectors are inputs the code must refuse, each with the error it
//! answers.
//!
//! The files are written only under `UPDATE_VECTORS=1` (in a build with the feature `pq`) and
//! never when `CI` is set. Without the feature `pq` the rows named `pq_...` cannot be recomputed:
//! they are checked to exist, to be in place, and (the hybrid link) to be refused as unsupported.
//!
//! Implements planning/architecture.md section 5.5 and launch rule 10 (formats).
//! Run with `--features test-support` (the test doubles) and, for the hybrid rows, `pq`.

#![cfg(feature = "test-support")]
#![allow(dead_code)]

mod hierarchy_common;

use std::path::PathBuf;

use azlin_keys::derive::CONTEXTS;
use azlin_keys::envelope::{wrap_x25519, Alg, Ctx, Envelope, Recipient, RecipientX25519};
use azlin_keys::group::GroupKey;
use azlin_keys::member::{KeyBundle, PublicBundle};
use azlin_keys::space::{chain_open_link, chain_wrap, DedupChoice, SpaceKey};
use azlin_keys::{
    DeviceId, Epoch, GroupId, KeyId, MapDirectory, Secret, SignKeypair, SpaceId, VerifyAt,
};
use hierarchy_common::vectors::{self, Row};
use hierarchy_common::{pat, pat16, pat32, to_hex, ScriptedEntropy};

const SPACE_KEYS: &str = "space_keys.json";
const EPOCH_CHAIN: &str = "epoch_chain.json";

fn epoch(n: u32) -> Epoch {
    Epoch::new(n).expect("an epoch from 1")
}

fn row(name: &str, valid: bool, hex: &str, expect: &str, inputs: &str) -> (String, Option<Row>) {
    (
        name.to_string(),
        Some(Row::new(name, valid, hex, expect, inputs)),
    )
}

/// A row that only a build with the feature `pq` computes.
fn pq_missing(name: &str) -> (String, Option<Row>) {
    (name.to_string(), None)
}

/// Writes a vector file from rows that were all computed (a build with the feature `pq`).
#[cfg(feature = "pq")]
fn write_rows(file: &str, rows: &[(String, Option<Row>)]) {
    let all: Vec<Row> = rows
        .iter()
        .map(|(_, r)| r.clone().expect("every row is computed in a build with pq"))
        .collect();
    vectors::write(file, &all);
}

#[cfg(not(feature = "pq"))]
fn write_rows(file: &str, _rows: &[(String, Option<Row>)]) {
    panic!("UPDATE_VECTORS=1 writes {file} only in a build with the feature pq: its pq_ rows need ML-KEM");
}

// ---------------------------------------------------------------------------------------------
// The inputs of the known answers
// ---------------------------------------------------------------------------------------------

/// The space of the known answers: `pat(0x50, 16)`.
fn kat_space() -> SpaceId {
    SpaceId::from_bytes(pat16(0x50))
}

/// One epoch of the known-answer space: the epoch, what the rotation into it is, the bytes the
/// entropy serves, and the text of the inputs.
struct Step {
    at: u32,
    choice: DedupChoice,
    draws: Vec<u8>,
    inputs: &'static str,
}

/// Epoch 1 draws both seeds; epochs 2 and 3 are removals (the dedup seed is carried); epoch 4 is
/// an explicit re-key (both seeds drawn); epoch 5 is a removal again.
fn steps() -> Vec<Step> {
    vec![
        Step {
            at: 1,
            choice: DedupChoice::Fresh,
            draws: [pat(0x10, 32), pat(0x30, 32)].concat(),
            inputs: "space=pat(0x50,16) epoch=1 choice=fresh prev=none draws=pat(0x10,32)+pat(0x30,32)",
        },
        Step {
            at: 2,
            choice: DedupChoice::Carry,
            draws: pat(0x50, 32),
            inputs: "space=pat(0x50,16) epoch=2 choice=carry prev=epoch1 draws=pat(0x50,32)",
        },
        Step {
            at: 3,
            choice: DedupChoice::Carry,
            draws: pat(0x70, 32),
            inputs: "space=pat(0x50,16) epoch=3 choice=carry prev=epoch2 draws=pat(0x70,32)",
        },
        Step {
            at: 4,
            choice: DedupChoice::Fresh,
            draws: [pat(0x90, 32), pat(0xb0, 32)].concat(),
            inputs: "space=pat(0x50,16) epoch=4 choice=fresh prev=epoch3 draws=pat(0x90,32)+pat(0xb0,32)",
        },
        Step {
            at: 5,
            choice: DedupChoice::Carry,
            draws: pat(0xd0, 32),
            inputs: "space=pat(0x50,16) epoch=5 choice=carry prev=epoch4 draws=pat(0xd0,32)",
        },
    ]
}

/// The five keys of the known-answer space, made by `SpaceKey::generate` from the scripted draws.
fn kat_keys(space: &SpaceId) -> Vec<SpaceKey> {
    let mut keys: Vec<SpaceKey> = Vec::new();
    for step in steps() {
        let mut ent = ScriptedEntropy::new(step.draws.clone());
        let key = SpaceKey::generate(space, epoch(step.at), step.choice, keys.last(), &mut ent)
            .expect("a known-answer key");
        assert_eq!(ent.left(), 0, "every scripted byte is used");
        keys.push(key);
    }
    keys
}

fn kat_signer() -> SignKeypair {
    SignKeypair::from_seed(&Secret::from_bytes(pat32(0x90)))
}

fn kat_device() -> DeviceId {
    DeviceId::from_bytes(pat16(0xd0))
}

fn kat_directory() -> MapDirectory {
    let mut dir = MapDirectory::new();
    dir.insert_active(kat_device(), kat_signer().verify_key());
    dir
}

// ---------------------------------------------------------------------------------------------
// space_keys.json
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "pq")]
fn member_pq_rows(public: &PublicBundle) -> Vec<(String, Option<Row>)> {
    let pq = public.pq_pub.as_ref().expect("a pq key in a pq build");
    vec![
        row(
            "pq_member_bundle_pq_pub_blake3",
            true,
            &to_hex(blake3::hash(pq).as_bytes()),
            "",
            "pq_pub = ML-KEM-768 encapsulation key of the seed pat(0x41,64) (d || z); hex = BLAKE3 of the 1184 bytes",
        ),
        row(
            "pq_member_bundle_key_id_hybrid",
            true,
            &to_hex(public.key_id().as_bytes()),
            "",
            "hex = KeyId::of_bundle(kx_pub, pq_pub) = derive_key(\"azlin key id v1\", kx_pub || pq_pub)",
        ),
    ]
}

#[cfg(not(feature = "pq"))]
fn member_pq_rows(_public: &PublicBundle) -> Vec<(String, Option<Row>)> {
    vec![
        pq_missing("pq_member_bundle_pq_pub_blake3"),
        pq_missing("pq_member_bundle_key_id_hybrid"),
    ]
}

/// The negative vectors of the three plaintext formats: what is refused and why.
fn refused_plaintext_rows() -> Vec<(String, Option<Row>)> {
    let space = kat_space();
    let group = GroupId::from_bytes(pat16(0xa0));
    let mut rows = Vec::new();
    let space_cases: Vec<(&str, Vec<u8>, &str)> = vec![
        (
            "neg_space_key_67_bytes",
            pat(0x10, 67),
            "a space key plaintext one byte short",
        ),
        (
            "neg_space_key_69_bytes",
            pat(0x10, 69),
            "a space key plaintext with one byte too many",
        ),
        (
            "neg_space_key_dedup_epoch_zero",
            [pat(0x10, 64), vec![0, 0, 0, 0]].concat(),
            "68 bytes, the dedup epoch is 0",
        ),
        (
            "neg_space_key_dedup_epoch_above_epoch",
            [pat(0x10, 64), vec![0, 0, 0, 4]].concat(),
            "68 bytes, the dedup epoch 4 is above the key's epoch 3",
        ),
    ];
    for (name, bytes, what) in space_cases {
        let error = SpaceKey::from_plaintext(&space, epoch(3), &bytes).unwrap_err();
        rows.push(row(
            name,
            false,
            &to_hex(&bytes),
            &format!("{error:?}"),
            &format!("SpaceKey::from_plaintext(space, epoch 3, hex): {what}"),
        ));
    }
    for (name, len) in [
        ("neg_member_bundle_127_bytes", 127usize),
        ("neg_member_bundle_129_bytes", 129),
    ] {
        let bytes = pat(0x01, len);
        let error = KeyBundle::from_plaintext(&bytes).unwrap_err();
        rows.push(row(
            name,
            false,
            &to_hex(&bytes),
            &format!("{error:?}"),
            "KeyBundle::from_plaintext(hex): a member key is exactly 128 bytes",
        ));
    }
    let bytes = pat(0x61, 31);
    let error = GroupKey::from_plaintext(&group, epoch(1), &bytes).unwrap_err();
    rows.push(row(
        "neg_group_key_31_bytes",
        false,
        &to_hex(&bytes),
        &format!("{error:?}"),
        "GroupKey::from_plaintext(group, epoch 1, hex): a group key is exactly 32 bytes",
    ));
    rows
}

fn space_key_rows() -> Vec<(String, Option<Row>)> {
    let mut rows = Vec::new();

    // The member bundle of the draws pat(0x01, 128): the Ed25519 seed, the X25519 secret, the
    // ML-KEM seed, in that order.
    let bundle = KeyBundle::generate(&mut ScriptedEntropy::new(pat(0x01, 128))).expect("a bundle");
    let public = bundle.public();
    rows.push(row(
        "member_bundle_keys",
        true,
        &to_hex(&[public.sign_pub, public.kx_pub].concat()),
        &to_hex(bundle.to_plaintext().expose()),
        "draws=pat(0x01,32)+pat(0x21,32)+pat(0x41,64) as ed25519 seed, x25519 secret, ml-kem seed; hex = sign_pub || kx_pub; expect = the 128 plaintext bytes",
    ));
    rows.push(row(
        "member_bundle_key_id_x25519",
        true,
        &to_hex(KeyId::of_bundle(&public.kx_pub, None).as_bytes()),
        "",
        "hex = KeyId::of_bundle(kx_pub, none) = derive_key(\"azlin key id v1\", kx_pub)",
    ));
    rows.extend(member_pq_rows(&public));

    // A group key of the draw pat(0x61, 32): the X25519 secret.
    let group = GroupId::from_bytes(pat16(0xa0));
    let g = GroupKey::generate(&group, epoch(3), &mut ScriptedEntropy::new(pat(0x61, 32)))
        .expect("a group key");
    rows.push(row(
        "group_key_public",
        true,
        &to_hex(&g.public().kx_pub),
        &to_hex(g.to_plaintext().expose()),
        "group=pat(0xa0,16) epoch=3 draws=pat(0x61,32) as the x25519 secret; hex = its public key; expect = the 32 plaintext bytes",
    ));
    rows.push(row(
        "group_key_id",
        true,
        &to_hex(g.id().as_bytes()),
        &to_hex(g.public().recipient_id().as_bytes()),
        "hex = derive_key(\"azlin 2026-11 group key id v1\", group || epoch as 4 bytes big-endian || kx_pub); expect = KeyId::of_bundle(kx_pub, none), the rcpt of a wrap to the group",
    ));

    // The five epochs of the known-answer space.
    let space = kat_space();
    for (step, key) in steps().iter().zip(kat_keys(&space)) {
        let derived = [
            key.k_seal().expose().as_slice(),
            key.k_chunk().expose().as_slice(),
            key.key_id().as_bytes().as_slice(),
        ]
        .concat();
        rows.push(row(
            &format!("space_key_epoch_{}", step.at),
            true,
            &to_hex(key.to_plaintext().expose()),
            &to_hex(&derived),
            step.inputs,
        ));
    }

    rows.extend(refused_plaintext_rows());
    rows
}

/// C3: for fixed seeds the K_seal and K_chunk of every epoch are pinned; the removals keep
/// K_chunk and change K_seal, the re-key changes both.
#[test]
fn c3_k_seal_and_k_chunk_known_answers() {
    let rows = space_key_rows();
    if vectors::updating() {
        write_rows(SPACE_KEYS, &rows);
        return;
    }
    vectors::check(SPACE_KEYS, &rows);

    // What the pinned answers say, read from the keys of the same scripts.
    let keys = kat_keys(&kat_space());
    let chunk = |k: &SpaceKey| k.k_chunk().expose().to_vec();
    let seal = |k: &SpaceKey| k.k_seal().expose().to_vec();
    assert_eq!(chunk(&keys[0]), chunk(&keys[1]));
    assert_eq!(chunk(&keys[1]), chunk(&keys[2]));
    assert_ne!(
        chunk(&keys[2]),
        chunk(&keys[3]),
        "the re-key changes K_chunk"
    );
    assert_eq!(chunk(&keys[3]), chunk(&keys[4]));
    for (i, a) in keys.iter().enumerate() {
        assert_ne!(seal(a), chunk(a), "K_seal and K_chunk differ");
        for b in keys.iter().skip(i + 1) {
            assert_ne!(seal(a), seal(b), "K_seal changes at every epoch");
        }
    }
    assert_eq!(
        keys.iter().map(SpaceKey::dedup_epoch).collect::<Vec<_>>(),
        vec![epoch(1), epoch(1), epoch(1), epoch(4), epoch(4)]
    );
    // At least three negative vectors per format; every refused row answers `BadPlaintext`.
    let pinned = vectors::read(SPACE_KEYS);
    let refused: Vec<&Row> = pinned.iter().filter(|r| !r.valid).collect();
    assert!(refused.len() >= 3);
    assert!(refused.iter().all(|r| r.expect == "BadPlaintext"));
    // The context strings of the keys are entries of S15a's table.
    for name in [
        "K_SEAL",
        "K_CHUNK",
        "CHAIN_WRAP",
        "SPACE_KEY_ID",
        "GROUP_KEY_ID",
    ] {
        assert!(CONTEXTS.iter().any(|c| c.name() == name), "{name}");
    }
}

// ---------------------------------------------------------------------------------------------
// epoch_chain.json
// ---------------------------------------------------------------------------------------------

/// The X25519 half of a chain recipient (the whole of it in a build without `pq`).
fn x25519_half(recipient: Recipient) -> RecipientX25519 {
    match recipient {
        Recipient::X25519(x) => x,
        Recipient::Hybrid(h) => RecipientX25519 { kx: h.kx },
    }
}

/// The link that seals `older` under `newer`, as an algorithm 1 envelope to the X25519 half of
/// `newer`'s chain recipient, from the draws `pat(eph, 32)` and `pat(nonce, 24)`: the bytes a
/// build without `pq` makes, and what any build must open.
fn classical_link(
    older: &SpaceKey,
    newer: &SpaceKey,
    space: &SpaceId,
    eph: u8,
    nonce: u8,
) -> Envelope {
    let mut ent = ScriptedEntropy::new([pat(eph, 32), pat(nonce, 24)].concat());
    let env = wrap_x25519(
        &x25519_half(newer.chain_recipient()),
        &Ctx::chain(space, older.epoch),
        older.to_plaintext().expose(),
        &kat_device(),
        &kat_signer(),
        &mut ent,
    )
    .expect("a classical link");
    assert_eq!(ent.draws, vec![32, 24]);
    assert_eq!(ent.left(), 0);
    env
}

#[cfg(feature = "pq")]
fn chain_pq_rows(keys: &[SpaceKey], space: &SpaceId) -> Vec<(String, Option<Row>)> {
    let Recipient::Hybrid(h) = keys[2].chain_recipient() else {
        panic!("a build with pq makes hybrid chain recipients");
    };
    let mut ent = ScriptedEntropy::new([pat(0x12, 32), pat(0x32, 32), pat(0x42, 24)].concat());
    let env = chain_wrap(
        &keys[1],
        &keys[2],
        space,
        &kat_device(),
        &kat_signer(),
        &mut ent,
    )
    .expect("a hybrid link");
    assert_eq!(ent.draws, vec![32, 32, 24]);
    assert_eq!(ent.left(), 0);
    assert_eq!(env.alg, Alg::X25519MlKem768);
    vec![
        row(
            "pq_chain_recipient_pq_blake3_epoch_3",
            true,
            &to_hex(blake3::hash(&h.pq).as_bytes()),
            "",
            "the ML-KEM-768 seed is bytes 32..96 of the derive_key stream of the seal seed of epoch 3; hex = BLAKE3 of its 1184-byte encapsulation key",
        ),
        row(
            "pq_chain_link_hybrid_epoch_2_under_3",
            true,
            &to_hex(&env.to_cbor().expect("the link's bytes")),
            &to_hex(keys[1].to_plaintext().expose()),
            "epoch 2 sealed under epoch 3 by chain_wrap, an algorithm 2 envelope to the chain recipient of epoch 3; ctx=chain/<space>/epoch/2 draws=eph pat(0x12,32), m pat(0x32,32), nonce pat(0x42,24) signer=pat(0x90,32) device=pat(0xd0,16); expect = the plaintext of epoch 2",
        ),
    ]
}

#[cfg(not(feature = "pq"))]
fn chain_pq_rows(keys: &[SpaceKey], space: &SpaceId) -> Vec<(String, Option<Row>)> {
    // Without pq the chain link is algorithm 1: `chain_wrap` makes exactly the classical bytes.
    let mut ent = ScriptedEntropy::new([pat(0x11, 32), pat(0x41, 24)].concat());
    let env = chain_wrap(
        &keys[1],
        &keys[2],
        space,
        &kat_device(),
        &kat_signer(),
        &mut ent,
    )
    .expect("a link");
    assert_eq!(env, classical_link(&keys[1], &keys[2], space, 0x11, 0x41));
    vec![
        pq_missing("pq_chain_recipient_pq_blake3_epoch_3"),
        pq_missing("pq_chain_link_hybrid_epoch_2_under_3"),
    ]
}

fn chain_rows() -> Vec<(String, Option<Row>)> {
    let space = kat_space();
    let keys = kat_keys(&space);
    let dir = kat_directory();
    let mut rows = Vec::new();

    rows.push(row(
        "chain_recipient_x25519_epoch_3",
        true,
        &to_hex(&x25519_half(keys[2].chain_recipient()).kx),
        "",
        "the X25519 secret is bytes 0..32 of the BLAKE3 derive_key stream (context \"azlin 2026-11 epoch chain wrap v1\") of the seal seed pat(0x70,32) of epoch 3; hex = its public key",
    ));
    let link_2 = classical_link(&keys[1], &keys[2], &space, 0x11, 0x41);
    let link_2_bytes = link_2.to_cbor().expect("the link's bytes");
    rows.push(row(
        "chain_link_x25519_epoch_2_under_3",
        true,
        &to_hex(&link_2_bytes),
        &to_hex(keys[1].to_plaintext().expose()),
        "epoch 2 sealed under epoch 3: an algorithm 1 envelope to the X25519 chain recipient of epoch 3; ctx=chain/<space>/epoch/2 draws=eph pat(0x11,32), nonce pat(0x41,24) signer=pat(0x90,32) device=pat(0xd0,16); expect = the plaintext of epoch 2",
    ));
    let link_3 = classical_link(&keys[2], &keys[3], &space, 0x13, 0x43);
    rows.push(row(
        "chain_link_x25519_epoch_3_under_4",
        true,
        &to_hex(&link_3.to_cbor().expect("the link's bytes")),
        &to_hex(keys[2].to_plaintext().expose()),
        "epoch 3 sealed under epoch 4 (across the re-key): ctx=chain/<space>/epoch/3 draws=eph pat(0x13,32), nonce pat(0x43,24); expect = the plaintext of epoch 3",
    ));
    rows.extend(chain_pq_rows(&keys, &space));

    // The refused links, each built from the first one.
    let refusal = |newer: &SpaceKey, bytes: &[u8], space: &SpaceId| {
        let env = Envelope::from_cbor(bytes).expect("the bytes are an envelope");
        let error = chain_open_link(newer, &env, space, &dir, VerifyAt::Now).unwrap_err();
        format!("{error:?}")
    };
    // A bit of the ciphertext flipped.
    let mut flipped = link_2.clone();
    flipped.ct[0] ^= 1;
    let flipped_bytes = flipped.to_cbor().expect("the flipped link's bytes");
    rows.push(row(
        "neg_chain_link_flipped_ct_bit",
        false,
        &to_hex(&flipped_bytes),
        &refusal(&keys[2], &flipped_bytes, &space),
        "chain_open_link(epoch 3 key, hex): the link of epoch 2 with the first bit of ct flipped; the signature covers the ciphertext",
    ));
    // The link of epoch 2 offered to the key of epoch 4, which expects the link of epoch 3.
    rows.push(row(
        "neg_chain_link_for_the_wrong_epoch",
        false,
        &to_hex(&link_2_bytes),
        &refusal(&keys[3], &link_2_bytes, &space),
        "chain_open_link(epoch 4 key, hex): the link of epoch 2 where the link of epoch 3 is expected",
    ));
    // A link that was made for another space.
    let other_space = SpaceId::from_bytes(pat16(0x51));
    let other_keys = kat_keys(&other_space);
    let other_link = classical_link(&other_keys[1], &other_keys[2], &other_space, 0x11, 0x41);
    let other_bytes = other_link.to_cbor().expect("the other link's bytes");
    rows.push(row(
        "neg_chain_link_made_for_another_space",
        false,
        &to_hex(&other_bytes),
        &refusal(&keys[2], &other_bytes, &space),
        "chain_open_link(epoch 3 key of space pat(0x50,16), hex): a link of epoch 2 of the space pat(0x51,16) (same seeds, other space)",
    ));
    // A key with the dedup material and the label of epoch 3 but another seal seed.
    let forged = SpaceKey::from_plaintext(
        &space,
        epoch(3),
        &[
            pat(0xee, 32),
            keys[2].to_plaintext().expose()[32..].to_vec(),
        ]
        .concat(),
    )
    .expect("a key");
    assert_eq!(forged.dedup_epoch(), keys[2].dedup_epoch());
    rows.push(row(
        "neg_chain_link_not_sealed_to_this_epoch_key",
        false,
        &to_hex(&link_2_bytes),
        &refusal(&forged, &link_2_bytes, &space),
        "chain_open_link(a key labelled epoch 3 with the dedup seed of epoch 3 and the seal seed pat(0xee,32), hex): the chain recipient derives from the seal seed alone",
    ));
    rows
}

/// C6: the chain links are pinned byte for byte, open to the older epoch, and the refused links
/// answer the errors the file names.
#[test]
fn c6_chain_known_answers_and_negative_vectors() {
    let rows = chain_rows();
    if vectors::updating() {
        write_rows(EPOCH_CHAIN, &rows);
        return;
    }
    vectors::check(EPOCH_CHAIN, &rows);

    let space = kat_space();
    let keys = kat_keys(&space);
    let dir = kat_directory();
    let pinned = vectors::read(EPOCH_CHAIN);
    let find = |name: &str| {
        pinned
            .iter()
            .find(|r| r.name == name)
            .unwrap_or_else(|| panic!("the row {name} is in the file"))
    };

    // Every positive link opens, with the key of the epoch above it, to the epoch it seals.
    for (name, older, newer) in [
        ("chain_link_x25519_epoch_2_under_3", &keys[1], &keys[2]),
        ("chain_link_x25519_epoch_3_under_4", &keys[2], &keys[3]),
    ] {
        let row = find(name);
        let link = Envelope::from_cbor(&hierarchy_common::from_hex(&row.hex)).expect("an envelope");
        assert_eq!(link.alg, Alg::X25519SealedBox);
        assert_eq!(link.ctx, Ctx::chain(&space, older.epoch));
        let opened = chain_open_link(newer, &link, &space, &dir, VerifyAt::Now).expect(name);
        assert_eq!(to_hex(opened.to_plaintext().expose()), row.expect);
    }
    let hybrid = find("pq_chain_link_hybrid_epoch_2_under_3");
    let link = Envelope::from_cbor(&hierarchy_common::from_hex(&hybrid.hex)).expect("an envelope");
    assert_eq!(link.alg, Alg::X25519MlKem768);
    assert_eq!(link.ctx, Ctx::chain(&space, keys[1].epoch));
    let result = chain_open_link(&keys[2], &link, &space, &dir, VerifyAt::Now);
    #[cfg(feature = "pq")]
    assert_eq!(
        to_hex(
            result
                .expect("the hybrid link opens")
                .to_plaintext()
                .expose()
        ),
        hybrid.expect
    );
    #[cfg(not(feature = "pq"))]
    assert_eq!(
        result.unwrap_err(),
        azlin_keys::member::KeyError::ChainLink {
            epoch: keys[1].epoch,
            fault: azlin_keys::member::LinkFault::Open(
                azlin_keys::envelope::OpenError::Unsupported(Alg::X25519MlKem768)
            )
        }
    );

    // At least three refused links, each with the error its row names.
    let refused: Vec<&Row> = pinned.iter().filter(|r| !r.valid).collect();
    assert!(refused.len() >= 3);
    for r in refused {
        assert!(
            r.expect.starts_with("ChainLink {"),
            "{}: {}",
            r.name,
            r.expect
        );
    }
}

// ---------------------------------------------------------------------------------------------
// C13: hygiene of the three modules
// ---------------------------------------------------------------------------------------------

fn source(file: &str) -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join(file),
    )
    .unwrap_or_else(|e| panic!("src/{file}: {e}"))
}

/// The code of a line: everything before a comment.
fn code_of(line: &str) -> &str {
    match line.find("//") {
        Some(at) => line.get(..at).unwrap_or(line),
        None => line,
    }
}

/// Clippy and the rule-6 denies are the crate's (lib.rs: `deny` of unwrap, expect, panic,
/// unreachable, todo, unimplemented and indexing in non-test code); this scan is what a test can
/// add: none of the forbidden constructs and none of the forbidden sources of time, files,
/// threads and randomness in the three files; no context string written out (every key is
/// derived under an entry of S15a's table); and a `Caller:` or `Seam:` line on every public item.
#[test]
fn c13_crate_hygiene() {
    let forbidden = [
        ".unwrap()",
        ".expect(",
        "panic!",
        "unreachable!",
        "todo!",
        "unimplemented!",
        "unwrap_or_default",
        ".ok()",
        "let _ =",
        "std::time",
        "std::fs",
        "std::net",
        "std::thread",
        "SystemTime",
        "Instant",
        "rand::",
        "unsafe",
        "azlin 2026-11",
        "azlin key id",
    ];
    let item_starts = [
        "pub fn ",
        "pub const fn ",
        "pub const ",
        "pub struct ",
        "pub enum ",
        "pub type ",
        "pub trait ",
    ];
    for file in ["member.rs", "group.rs", "space.rs"] {
        let text = source(file);
        let lines: Vec<&str> = text.lines().collect();
        let mut public_items = 0;
        for (i, line) in lines.iter().enumerate() {
            let code = code_of(line);
            for bad in forbidden {
                assert!(
                    !code.contains(bad),
                    "src/{file}:{}: `{bad}` in `{line}`",
                    i + 1
                );
            }
            let trimmed = line.trim_start();
            if !item_starts.iter().any(|start| trimmed.starts_with(start)) {
                continue;
            }
            public_items += 1;
            // The doc block above (attributes in between are skipped).
            let mut documented = false;
            let mut at = i;
            while at > 0 {
                at -= 1;
                let above = lines.get(at).copied().unwrap_or("").trim_start();
                if above.starts_with("#[") {
                    continue;
                }
                if !above.starts_with("///") {
                    break;
                }
                if above.contains("Caller:") || above.contains("Seam:") {
                    documented = true;
                }
            }
            assert!(
                documented,
                "src/{file}:{}: `{trimmed}` has no Caller: or Seam: line",
                i + 1
            );
        }
        assert!(
            public_items >= 5,
            "src/{file} lists {public_items} public items"
        );
        // The module cites the plan by section.
        assert!(
            text.contains("planning/architecture.md section 5.5"),
            "src/{file}"
        );
    }
    // The derivations are S15a's table's: the contexts are imported, never spelled out.
    let space = source("space.rs");
    for name in ["K_SEAL", "K_CHUNK", "CHAIN_WRAP", "SPACE_KEY_ID"] {
        assert!(space.contains(name), "src/space.rs uses {name}");
        assert!(
            CONTEXTS.iter().any(|c| c.name() == name),
            "{name} is in the table"
        );
    }
    assert!(source("group.rs").contains("GROUP_KEY_ID"));
    // The provisional readings are marked and numbered for the coordinator.
    let marked = [source("member.rs"), source("group.rs"), source("space.rs")].concat();
    for row in [
        "X-DELTA-W1S15C1-1",
        "X-DELTA-W1S15C1-3",
        "X-DELTA-W1S15C1-4",
        "X-DELTA-W1S15C1-5",
    ] {
        assert!(marked.contains(row), "{row} is marked in the code");
    }
}
