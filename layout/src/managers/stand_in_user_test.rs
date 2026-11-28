use super::*;

#[test]
fn a_new_user_has_nothing_set_up() {
    let user = StandInUser::new();
    assert!(!user.enrolled());
    assert!(!user.passcode_set());
    assert_eq!(user.epoch(), 0);
    assert_eq!(user.kind(), BiometricKind::NotAvailable);
    assert!(user.meets(KeyringProtection::DeviceUnlocked));
    assert!(!user.meets(KeyringProtection::UserPresence));
    assert!(!user.meets(KeyringProtection::Biometric));
    // The stand-in's standalone gate answers "unavailable", as it always did.
    assert_eq!(user.gate(false), BiometricResult::Unavailable);
    assert_eq!(user.gate(true), BiometricResult::Unavailable);
}

#[test]
fn a_clone_is_the_same_user() {
    let user = StandInUser::new();
    let other = user.clone();
    other.enroll(true, true);
    assert!(user.enrolled() && user.passcode_set());
    user.script([ScriptedOutcome::Cancel]);
    assert_eq!(other.pending_outcomes(), 1);
    // Two users are equal whatever they hold (a manager derives PartialEq without them).
    assert_eq!(user, StandInUser::new());
}

#[test]
fn the_levels_need_what_the_device_has_enrolled() {
    let user = StandInUser::new();
    user.enroll(false, true);
    assert!(user.meets(KeyringProtection::UserPresence), "the passcode meets it");
    assert!(!user.meets(KeyringProtection::Biometric), "a passcode is no biometric");
    user.enroll(true, false);
    assert!(user.meets(KeyringProtection::UserPresence), "a biometric meets it too");
    assert!(user.meets(KeyringProtection::Biometric));
    assert_eq!(user.kind(), BiometricKind::Fingerprint);
}

#[test]
fn an_enrolment_change_moves_the_epoch_and_a_new_biometric_is_one() {
    let user = StandInUser::new();
    assert_eq!(user.epoch(), 0);
    user.enroll(true, true);
    assert_eq!(user.epoch(), 1, "a biometric that was not there is a change of the set");
    user.enroll(true, false);
    assert_eq!(user.epoch(), 1, "the passcode alone is no change of the biometrics");
    user.enrollment_changed();
    assert_eq!(user.epoch(), 2);
    user.enroll(false, true);
    assert_eq!(user.epoch(), 3, "removing the biometric is one too");
}

#[test]
fn the_script_runs_in_order_and_a_user_matches_when_it_is_used_up() {
    let user = StandInUser::new();
    user.enroll(true, true);
    user.script([
        ScriptedOutcome::Mismatch,
        ScriptedOutcome::Cancel,
        ScriptedOutcome::Lockout,
        ScriptedOutcome::Match,
    ]);
    assert_eq!(user.check(KeyringProtection::Biometric), SheetOutcome::Failed);
    assert_eq!(user.check(KeyringProtection::Biometric), SheetOutcome::Cancelled);
    assert_eq!(user.check(KeyringProtection::Biometric), SheetOutcome::LockedOut);
    assert_eq!(user.check(KeyringProtection::Biometric), SheetOutcome::Passed);
    assert_eq!(user.pending_outcomes(), 0);
    assert_eq!(user.check(KeyringProtection::Biometric), SheetOutcome::Passed, "used up: a match");
}

#[test]
fn a_sheet_that_cannot_be_met_uses_nothing_of_the_script() {
    let user = StandInUser::new();
    user.script([ScriptedOutcome::Match]);
    assert_eq!(user.check(KeyringProtection::Biometric), SheetOutcome::NotEnrolled);
    assert_eq!(user.pending_outcomes(), 1);
}

#[test]
fn the_passcode_path_exists_on_a_user_presence_sheet_only() {
    let user = StandInUser::new();
    user.enroll(true, true);
    user.script([ScriptedOutcome::Passcode, ScriptedOutcome::Passcode]);
    assert_eq!(user.check(KeyringProtection::UserPresence), SheetOutcome::Passed);
    assert_eq!(
        user.check(KeyringProtection::Biometric),
        SheetOutcome::Cancelled,
        "a biometric-only sheet has no passcode: the user can only cancel"
    );
}

#[test]
fn the_standalone_gate_follows_the_script() {
    let user = StandInUser::new();
    user.enroll(true, true);
    user.script([
        ScriptedOutcome::Match,
        ScriptedOutcome::Passcode,
        ScriptedOutcome::Passcode,
        ScriptedOutcome::Mismatch,
        ScriptedOutcome::Cancel,
        ScriptedOutcome::Lockout,
        ScriptedOutcome::NotEnrolled,
    ]);
    assert_eq!(user.gate(false), BiometricResult::Authenticated);
    assert_eq!(user.gate(true), BiometricResult::FellBackToPasscode);
    assert_eq!(user.gate(false), BiometricResult::Cancelled, "no passcode path without the fallback");
    assert_eq!(user.gate(true), BiometricResult::Failed);
    assert_eq!(user.gate(true), BiometricResult::Cancelled);
    assert_eq!(user.gate(true), BiometricResult::LockedOut);
    assert_eq!(user.gate(true), BiometricResult::NotEnrolled);
}

#[test]
fn a_device_with_only_a_passcode_passes_the_gate_through_the_passcode() {
    let user = StandInUser::new();
    user.enroll(false, true);
    assert_eq!(user.gate(false), BiometricResult::Unavailable, "no biometric, no fallback asked");
    assert_eq!(user.gate(true), BiometricResult::FellBackToPasscode);
}

#[test]
fn a_script_is_cut_at_the_cap() {
    let user = StandInUser::new();
    user.script(core::iter::repeat_n(ScriptedOutcome::Cancel, MAX_SCRIPT * 3));
    assert_eq!(user.pending_outcomes(), MAX_SCRIPT);
    user.script([]);
    assert_eq!(user.pending_outcomes(), 0);
}

#[test]
fn the_outcomes_have_names_and_the_names_read_back() {
    for outcome in ScriptedOutcome::ALL {
        assert_eq!(ScriptedOutcome::from_name(outcome.name()), Some(outcome));
    }
    assert_eq!(ScriptedOutcome::from_name(" Not-Enrolled "), Some(ScriptedOutcome::NotEnrolled));
    assert_eq!(ScriptedOutcome::from_name("MATCH"), Some(ScriptedOutcome::Match));
    assert_eq!(ScriptedOutcome::from_name("thumb"), None);
    assert_eq!(ScriptedOutcome::from_name(""), None);
}
