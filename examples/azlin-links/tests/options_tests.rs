//! Check 1 (the structure of the strength table), the types of the options: the words of every
//! vocabulary are strict, `LinkOptions::new` turns everything off ("`open_log`: off by default"), a
//! sealed value has a least and a greatest length, and the audience answers the questions the rules
//! ask of it.
//!
//! Implements planning/architecture.md section 5.12 (the options table).

mod common;

use std::collections::BTreeSet;
use std::fmt::Debug;
use std::str::FromStr;

use azlin_links::{
    Audience, Group, LinkEnded, LinkKind, LinkOptions, Notify, OptionName, PausedBy, Rights,
    Sealed, SealedError, Strength, MESSAGE_MAX_LEN, SEALED_MAX_LEN, SEALED_MIN_LEN, STRENGTH,
    TITLE_MAX_LEN,
};
use azlin_policy::{AudienceClass, LINK_OPTION_NAMES};
use azlin_seams::UnknownWord;
use common::{every_audience, groups, people, sealed};

/// Every value of a vocabulary round-trips through its word, and nothing else parses.
fn assert_strict_words<T>(all: &[T], as_str: impl Fn(&T) -> &'static str)
where
    T: Copy + Debug + PartialEq + FromStr<Err = UnknownWord> + std::fmt::Display,
{
    let mut words = BTreeSet::new();
    for value in all {
        let word = as_str(value);
        assert!(words.insert(word), "{word} is two values");
        assert_eq!(word.parse::<T>(), Ok(*value), "{word}");
        assert_eq!(value.to_string(), word);
        // No other case, no padding, no other separator.
        assert!(
            word.to_uppercase().parse::<T>().is_err(),
            "{word} upper case"
        );
        assert!(format!(" {word}").parse::<T>().is_err(), "{word} padded");
        assert!(format!("{word} ").parse::<T>().is_err(), "{word} padded");
        assert!(word.replace('_', "-").parse::<T>().is_err() || !word.contains('_'));
    }
    assert!("".parse::<T>().is_err());
    assert!("nonsense".parse::<T>().is_err());
    assert_eq!(words.len(), all.len());
}

#[test]
fn the_words_of_every_vocabulary_are_strict() {
    assert_strict_words(Rights::ALL, |v| v.as_str());
    assert_strict_words(Notify::ALL, |v| v.as_str());
    assert_strict_words(LinkKind::ALL, |v| v.as_str());
    assert_strict_words(Strength::ALL, |v| v.as_str());
    assert_strict_words(Group::ALL, |v| v.as_str());
    assert_strict_words(OptionName::ALL, |v| v.as_str());
    assert_strict_words(PausedBy::ALL, |v| v.as_str());
    assert_strict_words(LinkEnded::ALL, |v| v.as_str());

    // The words are the ones of the plan and of the contract table.
    let words = |all: &[LinkKind]| all.iter().map(|v| v.as_str()).collect::<Vec<_>>();
    assert_eq!(
        words(LinkKind::ALL),
        [
            "file",
            "selection",
            "folder_snapshot",
            "folder_live",
            "version"
        ]
    );
    assert_eq!(
        Rights::ALL.iter().map(|v| v.as_str()).collect::<Vec<_>>(),
        ["download", "preview_only"]
    );
    assert_eq!(
        Notify::ALL.iter().map(|v| v.as_str()).collect::<Vec<_>>(),
        ["never", "first_open", "every_open"]
    );
    assert_eq!(
        Notify::ALL
            .iter()
            .map(|v| v.doc_phrase())
            .collect::<Vec<_>>(),
        ["never", "the first open", "every open"]
    );
    assert_eq!(
        Strength::ALL.iter().map(|v| v.as_str()).collect::<Vec<_>>(),
        ["key", "server", "app"]
    );
    assert_eq!(
        PausedBy::ALL.iter().map(|v| v.as_str()).collect::<Vec<_>>(),
        ["owner", "reports", "operator"]
    );
    assert_eq!(
        LinkEnded::ALL
            .iter()
            .map(|v| v.as_str())
            .collect::<Vec<_>>(),
        ["revoked", "expired", "rotated"]
    );
}

#[test]
fn the_pause_sources_are_declared_in_the_order_of_strength() {
    assert!(PausedBy::Owner < PausedBy::Reports);
    assert!(PausedBy::Reports < PausedBy::Operator);
}

#[test]
fn the_strength_table_lists_every_option_once_in_the_order_of_the_names() {
    // 24 options, the names of azlin-policy's list, in its order.
    assert_eq!(STRENGTH.len(), 24);
    assert_eq!(OptionName::ALL.len(), 24);
    let table: Vec<&str> = STRENGTH.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(table, LINK_OPTION_NAMES);
    let all: Vec<&str> = OptionName::ALL.iter().map(|name| name.as_str()).collect();
    assert_eq!(all, LINK_OPTION_NAMES);
    for ((name, strengths), expected) in STRENGTH.iter().zip(OptionName::ALL) {
        assert_eq!(name, expected);
        assert_eq!(*strengths, name.strengths());
        assert!(!strengths.is_empty(), "{name}");
        // No strength twice, and the order is key, server, app.
        let mut sorted = strengths.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted, strengths.to_vec(), "{name}");
    }
    // Spot checks of the cells of the plan's table.
    let strengths = |name: OptionName| name.strengths().to_vec();
    assert_eq!(
        strengths(OptionName::Audience),
        [Strength::Key, Strength::Server]
    );
    assert_eq!(
        strengths(OptionName::Password),
        [Strength::Key, Strength::Server]
    );
    assert_eq!(strengths(OptionName::AppOnly), [Strength::App]);
    assert_eq!(strengths(OptionName::Kind), [Strength::Key]);
    assert_eq!(strengths(OptionName::Rights), [Strength::App]);
    assert_eq!(strengths(OptionName::OneTime), [Strength::Server]);
    assert_eq!(
        strengths(OptionName::AllowUpload),
        [Strength::Key, Strength::Server]
    );
    assert_eq!(
        strengths(OptionName::Edit),
        [Strength::Key, Strength::Server]
    );
    assert_eq!(strengths(OptionName::Comment), [Strength::Server]);
    assert_eq!(strengths(OptionName::AllowReshare), [Strength::App]);
    assert_eq!(strengths(OptionName::Watermark), [Strength::App]);
    assert_eq!(strengths(OptionName::Title), [Strength::Key]);
    assert_eq!(strengths(OptionName::Slug), [Strength::Server]);
}

