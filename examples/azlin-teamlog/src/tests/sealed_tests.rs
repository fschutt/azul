//! Tests of the sealed fields (check 13 of the brief; planning/architecture.md section 5.9 "`sealed` |
//! display data (names, emails), encrypted under the account's seal key" and section 5.17 "deleting that
//! key shreds them"; lane W1-S17c2).

use azlin_seams::sim::Rng;
use azlin_seams::{AccountKind, Entropy, EntropyError, FakeScheme, Secret};
use zeroize::Zeroizing;

use super::kit::{account, hash};
use super::signed::{trusted, SignedKit};
use crate::ops::{Op, SEALED_MAX};
use crate::replay::replay;
use crate::sealed::{open_field, seal_field, SealError, SealKey};
use crate::state::TeamState;

/// An entropy source for the tests: a seeded generator (the same seed, the same bytes).
struct SeededSource(Rng);

impl Entropy for SeededSource {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError> {
        self.0.fill_bytes(out);
        Ok(())
    }
}

/// An entropy source that has nothing to give.
struct DrySource;

impl Entropy for DrySource {
    fn fill(&mut self, _out: &mut [u8]) -> Result<(), EntropyError> {
        Err(EntropyError::Exhausted)
    }
}

fn seal_key(n: u8) -> SealKey {
    SealKey::new(Secret::from_bytes([n; 32]))
}

#[test]
fn sealed_roundtrip() {
    let key = seal_key(7);
    let mut source = SeededSource(Rng::new(1));
    let plains: [&[u8]; 6] = [
        b"Engineering",
        b"",
        b"a",
        b"alice@example.org",
        &[0u8; 100],
        &[0xFFu8; 1000],
    ];
    for plain in plains {
        let sealed = seal_field(&key, &hash(1), "name", plain, &mut source).expect("sealed");
        assert_eq!(
            sealed.len(),
            1 + 24 + 16 + plain.len(),
            "version, nonce, tag"
        );
        assert_eq!(sealed[0], 1, "the version byte");
        let opened: Zeroizing<Vec<u8>> =
            open_field(&key, &hash(1), "name", &sealed).expect("opens");
        assert_eq!(opened.as_slice(), plain);
    }
    // A fresh nonce for every field: the same plaintext seals to different bytes; the same entropy
    // (the same seed) seals to the same bytes, because the randomness is a parameter.
    let again = seal_field(&key, &hash(1), "name", b"same", &mut source).expect("sealed");
    let other = seal_field(&key, &hash(1), "name", b"same", &mut source).expect("sealed");
    assert_ne!(again, other);
    let replayed_a = seal_field(
        &key,
        &hash(1),
        "name",
        b"same",
        &mut SeededSource(Rng::new(9)),
    )
    .expect("sealed");
    let replayed_b = seal_field(
        &key,
        &hash(1),
        "name",
        b"same",
        &mut SeededSource(Rng::new(9)),
    )
    .expect("sealed");
    assert_eq!(replayed_a, replayed_b);
    // The plaintext is not in the sealed bytes.
    assert!(!other.windows(4).any(|window| window == b"same"));
}

#[test]
fn sealed_field_bound_to_entry_hash_and_field_name() {
    let key = seal_key(7);
    let mut source = SeededSource(Rng::new(2));
    let sealed = seal_field(&key, &hash(1), "name", b"Engineering", &mut source).expect("sealed");
    // Another entry, another field name, another key: none opens it.
    assert_eq!(
        open_field(&key, &hash(2), "name", &sealed).err(),
        Some(SealError::Open)
    );
    assert_eq!(
        open_field(&key, &hash(1), "email", &sealed).err(),
        Some(SealError::Open)
    );
    assert_eq!(
        open_field(&key, &hash(1), "nam", &sealed).err(),
        Some(SealError::Open)
    );
    assert_eq!(
        open_field(&key, &hash(1), "names", &sealed).err(),
        Some(SealError::Open)
    );
    assert_eq!(
        open_field(&seal_key(8), &hash(1), "name", &sealed).err(),
        Some(SealError::Open)
    );
    // The right ones do.
    assert!(open_field(&key, &hash(1), "name", &sealed).is_ok());
    // Every byte of the sealed field is covered: nonce, ciphertext and tag; the version byte and the
    // length are checked as the format.
    for position in 1..sealed.len() {
        let mut damaged = sealed.clone();
        damaged[position] ^= 0x01;
        assert_eq!(
            open_field(&key, &hash(1), "name", &damaged).err(),
            Some(SealError::Open),
            "byte {position}"
        );
    }
    let mut other_version = sealed.clone();
    other_version[0] = 2;
    assert_eq!(
        open_field(&key, &hash(1), "name", &other_version).err(),
        Some(SealError::Malformed)
    );
    assert_eq!(
        open_field(&key, &hash(1), "name", &[]).err(),
        Some(SealError::Malformed)
    );
    assert_eq!(
        open_field(&key, &hash(1), "name", &sealed[..40]).err(),
        Some(SealError::Malformed)
    );
    let mut longer = sealed.clone();
    longer.push(0);
    assert_eq!(
        open_field(&key, &hash(1), "name", &longer).err(),
        Some(SealError::Open)
    );
    // The sealed field of one entry moved to another entry's group name does not open there.
    let moved = seal_field(&key, &hash(5), "name", b"Sales", &mut source).expect("sealed");
    assert!(open_field(&key, &hash(6), "name", &moved).is_err());
}

