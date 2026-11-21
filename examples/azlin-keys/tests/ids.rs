//! C1: the typed ids. Every id type round-trips its text form and refuses everything that is not
//! exactly that form; ids of different types do not unify (the `compile_fail` doctest in
//! `src/ids.rs` holds that); `Epoch` and `ChainSeq` count with checked steps.
//!
//! Implements planning/architecture.md section 5.3 and 5.5 (ids), COMMON-W1R R3.

#![cfg_attr(not(feature = "test-support"), allow(unused_imports, dead_code))]

mod common;

use std::collections::BTreeSet;
use std::fmt::{Debug, Display};
use std::str::FromStr;

use azlin_keys::{
    AccountId, ChainSeq, DeviceId, Epoch, GroupId, IdError, KeyId, MemberId, SpaceId,
};
#[cfg(feature = "test-support")]
use azlin_keys::{EntropyError, FailingEntropy, SeededEntropy};

use common::{from_hex_array, SplitMix64};

/// A negative-trait check: `Probe::<T>::check()` compiles only when `T` does NOT implement
/// `Default` (the ambiguity of two blanket impls).
trait AmbiguousIfDefault<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfDefault<()> for T {}
impl<T: Default> AmbiguousIfDefault<u8> for T {}

/// The behaviour every 16-byte id type must have, checked for each one by the same function.
fn id16_behaviour<T>(make: fn([u8; 16]) -> T, bytes_of: fn(&T) -> [u8; 16])
where
    T: Copy + Eq + Ord + std::hash::Hash + Debug + Display + FromStr<Err = IdError>,
{
    let mut rng = SplitMix64::new(0xC1);
    let mut patterns: Vec<[u8; 16]> = vec![[0u8; 16], [0xffu8; 16], [0xabu8; 16]];
    let mut counting = [0u8; 16];
    for (i, b) in counting.iter_mut().enumerate() {
        *b = i as u8;
    }
    patterns.push(counting);
    for _ in 0..200 {
        patterns.push(from_hex_array::<16>(&common::to_hex(&rng.bytes(16))));
    }

    for bytes in &patterns {
        let id = make(*bytes);
        let text = id.to_string();
        assert_eq!(text.len(), 32, "32 hex digits");
        assert!(
            text.bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
            "lowercase hex only: {text}"
        );
        assert_eq!(
            text,
            common::to_hex(bytes),
            "Display is the lowercase hex of the bytes"
        );
        let back: T = text.parse().expect("the text form parses");
        assert!(back == id, "round trip of {text}");
        assert_eq!(bytes_of(&back), *bytes);
        assert!(format!("{id:?}").contains(&text), "Debug names the value");
    }

    // Everything that is not exactly 32 lowercase hex digits is refused, with the right word.
    let good = "0123456789abcdef0123456789abcdef";
    assert!(good.parse::<T>().is_ok());
    let upper = "0123456789ABCDEF0123456789ABCDEF";
    let mixed = "0123456789abcdef0123456789abcdeF";
    let refused_digits = [
        upper,
        mixed,
        "0123456789abcdef0123456789abcdeg",  // not hex
        "0123456789abcdef 123456789abcdef",  // inner space
        "0123456789abcdef0123456789abcde\n", // newline
        "0123456789abcdef0123456789abcde\0", // NUL
        "0x23456789abcdef0123456789abcdef",  // 0x prefix, 32 characters
        "-123456789abcdef0123456789abcdef",  // sign, 32 characters
        "0123456789abcdef0123456789abcdé",   // non-ASCII, 32 bytes
    ];
    for text in refused_digits {
        assert_eq!(
            text.parse::<T>().err(),
            Some(IdError::BadDigit),
            "{text:?} has the right length and a wrong character"
        );
    }
    let refused_length = [
        "",
        "0",
        "0123456789abcdef0123456789abcde",   // 31 digits
        "0123456789abcdef0123456789abcdef0", // 33 digits
        " 0123456789abcdef0123456789abcdef", // leading space
        "0123456789abcdef0123456789abcdef ", // trailing space
        "\t0123456789abcdef0123456789abcdef\n",
        "0x0123456789abcdef0123456789abcdef", // 0x prefix before the 32 digits
        "0123456789abcdef-0123456789abcdef",  // a dash (UUID-like)
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", // a 64-digit key id
    ];
    for text in refused_length {
        assert_eq!(
            text.parse::<T>().err(),
            Some(IdError::Length),
            "{text:?} has the wrong length"
        );
    }
    assert!(IdError::Length.to_string() != IdError::BadDigit.to_string());

    // Ordering and equality are those of the bytes (an id is a map key).
    let low = make([0u8; 16]);
    let high = make([1u8; 16]);
    assert!(low < high);
    let set: BTreeSet<T> = [high, low, high].into_iter().collect();
    assert_eq!(set.len(), 2);
}