#[test]
fn every_option_belongs_to_the_group_of_the_table() {
    let group = |name: OptionName| name.group().as_str();
    assert_eq!(group(OptionName::Audience), "who");
    assert_eq!(group(OptionName::Password), "who");
    assert_eq!(group(OptionName::AppOnly), "who");
    assert_eq!(group(OptionName::Kind), "what");
    assert_eq!(group(OptionName::Subfolders), "what");
    assert_eq!(group(OptionName::NotBefore), "when");
    assert_eq!(group(OptionName::OneTime), "when");
    assert_eq!(group(OptionName::MaxDevices), "how much");
    assert_eq!(group(OptionName::Rights), "rights");
    assert_eq!(group(OptionName::Watermark), "rights");
    assert_eq!(group(OptionName::Notify), "telling the sharer");
    assert_eq!(group(OptionName::OpenLog), "telling the sharer");
    assert_eq!(group(OptionName::Title), "presentation");
    assert_eq!(group(OptionName::Slug), "presentation");
    // Seven groups, each with an option.
    let used: BTreeSet<Group> = OptionName::ALL.iter().map(|name| name.group()).collect();
    assert_eq!(used.len(), 7);
    assert_eq!(Group::ALL.len(), 7);
}

#[test]
fn link_options_new_turns_everything_off() {
    for audience in every_audience() {
        let o = LinkOptions::new(audience.clone(), LinkKind::Selection);
        assert_eq!(o.audience, audience);
        assert_eq!(o.kind, LinkKind::Selection);
        assert!(!o.password);
        assert!(!o.app_only);
        assert!(!o.subfolders);
        assert_eq!(o.not_before, None);
        assert_eq!(o.expires_at, None);
        assert_eq!(o.expires_after_open, None);
        assert_eq!(o.idle_expiry, None);
        assert!(!o.one_time);
        assert_eq!(o.max_downloads, None);
        assert_eq!(o.bytes_cap, None);
        assert_eq!(o.max_devices, None);
        assert_eq!(o.rights, Rights::Download);
        assert!(!o.allow_upload);
        assert!(!o.edit);
        assert!(!o.comment);
        assert!(!o.allow_reshare);
        assert!(!o.watermark);
        assert_eq!(o.notify, Notify::Never);
        // "`open_log`: off by default".
        assert!(!o.open_log);
        assert_eq!(o.title, None);
        assert_eq!(o.message, None);
        assert_eq!(o.slug, None);
    }
}

