//! S9A-29 `bounded_newtypes_refuse_oversize`, and the checks of `ClientVersion`.

use azlin_regbase::{
    BoundError, ClientVersion, Features, RegionId, RegionSet, SealedBlob, SignedBytes, SmallText,
    TokenHash, UsedHashes, VersionError, FEATURES_MAX, REGIONS_MAX, SEALED_BLOB_MAX,
    SIGNED_BYTES_MAX, SMALL_TEXT_MAX, USED_HASHES_KEPT,
};

use crate::common::Rng;

fn word(text: &str) -> SmallText {
    SmallText::new(text).expect("a word")
}

fn hash(n: u8) -> TokenHash {
    TokenHash::from_bytes([n; 32])
}

#[test]
fn s9a_29_the_limits_are_the_documented_ones() {
    assert_eq!(SEALED_BLOB_MAX.get(), 512);
    assert_eq!(SIGNED_BYTES_MAX.get(), 8 * 1024);
    assert_eq!(SMALL_TEXT_MAX.get(), 64);
    assert_eq!(FEATURES_MAX, 32);
    assert_eq!(REGIONS_MAX, 64);
    assert_eq!(USED_HASHES_KEPT, 20);
}

#[test]
fn s9a_29_a_sealed_blob_refuses_empty_and_oversize_and_never_truncates() {
    assert_eq!(
        SealedBlob::new(Vec::new()),
        Err(BoundError::Empty { what: "SealedBlob" })
    );
    assert!(SealedBlob::new(vec![1]).is_ok());
    let at_limit = SealedBlob::new(vec![9; 512]).expect("512 bytes");
    assert_eq!(at_limit.len(), 512);
    assert!(!at_limit.is_empty());
    assert_eq!(
        SealedBlob::new(vec![9; 513]),
        Err(BoundError::TooLong {
            what: "SealedBlob",
            len: 513,
            max: 512
        })
    );
    assert!(SealedBlob::from_slice(&[0u8; 100_000]).is_err());
    // the bytes are kept exactly
    let bytes: Vec<u8> = (0..=255u8).collect();
    let blob = SealedBlob::from_slice(&bytes).expect("256 bytes");
    assert_eq!(blob.as_bytes(), bytes.as_slice());
}

#[test]
fn s9a_29_a_signed_structure_refuses_empty_and_oversize() {
    assert_eq!(
        SignedBytes::new(Vec::new()),
        Err(BoundError::Empty {
            what: "SignedBytes"
        })
    );
    let at_limit = SignedBytes::new(vec![3; 8 * 1024]).expect("8 KiB");
    assert_eq!(at_limit.len(), 8 * 1024);
    assert_eq!(
        SignedBytes::new(vec![3; 8 * 1024 + 1]),
        Err(BoundError::TooLong {
            what: "SignedBytes",
            len: 8 * 1024 + 1,
            max: 8 * 1024
        })
    );
    assert!(SignedBytes::from_slice(&[0u8; 1 << 20]).is_err());
}

#[test]
fn s9a_29_debug_of_a_blob_prints_its_length_only() {
    let planted = vec![0xab; 100];
    let sealed = SealedBlob::new(planted.clone()).expect("blob");
    let text = format!("{sealed:?}");
    assert_eq!(text, "SealedBlob(len 100)");
    assert!(!text.contains("ab") && !text.contains("171"), "{text}");
    let signed = SignedBytes::new(planted).expect("signed");
    let text = format!("{signed:?}");
    assert_eq!(text, "SignedBytes(len 100)");
    assert!(!text.contains("171"), "{text}");
}

