//! The codes the Java helper answers and what they mean to the engine. No device.

use super::*;

/// The numbers are a contract with `AzulKeyring.java` (its header and its constants): each names ONE cause.
///
/// Mutation: 4 and 12 swapped (a protected read answering Denied: the app would say 'refused' instead of 'ask the user').
#[test]
fn every_code_of_the_java_helper_names_one_result() {
    let secret = || Some("s3cret".to_string());
    assert_eq!(result_of(0, None), KeyringResult::Stored);
    assert_eq!(result_of(1, None), KeyringResult::Deleted);
    assert_eq!(result_of(2, secret()), KeyringResult::Retrieved("s3cret".to_string().into()));
    assert_eq!(result_of(3, None), KeyringResult::NotFound);
    assert_eq!(result_of(4, None), KeyringResult::Denied);
    assert_eq!(result_of(5, None), KeyringResult::Unavailable);
    assert_eq!(result_of(6, None), KeyringResult::Error);
    assert_eq!(result_of(7, None), KeyringResult::Cancelled);
    assert_eq!(result_of(8, None), KeyringResult::Failed);
    assert_eq!(result_of(9, None), KeyringResult::LockedOut);
    assert_eq!(result_of(10, None), KeyringResult::NotEnrolled);
    assert_eq!(result_of(11, None), KeyringResult::Invalidated);
    assert_eq!(result_of(12, None), KeyringResult::WouldPrompt);
}

/// A secret that did not arrive, and a code nobody sends, are errors: never a Retrieved of nothing.
#[test]
fn a_retrieved_without_its_secret_and_an_unknown_code_are_errors() {
    assert_eq!(result_of(2, None), KeyringResult::Error);
    for code in [-1, 13, 99] {
        assert_eq!(result_of(code, Some("x".into())), KeyringResult::Error, "code {code}");
    }
}

/// The two probes answer in the order of their enums, 0-based (the contract with `needsPrompt` and `support`).
#[test]
fn the_probe_codes_follow_the_order_of_the_enums() {
    let needs = [
        KeyringNeed::NotFound,
        KeyringNeed::Nothing,
        KeyringNeed::Unlock,
        KeyringNeed::UserPresence,
        KeyringNeed::Biometric,
        KeyringNeed::Invalidated,
        KeyringNeed::Unavailable,
        KeyringNeed::Error,
    ];
    for (code, need) in needs.into_iter().enumerate() {
        assert_eq!(need_of(code as i32), need, "need code {code}");
    }
    assert_eq!(need_of(8), KeyringNeed::Error);
    let supports = [
        KeyringSupport::Hardware,
        KeyringSupport::Library,
        KeyringSupport::NotEnrolled,
        KeyringSupport::Unavailable,
    ];
    for (code, support) in supports.into_iter().enumerate() {
        assert_eq!(support_of(code as i32), support, "support code {code}");
    }
    assert_eq!(support_of(7), KeyringSupport::Unavailable);
}

/// The Java helper takes the level as 0, 1, 2 (`DeviceUnlocked`, `UserPresence`, `Biometric`).
#[test]
fn the_protection_levels_are_zero_one_two() {
    assert_eq!(level_of(KeyringProtection::DeviceUnlocked), 0);
    assert_eq!(level_of(KeyringProtection::UserPresence), 1);
    assert_eq!(level_of(KeyringProtection::Biometric), 2);
}
