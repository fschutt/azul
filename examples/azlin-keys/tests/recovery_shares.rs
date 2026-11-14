//! C6 (Shamir over GF(256): known answers and laws), C7 (a recombination is verified), C8 (a
//! share sealed to a contact) and C10 (the safety number of two keys) of lane W1-S15d.
//!
//! Implements planning/architecture.md section 5.5 ("its 2-of-3 Shamir shares to contacts"). The
//! known answers are rows of `tests/vectors/shamir_kat.json`, `recovery_wrap.json` and
//! `contacts.json`, made by the independent program of the lane: its GF(256) works by log and exp
//! tables (the crate's by bit masks), and today's `crypto/shamir.rs` known answers (made by the OLD
//! implementation) are among them.

#![cfg(feature = "test-support")]
#![allow(unused_imports, dead_code, clippy::needless_range_loop)]

mod recovery_common;

use std::collections::BTreeSet;

use azlin_keys::envelope::{
    Alg, Ctx, Envelope, OpenError, Params, RecipientSecret, RecoveryMethod,
};
use azlin_keys::recovery::{
    combine, gf_inv, gf_mul, new_contact_secret, open_share, safety_number, safety_number_text,
    seal_share_to_contact, share_from_text, share_text, split, unwrap_with_code, wrap_with_code,
    ContactKey, KdfFloor, KdfProfile, OpenedShare, RecoveryCode, RecoveryError, RequestKey, Share,
    DEFAULT_SHARES, DEFAULT_THRESHOLD, MAX_OWNER_WORDS, MAX_SHARES,
};
use azlin_keys::{
    DeviceId, EntropyError, FailingEntropy, MapDirectory, MemberId, Secret, SeededEntropy,
    SignKeypair, VerifyAt,
};
use recovery_common::vectors::{self, row};
use recovery_common::{
    from_hex, input, kat_code, kat_contact_secret, kat_ctx, kat_device, kat_directory, kat_member,
    kat_other_member, kat_signer, pat, pat16, pat32, pat64, to_hex, word, ScriptedEntropy,
    SplitMix64,
};

fn secret16(bytes: &[u8]) -> Secret<16> {
    Secret::from_slice(bytes).expect("16 bytes")
}

fn code_of(bytes: [u8; 16]) -> RecoveryCode {
    RecoveryCode::from_secret(Secret::from_bytes(bytes))
}

/// The 23 bytes of a share as the vector rows write them: x, k, n, split id, y.
fn share_bytes(share: &Share) -> Vec<u8> {
    let mut out = vec![share.x, share.k, share.n];
    out.extend_from_slice(&share.split_id);
    out.extend_from_slice(share.y.expose());
    out
}

fn share_of_bytes(bytes: &[u8]) -> Share {
    Share {
        x: bytes[0],
        k: bytes[1],
        n: bytes[2],
        split_id: <[u8; 4]>::try_from(&bytes[3..7]).expect("4 bytes"),
        y: secret16(&bytes[7..23]),
    }
}

fn shares_of_bytes(bytes: &[u8]) -> Vec<Share> {
    bytes.chunks_exact(23).map(share_of_bytes).collect()
}

fn duplicate(shares: &[Share], picks: &[usize]) -> Vec<Share> {
    picks.iter().map(|&i| shares[i].duplicate()).collect()
}

fn combine_bytes(shares: &[Share]) -> Result<Vec<u8>, RecoveryError> {
    combine(shares).map(|secret| secret.expose().to_vec())
}

// ---------------------------------------------------------------------------------------------
// C6 [M]
// ---------------------------------------------------------------------------------------------

/// The AES field's exp and log tables for the generator 3 (polynomial `0x11B`): the REFERENCE the
/// crate's table-free multiplication is compared with.
fn tables() -> ([u8; 512], [u8; 256]) {
    let mut exp = [0u8; 512];
    let mut log = [0u8; 256];
    let mut x: u16 = 1;
    for i in 0..255usize {
        exp[i] = x as u8;
        log[usize::from(x as u8)] = i as u8;
        let mut next = (x << 1) ^ x;
        if next & 0x100 != 0 {
            next ^= 0x11B;
        }
        x = next;
    }
    for i in 255..512usize {
        exp[i] = exp[i - 255];
    }
    (exp, log)
}

/// C6 [M]: `gf_mul` equals the table product for all 65,536 pairs, `gf_inv` the table inverse, the
/// field laws hold on a sample, and FIPS-197's examples are the examples.
#[test]
fn c6_gf256_multiplication_equals_the_reference_table() {
    let (exp, log) = tables();
    let reference = |a: u8, b: u8| -> u8 {
        if a == 0 || b == 0 {
            0
        } else {
            exp[usize::from(log[usize::from(a)]) + usize::from(log[usize::from(b)])]
        }
    };
    for a in 0..=255u8 {
        for b in 0..=255u8 {
            assert_eq!(gf_mul(a, b), reference(a, b), "{a:#04x} x {b:#04x}");
        }
    }
    assert_eq!(gf_inv(0), 0);
    for a in 1..=255u8 {
        assert_eq!(
            gf_inv(a),
            exp[255 - usize::from(log[usize::from(a)])],
            "inverse of {a:#04x}"
        );
        assert_eq!(gf_mul(a, gf_inv(a)), 1, "a times its inverse, a = {a:#04x}");
        assert_eq!(gf_mul(a, 1), a);
        assert_eq!(gf_mul(1, a), a);
        assert_eq!(gf_mul(a, 0), 0);
    }
    // FIPS-197 section 4.2: {57} x {83} = {c1}; 4.2.1: {57} x {13} = {fe}, the xtime steps, and the
    // inverse the S-box starts from: {53}^-1 = {ca}.
    assert_eq!(gf_mul(0x57, 0x83), 0xC1);
    assert_eq!(gf_mul(0x57, 0x13), 0xFE);
    assert_eq!(
        [0x02, 0x04, 0x08, 0x10].map(|b| gf_mul(0x57, b)),
        [0xAE, 0x47, 0x8E, 0x07]
    );
    assert_eq!(gf_inv(0x53), 0xCA);
    // Commutative, associative and distributive over a seeded sample.
    let mut rng = SplitMix64::new(256);
    for _ in 0..20_000 {
        let (a, b, c) = (
            rng.below(256) as u8,
            rng.below(256) as u8,
            rng.below(256) as u8,
        );
        assert_eq!(gf_mul(a, b), gf_mul(b, a));
        assert_eq!(gf_mul(gf_mul(a, b), c), gf_mul(a, gf_mul(b, c)));
        assert_eq!(gf_mul(a, b ^ c), gf_mul(a, b) ^ gf_mul(a, c));
    }
}