#[test]
fn the_strength_the_apps_show_follows_the_chosen_audience() {
    // "key for `anyone`, `people` and `approval`; server for `signed_in` and `members`".
    let expected = [
        (Audience::Anyone, Strength::Key),
        (Audience::SignedIn, Strength::Server),
        (groups(&[1]), Strength::Server),
        (groups(&[]), Strength::Server),
        (people(&[10]), Strength::Key),
        (Audience::Approval, Strength::Key),
    ];
    for (audience, strength) in expected {
        assert_eq!(audience.strength(), strength, "{audience:?}");
        let o = LinkOptions::new(audience.clone(), LinkKind::File);
        assert_eq!(
            o.strengths(OptionName::Audience),
            [strength],
            "{audience:?}"
        );
        // Every other option shows the table's strength.
        for name in OptionName::ALL {
            if *name != OptionName::Audience {
                assert_eq!(o.strengths(*name), name.strengths());
            }
        }
    }
}

#[test]
fn sealed_has_a_least_and_a_greatest_length() {
    assert_eq!(SEALED_MIN_LEN, 28);
    assert_eq!(
        Sealed::new(Vec::new()),
        Err(SealedError::TooShort { len: 0 })
    );
    assert_eq!(
        Sealed::new(vec![1; SEALED_MIN_LEN - 1]),
        Err(SealedError::TooShort {
            len: SEALED_MIN_LEN - 1
        })
    );
    assert_eq!(
        Sealed::new(vec![1; SEALED_MIN_LEN]).map(|s| s.len()),
        Ok(SEALED_MIN_LEN)
    );
    assert_eq!(
        Sealed::new(vec![1; SEALED_MAX_LEN]).map(|s| s.len()),
        Ok(SEALED_MAX_LEN)
    );
    assert_eq!(
        Sealed::new(vec![1; SEALED_MAX_LEN + 1]),
        Err(SealedError::TooLong {
            len: SEALED_MAX_LEN + 1
        })
    );
    // The field caps are below the greatest length (checked when this file is compiled).
    const _: () = assert!(TITLE_MAX_LEN < MESSAGE_MAX_LEN && MESSAGE_MAX_LEN < SEALED_MAX_LEN);
    const _: () = assert!(SEALED_MIN_LEN < TITLE_MAX_LEN);
    // It keeps its bytes and prints its length, never its bytes.
    let value = sealed(0xab, 40);
    assert_eq!(value.as_bytes(), vec![0xab; 40].as_slice());
    assert!(!value.is_empty());
    assert_eq!(format!("{value:?}"), "Sealed(40 bytes)");
}

