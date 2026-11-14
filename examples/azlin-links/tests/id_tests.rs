//! Check 5 (ids) and the slug shape of check 2: the public id of a link is 60 random bits in 12
//! base32 characters, reproducible for a seed, never colliding in 100,000 seeded draws; a slug is a
//! readable name that never has the shape of an id.
//!
//! Implements planning/architecture.md section 5.12 (the URL: "The link id has ~60 random bits"; `slug`).

mod common;

use std::collections::HashSet;

use azlin_links::{
    has_link_id_shape, new_id, PublicLinkId, Slug, SlugError, ID_BITS, ID_CHARS, SLUG_MAX_LEN,
    SLUG_MIN_LEN,
};
use azlin_seams::{EntropyError, IdError};
use common::{DeadEntropy, RepeatingEntropy, TestEntropy};

/// The id alphabet: RFC 4648 base32, lower case.
const ALPHABET: &str = "abcdefghijklmnopqrstuvwxyz234567";

#[test]
fn public_id_is_60_bits_in_12_base32_chars() {
    assert_eq!(ID_BITS, 60);
    assert_eq!(ID_CHARS, 12);
    let mut entropy = TestEntropy::new(11);
    let mut or = 0u64;
    let mut and = u64::MAX;
    for _ in 0..10_000 {
        let id = new_id(&mut entropy).expect("an id");
        let text = id.to_string();
        assert_eq!(text.len(), 12, "{text}");
        assert!(text.chars().all(|c| ALPHABET.contains(c)), "{text}");
        // 60 bits in 8 bytes: the top four bits are zero.
        let value = u64::from_be_bytes(*id.as_bytes());
        assert!(value < (1u64 << 60), "{value}");
        or |= value;
        and &= value;
        // The text names the id and nothing else.
        assert_eq!(text.parse::<PublicLinkId>(), Ok(id));
        assert_eq!(PublicLinkId::from_bytes(*id.as_bytes()), Ok(id));
    }
    // Every one of the 60 bits is used (set in some draw, clear in another).
    assert_eq!(or, (1u64 << 60) - 1);
    assert_eq!(and, 0);

    // The text of known values: five bits for each character, the most significant first.
    let id = |value: u64| PublicLinkId::from_bytes(value.to_be_bytes()).expect("below 2^60");
    assert_eq!(id(0).to_string(), "aaaaaaaaaaaa");
    assert_eq!(id(1).to_string(), "aaaaaaaaaaab");
    assert_eq!(id(25).to_string(), "aaaaaaaaaaaz");
    assert_eq!(id(26).to_string(), "aaaaaaaaaaa2");
    assert_eq!(id(31).to_string(), "aaaaaaaaaaa7");
    assert_eq!(id(32).to_string(), "aaaaaaaaaaba");
    assert_eq!(id((1 << 60) - 1).to_string(), "777777777777");
    assert_eq!(id(1 << 59).to_string(), "qaaaaaaaaaaa");
}

#[test]
fn public_id_text_and_bytes_are_strict() {
    // More than 60 bits is no id.
    assert_eq!(
        PublicLinkId::from_bytes([0x10, 0, 0, 0, 0, 0, 0, 0]),
        Err(IdError::OutOfRange)
    );
    assert_eq!(
        PublicLinkId::from_bytes([0xff; 8]),
        Err(IdError::OutOfRange)
    );
    assert!(PublicLinkId::from_bytes([0x0f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]).is_ok());

    // The text: exactly 12 characters of the alphabet.
    for (text, error) in [
        ("", IdError::Length),
        ("abcdefghijk", IdError::Length),
        ("abcdefghijklm", IdError::Length),
        ("ABCDEFGHIJKL", IdError::BadDigit),
        ("abcdefghijkL", IdError::BadDigit),
        ("aaaaaaaaaaa0", IdError::BadDigit),
        ("aaaaaaaaaaa1", IdError::BadDigit),
        ("aaaaaaaaaaa8", IdError::BadDigit),
        ("aaaaaaaaaaa9", IdError::BadDigit),
        ("abcdefghijk=", IdError::BadDigit),
        ("abcdefghij k", IdError::BadDigit),
        ("abcdefghijk-", IdError::BadDigit),
        ("aaaaaaaaaa\u{e9}", IdError::BadDigit),
    ] {
        assert_eq!(text.parse::<PublicLinkId>(), Err(error), "{text:?}");
    }
    assert_eq!(
        "abcdefghijkl"
            .parse::<PublicLinkId>()
            .map(|id| id.to_string()),
        Ok("abcdefghijkl".to_string())
    );
}

