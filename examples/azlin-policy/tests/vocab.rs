//! The words of the value types of the policy (planning/architecture.md section 5.10 and section
//! 5.15): each enum has one lower-case spelling for each value, `FromStr` accepts exactly that
//! spelling, and an unknown word is an error and never a default.

use std::fmt::{Debug, Display};
use std::str::FromStr;

use azlin_policy::{
    AudienceClass, AuditLevel, DeviceApproval, ExternalSharing, ManagedDefault, PaddingPolicy,
    QuorumClass, UpdatePolicy,
};
use azlin_seams::UnknownWord;

/// One vocabulary: the words in order, the round trip, the rejections.
fn check<T>(all: &[T], as_str: impl Fn(&T) -> &'static str, words: &[&str], vocabulary: &str)
where
    T: Copy + PartialEq + Debug + Display + FromStr<Err = UnknownWord>,
{
    assert_eq!(all.len(), words.len(), "{vocabulary}: the number of values");
    for (value, word) in all.iter().zip(words) {
        assert_eq!(as_str(value), *word, "{vocabulary}: the word of {value:?}");
        assert_eq!(value.to_string(), *word, "{vocabulary}: Display");
        assert_eq!(word.parse::<T>(), Ok(*value), "{vocabulary}: {word}");
        // Lower case with underscores, nothing else.
        assert!(
            word.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
                && !word.starts_with('_')
                && !word.ends_with('_'),
            "{vocabulary}: {word}"
        );
        // Another case, padding, another separator, a longer word: refused.
        for bad in [
            word.to_uppercase(),
            format!(" {word}"),
            format!("{word} "),
            format!("{word}\n"),
            format!("{word}x"),
            word.replace('_', "-"),
            word.replace('_', " "),
        ] {
            if words.contains(&bad.as_str()) {
                continue;
            }
            match bad.parse::<T>() {
                Err(error) => assert_eq!(error.vocabulary, vocabulary, "{bad:?}"),
                Ok(value) => panic!("{vocabulary}: {bad:?} was accepted as {value:?}"),
            }
        }
    }
    for bad in ["", "unknown", "default", "\0"] {
        assert!(bad.parse::<T>().is_err(), "{vocabulary}: {bad:?}");
    }
    // The values are distinct.
    for (i, a) in all.iter().enumerate() {
        for b in all.iter().skip(i + 1) {
            assert_ne!(a, b);
        }
    }
}

#[test]
fn device_approval_words() {
    check(
        DeviceApproval::ALL,
        |v| v.as_str(),
        &["self", "self_or_quorum"],
        "device approval",
    );
}

#[test]
fn managed_default_words() {
    check(
        ManagedDefault::ALL,
        |v| v.as_str(),
        &["no", "yes"],
        "managed default",
    );
    assert!(!ManagedDefault::No.is_managed());
    assert!(ManagedDefault::Yes.is_managed());
}

#[test]
fn audit_level_words_are_the_list_of_the_plan() {
    // 5.10: "none, admin, access_summary, access_full".
    check(
        AuditLevel::ALL,
        |v| v.as_str(),
        &["none", "admin", "access_summary", "access_full"],
        "audit level",
    );
}

#[test]
fn external_sharing_words() {
    check(
        ExternalSharing::ALL,
        |v| v.as_str(),
        &["allowed", "same_workspace_only"],
        "external sharing",
    );
}

#[test]
fn update_policy_words() {
    check(
        UpdatePolicy::ALL,
        |v| v.as_str(),
        &["auto", "notify", "managed_by_mdm"],
        "update policy",
    );
}

#[test]
fn padding_policy_words() {
    // 5.15 M4: none, buckets (the default) or full.
    check(
        PaddingPolicy::ALL,
        |v| v.as_str(),
        &["none", "buckets", "full"],
        "padding policy",
    );
}

#[test]
fn link_audience_words() {
    // 5.12: anyone, signed_in, members, people, approval.
    check(
        AudienceClass::ALL,
        |v| v.as_str(),
        &["anyone", "signed_in", "members", "people", "approval"],
        "link audience",
    );
}

#[test]
fn quorum_class_words() {
    check(
        QuorumClass::ALL,
        |v| v.as_str(),
        &[
            "admin_change",
            "policy_change",
            "escrow_change",
            "escrow_open",
            "quorum_enrollment",
            "other",
        ],
        "quorum class",
    );
}

#[test]
fn the_shared_enums_are_the_ones_of_azlin_seams() {
    // COMMON-W1R R3b: one definition, imported. The same type under both paths: a function that
    // takes one takes the other.
    fn takes_seams_kind(kind: azlin_seams::AccountKind) -> azlin_policy::AccountKind {
        kind
    }
    fn takes_seams_member(kind: azlin_seams::MemberKind) -> azlin_policy::MemberKind {
        kind
    }
    fn takes_seams_epoch(epoch: azlin_seams::PolicyEpoch) -> azlin_policy::PolicyEpoch {
        epoch
    }
    assert_eq!(
        takes_seams_kind(azlin_seams::AccountKind::Team),
        azlin_policy::AccountKind::Team
    );
    assert_eq!(
        takes_seams_member(azlin_seams::MemberKind::App),
        azlin_policy::MemberKind::App
    );
    assert_eq!(takes_seams_epoch(azlin_seams::PolicyEpoch::new(3)).get(), 3);
}