/// C6 [M]: the field rows of the vector file (FIPS-197's products and inverses).
#[test]
fn c6_the_field_rows_of_the_vector_file() {
    let rows = vectors::read("shamir_kat.json");
    let mut seen = 0;
    for r in rows.iter().filter(|r| r.name.starts_with("gf_")) {
        let bytes = from_hex(&r.hex);
        let want = from_hex(&r.expect);
        let got = match bytes.as_slice() {
            [a, b] => gf_mul(*a, *b),
            [a] => gf_inv(*a),
            other => panic!("{}: {} bytes", r.name, other.len()),
        };
        assert_eq!(vec![got], want, "{}", r.name);
        seen += 1;
    }
    assert!(seen >= 12, "{seen} field rows");
}

/// C6 [M]: the known splits through the new API: the coefficients are ONE draw of 16 (k - 1) bytes,
/// the shares are the pinned points (today's `crypto/shamir.rs` answers among them), and any k of
/// them recombine to the secret.
#[test]
fn c6_shamir_known_answers_and_laws() {
    let rows = vectors::read("shamir_kat.json");
    let mut splits = 0;
    for r in rows.iter().filter(|r| r.name.starts_with("split_")) {
        let secret = from_hex(input(&r.inputs, "secret"));
        let k: u8 = input(&r.inputs, "k").parse().expect("k");
        let n: u8 = input(&r.inputs, "n").parse().expect("n");
        let picks: Vec<u8> = input(&r.inputs, "shares_listed")
            .split(',')
            .map(|p| p.parse().expect("a share index"))
            .collect();
        let mut ent = ScriptedEntropy::new(from_hex(input(&r.inputs, "coeffs")));
        let shares = split(&secret16(&secret), k, n, &mut ent)
            .unwrap_or_else(|e| panic!("{}: {e:?}", r.name));
        assert_eq!(
            ent.draws,
            [16 * usize::from(k - 1)],
            "{}: ONE draw of 16 (k - 1) bytes",
            r.name
        );
        assert_eq!(ent.left(), 0, "{}", r.name);
        assert_eq!(shares.len(), usize::from(n), "{}", r.name);
        assert_eq!(
            shares.iter().map(|s| s.x).collect::<Vec<u8>>(),
            (1..=n).collect::<Vec<u8>>(),
            "{}: the indices",
            r.name
        );
        let listed: Vec<u8> = shares
            .iter()
            .filter(|s| picks.contains(&s.x))
            .flat_map(share_bytes)
            .collect();
        assert_eq!(to_hex(&listed), r.hex, "{}: the pinned points", r.name);
        assert_eq!(r.expect, input(&r.inputs, "secret"), "{}", r.name);
        for share in &shares {
            assert_eq!(
                to_hex(&share.split_id),
                input(&r.inputs, "split_id"),
                "{}",
                r.name
            );
            assert_eq!((share.k, share.n), (k, n));
        }
        // The listed shares give the secret back when there are enough of them, `NotEnough`
        // otherwise (never another secret).
        let listed_shares: Vec<Share> = shares
            .iter()
            .filter(|s| picks.contains(&s.x))
            .map(Share::duplicate)
            .collect();
        if listed_shares.len() >= usize::from(k) {
            assert_eq!(
                combine_bytes(&listed_shares),
                Ok(secret.clone()),
                "{}",
                r.name
            );
        } else {
            assert_eq!(
                combine_bytes(&listed_shares),
                Err(RecoveryError::NotEnough),
                "{}",
                r.name
            );
        }
        splits += 1;
    }
    assert!(splits >= 6, "{splits} split rows");
}

/// C6 [M]: the combine rows: any two of the three, in any order, all three, and every refusal with
/// its word (`NotEnough`, `DuplicateShare`, `ShareIndex`, `MixedSplits`, `BadShare`,
/// `CheckMismatch`).
#[test]
fn c6_combine_rows() {
    let rows = vectors::read("shamir_kat.json");
    let mut seen = 0;
    for r in rows.iter().filter(|r| r.name.starts_with("combine_")) {
        let shares = shares_of_bytes(&from_hex(&r.hex));
        let got = combine_bytes(&shares);
        if r.valid {
            assert_eq!(got.map(|b| to_hex(&b)), Ok(r.expect.clone()), "{}", r.name);
        } else {
            assert_eq!(
                got.map_err(|e| word(&e)),
                Err(r.expect.clone()),
                "{}",
                r.name
            );
        }
        seen += 1;
    }
    assert!(seen >= 13, "{seen} combine rows");
}

/// C6 [M]: any k of n shares recombine (exhaustive for 2-of-3 and 3-of-5, sampled for 2-of-255 and
/// for 150 random splits), fewer than k are `NotEnough` and never a wrong secret.
#[test]
fn c6_any_k_of_n_shares_recombine_and_fewer_never_do() {
    // Exhaustive: every subset of a 2-of-3 and a 3-of-5 split.
    for (k, n, seed) in [(2u8, 3u8, 1u64), (3, 5, 2), (4, 4, 3), (2, 2, 4), (5, 8, 5)] {
        let secret = secret16(&pat(0x30, 16));
        let shares = split(&secret, k, n, &mut SeededEntropy::from_u64(seed)).expect("a split");
        for mask in 0u32..(1 << n) {
            let picks: Vec<usize> = (0..usize::from(n))
                .filter(|i| (mask >> i) & 1 == 1)
                .collect();
            let subset = duplicate(&shares, &picks);
            let got = combine_bytes(&subset);
            if picks.len() >= usize::from(k) {
                assert_eq!(got, Ok(pat(0x30, 16)), "{k} of {n}, shares {picks:?}");
            } else {
                assert_eq!(
                    got,
                    Err(RecoveryError::NotEnough),
                    "{k} of {n}, shares {picks:?}"
                );
            }
            // The order does not matter.
            let mut reversed = subset;
            reversed.reverse();
            if picks.len() >= usize::from(k) {
                assert_eq!(
                    combine_bytes(&reversed),
                    Ok(pat(0x30, 16)),
                    "reversed {picks:?}"
                );
            }
        }
    }

    // 2-of-255: every pair with the first share, and 300 random pairs, triples and larger sets.
    let secret = secret16(&pat(0x77, 16));
    let shares = split(&secret, 2, 255, &mut SeededEntropy::from_u64(9)).expect("a split");
    assert_eq!(shares.len(), 255);
    assert_eq!(MAX_SHARES, 255);
    for i in 1..255 {
        assert_eq!(
            combine_bytes(&duplicate(&shares, &[0, i])),
            Ok(pat(0x77, 16)),
            "shares 1 and {}",
            i + 1
        );
        assert_eq!(
            combine_bytes(&duplicate(&shares, &[i])),
            Err(RecoveryError::NotEnough)
        );
    }
    let mut rng = SplitMix64::new(255);
    for _ in 0..300 {
        let size = 2 + rng.below(6) as usize;
        let mut picks = BTreeSet::new();
        while picks.len() < size {
            picks.insert(rng.below(255) as usize);
        }
        let picks: Vec<usize> = picks.into_iter().collect();
        assert_eq!(
            combine_bytes(&duplicate(&shares, &picks)),
            Ok(pat(0x77, 16)),
            "{picks:?}"
        );
    }
    let all: Vec<usize> = (0..255).collect();
    assert_eq!(
        combine_bytes(&duplicate(&shares, &all)),
        Ok(pat(0x77, 16)),
        "all 255"
    );

    // 150 random splits: any k of n recombine, k - 1 never do.
    let mut rng = SplitMix64::new(150);
    for case in 0..150u64 {
        let k = 2 + rng.below(5) as u8;
        let n = k + rng.below(6) as u8;
        let secret_bytes = rng.bytes(16);
        let shares = split(
            &secret16(&secret_bytes),
            k,
            n,
            &mut SeededEntropy::from_u64(1000 + case),
        )
        .expect("a split");
        let mut order: Vec<usize> = (0..usize::from(n)).collect();
        for i in (1..order.len()).rev() {
            order.swap(i, rng.below(i as u64 + 1) as usize);
        }
        let enough = usize::from(k) + rng.below(u64::from(n - k) + 1) as usize;
        assert_eq!(
            combine_bytes(&duplicate(&shares, &order[..enough])),
            Ok(secret_bytes.clone()),
            "case {case}: {k} of {n}, {enough} shares"
        );
        assert_eq!(
            combine_bytes(&duplicate(&shares, &order[..usize::from(k) - 1])),
            Err(RecoveryError::NotEnough),
            "case {case}: {k} of {n}"
        );
    }
}

