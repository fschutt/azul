//! The shared vocabulary: re-exports from azlin-seams (one definition), `EntitlementState` and the
//! closed set of refusal words.

use std::collections::BTreeSet;

use azlin_regbase::{
    AccountKind, AccountRole, EntitlementState, MemberKind, PolicyEpoch, RefusalWord, UnknownWord,
};

/// `AccountKind`, `AccountRole`, `MemberKind` and `PolicyEpoch` are azlin-seams' types, not copies
/// (COMMON-W1R R3b): a value of the seams type is a value of the regbase type.
#[test]
fn s9a_vocabulary_is_defined_once_in_azlin_seams() {
    let kind: azlin_seams::vocab::AccountKind = AccountKind::Business;
    let role: azlin_seams::vocab::AccountRole = AccountRole::GroupAdmin;
    let member: azlin_seams::vocab::MemberKind = MemberKind::App;
    let epoch: azlin_seams::vocab::PolicyEpoch = PolicyEpoch::new(7);
    assert_eq!(kind.as_str(), "business");
    assert_eq!(role.as_str(), "group_admin");
    assert_eq!(member.as_str(), "app");
    assert_eq!(epoch.next(), Some(PolicyEpoch::new(8)));
    // the types are azlin-keys' too (ids): one definition for the whole engine
    let _: azlin_keys::AccountId = azlin_regbase::AccountId::from_bytes([1; 16]);
    let _: azlin_keys::GroupId = azlin_regbase::UserGroupId::from_bytes([2; 16]);
    let _: azlin_keys::VerifyKey = azlin_regbase::PublicKey32::from_bytes([3; 32]);
    let _: azlin_keys::Signature = azlin_regbase::Signature64::from_bytes([4; 64]);
    let _: azlin_seams::Stamp = azlin_regbase::LeaderTime::from_secs(5);
}

#[test]
fn s9a_entitlement_state_is_active_or_lapsed_and_strict() {
    assert_eq!(
        EntitlementState::ALL,
        [EntitlementState::Active, EntitlementState::Lapsed]
    );
    assert_eq!(EntitlementState::Active.as_str(), "active");
    assert_eq!(EntitlementState::Lapsed.as_str(), "lapsed");
    assert_eq!(
        "active".parse::<EntitlementState>(),
        Ok(EntitlementState::Active)
    );
    assert_eq!(
        "lapsed".parse::<EntitlementState>(),
        Ok(EntitlementState::Lapsed)
    );
    // the old draft's words, other case, whitespace and the empty text are unknown, never a default
    for bad in [
        "past_due",
        "cancelled",
        "Active",
        "LAPSED",
        " active",
        "active ",
        "",
        "free",
    ] {
        assert_eq!(
            bad.parse::<EntitlementState>(),
            Err(UnknownWord {
                vocabulary: "entitlement state"
            }),
            "{bad:?}"
        );
    }
    assert_eq!(EntitlementState::Active.to_string(), "active");
}

/// The words of S9b's `RefreshRefusal` (its brief, section 4): exactly these eleven, lower-case
/// snake case, unique, and each round-trips.
#[test]
fn s9a_refusal_words_are_the_eleven_words_of_the_refresh_refusals() {
    let listed = [
        "unauthorized",
        "token_reuse",
        "credentials_revoked",
        "device_pending",
        "account_locked",
        "account_banned",
        "client_too_old",
        "sso_required",
        "sso_stale",
        "app_expired",
        "app_not_active",
    ];
    let words: Vec<&str> = RefusalWord::ALL.iter().map(|w| w.as_str()).collect();
    assert_eq!(words, listed);
    let unique: BTreeSet<&str> = words.iter().copied().collect();
    assert_eq!(unique.len(), RefusalWord::ALL.len());
    for word in RefusalWord::ALL {
        let text = word.as_str();
        assert!(!text.is_empty() && text.is_ascii());
        assert!(
            text.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'),
            "{text}"
        );
        assert_eq!(text.parse::<RefusalWord>(), Ok(*word));
        assert_eq!(word.to_string(), text);
    }
    for bad in [
        "banned",
        "archived",
        "locked",
        "TOKEN_REUSE",
        "token reuse",
        "",
        "token_reuse ",
    ] {
        assert_eq!(
            bad.parse::<RefusalWord>(),
            Err(UnknownWord {
                vocabulary: "refusal word"
            }),
            "{bad:?}"
        );
    }
}