#[test]
fn a_field_has_a_size_limit_and_an_entropy_failure_seals_nothing() {
    let key = seal_key(7);
    let mut source = SeededSource(Rng::new(3));
    let room = SEALED_MAX.get() as usize - (1 + 24 + 16);
    let biggest = vec![5u8; room];
    let sealed = seal_field(&key, &hash(1), "name", &biggest, &mut source).expect("fits");
    assert_eq!(sealed.len() as u64, SEALED_MAX.get());
    assert!(open_field(&key, &hash(1), "name", &sealed).is_ok());
    assert_eq!(
        seal_field(&key, &hash(1), "name", &vec![5u8; room + 1], &mut source).err(),
        Some(SealError::TooLong)
    );
    // No randomness, no nonce, no field: never a field with a zero nonce.
    assert_eq!(
        seal_field(&key, &hash(1), "name", b"x", &mut DrySource).err(),
        Some(SealError::Entropy(EntropyError::Exhausted))
    );
    assert_eq!(
        SealError::Entropy(EntropyError::Exhausted).to_string(),
        "the random source gave no nonce: the random source has no more bytes"
    );
    // The seal key never prints a byte.
    let text = format!("{key:?}");
    assert!(text.contains("***"));
    assert!(!text.contains("07"));
}

#[test]
fn deleting_the_seal_key_leaves_the_chain_verifying_and_the_fields_unreadable() {
    let key = seal_key(7);
    let control = seal_key(7);
    let mut source = SeededSource(Rng::new(4));
    // A log whose entries carry sealed names: the group names, sealed under the account's key.
    let mut kit = SignedKit::new(AccountKind::Team);
    let names: [(u8, &[u8]); 3] = [
        (1, b"Engineering"),
        (2, b"Sales"),
        (3, b"alice@example.org"),
    ];
    for (n, name) in names {
        let mut entry = kit.entry_by(
            1,
            &[],
            Op::GroupCreate {
                group: super::kit::group(n),
                parent: None,
            },
        );
        entry.sealed =
            Some(seal_field(&key, &entry.hash, "name", name, &mut source).expect("sealed"));
        kit.push(entry).expect("admitted");
    }
    let live_hash = kit.state.state_hash();
    // The account's seal key is deleted (§5.17): nothing holds it any more.
    drop(key);
    // The chain still verifies from its first entry: no step of the check needs the key.
    let entries: Vec<_> = kit.log.iter().map(|(entry, _)| entry.clone()).collect();
    let stamps: Vec<_> = kit.log.iter().map(|(_, stamp)| *stamp).collect();
    let verified = replay(
        TeamState::empty(account(), AccountKind::Team),
        &entries,
        &FakeScheme,
        &trusted(),
        &stamps,
    )
    .expect("the chain verifies without the seal key");
    assert_eq!(verified.state_hash(), live_hash);
    // The fields are unreadable: a new key, a guessed key, all-zero keys, open nothing.
    for (index, (n, _)) in names.iter().enumerate() {
        let (entry, _) = &kit.log[index + 1];
        let sealed = verified
            .groups()
            .get(&super::kit::group(*n))
            .and_then(|group| group.name_sealed.clone())
            .expect("the group row keeps the sealed name");
        assert_eq!(Some(&sealed), entry.sealed.as_ref());
        for guess in [seal_key(0), seal_key(1), seal_key(8), seal_key(255)] {
            assert_eq!(
                open_field(&guess, &entry.hash, "name", &sealed).err(),
                Some(SealError::Open)
            );
        }
        // Control: the very bytes of the deleted key still open them (the test kept one copy).
        assert!(open_field(&control, &entry.hash, "name", &sealed).is_ok());
    }
}

#[test]
fn admit_never_needs_the_seal_key() {
    // `admit` and `verify_signatures` take no seal key. A sealed field that is not a sealed field at all
    // (random bytes, empty bytes, bytes that would not open under any key) is admitted: it is opaque
    // display data with a size limit, not something the permission check reads.
    let kit = SignedKit::new(AccountKind::Team);
    for sealed in [
        vec![],
        vec![0xAB; 100],
        vec![1; 41],
        b"not sealed at all".to_vec(),
    ] {
        let mut entry = kit.entry_by(
            1,
            &[],
            Op::GroupCreate {
                group: super::kit::group(9),
                parent: None,
            },
        );
        entry.sealed = Some(sealed);
        assert_eq!(kit.verdict(&entry), Ok(()));
    }
    // And a sealed field never changes a verdict: the same refusal with and without one.
    let mut bad = kit.entry_by(
        2,
        &[],
        Op::GroupCreate {
            group: super::kit::group(9),
            parent: None,
        },
    );
    let plain = kit.verdict(&bad);
    bad.sealed = Some(vec![7; 64]);
    assert_eq!(kit.verdict(&bad), plain);
}