/// C6: one share of a 2-of-n split is uniform whatever the secret: its byte runs through all 256
/// values as the coefficient does.
#[test]
fn c6_one_share_of_a_two_of_n_split_is_uniform_whatever_the_secret() {
    for secret_byte in [0x00u8, 0x01, 0x5A, 0xFF] {
        for x in [1u8, 2, 3, 200] {
            let mut seen = [false; 256];
            for coefficient in 0..=255u8 {
                let mut ent = ScriptedEntropy::new(vec![coefficient; 16]);
                let shares =
                    split(&secret16(&[secret_byte; 16]), 2, x.max(3), &mut ent).expect("a split");
                let share = shares.iter().find(|s| s.x == x).expect("the share");
                assert!(share.y.expose().iter().all(|b| *b == share.y.expose()[0]));
                seen[usize::from(share.y.expose()[0])] = true;
            }
            assert!(
                seen.iter().all(|s| *s),
                "secret {secret_byte:#04x}, x = {x}"
            );
        }
    }
}

/// C6: splits that cannot work are refused (`BadSplit`), a split is deterministic in its entropy,
/// draws what it says, and a failing source makes no share.
#[test]
fn c6_splits_that_cannot_work_are_refused_and_the_draw_is_what_it_says() {
    let secret = secret16(&pat(0x10, 16));
    let mut ent = ScriptedEntropy::new(vec![1u8; 4096]);
    for (k, n) in [
        (1u8, 3u8),
        (0, 3),
        (4, 3),
        (2, 1),
        (0, 0),
        (2, 0),
        (255, 254),
    ] {
        assert_eq!(
            split(&secret, k, n, &mut ent).err(),
            Some(RecoveryError::BadSplit),
            "{k} of {n}"
        );
    }
    assert!(
        ent.draws.is_empty(),
        "nothing is drawn for a split that cannot be made"
    );
    for (k, n) in [(2u8, 2u8), (2, 255), (255, 255), (3, 7)] {
        let mut ent = ScriptedEntropy::new(vec![7u8; 16 * usize::from(k - 1)]);
        let shares = split(&secret, k, n, &mut ent).expect("a split");
        assert_eq!(shares.len(), usize::from(n));
        assert_eq!(ent.draws, [16 * usize::from(k - 1)]);
        assert_eq!(ent.left(), 0);
    }
    // The default split of section 5.5.
    assert_eq!((DEFAULT_THRESHOLD, DEFAULT_SHARES), (2, 3));
    // Deterministic in the entropy; another stream gives other shares; every share carries the
    // check value of the code.
    let a = split(&secret, 2, 3, &mut SeededEntropy::from_u64(1)).expect("a split");
    let b = split(&secret, 2, 3, &mut SeededEntropy::from_u64(1)).expect("a split");
    let c = split(&secret, 2, 3, &mut SeededEntropy::from_u64(2)).expect("a split");
    assert_eq!(
        a.iter().map(share_bytes).collect::<Vec<_>>(),
        b.iter().map(share_bytes).collect::<Vec<_>>()
    );
    assert_ne!(share_bytes(&a[0]), share_bytes(&c[0]));
    let code = code_of(pat16(0x10));
    assert!(a.iter().all(|s| s.split_id == code.check_value()));
    // A failing source: one byte short, no share.
    assert_eq!(
        split(&secret, 2, 3, &mut FailingEntropy::after(15)).err(),
        Some(RecoveryError::Entropy(EntropyError::Exhausted))
    );
    assert_eq!(
        split(&secret, 3, 5, &mut FailingEntropy::after(31)).err(),
        Some(RecoveryError::Entropy(EntropyError::Exhausted))
    );
    assert!(split(&secret, 3, 5, &mut FailingEntropy::after(32)).is_ok());
    // A duplicate is the same share: equal in every field, and only a copy.
    let copy = a[1].duplicate();
    assert_eq!(share_bytes(&copy), share_bytes(&a[1]));
}

// ---------------------------------------------------------------------------------------------
// C7 [M]
// ---------------------------------------------------------------------------------------------