#[test]
fn public_id_is_reproducible_for_a_seed() {
    // The same seed gives the same ids, however often it is run.
    let draw = |seed: u64| -> Vec<PublicLinkId> {
        let mut entropy = TestEntropy::new(seed);
        (0..100)
            .map(|_| new_id(&mut entropy).expect("an id"))
            .collect()
    };
    assert_eq!(draw(42), draw(42));
    assert_eq!(draw(7), draw(7));
    assert_ne!(draw(42), draw(7));

    // Known answers (the seeded source is SplitMix64: scratch/W1-S16c1/golden_ids.py computes them
    // from its definition): a change of the id's bit layout or alphabet turns these red.
    let first = |seed: u64| -> Vec<String> {
        let mut entropy = TestEntropy::new(seed);
        (0..3)
            .map(|_| new_id(&mut entropy).expect("an id").to_string())
            .collect()
    };
    assert_eq!(first(42), ["k3xlf4tdfv55", "h4lgwiz6h3zi", "fhypcnlwoush"]);
    assert_eq!(first(7), ["odjslhsods3d", "yzr46tltytae", "ekvrxkaebghg"]);
}

#[test]
fn no_collision_in_100000_seeded_draws() {
    for seed in [1u64, 2, 3] {
        let mut entropy = TestEntropy::new(seed);
        let mut seen = HashSet::new();
        for draw in 0..100_000 {
            let id = new_id(&mut entropy).expect("an id");
            assert!(seen.insert(id), "seed {seed}: draw {draw} repeated {id}");
        }
        assert_eq!(seen.len(), 100_000);
    }
}

#[test]
fn a_failed_draw_is_an_error_never_an_id() {
    assert_eq!(new_id(&mut DeadEntropy), Err(EntropyError::Exhausted));
}

#[test]
fn a_repeating_source_repeats_the_id_and_the_caller_can_see_it() {
    // A broken source gives the same id twice: the node's unique index (and `rotate`'s SameId) are
    // what catches it; the generator itself cannot know.
    let mut broken = RepeatingEntropy(0xff);
    let first = new_id(&mut broken).expect("an id");
    let second = new_id(&mut broken).expect("an id");
    assert_eq!(first, second);
    assert_eq!(first.to_string(), "777777777777");
}

#[test]
fn slug_shape_and_never_a_link_id_shape() {
    // Good slugs: lower-case letters, digits and hyphens, 3 to 64 of them.
    let long = "x".repeat(SLUG_MAX_LEN);
    for good in [
        "abc",
        "quarterly-report",
        "a1-b2-c3",
        "photos2026",
        "abcdefghijk",
        "abcdefghijklm",
        "abcdefghij1l",
        "0189",
        long.as_str(),
    ] {
        let slug = Slug::parse(good).unwrap_or_else(|e| panic!("{good:?}: {e}"));
        assert_eq!(slug.as_str(), good);
        assert_eq!(slug.to_string(), good);
        assert_eq!(good.parse::<Slug>(), Ok(slug));
    }

    // The refusals, each with its own word.
    let too_long = "x".repeat(SLUG_MAX_LEN + 1);
    for (text, error) in [
        ("", SlugError::Length),
        ("a", SlugError::Length),
        ("ab", SlugError::Length),
        (too_long.as_str(), SlugError::Length),
        ("Quarterly", SlugError::BadCharacter),
        ("quarterly report", SlugError::BadCharacter),
        ("quarterly_report", SlugError::BadCharacter),
        ("a/b", SlugError::BadCharacter),
        ("a.b", SlugError::BadCharacter),
        ("a%20b", SlugError::BadCharacter),
        ("\u{fc}nicode", SlugError::BadCharacter),
        ("-abc", SlugError::HyphenAtEdge),
        ("abc-", SlugError::HyphenAtEdge),
        ("a--b", SlugError::DoubleHyphen),
        ("xn--abc", SlugError::DoubleHyphen),
        // The shape of an id: 12 characters of a-z and 2-7.
        ("abcdefghijkl", SlugError::LooksLikeLinkId),
        ("aaaaaaaaaaaa", SlugError::LooksLikeLinkId),
        ("234567234567", SlugError::LooksLikeLinkId),
        ("zzzzzzzzzzzz", SlugError::LooksLikeLinkId),
    ] {
        assert_eq!(Slug::parse(text), Err(error), "{text:?}");
    }
    assert_eq!(SLUG_MIN_LEN, 3);
    assert_eq!(SLUG_MAX_LEN, 64);

    // The shape test itself.
    assert!(has_link_id_shape("abcdefghijkl"));
    assert!(has_link_id_shape("777777777777"));
    assert!(!has_link_id_shape("abcdefghijk"));
    assert!(!has_link_id_shape("abcdefghijklm"));
    assert!(!has_link_id_shape("abcdefghijk1"));
    assert!(!has_link_id_shape("abcdefghijkL"));
    assert!(!has_link_id_shape("abcdefghij-l"));
    assert!(!has_link_id_shape(""));

    // No id of any draw is a slug: the URL `/l/<x>` is an id or a slug, never both.
    let mut entropy = TestEntropy::new(5);
    for _ in 0..2_000 {
        let id = new_id(&mut entropy).expect("an id");
        assert!(has_link_id_shape(&id.to_string()));
        assert_eq!(
            Slug::parse(&id.to_string()),
            Err(SlugError::LooksLikeLinkId),
            "{id}"
        );
    }
}