#[test]
fn c1_ids_roundtrip_and_reject() {
    id16_behaviour(AccountId::from_bytes, |id| *id.as_bytes());
    id16_behaviour(MemberId::from_bytes, |id| *id.as_bytes());
    id16_behaviour(DeviceId::from_bytes, |id| *id.as_bytes());
    id16_behaviour(SpaceId::from_bytes, |id| *id.as_bytes());
    id16_behaviour(GroupId::from_bytes, |id| *id.as_bytes());
}

#[test]
fn c1_ids_are_not_defaultable_and_have_a_fixed_length() {
    // Compiles only while none of the types implements `Default`.
    <AccountId as AmbiguousIfDefault<_>>::check();
    <MemberId as AmbiguousIfDefault<_>>::check();
    <DeviceId as AmbiguousIfDefault<_>>::check();
    <SpaceId as AmbiguousIfDefault<_>>::check();
    <GroupId as AmbiguousIfDefault<_>>::check();
    <KeyId as AmbiguousIfDefault<_>>::check();
    <Epoch as AmbiguousIfDefault<_>>::check();
    <ChainSeq as AmbiguousIfDefault<_>>::check();
    assert_eq!(AccountId::LEN, 16);
    assert_eq!(MemberId::LEN, 16);
    assert_eq!(DeviceId::LEN, 16);
    assert_eq!(SpaceId::LEN, 16);
    assert_eq!(GroupId::LEN, 16);
    assert_eq!(KeyId::LEN, 32);
}

#[test]
fn c1_ids_of_one_value_differ_by_type_only_in_the_type() {
    // The same bytes are equal as text for every type, and the types cannot be mixed up by
    // accident: this asserts the text equality, the compile_fail doctest asserts the types.
    let bytes = [9u8; 16];
    let texts = [
        AccountId::from_bytes(bytes).to_string(),
        MemberId::from_bytes(bytes).to_string(),
        DeviceId::from_bytes(bytes).to_string(),
        SpaceId::from_bytes(bytes).to_string(),
        GroupId::from_bytes(bytes).to_string(),
    ];
    assert!(texts.iter().all(|t| *t == texts[0]));
    assert_eq!(
        format!("{:?}", SpaceId::from_bytes(bytes)),
        format!("SpaceId({})", texts[0])
    );
    assert_eq!(
        format!("{:?}", DeviceId::from_bytes(bytes)),
        format!("DeviceId({})", texts[0])
    );
}

#[test]
fn c1_key_id_text_form_is_64_lowercase_hex_digits() {
    let mut rng = SplitMix64::new(0xC1_0000_0001);
    for _ in 0..200 {
        let bytes = from_hex_array::<32>(&common::to_hex(&rng.bytes(32)));
        let id = KeyId::from_bytes(bytes);
        let text = id.to_string();
        assert_eq!(text.len(), 64);
        assert_eq!(text, common::to_hex(&bytes));
        assert_eq!(text.parse::<KeyId>(), Ok(id));
        assert_eq!(id.as_bytes(), &bytes);
        assert_eq!(format!("{id:?}"), format!("KeyId({text})"));
    }
    let good = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
    assert!(good.parse::<KeyId>().is_ok());
    assert_eq!(good.to_uppercase().parse::<KeyId>(), Err(IdError::BadDigit));
    assert_eq!(good[..63].parse::<KeyId>(), Err(IdError::Length));
    assert_eq!(format!("{good}0").parse::<KeyId>(), Err(IdError::Length));
    assert_eq!(
        format!(" {}", &good[..63]).parse::<KeyId>(),
        Err(IdError::BadDigit)
    );
    assert_eq!(
        format!("0x{}", &good[..62]).parse::<KeyId>(),
        Err(IdError::BadDigit)
    );
    assert_eq!(
        good[..32].parse::<KeyId>(),
        Err(IdError::Length),
        "a 16-byte id is no key id"
    );
}

#[test]
fn c1_epoch_starts_at_one_and_counts_checked() {
    assert_eq!(Epoch::FIRST.get(), 1);
    assert_eq!(Epoch::new(0), None, "epoch 0 does not exist");
    assert_eq!(Epoch::new(1), Some(Epoch::FIRST));
    assert_eq!(Epoch::FIRST.next().map(Epoch::get), Some(2));
    assert_eq!(Epoch::FIRST.prev(), None, "nothing before the first epoch");
    assert_eq!(Epoch::new(7).and_then(Epoch::prev).map(Epoch::get), Some(6));
    let last = Epoch::new(u32::MAX).expect("the largest epoch");
    assert_eq!(last.next(), None, "the next epoch is checked: no wrap to 0");
    assert_eq!(last.prev().map(Epoch::get), Some(u32::MAX - 1));
    assert!(Epoch::FIRST < Epoch::new(2).unwrap());
    assert_eq!(Epoch::new(12).unwrap().to_string(), "12");
    assert_eq!(format!("{:?}", Epoch::FIRST), "Epoch(1)");
}