/// C7 [M]: any single bit changed in the bytes of any share of a split makes `combine` refuse
/// (`CheckMismatch`): the recombination is checked against the code's check value, not returned as
/// another code.
#[test]
fn c7_recombination_is_verified() {
    let code = kat_code();
    let shares = split(code.as_secret(), 2, 3, &mut SeededEntropy::from_u64(7)).expect("a split");
    for victim in 0..3usize {
        for byte in 0..16usize {
            for bit in 0..8u8 {
                let mut tampered = duplicate(&shares, &[0, 1, 2]);
                tampered[victim].y.expose_mut()[byte] ^= 1 << bit;
                // Every pair of the three that holds the tampered share: refused.
                for pair in [[0usize, 1], [0, 2], [1, 2]] {
                    let picks: Vec<Share> = pair.iter().map(|&i| tampered[i].duplicate()).collect();
                    let expected = if pair.contains(&victim) {
                        Err(RecoveryError::CheckMismatch)
                    } else {
                        Ok(pat(0x10, 16))
                    };
                    assert_eq!(
                        combine_bytes(&picks),
                        expected,
                        "share {victim}, byte {byte}, bit {bit}, pair {pair:?}"
                    );
                }
            }
        }
    }
    // All three, one tampered: all of them are interpolated, the check fails.
    let mut tampered = duplicate(&shares, &[0, 1, 2]);
    tampered[2].y.expose_mut()[0] ^= 0x80;
    assert_eq!(combine_bytes(&tampered), Err(RecoveryError::CheckMismatch));
}

/// C7 [M]: the other things that can be wrong with the shares of one split: another index (a
/// duplicate, an index above the count), another threshold or count, another code.
#[test]
fn c7_other_wrong_shares_are_refused_with_their_own_word() {
    let code = kat_code();
    let shares = split(code.as_secret(), 2, 3, &mut SeededEntropy::from_u64(7)).expect("a split");
    // Share 3 claims to be share 2: the same index twice.
    let mut dup = duplicate(&shares, &[1, 2]);
    dup[1].x = 2;
    assert_eq!(combine_bytes(&dup), Err(RecoveryError::DuplicateShare));
    // Share 3 claims to be share 4 of 3, or share 0.
    let mut above = duplicate(&shares, &[0, 2]);
    above[1].x = 4;
    assert_eq!(combine_bytes(&above), Err(RecoveryError::BadShare));
    let mut zero = duplicate(&shares, &[0, 2]);
    zero[1].x = 0;
    assert_eq!(combine_bytes(&zero), Err(RecoveryError::ShareIndex));
    // A share of another threshold, count or split id.
    for edit in 0..3 {
        let mut mixed = duplicate(&shares, &[0, 1]);
        match edit {
            0 => mixed[1].k = 3,
            1 => mixed[1].n = 4,
            _ => mixed[1].split_id[0] ^= 1,
        }
        assert_eq!(
            combine_bytes(&mixed),
            Err(RecoveryError::MixedSplits),
            "edit {edit}"
        );
    }
    // A threshold of 1 or above the count, in every share (so the split is "one" split).
    let mut one = duplicate(&shares, &[0, 1]);
    for share in &mut one {
        share.k = 1;
    }
    assert_eq!(combine_bytes(&one), Err(RecoveryError::BadShare));
    let mut over = duplicate(&shares, &[0, 1]);
    for share in &mut over {
        share.k = 4;
    }
    assert_eq!(combine_bytes(&over), Err(RecoveryError::BadShare));
    // Shares of two codes.
    let other = split(
        &secret16(&pat(0x77, 16)),
        2,
        3,
        &mut SeededEntropy::from_u64(8),
    )
    .expect("a split");
    let across = vec![shares[0].duplicate(), other[1].duplicate()];
    assert_eq!(combine_bytes(&across), Err(RecoveryError::MixedSplits));
    // A re-split of the SAME code has the same split id: its shares and the older split's cannot be
    // told apart by the id, but they do not recombine, and the check says so (an older split's
    // shares give the code together, not mixed with a newer split's).
    let newer = split(code.as_secret(), 2, 3, &mut SeededEntropy::from_u64(9)).expect("a split");
    assert_eq!(shares[0].split_id, newer[0].split_id);
    for pair in [&shares, &newer] {
        assert_eq!(combine_bytes(&duplicate(pair, &[0, 2])), Ok(pat(0x10, 16)));
    }
    let mixed = vec![shares[0].duplicate(), newer[1].duplicate()];
    assert_eq!(combine_bytes(&mixed), Err(RecoveryError::CheckMismatch));
    // Nothing at all.
    assert_eq!(combine_bytes(&[]), Err(RecoveryError::NotEnough));
}

/// C7 [M]: the chain shares to code to member key: the member key is wrapped under the code, two
/// shares give the code, the code opens the wrap; a tampered share gives no code, so nothing is
/// derived; no share wraps the member key itself.
#[test]
fn c7_the_chain_shares_to_code_to_member_key() {
    let mut ent = SeededEntropy::from_u64(77);
    let code = RecoveryCode::generate(&mut ent).expect("a code");
    let member_key = Secret::<32>::from_bytes(pat32(0x60));
    let ctx = kat_ctx(RecoveryMethod::Code);
    let wrap = wrap_with_code(
        &member_key,
        &code,
        &ctx,
        KdfProfile::Test,
        &kat_device(),
        &kat_signer(),
        &mut ent,
    )
    .expect("a wrap");
    let shares = split(
        code.as_secret(),
        DEFAULT_THRESHOLD,
        DEFAULT_SHARES,
        &mut ent,
    )
    .expect("a split");
    let dir = kat_directory();
    // Two shares give the code; the code opens the wrap.
    for picks in [[0usize, 1], [0, 2], [2, 1]] {
        let back =
            RecoveryCode::from_secret(combine(&duplicate(&shares, &picks)).expect("two shares"));
        assert!(back.ct_eq(&code));
        assert_eq!(back.check_value(), code.check_value());
    }
    let back =
        RecoveryCode::from_secret(combine(&duplicate(&shares, &[0, 2])).expect("two shares"));
    let opened =
        unwrap_with_code(&wrap, &back, &ctx, &dir, VerifyAt::Now, KdfFloor::TEST).expect("opens");
    assert_eq!(opened.as_slice(), member_key.expose());
    // One share is not enough, and a tampered pair gives no code at all.
    assert_eq!(
        combine_bytes(&duplicate(&shares, &[1])),
        Err(RecoveryError::NotEnough)
    );
    let mut tampered = duplicate(&shares, &[0, 2]);
    tampered[1].y.expose_mut()[9] ^= 0x01;
    assert_eq!(combine_bytes(&tampered), Err(RecoveryError::CheckMismatch));
    // A share is a share of the CODE: its bytes are not the member key's, and opening the wrap with
    // a share's bytes as a code fails.
    let as_code = RecoveryCode::from_secret(shares[0].y.duplicate());
    assert_eq!(
        unwrap_with_code(&wrap, &as_code, &ctx, &dir, VerifyAt::Now, KdfFloor::TEST)
            .map(|p| p.to_vec()),
        Err(RecoveryError::Auth)
    );
}

// ---------------------------------------------------------------------------------------------
// C8
// ---------------------------------------------------------------------------------------------