#[test]
fn s9a_29_a_small_text_is_printable_ascii_of_at_most_64_bytes() {
    assert!(SmallText::new("").is_ok());
    assert!(SmallText::empty().is_empty());
    assert!(SmallText::new("lockdown").is_ok());
    assert!(SmallText::new("Azlin Mail 1.0 (beta)").is_ok());
    assert!(SmallText::new(&"a".repeat(64)).is_ok());
    assert_eq!(
        SmallText::new(&"a".repeat(65)),
        Err(BoundError::TooLong {
            what: "SmallText",
            len: 65,
            max: 64
        })
    );
    // control bytes, DEL, and everything above ASCII
    for bad in [
        "a\u{0}b",
        "tab\t",
        "new\nline",
        "\u{1f}",
        "del\u{7f}",
        "caf\u{e9}",
        "\u{1f600}",
    ] {
        assert_eq!(
            SmallText::new(bad),
            Err(BoundError::BadChar { what: "SmallText" }),
            "{bad:?}"
        );
    }
    // 64 bytes counted in bytes, not characters
    assert!(SmallText::new(&"\u{e9}".repeat(32)).is_err());
    // the word test
    assert!(word("device_add").is_word());
    assert!(word("a1_2").is_word());
    for not in ["", "Device", "a b", "a-b", "a.b"] {
        assert!(
            !SmallText::new(not).expect("printable").is_word(),
            "{not:?}"
        );
    }
}

#[test]
fn s9a_29_features_keep_unknown_names_sort_and_dedup() {
    let features = Features::new(vec![
        word("team"),
        word("audit"),
        word("future_feature_of_a_newer_node"),
        word("team"),
        word("sso"),
        word("audit"),
    ])
    .expect("features");
    let names: Vec<&str> = features.as_slice().iter().map(SmallText::as_str).collect();
    assert_eq!(
        names,
        ["audit", "future_feature_of_a_newer_node", "sso", "team"]
    );
    assert!(features.contains("future_feature_of_a_newer_node"));
    assert!(features.contains("team") && !features.contains("escrow"));
    assert_eq!(features.len(), 4);
    assert!(Features::empty().is_empty());
    // the same set from any order
    let reversed = Features::new(vec![
        word("team"),
        word("sso"),
        word("future_feature_of_a_newer_node"),
        word("audit"),
    ])
    .expect("features");
    assert_eq!(reversed, features);
    // exactly the limit is fine; one more distinct name is refused, never dropped
    let at_limit: Vec<SmallText> = (0..32).map(|n| word(&format!("f{n:02}"))).collect();
    assert_eq!(Features::new(at_limit.clone()).expect("32 names").len(), 32);
    let mut over = at_limit.clone();
    over.push(word("one_more"));
    assert_eq!(
        Features::new(over),
        Err(BoundError::TooMany {
            what: "Features",
            count: 33,
            max: 32
        })
    );
    // duplicates do not count toward the limit
    let mut dup = at_limit.clone();
    dup.extend(at_limit);
    assert_eq!(Features::new(dup).expect("duplicates collapse").len(), 32);
    // a name that is not a machine word is refused
    for bad in ["Team", "a b", "sso!", ""] {
        assert_eq!(
            Features::new(vec![SmallText::new(bad).expect("printable")]),
            Err(BoundError::BadChar { what: "Features" }),
            "{bad:?}"
        );
    }
}

#[test]
fn s9a_29_a_region_set_is_sorted_deduped_and_bounded() {
    let regions = RegionSet::new(vec![
        RegionId::new(9),
        RegionId::new(2),
        RegionId::new(9),
        RegionId::new(5),
    ])
    .expect("regions");
    let numbers: Vec<u16> = regions.as_slice().iter().map(|r| r.get()).collect();
    assert_eq!(numbers, [2, 5, 9]);
    assert!(regions.contains(RegionId::new(5)) && !regions.contains(RegionId::new(6)));
    assert!(RegionSet::empty().is_empty());
    let at_limit: Vec<RegionId> = (0..64).map(RegionId::new).collect();
    assert_eq!(RegionSet::new(at_limit.clone()).expect("64").len(), 64);
    let mut over = at_limit;
    over.push(RegionId::new(64));
    assert_eq!(
        RegionSet::new(over),
        Err(BoundError::TooMany {
            what: "RegionSet",
            count: 65,
            max: 64
        })
    );
}

#[test]
fn s9a_29_used_hashes_refuse_a_21st_and_keep_oldest_first() {
    let twenty: Vec<TokenHash> = (1..=20).map(hash).collect();
    let used = UsedHashes::new(twenty.clone()).expect("20 hashes");
    assert_eq!(used.len(), 20);
    assert_eq!(used.newest(), Some(&hash(20)));
    assert_eq!(used.as_slice(), twenty.as_slice());
    let mut over = twenty;
    over.push(hash(21));
    assert_eq!(
        UsedHashes::new(over),
        Err(BoundError::TooMany {
            what: "UsedHashes",
            count: 21,
            max: 20
        })
    );
    assert!(UsedHashes::empty().is_empty());
    assert_eq!(UsedHashes::empty().newest(), None);
}

