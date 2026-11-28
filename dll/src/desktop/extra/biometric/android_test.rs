//! The codes of the Java biometric helper and what they mean to the engine. No device.

use super::*;

/// One cause per code (the contract of `AzulBiometric.mapError`): the user closing the sheet is Cancelled, never a fall back
/// to the passcode; too many tries are LockedOut; nothing enrolled is NotEnrolled; a code nobody sends is an Error.
///
/// Mutation: 6 and 7 mapped to `Error` (the state before the model: a lockout and an empty enrolment were both "Error").
#[test]
fn every_code_of_the_java_helper_names_one_result() {
    assert_eq!(map_result(0), BiometricResult::Authenticated);
    assert_eq!(map_result(1), BiometricResult::Failed);
    assert_eq!(map_result(2), BiometricResult::Cancelled);
    assert_eq!(map_result(3), BiometricResult::FellBackToPasscode);
    assert_eq!(map_result(4), BiometricResult::Unavailable);
    assert_eq!(map_result(5), BiometricResult::Error);
    assert_eq!(map_result(6), BiometricResult::LockedOut);
    assert_eq!(map_result(7), BiometricResult::NotEnrolled);
    for code in [-1, 8, 99] {
        assert_eq!(map_result(code), BiometricResult::Error, "code {code}");
    }
}

/// The sensor kinds `canAuthenticate` answers (the Java side reads them from the package manager's features).
#[test]
fn the_sensor_kinds_follow_the_java_helper() {
    assert_eq!(map_kind(1), BiometricKind::Fingerprint);
    assert_eq!(map_kind(2), BiometricKind::Face);
    assert_eq!(map_kind(3), BiometricKind::Iris);
    for code in [0, -1, 4] {
        assert_eq!(map_kind(code), BiometricKind::NotAvailable, "code {code}");
    }
}