const WORDS: &str = "Felix, drive Photos";

/// The shares of the vectors' code: split 2-of-3 with the coefficient bytes `pat(0x80, 16)`.
fn kat_shares() -> Vec<Share> {
    split(
        kat_code().as_secret(),
        2,
        3,
        &mut ScriptedEntropy::new(pat(0x80, 16)),
    )
    .expect("a split")
}

struct Alg5Case {
    name: &'static str,
    share: usize,
    eph: u8,
    nonce: u8,
    hybrid_message: Option<u8>,
    words: &'static str,
}

fn alg5_cases() -> Vec<Alg5Case> {
    vec![
        Alg5Case {
            name: "alg5_over_alg1_share_2_of_3",
            share: 1,
            eph: 0x11,
            nonce: 0x41,
            hybrid_message: None,
            words: WORDS,
        },
        Alg5Case {
            name: "alg5_over_alg1_empty_words",
            share: 1,
            eph: 0x12,
            nonce: 0x42,
            hybrid_message: None,
            words: "",
        },
        Alg5Case {
            name: "alg5_over_alg2_share_3_of_3",
            share: 2,
            eph: 0x13,
            nonce: 0x43,
            hybrid_message: Some(0x33),
            words: WORDS,
        },
    ]
}

fn seal_case(case: &Alg5Case, shares: &[Share]) -> (Envelope, ScriptedEntropy) {
    let contact = ContactKey::of_secret(&kat_contact_secret(case.hybrid_message.is_some()));
    let mut script = pat(case.eph, 32);
    if let Some(m) = case.hybrid_message {
        script.extend(pat(m, 32));
    }
    script.extend(pat(case.nonce, 24));
    let mut ent = ScriptedEntropy::new(script);
    let env = seal_share_to_contact(
        &shares[case.share],
        &contact,
        case.words,
        &kat_ctx(RecoveryMethod::Share),
        &kat_device(),
        &kat_signer(),
        &mut ent,
    )
    .unwrap_or_else(|e| panic!("{}: {e:?}", case.name));
    (env, ent)
}

/// C8: the sealed shares are the pinned vectors (the draws are the ephemeral key, for the hybrid the
/// ML-KEM message, and the nonce), the vectors' bytes open with the contact's secret to the share
/// and the owner's words, over algorithm 1 and (with the feature `pq`) over algorithm 2.
#[test]
fn c8_sealed_share_to_contact() {
    let rows = vectors::read("recovery_wrap.json");
    let shares = kat_shares();
    let dir = kat_directory();
    let ctx = kat_ctx(RecoveryMethod::Share);
    let mut checked = 0;
    for case in alg5_cases() {
        if case.hybrid_message.is_some() && !cfg!(feature = "pq") {
            continue;
        }
        let r = row(&rows, case.name);
        let (env, ent) = seal_case(&case, &shares);
        assert_eq!(
            ent.draws,
            if case.hybrid_message.is_some() {
                vec![32, 32, 24]
            } else {
                vec![32, 24]
            },
            "{}: the draws",
            case.name
        );
        assert_eq!(ent.left(), 0, "{}", case.name);
        assert_eq!(
            to_hex(&env.to_cbor().expect("bytes")),
            r.hex,
            "{}: the bytes",
            case.name
        );
        let from_file = Envelope::from_cbor(&from_hex(&r.hex)).expect("an envelope");
        assert_eq!(from_file, env);
        assert_eq!(env.alg, Alg::Shamir);
        assert_eq!(env.params, shares[case.share].params_for_test());
        assert!(env.epk.is_some());
        assert_eq!(env.kem_ct.is_some(), case.hybrid_message.is_some());

        let me = kat_contact_secret(case.hybrid_message.is_some());
        let opened = open_share(&from_file, &me, &ctx, &dir, VerifyAt::Now)
            .unwrap_or_else(|e| panic!("{}: {e:?}", case.name));
        assert_eq!(opened.owner_words, case.words, "{}", case.name);
        assert_eq!(
            share_bytes(&opened.share),
            share_bytes(&shares[case.share]),
            "{}",
            case.name
        );
        let mut plaintext = opened.share.y.expose().to_vec();
        plaintext.extend_from_slice(opened.owner_words.as_bytes());
        assert_eq!(to_hex(&plaintext), r.expect, "{}", case.name);
        checked += 1;
    }
    assert!(checked >= 2);
}

trait ParamsForTest {
    fn params_for_test(&self) -> Params;
}

impl ParamsForTest for Share {
    fn params_for_test(&self) -> Params {
        Params::Shamir {
            x: self.x,
            k: self.k,
            n: self.n,
            split_id: self.split_id,
        }
    }
}

/// A fixture of the round trips: a signer the directory knows, an owner, a code split 2-of-3.
struct Owner {
    member: MemberId,
    device: DeviceId,
    key: SignKeypair,
    dir: MapDirectory,
    shares: Vec<Share>,
}

fn owner(seed: u64) -> Owner {
    let mut ent = SeededEntropy::from_u64(seed);
    let member = MemberId::generate(&mut ent).expect("an id");
    let device = DeviceId::generate(&mut ent).expect("an id");
    let key = SignKeypair::generate(&mut ent).expect("a key");
    let mut dir = MapDirectory::new();
    dir.insert_active(device, key.verify_key());
    let code = RecoveryCode::generate(&mut ent).expect("a code");
    let shares = split(code.as_secret(), 2, 3, &mut ent).expect("a split");
    Owner {
        member,
        device,
        key,
        dir,
        shares,
    }
}

fn seal(
    owner: &Owner,
    i: usize,
    contact: &ContactKey,
    words: &str,
    seed: u64,
) -> Result<Envelope, RecoveryError> {
    seal_share_to_contact(
        &owner.shares[i],
        contact,
        words,
        &Ctx::recovery(&owner.member, RecoveryMethod::Share),
        &owner.device,
        &owner.key,
        &mut SeededEntropy::from_u64(seed),
    )
}

fn open(owner: &Owner, env: &Envelope, me: &RecipientSecret) -> Result<OpenedShare, RecoveryError> {
    open_share(
        env,
        me,
        &Ctx::recovery(&owner.member, RecoveryMethod::Share),
        &owner.dir,
        VerifyAt::Now,
    )
}

/// The two ways a contact key is made: X25519 only, and with an ML-KEM half (the build's `pq`).
fn contact_secrets(seed: u64) -> Vec<RecipientSecret> {
    let mut ent = SeededEntropy::from_u64(seed);
    let mut out = vec![new_contact_secret(&mut ent, false).expect("a contact key")];
    if cfg!(feature = "pq") {
        out.push(new_contact_secret(&mut ent, true).expect("a contact key"));
    }
    out
}