#[test]
fn c1_epoch_text_form_is_strict_decimal() {
    for (text, want) in [("1", 1u32), ("2", 2), ("10", 10), ("4294967295", u32::MAX)] {
        assert_eq!(text.parse::<Epoch>().map(Epoch::get), Ok(want), "{text}");
    }
    for text in [
        "", "+1", "-1", "01", "001", "0x1", "1 ", " 1", "1.0", "１", "1e3", "a",
    ] {
        let e = text.parse::<Epoch>().err();
        assert!(
            e == Some(IdError::BadDigit) || e == Some(IdError::Length),
            "{text:?} must be refused, got {e:?}"
        );
    }
    assert_eq!("".parse::<Epoch>(), Err(IdError::Length));
    assert_eq!("+1".parse::<Epoch>(), Err(IdError::BadDigit));
    assert_eq!("01".parse::<Epoch>(), Err(IdError::BadDigit));
    assert_eq!(
        "0".parse::<Epoch>(),
        Err(IdError::OutOfRange),
        "epoch 0 is out of range"
    );
    assert_eq!("4294967296".parse::<Epoch>(), Err(IdError::OutOfRange));
    assert_eq!(
        "99999999999999999999999".parse::<Epoch>(),
        Err(IdError::OutOfRange)
    );
    // The text form is the Display form.
    for n in [1u32, 9, 10, 99, 100, 65_535, u32::MAX] {
        let e = Epoch::new(n).unwrap();
        assert_eq!(e.to_string().parse::<Epoch>(), Ok(e));
    }
}

#[test]
fn c1_chain_seq_counts_checked() {
    assert_eq!(ChainSeq::MIN.get(), 0);
    assert_eq!(ChainSeq::new(41).next(), Some(ChainSeq::new(42)));
    assert_eq!(
        ChainSeq::new(u64::MAX).next(),
        None,
        "the next position is checked"
    );
    assert!(ChainSeq::new(3) < ChainSeq::new(4));
    assert_eq!(ChainSeq::new(7).to_string(), "7");
    assert_eq!(format!("{:?}", ChainSeq::new(7)), "ChainSeq(7)");
}

#[cfg(feature = "test-support")]
#[test]
fn c1_generate_draws_the_entropy_seam_and_never_returns_zeroes_on_failure() {
    // Two ids from one stream differ; the same seed gives the same ids.
    let mut a = SeededEntropy::from_u64(5);
    let first = SpaceId::generate(&mut a).unwrap();
    let second = SpaceId::generate(&mut a).unwrap();
    assert_ne!(first, second);
    assert_eq!(a.drawn(), 32, "an id draws exactly its 16 bytes");
    let mut b = SeededEntropy::from_u64(5);
    assert_eq!(SpaceId::generate(&mut b).unwrap(), first);
    // A failing source gives no id at all.
    let mut failing = FailingEntropy::after(0);
    assert_eq!(
        DeviceId::generate(&mut failing).err(),
        Some(EntropyError::Exhausted)
    );
    let mut short = FailingEntropy::after(15);
    assert_eq!(
        AccountId::generate(&mut short).err(),
        Some(EntropyError::Exhausted)
    );
    let mut exact = FailingEntropy::after(16);
    assert!(MemberId::generate(&mut exact).is_ok());
    assert_eq!(
        GroupId::generate(&mut exact).err(),
        Some(EntropyError::Exhausted)
    );
}

#[test]
fn c1_key_id_of_bundle_is_one_id_per_bundle() {
    let kx = [0x11u8; 32];
    let pq = vec![0x22u8; 1184];
    let plain = KeyId::of_bundle(&kx, None);
    let hybrid = KeyId::of_bundle(&kx, Some(&pq));
    assert_ne!(plain, hybrid, "the pq key is part of the bundle");
    assert_eq!(KeyId::of_bundle(&kx, None), plain, "deterministic");
    assert_eq!(KeyId::of_bundle(&kx, Some(&pq)), hybrid);
    assert_eq!(
        KeyId::of_bundle(&kx, Some(&[])),
        plain,
        "an empty pq key is no pq key"
    );
    let mut other_kx = kx;
    other_kx[0] ^= 1;
    assert_ne!(KeyId::of_bundle(&other_kx, None), plain);
    let mut other_pq = pq.clone();
    other_pq[1183] ^= 1;
    assert_ne!(KeyId::of_bundle(&kx, Some(&other_pq)), hybrid);
    // The id is the one `derive_key` of the pinned context over `kx || pq` (the known answers are
    // in tests/vectors/derive_kat.json, checked by c5).
    let mut concatenated = kx.to_vec();
    concatenated.extend_from_slice(&pq);
    assert_eq!(
        hybrid.as_bytes(),
        &azlin_keys::derive::derive_public(&azlin_keys::derive::KEY_ID, &concatenated)
    );
}