#[test]
fn the_audience_answers_the_questions_of_the_rules() {
    struct Row {
        audience: Audience,
        class: AudienceClass,
        signed_in: bool,
        named: bool,
        key_wrapped: bool,
        needs_request: bool,
    }
    let rows = [
        Row {
            audience: Audience::Anyone,
            class: AudienceClass::Anyone,
            signed_in: false,
            named: false,
            key_wrapped: false,
            needs_request: false,
        },
        Row {
            audience: Audience::SignedIn,
            class: AudienceClass::SignedIn,
            signed_in: true,
            named: false,
            key_wrapped: false,
            needs_request: false,
        },
        Row {
            audience: groups(&[1, 2]),
            class: AudienceClass::Members,
            signed_in: true,
            named: false,
            key_wrapped: false,
            needs_request: false,
        },
        Row {
            audience: groups(&[]),
            class: AudienceClass::Members,
            signed_in: true,
            named: false,
            key_wrapped: false,
            needs_request: false,
        },
        Row {
            audience: people(&[10, 11]),
            class: AudienceClass::People,
            signed_in: true,
            named: true,
            key_wrapped: true,
            needs_request: false,
        },
        Row {
            audience: Audience::Approval,
            class: AudienceClass::Approval,
            signed_in: true,
            named: false,
            key_wrapped: true,
            needs_request: true,
        },
    ];
    for row in &rows {
        let a = &row.audience;
        assert_eq!(a.class(), row.class, "{a:?}");
        assert_eq!(a.is_signed_in(), row.signed_in, "{a:?}");
        assert_eq!(a.is_named(), row.named, "{a:?}");
        assert_eq!(a.key_is_wrapped(), row.key_wrapped, "{a:?}");
        assert_eq!(a.needs_paired_request(), row.needs_request, "{a:?}");
    }
    // Every class of the policy has an audience.
    let classes: BTreeSet<AudienceClass> = rows.iter().map(|row| row.audience.class()).collect();
    assert_eq!(classes.len(), AudienceClass::ALL.len());
}

#[test]
fn audiences_are_sets_so_two_replicas_hold_the_same_state() {
    // The same people and groups in another order, with a repeat, are the same audience.
    assert_eq!(people(&[11, 10, 11]), people(&[10, 11]));
    assert_eq!(groups(&[2, 1, 2]), groups(&[1, 2]));
    assert_ne!(people(&[10]), people(&[10, 11]));
    assert_ne!(groups(&[1]), groups(&[]));
}

#[test]
fn an_audience_changes_without_a_rotation_only_between_the_server_filters() {
    // A change of the audience is a rotation unless it changes a server filter only: between
    // `signed_in` and `members`, and from one list of groups to another.
    let all = [
        Audience::Anyone,
        Audience::SignedIn,
        groups(&[1, 2]),
        groups(&[3]),
        people(&[10, 11]),
        people(&[12]),
        Audience::Approval,
    ];
    // (from, to) by index in `all`: signed in <-> members, and one list of groups for another.
    let free: [(usize, usize); 6] = [(1, 2), (2, 1), (1, 3), (3, 1), (2, 3), (3, 2)];
    for (i, from) in all.iter().enumerate() {
        for (j, to) in all.iter().enumerate() {
            if i == j {
                // Two equal audiences are no change at all: the caller checks.
                continue;
            }
            let expected = free.contains(&(i, j));
            assert_eq!(
                from.changes_without_rotation(to),
                expected,
                "{from:?} -> {to:?}"
            );
        }
    }
}

#[test]
fn the_kind_answers_the_questions_of_the_rules() {
    let rows = [
        (LinkKind::File, false, false, true),
        (LinkKind::Selection, false, false, true),
        (LinkKind::FolderSnapshot, true, false, true),
        (LinkKind::FolderLive, true, true, false),
        (LinkKind::Version, false, false, true),
    ];
    for (kind, folder, follows, needs_objects) in rows {
        assert_eq!(kind.is_folder(), folder, "{kind}");
        assert_eq!(kind.follows_latest(), follows, "{kind}");
        assert_eq!(kind.needs_objects(), needs_objects, "{kind}");
    }
}