#[test]
fn s9a_29_pushing_a_hash_drops_the_oldest_beyond_the_limit() {
    let mut used = UsedHashes::empty();
    for n in 1..=25u8 {
        used = used.pushed(hash(n));
        assert!(used.len() <= USED_HASHES_KEPT);
        assert_eq!(used.newest(), Some(&hash(n)));
    }
    // 25 pushed, 20 kept: 6..=25, oldest first
    let kept: Vec<TokenHash> = (6..=25).map(hash).collect();
    assert_eq!(used.as_slice(), kept.as_slice());

    // the lost-answer swap puts a hash just before the newest
    let used = UsedHashes::new(vec![hash(1), hash(2), hash(3)]).expect("3");
    let swapped = used.pushed_before_newest(hash(9));
    assert_eq!(swapped.as_slice(), [hash(1), hash(2), hash(9), hash(3)]);
    assert_eq!(
        UsedHashes::empty().pushed_before_newest(hash(9)).as_slice(),
        [hash(9)]
    );
    // and drops the oldest beyond the limit too
    let full = UsedHashes::new((1..=20).map(hash).collect()).expect("20");
    let swapped = full.pushed_before_newest(hash(99));
    assert_eq!(swapped.len(), 20);
    assert_eq!(swapped.as_slice().first(), Some(&hash(2)));
    assert_eq!(swapped.newest(), Some(&hash(20)));
}

#[test]
fn s9a_29_debug_of_used_hashes_prints_a_count_never_a_hash() {
    let used = UsedHashes::new(vec![TokenHash::from_bytes([0xcd; 32]); 3]).expect("3");
    let text = format!("{used:?}");
    assert_eq!(text, "UsedHashes(len 3)");
    assert!(!text.contains("cd"), "{text}");
}

#[test]
fn s9a_29_client_versions_order_and_parse_strictly() {
    let v = ClientVersion::new;
    assert!(v(1, 9, 9) < v(2, 0, 0));
    assert!(v(1, 2, 3) < v(1, 10, 0));
    assert!(v(1, 2, 3) < v(1, 2, 4));
    assert_eq!(v(1, 2, 3), v(1, 2, 3));
    assert_eq!(ClientVersion::parse("0.3.0"), Ok(v(0, 3, 0)));
    assert_eq!(
        ClientVersion::parse("65535.65535.65535"),
        Ok(v(65535, 65535, 65535))
    );
    assert_eq!(v(4, 5, 6).to_string(), "4.5.6");
    assert_eq!(
        (v(4, 5, 6).major(), v(4, 5, 6).minor(), v(4, 5, 6).patch()),
        (4, 5, 6)
    );
    for shape in ["", "1", "1.2", "1.2.3.4", "1..3", ".1.2", "1.2.", "1.2.3."] {
        assert!(ClientVersion::parse(shape).is_err(), "{shape:?}");
    }
    for number in [
        "01.2.3",
        "1.02.3",
        "1.2.03",
        "1.2.-3",
        "+1.2.3",
        "1.2.65536",
        " 1.2.3",
        "1.2.3 ",
        "a.b.c",
        "1.2.3e",
    ] {
        assert!(ClientVersion::parse(number).is_err(), "{number:?}");
    }
    assert_eq!(ClientVersion::parse("1.2"), Err(VersionError::Shape));
    assert_eq!(ClientVersion::parse("1.2.65536"), Err(VersionError::Number));
}

/// Every value built through any constructor is within its bound, for random lengths either side
/// of the limit: a constructor either refuses or holds exactly what it was given.
#[test]
fn s9a_29_a_value_that_is_made_holds_exactly_what_it_was_given() {
    let mut rng = Rng::new(29);
    for _ in 0..2_000 {
        let len = rng.below(700) as usize;
        let bytes: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        match SealedBlob::new(bytes.clone()) {
            Ok(blob) => {
                assert!((1..=512).contains(&len));
                assert_eq!(blob.as_bytes(), bytes.as_slice());
            }
            Err(_) => assert!(len == 0 || len > 512),
        }
    }
}