/// C8: seal and open round trip over algorithm 1 and over algorithm 2: the share, its index, threshold,
/// count and split id, and the owner's words (UTF-8 included) come back; the envelope names the
/// contact key and the owner's context; and the same share sealed twice differs.
#[test]
fn c8_round_trip_over_alg1_and_alg2() {
    let o = owner(1);
    for (n, me) in contact_secrets(2).iter().enumerate() {
        let contact = ContactKey::of_secret(me);
        let long = "é".repeat(128);
        for (i, words) in [
            "Felix, drive Photos",
            "",
            "Zoë, Fotos \u{2713} für Mama",
            long.as_str(),
        ]
        .iter()
        .enumerate()
        {
            let env = seal(&o, i % 3, &contact, words, 10 + i as u64).expect("a sealed share");
            assert_eq!(env.alg, Alg::Shamir);
            assert_eq!(env.rcpt, contact.key_id());
            assert_eq!(env.ctx, Ctx::recovery(&o.member, RecoveryMethod::Share));
            assert_eq!(env.signer, o.device);
            assert!(env.epk.is_some());
            assert_eq!(env.kem_ct.is_some(), contact.pq().is_some(), "contact {n}");
            if let Some(kem_ct) = &env.kem_ct {
                assert_eq!(kem_ct.len(), 1088);
            }
            // Through the bytes and through the text.
            let bytes = env.to_cbor().expect("bytes");
            let again = Envelope::from_cbor(&bytes).expect("an envelope");
            assert_eq!(
                share_from_text(&share_text(&again).expect("a text")),
                Ok(env)
            );
            let opened = open(&o, &again, me).expect("opens");
            assert_eq!(opened.owner_words, *words);
            assert_eq!(share_bytes(&opened.share), share_bytes(&o.shares[i % 3]));
        }
        // Two seals of one share differ (a fresh ephemeral key and nonce), and so do two contacts'.
        let a = seal(&o, 0, &contact, "x", 1).expect("sealed");
        let b = seal(&o, 0, &contact, "x", 2).expect("sealed");
        assert_ne!(a.to_cbor().expect("bytes"), b.to_cbor().expect("bytes"));
        assert_ne!(a.epk, b.epk);
        assert_ne!(a.nonce, b.nonce);
    }
}

/// C8 [M]: a share sealed to contact A does not open for contact B (`NotForMe`), not even when B
/// knows the context; a contact key of the other kind (X25519 against hybrid) does not open it
/// either.
#[test]
fn c8_a_share_sealed_to_a_contact_opens_only_for_that_contact() {
    let o = owner(3);
    let secrets = contact_secrets(4);
    let strangers = contact_secrets(5);
    for (me, stranger) in secrets.iter().zip(&strangers) {
        let env = seal(&o, 1, &ContactKey::of_secret(me), "words", 1).expect("sealed");
        assert!(open(&o, &env, me).is_ok());
        assert_eq!(
            open(&o, &env, stranger).err(),
            Some(RecoveryError::Open(OpenError::NotForMe))
        );
        // A contact with the same X25519 key but another ML-KEM seed (or none): not for them.
        for other in &secrets {
            if !std::ptr::eq(other, me) {
                assert_eq!(
                    open(&o, &env, other).err(),
                    Some(RecoveryError::Open(OpenError::NotForMe))
                );
            }
        }
    }
}

/// C8: the owner's words are authenticated (and only the contact reads them): a changed
/// ciphertext, header or parameter does not open (the signature refuses; signed again, the cipher
/// refuses: the `neg_alg5_*` rows), and the words have limits.
#[test]
fn c8_the_owners_words_are_authenticated_and_limited() {
    let o = owner(6);
    let secrets = contact_secrets(7);
    let me = &secrets[0];
    let contact = ContactKey::of_secret(me);
    let env = seal(&o, 0, &contact, "Felix", 1).expect("sealed");
    // Not signed again: the signature refuses every change of the bytes it covers.
    let mut changed = env.clone();
    changed.ct[0] ^= 1;
    assert_eq!(
        open(&o, &changed, me).err(),
        Some(RecoveryError::Open(OpenError::BadSignature))
    );
    let mut changed = env.clone();
    changed.params = Params::Shamir {
        x: 3,
        k: 2,
        n: 3,
        split_id: o.shares[0].split_id,
    };
    assert_eq!(
        open(&o, &changed, me).err(),
        Some(RecoveryError::Open(OpenError::BadSignature))
    );
    // Signed again by the legitimate device: the cipher refuses (the header and the parameters are
    // its associated data).
    let mut resigned = env.clone();
    resigned.ct[16] ^= 1;
    resigned.sign(&o.key).expect("signs");
    assert_eq!(open(&o, &resigned, me).err(), Some(RecoveryError::Auth));
    let mut resigned = env.clone();
    resigned.params = Params::Shamir {
        x: 3,
        k: 2,
        n: 3,
        split_id: o.shares[0].split_id,
    };
    resigned.sign(&o.key).expect("signs");
    assert_eq!(open(&o, &resigned, me).err(), Some(RecoveryError::Auth));

    // The words: 256 bytes are the most, a control character is refused.
    let at_limit = "a".repeat(MAX_OWNER_WORDS);
    assert_eq!(MAX_OWNER_WORDS, 256);
    let sealed = seal(&o, 0, &contact, &at_limit, 2).expect("the longest words");
    assert_eq!(open(&o, &sealed, me).expect("opens").owner_words, at_limit);
    for bad in [
        "a".repeat(MAX_OWNER_WORDS + 1),
        "é".repeat(129),
        "line\nbreak".to_string(),
        "tab\there".to_string(),
        "bell\u{7}".to_string(),
        "del\u{7f}".to_string(),
        "nel\u{85}".to_string(),
    ] {
        assert_eq!(
            seal(&o, 0, &contact, &bad, 3).err(),
            Some(RecoveryError::BadWords),
            "{bad:?}"
        );
    }
}

