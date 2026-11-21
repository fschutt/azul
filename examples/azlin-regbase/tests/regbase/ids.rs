//! S9A-23 `ids_are_strict` and the checks of the counters and the token hash.

use std::collections::BTreeMap;

use azlin_keys::Secret;
use azlin_regbase::{
    CredentialNonce, FamilyId, Generation, IdParseError, KeyEpoch, LogIndex, MetaGroupEpoch,
    MetaGroupId, TokenHash, VoterId,
};

use crate::common::{Counting, Failing, Rng};

/// The strictness checks of one id type of byte length `$len`.
macro_rules! strict_id {
    ($test:ident, $ty:ty, $len:expr) => {
        #[test]
        fn $test() {
            let len: usize = $len;
            let mut rng = Rng::new(0x5eed_0000 + len as u64);
            for _ in 0..200 {
                let mut bytes = vec![0u8; len];
                for byte in bytes.iter_mut() {
                    *byte = rng.next() as u8;
                }
                let mut array = [0u8; $len];
                array.copy_from_slice(&bytes);
                let id = <$ty>::from_bytes(array);
                // from_bytes / as_bytes round-trip, and the text is lower-case hex of the bytes
                assert_eq!(id.as_bytes(), &array);
                let text = id.to_hex();
                assert_eq!(text.len(), 2 * len);
                assert!(text
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
                assert_eq!(<$ty>::parse_hex(&text), Ok(id));
                assert_eq!(text.parse::<$ty>(), Ok(id));
                assert_eq!(id.to_string(), text);
            }

            let good = "ab".repeat(len);
            assert!(<$ty>::parse_hex(&good).is_ok());

            // upper case is refused (a hash has exactly one spelling)
            let upper = good.to_uppercase();
            assert_eq!(<$ty>::parse_hex(&upper), Err(IdParseError::BadDigit));
            let mixed = format!("A{}", &good[1..]);
            assert_eq!(<$ty>::parse_hex(&mixed), Err(IdParseError::BadDigit));
            // a wrong length, shorter, longer, empty
            assert_eq!(<$ty>::parse_hex(&good[1..]), Err(IdParseError::Length));
            assert_eq!(<$ty>::parse_hex(&good[2..]), Err(IdParseError::Length));
            assert_eq!(
                <$ty>::parse_hex(&format!("{good}0")),
                Err(IdParseError::Length)
            );
            assert_eq!(
                <$ty>::parse_hex(&format!("{good}ab")),
                Err(IdParseError::Length)
            );
            assert_eq!(<$ty>::parse_hex(""), Err(IdParseError::Length));
            // whitespace anywhere, at the right length and at the wrong one
            for ws in [" ", "\t", "\n", "\r", "\u{a0}"] {
                let inside = format!("{}{}", ws, &good[ws.len()..]);
                assert!(<$ty>::parse_hex(&inside).is_err(), "{ws:?} inside");
                assert!(<$ty>::parse_hex(&format!("{good}{ws}")).is_err());
                assert!(<$ty>::parse_hex(&format!("{ws}{good}")).is_err());
            }
            // non-hex characters, a 0x prefix, a multi-byte character of the right byte length
            let bad = format!("g{}", &good[1..]);
            assert_eq!(<$ty>::parse_hex(&bad), Err(IdParseError::BadDigit));
            assert!(<$ty>::parse_hex(&format!("0x{}", &good[2..])).is_err());
            let wide = format!("\u{e9}{}", &good[2..]);
            assert_eq!(wide.len(), good.len());
            assert_eq!(<$ty>::parse_hex(&wide), Err(IdParseError::BadDigit));
            let minus = format!("-{}", &good[1..]);
            assert!(<$ty>::parse_hex(&minus).is_err());
        }
    };
}

strict_id!(s9a_23_family_ids_are_strict, FamilyId, 16);
strict_id!(s9a_23_credential_nonces_are_strict, CredentialNonce, 16);
strict_id!(s9a_23_token_hashes_are_strict, TokenHash, 32);

/// Ids sort bytewise: a `BTreeMap`'s order is the order of the bytes (it is the digest order of the
/// registry).
#[test]
fn s9a_23_ids_sort_bytewise() {
    let mut rng = Rng::new(7);
    let mut ids = Vec::new();
    for _ in 0..300 {
        let mut bytes = [0u8; 16];
        for byte in bytes.iter_mut() {
            *byte = rng.next() as u8;
        }
        ids.push(FamilyId::from_bytes(bytes));
    }
    let mut map = BTreeMap::new();
    for (index, id) in ids.iter().enumerate().rev() {
        map.insert(*id, index);
    }
    let in_map: Vec<[u8; 16]> = map.keys().map(|id| *id.as_bytes()).collect();
    let mut sorted: Vec<[u8; 16]> = ids.iter().map(|id| *id.as_bytes()).collect();
    sorted.sort();
    sorted.dedup();
    assert_eq!(in_map, sorted);

    let low = TokenHash::from_bytes([0; 32]);
    let mut high_bytes = [0u8; 32];
    high_bytes[31] = 1;
    let high = TokenHash::from_bytes(high_bytes);
    assert!(low < high);
    let mut first_bytes = [0u8; 32];
    first_bytes[0] = 1;
    assert!(TokenHash::from_bytes(first_bytes) > high);
}

/// `Debug` of an id of this crate prints at most its first four bytes.
#[test]
fn s9a_23_debug_prints_at_most_four_bytes() {
    let family = FamilyId::from_bytes([0xab; 16]);
    let text = format!("{family:?}");
    assert!(text.starts_with("FamilyId("), "{text}");
    assert!(text.contains("abababab"), "{text}");
    assert!(!text.contains("ababababab"), "five bytes: {text}");

    let nonce = CredentialNonce::from_bytes([0xcd; 16]);
    let text = format!("{nonce:?}");
    assert!(
        text.contains("cdcdcdcd") && !text.contains("cdcdcdcdcd"),
        "{text}"
    );

    let hash = TokenHash::from_bytes([0xef; 32]);
    let text = format!("{hash:?}");
    assert!(
        text.contains("efefefef") && !text.contains("efefefefef"),
        "{text}"
    );
}

/// `generate` fills from the entropy seam, and a failing entropy is an error, never an id of zeroes.
#[test]
fn s9a_23_generate_takes_the_entropy_seam_and_fails_without_it() {
    let mut entropy = Counting(0);
    let first = FamilyId::generate(&mut entropy).expect("entropy");
    let second = FamilyId::generate(&mut entropy).expect("entropy");
    assert_ne!(first, second);
    assert_eq!(first.as_bytes()[0], 1);
    assert_eq!(second.as_bytes()[0], 17);
    assert!(CredentialNonce::generate(&mut entropy).is_ok());

    assert!(FamilyId::generate(&mut Failing).is_err());
    assert!(CredentialNonce::generate(&mut Failing).is_err());
}

/// The counters have a checked successor (a counter never wraps) and read as decimal.
#[test]
fn s9a_23_counters_never_wrap() {
    assert_eq!(MetaGroupId::FIRST.get(), 0);
    assert_eq!(MetaGroupId::new(u16::MAX).next(), None);
    assert_eq!(MetaGroupId::new(4).next(), Some(MetaGroupId::new(5)));
    assert_eq!(VoterId::new(u64::MAX).next(), None);
    assert_eq!(MetaGroupEpoch::new(u64::MAX).next(), None);
    assert_eq!(LogIndex::new(u64::MAX).next(), None);
    assert_eq!(KeyEpoch::new(u32::MAX).next(), None);
    assert_eq!(Generation::new(u32::MAX).next(), None);
    assert_eq!(Generation::ZERO.next(), Some(Generation::new(1)));
    assert_eq!(LogIndex::new(41).next().map(LogIndex::get), Some(42));
    assert_eq!(LogIndex::new(7).to_string(), "7");
    assert_eq!(format!("{:?}", Generation::new(3)), "Generation(3)");
    assert!(LogIndex::new(1) < LogIndex::new(2));
}

/// A token hash is BLAKE3's `derive_key` under `azlin-token-hash-v1` of the secret's bytes: the one
/// derivation (a hash made another way never matches), and equal hashes compare equal.
#[test]
fn s9a_23_a_token_hash_is_the_domain_separated_hash_of_the_secret() {
    assert_eq!(TokenHash::DOMAIN, "azlin-token-hash-v1");
    let secret = Secret::<32>::from_bytes([7u8; 32]);
    let hash = TokenHash::of_secret(&secret);
    assert_eq!(
        hash.as_bytes(),
        &blake3::derive_key("azlin-token-hash-v1", &[7u8; 32])
    );
    // another secret, another hash; another domain, another hash
    let other = TokenHash::of_secret(&Secret::<32>::from_bytes([8u8; 32]));
    assert_ne!(hash, other);
    assert_ne!(
        hash.as_bytes(),
        &blake3::derive_key("azlin-token-hash-v2", &[7u8; 32])
    );
    // a secret of another length hashes too (S9b picks the token's secret length)
    let long = TokenHash::of_secret(&Secret::<48>::from_bytes([7u8; 48]));
    assert_ne!(hash, long);
    // equality is the constant-time one and agrees with the bytes
    let copy = TokenHash::from_bytes(*hash.as_bytes());
    assert!(hash == copy && hash.ct_equal(&copy) && bool::from(hash.ct_choice(&copy)));
    assert!(!hash.ct_equal(&other) && !bool::from(hash.ct_choice(&other)));
    // one differing byte anywhere is unequal
    for index in 0..32 {
        let mut bytes = *hash.as_bytes();
        bytes[index] ^= 0x01;
        assert!(
            !hash.ct_equal(&TokenHash::from_bytes(bytes)),
            "byte {index}"
        );
    }
    // the text round-trips
    assert_eq!(TokenHash::parse_hex(&hash.to_hex()), Ok(hash));
}
