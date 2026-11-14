//! The four shared vocabularies of `azlin_seams::vocab` (COMMON-W1R R3b, conflict C-04): strict
//! words, `Copy`/`Ord`/`Hash`, and the checked `PolicyEpoch`.
//!
//! Implements planning/architecture.md section 5.3 (`reg_account.kind`, `reg_member.kind`,
//! `reg_member.role`) and section 5.11 (`policy_epoch`).

use std::collections::BTreeSet;
use std::fmt::Debug;
use std::hash::Hash;
use std::str::FromStr;

use azlin_seams::{AccountKind, AccountRole, IdError, MemberKind, PolicyEpoch, UnknownWord};

/// The traits every enum of the vocabulary carries, checked at compile time.
fn assert_traits<T: Copy + Ord + Hash + Debug + Eq + FromStr<Err = UnknownWord> + 'static>() {}

/// Checks one vocabulary: the words, the round trip, the rejections, the order.
fn check<T>(all: &[T], words: &[&str], what: &str, others: &[&str])
where
    T: Copy + Ord + Debug + FromStr<Err = UnknownWord> + std::fmt::Display + AsWord,
{
    assert_eq!(all.len(), words.len(), "{what}: the number of values");
    let mut seen = BTreeSet::new();
    for (value, word) in all.iter().zip(words) {
        assert_eq!(value.word(), *word, "{what}: the word of {value:?}");
        assert_eq!(value.to_string(), *word, "{what}: Display is the word");
        assert_eq!(word.parse::<T>(), Ok(*value), "{what}: {word}");
        assert!(seen.insert(*word), "{what}: {word} twice");
        // Lower-case snake case: lowercase ASCII letters and single underscores inside.
        assert!(
            word.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
                && !word.starts_with('_')
                && !word.ends_with('_')
                && !word.contains("__"),
            "{what}: {word} is not lower-case snake case"
        );
    }
    // The order is the declaration order, and `ALL` is that order.
    let mut sorted = all.to_vec();
    sorted.sort();
    assert_eq!(sorted, all, "{what}: Ord is the declaration order");

    // Unknown words are refused: another case, padding, another separator, another vocabulary.
    let mut bad: Vec<String> = vec![
        String::new(),
        " ".into(),
        "unknown".into(),
        "none".into(),
        "default".into(),
        "\0".into(),
    ];
    for word in words {
        bad.push(word.to_uppercase());
        bad.push(format!(" {word}"));
        bad.push(format!("{word} "));
        bad.push(format!("{word}\n"));
        bad.push(format!("{word}\0"));
        bad.push(word.replace('_', "-"));
        bad.push(word.replace('_', ""));
        bad.push(format!("{word}s"));
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            bad.push(format!("{}{}", first.to_uppercase(), chars.as_str()));
        }
        bad.push(word[..word.len() - 1].to_string());
    }
    for other in others {
        bad.push((*other).to_string());
    }
    // A variation that happens to be a word again (a word with no underscore, `replace`d) is no
    // bad input.
    bad.retain(|text| !words.contains(&text.as_str()));
    for text in &bad {
        match text.parse::<T>() {
            Err(error) => assert_eq!(error.vocabulary, what, "{text:?}"),
            Ok(value) => panic!("{what}: {text:?} was accepted as {value:?}"),
        }
    }
}

/// `as_str` as a trait, so that `check` is one function for the three enums.
trait AsWord {
    fn word(&self) -> &'static str;
}
impl AsWord for AccountKind {
    fn word(&self) -> &'static str {
        self.as_str()
    }
}
impl AsWord for AccountRole {
    fn word(&self) -> &'static str {
        self.as_str()
    }
}
impl AsWord for MemberKind {
    fn word(&self) -> &'static str {
        self.as_str()
    }
}