/// C8: `ctx` binds the owner and the method: another member's share context, or the code's, is
/// refused on opening, and a sealing for a context that is not a share's makes nothing.
#[test]
fn c8_ctx_binds_the_owner_and_the_method() {
    let o = owner(8);
    let secrets = contact_secrets(9);
    let me = &secrets[0];
    let contact = ContactKey::of_secret(me);
    let env = seal(&o, 0, &contact, "w", 1).expect("sealed");
    let open_as = |expect: &Ctx| open_share(&env, me, expect, &o.dir, VerifyAt::Now).err();
    assert_eq!(
        open_as(&Ctx::recovery(&kat_other_member(), RecoveryMethod::Share)),
        Some(RecoveryError::Open(OpenError::CtxMismatch))
    );
    assert_eq!(
        open_as(&Ctx::recovery(&o.member, RecoveryMethod::Code)),
        Some(RecoveryError::WrongCtx)
    );
    assert_eq!(
        open_as(&Ctx::recovery(&o.member, RecoveryMethod::Prf)),
        Some(RecoveryError::WrongCtx)
    );
    assert_eq!(
        open_as(&Ctx::member_key(&o.member)),
        Some(RecoveryError::WrongCtx)
    );
    for ctx in [
        Ctx::recovery(&o.member, RecoveryMethod::Code),
        Ctx::recovery(&o.member, RecoveryMethod::Prf),
        Ctx::member_key(&o.member),
    ] {
        let mut ent = ScriptedEntropy::new(vec![0u8; 100]);
        assert_eq!(
            seal_share_to_contact(
                &o.shares[0],
                &contact,
                "w",
                &ctx,
                &o.device,
                &o.key,
                &mut ent
            )
            .err(),
            Some(RecoveryError::WrongCtx)
        );
        assert!(ent.draws.is_empty());
    }
    // The split id and index are bound through the parameters (the associated data).
    assert_eq!(env.params, o.shares[0].params_for_test());
}

/// C8: a share that cannot come from a split is not sealed, the signer must be known, and a failing
/// source or a contact key of low order makes no envelope.
#[test]
fn c8_what_cannot_be_sealed_or_opened() {
    let o = owner(10);
    let secrets = contact_secrets(11);
    let me = &secrets[0];
    let contact = ContactKey::of_secret(me);
    let ctx = Ctx::recovery(&o.member, RecoveryMethod::Share);
    let seal_bad = |edit: &dyn Fn(&mut Share)| {
        let mut share = o.shares[0].duplicate();
        edit(&mut share);
        seal_share_to_contact(
            &share,
            &contact,
            "w",
            &ctx,
            &o.device,
            &o.key,
            &mut SeededEntropy::from_u64(1),
        )
        .err()
    };
    assert_eq!(seal_bad(&|s| s.x = 0), Some(RecoveryError::ShareIndex));
    assert_eq!(seal_bad(&|s| s.k = 1), Some(RecoveryError::BadShare));
    assert_eq!(seal_bad(&|s| s.x = 9), Some(RecoveryError::BadShare));
    assert_eq!(seal_bad(&|s| s.k = 4), Some(RecoveryError::BadShare));

    let env = seal(&o, 0, &contact, "w", 1).expect("sealed");
    // An unknown signer, and a directory that is down.
    assert_eq!(
        open_share(&env, me, &ctx, &MapDirectory::new(), VerifyAt::Now).err(),
        Some(RecoveryError::Open(OpenError::UnknownSigner))
    );
    let mut down = MapDirectory::new();
    down.set_unavailable(true);
    assert_eq!(
        open_share(&env, me, &ctx, &down, VerifyAt::Now).err(),
        Some(RecoveryError::Open(OpenError::DirectoryUnavailable))
    );
    // A failing source: after the ephemeral secret, no nonce; no ephemeral secret at all.
    let seal_with = |ent: &mut dyn azlin_keys::Entropy| {
        seal_share_to_contact(&o.shares[0], &contact, "w", &ctx, &o.device, &o.key, ent).err()
    };
    let fails = Some(RecoveryError::Entropy(EntropyError::Exhausted));
    if contact.pq().is_none() {
        assert_eq!(seal_with(&mut FailingEntropy::after(0)), fails);
        assert_eq!(seal_with(&mut FailingEntropy::after(31)), fails);
        assert_eq!(seal_with(&mut FailingEntropy::after(32)), fails);
        assert_eq!(seal_with(&mut FailingEntropy::after(55)), fails);
        assert_eq!(seal_with(&mut FailingEntropy::after(56)), None);
    }
    // A contact key of low order: the key exchange gives the zero secret and no envelope exists.
    let low_order = ContactKey::new([0u8; 32], None).expect("a key");
    assert_eq!(
        seal_share_to_contact(
            &o.shares[0],
            &low_order,
            "w",
            &ctx,
            &o.device,
            &o.key,
            &mut SeededEntropy::from_u64(1)
        )
        .err(),
        Some(RecoveryError::Wrap(
            azlin_keys::envelope::WrapError::NonContributory
        ))
    );
}

/// C8: without the feature `pq` a contact key with an ML-KEM half is `Unsupported` before any
/// entropy is drawn: nothing falls back to algorithm 1.
#[cfg(not(feature = "pq"))]
#[test]
fn c8_a_hybrid_contact_key_is_unsupported_without_pq() {
    let o = owner(12);
    let contact = ContactKey::new([5u8; 32], Some(vec![7u8; 1184])).expect("a key");
    let mut ent = ScriptedEntropy::new(vec![0u8; 200]);
    assert_eq!(
        seal_share_to_contact(
            &o.shares[0],
            &contact,
            "w",
            &Ctx::recovery(&o.member, RecoveryMethod::Share),
            &o.device,
            &o.key,
            &mut ent
        )
        .err(),
        Some(RecoveryError::Wrap(
            azlin_keys::envelope::WrapError::Unsupported(Alg::X25519MlKem768)
        ))
    );
    assert!(ent.draws.is_empty());
}

/// C8: with the feature `pq`, a contact key whose ML-KEM half is not a valid key makes no envelope.
#[cfg(feature = "pq")]
#[test]
fn c8_a_hybrid_contact_key_that_is_no_key_makes_no_envelope() {
    let o = owner(12);
    let contact =
        ContactKey::new([5u8; 32], Some(vec![0xFFu8; 1184])).expect("a key of the right length");
    let mut ent = ScriptedEntropy::new(vec![0u8; 200]);
    assert_eq!(
        seal_share_to_contact(
            &o.shares[0],
            &contact,
            "w",
            &Ctx::recovery(&o.member, RecoveryMethod::Share),
            &o.device,
            &o.key,
            &mut ent
        )
        .err(),
        Some(RecoveryError::Wrap(
            azlin_keys::envelope::WrapError::BadRecipientKey
        ))
    );
    assert!(
        ent.draws.is_empty(),
        "nothing is drawn for a key that is none"
    );
}