#[test]
fn vocab_words_round_trip_and_reject_unknown() {
    assert_traits::<AccountKind>();
    assert_traits::<AccountRole>();
    assert_traits::<MemberKind>();

    // The words of the registry's columns (planning/architecture.md section 5.3), pinned.
    check(
        AccountKind::ALL,
        &["personal", "family", "team", "business"],
        "account kind",
        &["owner", "person", "app"],
    );
    check(
        AccountRole::ALL,
        &[
            "owner",
            "admin",
            "group_admin",
            "helpdesk",
            "member",
            "guest",
        ],
        "account role",
        &["personal", "person", "root", "administrator", "groupadmin"],
    );
    check(
        MemberKind::ALL,
        &["person", "app"],
        "member kind",
        &["owner", "team", "bot", "application"],
    );

    // The two kinds of `person` and `member` are different vocabularies: `member` is a role.
    assert!("member".parse::<MemberKind>().is_err());
    assert!("person".parse::<AccountRole>().is_err());

    // The variants are the ones of the plan, by name.
    assert_eq!(
        AccountKind::ALL,
        &[
            AccountKind::Personal,
            AccountKind::Family,
            AccountKind::Team,
            AccountKind::Business
        ]
    );
    assert_eq!(
        AccountRole::ALL,
        &[
            AccountRole::Owner,
            AccountRole::Admin,
            AccountRole::GroupAdmin,
            AccountRole::Helpdesk,
            AccountRole::Member,
            AccountRole::Guest
        ]
    );
    assert_eq!(MemberKind::ALL, &[MemberKind::Person, MemberKind::App]);

    // The error names the vocabulary and prints without echoing the input.
    let error = "bogus".parse::<AccountRole>().expect_err("not a word");
    assert_eq!(
        error,
        UnknownWord {
            vocabulary: "account role"
        }
    );
    assert_eq!(
        error.to_string(),
        "not a word of the account role vocabulary"
    );
}

#[test]
fn vocab_values_are_usable_as_map_keys() {
    // `Copy + Ord + Hash`: sorted and de-duplicated by a set, hashed by a map.
    let mut set = BTreeSet::new();
    for role in AccountRole::ALL.iter().rev() {
        set.insert(*role);
        set.insert(*role);
    }
    assert_eq!(set.len(), 6);
    assert_eq!(
        set.iter().copied().collect::<Vec<_>>(),
        AccountRole::ALL.to_vec()
    );
    let mut map = std::collections::HashMap::new();
    for kind in AccountKind::ALL {
        map.insert(*kind, kind.as_str());
    }
    assert_eq!(map[&AccountKind::Team], "team");
}

#[test]
fn policy_epoch_counts_with_a_checked_next_and_a_strict_decimal_text() {
    assert_eq!(PolicyEpoch::ZERO.get(), 0);
    assert_eq!(PolicyEpoch::new(7).get(), 7);
    assert_eq!(PolicyEpoch::ZERO.next(), Some(PolicyEpoch::new(1)));
    assert_eq!(PolicyEpoch::new(41).next(), Some(PolicyEpoch::new(42)));
    assert_eq!(
        PolicyEpoch::new(u32::MAX - 1).next(),
        Some(PolicyEpoch::new(u32::MAX))
    );
    assert_eq!(
        PolicyEpoch::new(u32::MAX).next(),
        None,
        "a counter never wraps"
    );
    assert!(PolicyEpoch::new(1) < PolicyEpoch::new(2));
    assert!(PolicyEpoch::new(2) > PolicyEpoch::ZERO);
    assert_eq!(PolicyEpoch::new(5), PolicyEpoch::new(5));

    // Decimal text, and strict parsing.
    assert_eq!(PolicyEpoch::new(0).to_string(), "0");
    assert_eq!(PolicyEpoch::new(4_294_967_295).to_string(), "4294967295");
    assert_eq!(format!("{:?}", PolicyEpoch::new(9)), "PolicyEpoch(9)");
    for number in [0u32, 1, 9, 10, 99, 100, 4_294_967_295] {
        let epoch = PolicyEpoch::new(number);
        assert_eq!(epoch.to_string().parse::<PolicyEpoch>(), Ok(epoch));
    }
    assert_eq!("".parse::<PolicyEpoch>(), Err(IdError::Length));
    for text in [
        "01", "00", "+1", "-1", " 1", "1 ", "1e3", "0x10", "1_000", "\u{663}", "\u{ff11}", "a",
    ] {
        assert_eq!(
            text.parse::<PolicyEpoch>(),
            Err(IdError::BadDigit),
            "{text:?}"
        );
    }
    for text in ["4294967296", "99999999999", "18446744073709551616"] {
        assert_eq!(
            text.parse::<PolicyEpoch>(),
            Err(IdError::OutOfRange),
            "{text:?}"
        );
    }
}