/// C8: the reply: a contact opens the share the owner sent, seals it again to the one-time request
/// key of the recovering computer, and that computer opens the reply with the request's secret; the
/// two sides compute the same safety number of the two public keys.
#[test]
fn c8_the_contact_answers_a_recovery_request_that_only_the_request_opens() {
    let o = owner(13);
    let secrets = contact_secrets(14);
    let contact_secret = &secrets[0];
    let sealed = seal(
        &o,
        2,
        &ContactKey::of_secret(contact_secret),
        "Felix, drive Photos",
        1,
    )
    .expect("sealed");

    // The recovering computer makes a one-time request key and shows its text and number.
    let request_secret =
        new_contact_secret(&mut SeededEntropy::from_u64(15), false).expect("a request key");
    let request = RequestKey::new(*request_secret.kx_public());
    let asked = RequestKey::from_text(&request.to_text()).expect("a request text");
    assert_eq!(asked, request);

    // The contact opens its share, then answers: the share sealed to the request key.
    let opened = open(&o, &sealed, contact_secret).expect("the contact's share");
    let reply = seal_share_to_contact(
        &opened.share,
        &asked.as_contact(),
        "",
        &Ctx::recovery(&o.member, RecoveryMethod::Share),
        &o.device,
        &o.key,
        &mut SeededEntropy::from_u64(16),
    )
    .expect("a reply");
    // Only the request's secret opens it.
    let got = open(&o, &reply, &request_secret).expect("the request opens the reply");
    assert_eq!(share_bytes(&got.share), share_bytes(&o.shares[2]));
    assert_eq!(got.owner_words, "");
    assert_eq!(
        open(&o, &reply, contact_secret).err(),
        Some(RecoveryError::Open(OpenError::NotForMe))
    );
    assert_eq!(
        open(&o, &sealed, &request_secret).err(),
        Some(RecoveryError::Open(OpenError::NotForMe))
    );

    // Two replies' worth of shares recombine to the code.
    let second = seal(&o, 0, &ContactKey::of_secret(contact_secret), "x", 2).expect("sealed");
    let second = open(&o, &second, contact_secret).expect("opens");
    let code = combine(&[got.share, second.share]).expect("two shares");
    assert_eq!(
        RecoveryCode::from_secret(code).check_value(),
        o.shares[0].split_id
    );

    // Both sides compute one safety number from the same two public keys.
    let owner_pub = pat32(0x55);
    assert_eq!(
        safety_number(request.kx(), &owner_pub),
        safety_number(&owner_pub, request.kx())
    );
}

// ---------------------------------------------------------------------------------------------
// C10 [M]
// ---------------------------------------------------------------------------------------------

fn random_key(rng: &mut SplitMix64) -> [u8; 32] {
    <[u8; 32]>::try_from(rng.bytes(32)).expect("32 bytes")
}

/// C10 [M]: the numbers of the vector file, in either argument order.
#[test]
fn c10_safety_number_matches_the_vectors() {
    let rows = vectors::read("contacts.json");
    let mut seen = 0;
    for r in rows.iter().filter(|r| r.name.starts_with("safety_number_")) {
        let bytes = from_hex(&r.hex);
        let (a, b) = bytes.split_at(32);
        let (a, b) = (
            <[u8; 32]>::try_from(a).expect("32"),
            <[u8; 32]>::try_from(b).expect("32"),
        );
        assert_eq!(
            safety_number_text(&safety_number(&a, &b)),
            r.expect,
            "{}",
            r.name
        );
        assert_eq!(
            safety_number_text(&safety_number(&b, &a)),
            r.expect,
            "{}: the other order",
            r.name
        );
        seen += 1;
    }
    assert!(seen >= 5);
}

/// C10 [M]: owner and contact compute the same 20 digits from the same two public keys in either
/// argument order; different keys give different numbers (10,000 seeded pairs: no two numbers meet);
/// the number is 20 decimal digits; a flipped bit of either key changes it; and every digit
/// position takes every value.
#[test]
fn c10_safety_number_matches_on_both_sides() {
    let mut rng = SplitMix64::new(1010);
    let mut numbers = BTreeSet::new();
    let mut positions = vec![BTreeSet::new(); 20];
    for _ in 0..10_000 {
        let (a, b) = (random_key(&mut rng), random_key(&mut rng));
        let number = safety_number(&a, &b);
        assert_eq!(number, safety_number(&b, &a), "symmetric");
        assert_eq!(number.len(), 20);
        assert!(number.iter().all(|d| *d <= 9), "{number:?}");
        for (position, digit) in number.iter().enumerate() {
            positions[position].insert(*digit);
        }
        assert!(numbers.insert(number), "two pairs of keys have one number");
    }
    for (position, values) in positions.iter().enumerate() {
        assert_eq!(
            values.len(),
            10,
            "digit {position} never takes some value in 10,000 numbers"
        );
    }

    // A flipped bit of either key changes the number.
    let (a, b) = (random_key(&mut rng), random_key(&mut rng));
    let base = safety_number(&a, &b);
    for bit in 0..256usize {
        let mut flipped = a;
        flipped[bit / 8] ^= 1 << (bit % 8);
        assert_ne!(
            safety_number(&flipped, &b),
            base,
            "bit {bit} of the first key"
        );
        assert_ne!(
            safety_number(&b, &flipped),
            base,
            "bit {bit} of the first key, other order"
        );
        let mut flipped = b;
        flipped[bit / 8] ^= 1 << (bit % 8);
        assert_ne!(
            safety_number(&a, &flipped),
            base,
            "bit {bit} of the second key"
        );
    }
    // The same key twice, and the extremes, are numbers like any other.
    assert_eq!(safety_number(&a, &a), safety_number(&a, &a));
    assert_ne!(safety_number(&a, &a), base);
    assert_ne!(
        safety_number(&[0u8; 32], &[0u8; 32]),
        safety_number(&[0xFF; 32], &[0u8; 32])
    );
    // The order of the arguments is by the bytes of the keys, not by which one is the request's.
    assert_eq!(
        safety_number(&[0u8; 32], &[0xFF; 32]),
        safety_number(&[0xFF; 32], &[0u8; 32])
    );
}

/// C10: the number as people read it: five groups of four digits.
#[test]
fn c10_the_number_is_read_in_five_groups_of_four() {
    let digits: [u8; 20] = [0, 4, 2, 9, 1, 3, 5, 5, 7, 1, 2, 0, 3, 3, 4, 4, 7, 7, 6, 5];
    assert_eq!(safety_number_text(&digits), "0429 1355 7120 3344 7765");
    let mut rng = SplitMix64::new(77);
    for _ in 0..100 {
        let text = safety_number_text(&safety_number(&random_key(&mut rng), &random_key(&mut rng)));
        assert_eq!(text.len(), 24, "{text}");
        let groups: Vec<&str> = text.split(' ').collect();
        assert_eq!(groups.len(), 5);
        assert!(
            groups
                .iter()
                .all(|g| g.len() == 4 && g.bytes().all(|b| b.is_ascii_digit())),
            "{text}"
        );
    }
}
