use super::*;

/// A level is a constraint on the OS's own check, never a library flag: the values are `SecAccessControlCreateFlags`.
#[test]
fn a_level_is_the_constraint_the_os_evaluates() {
    assert_eq!(access_flags(Level::UserPresence), 1, "kSecAccessControlUserPresence");
    assert_eq!(access_flags(Level::Biometric), 8, "kSecAccessControlBiometryCurrentSet (1 << 3)");
    assert_ne!(access_flags(Level::UserPresence), access_flags(Level::Biometric));
}

#[test]
fn the_state_of_the_device_comes_from_the_la_error_of_a_refusal() {
    assert_eq!(device_state(None), DeviceState::Ready);
    assert_eq!(device_state(Some(LA_BIOMETRY_LOCKOUT)), DeviceState::LockedOut);
    assert_eq!(device_state(Some(LA_BIOMETRY_NOT_ENROLLED)), DeviceState::NotEnrolled);
    assert_eq!(device_state(Some(LA_PASSCODE_NOT_SET)), DeviceState::NotEnrolled);
    assert_eq!(device_state(Some(LA_BIOMETRY_NOT_AVAILABLE)), DeviceState::Unavailable);
    assert_eq!(device_state(Some(LA_INVALID_CONTEXT)), DeviceState::Unavailable);
}

// ---- the read after the sheet (`keyring_unlock`) ---------------------------------------------------------------------

#[test]
fn an_unlock_that_worked_is_ok_and_a_dismissed_sheet_is_cancelled() {
    assert_eq!(after_unlock(0, DeviceState::Ready, true), Outcome::Ok);
    assert_eq!(after_unlock(ERR_SEC_USER_CANCELED, DeviceState::Ready, true), Outcome::Cancelled);
}

/// `errSecAuthFailed` is "the biometric did not match": a lockout and a missing enrolment are not the same thing, so
/// the device is asked after the failure.
#[test]
fn a_failed_check_is_failed_unless_the_device_says_locked_out_or_not_enrolled() {
    assert_eq!(after_unlock(ERR_SEC_AUTH_FAILED, DeviceState::Ready, true), Outcome::Failed);
    assert_eq!(after_unlock(ERR_SEC_AUTH_FAILED, DeviceState::LockedOut, true), Outcome::LockedOut);
    assert_eq!(after_unlock(ERR_SEC_AUTH_FAILED, DeviceState::NotEnrolled, true), Outcome::NotEnrolled);
    assert_eq!(after_unlock(ERR_SEC_AUTH_FAILED, DeviceState::Unavailable, true), Outcome::Failed);
}

/// The item of a protected key is deleted by the OS when the enrolled set changes: with its meta record present that
/// is `Invalidated` (store again), without one the key never existed.
#[test]
fn an_item_that_is_gone_while_its_meta_record_stays_is_invalidated() {
    assert_eq!(after_unlock(ERR_SEC_ITEM_NOT_FOUND, DeviceState::Ready, true), Outcome::Invalidated);
    assert_eq!(after_unlock(ERR_SEC_ITEM_NOT_FOUND, DeviceState::Ready, false), Outcome::NotFound);
    assert_eq!(after_plain_read(ERR_SEC_ITEM_NOT_FOUND, true), Outcome::Invalidated);
    assert_eq!(after_plain_read(ERR_SEC_ITEM_NOT_FOUND, false), Outcome::NotFound);
}

#[test]
fn a_sheet_that_was_forbidden_is_would_prompt_not_denied() {
    assert_eq!(after_unlock(ERR_SEC_INTERACTION_NOT_ALLOWED, DeviceState::Ready, true), Outcome::WouldPrompt);
    for status in [ERR_SEC_INTERACTION_NOT_ALLOWED, ERR_SEC_AUTH_FAILED, ERR_SEC_USER_CANCELED] {
        assert_eq!(after_plain_read(status, true), Outcome::WouldPrompt, "{status}: a protected item is not read without the check");
    }
}

/// `Denied` is the SYSTEM refusing this app: it is not what a cancelled sheet, a wrong finger or a quiet read of a
/// protected item turn into (the old `map_err` merged all of them).
#[test]
fn no_status_of_the_user_check_is_denied() {
    for status in [ERR_SEC_USER_CANCELED, ERR_SEC_AUTH_FAILED, ERR_SEC_INTERACTION_NOT_ALLOWED, ERR_SEC_ITEM_NOT_FOUND, ERR_SEC_PARAM] {
        assert_ne!(after_unlock(status, DeviceState::Ready, true), Outcome::Denied, "{status}");
        assert_ne!(after_plain_read(status, true), Outcome::Denied, "{status}");
    }
}

#[test]
fn a_build_that_cannot_bind_items_is_unavailable() {
    assert_eq!(after_unlock(ERR_SEC_MISSING_ENTITLEMENT, DeviceState::Ready, false), Outcome::Unavailable);
    assert_eq!(after_plain_read(ERR_SEC_MISSING_ENTITLEMENT, false), Outcome::Unavailable);
    assert_eq!(after_store(ERR_SEC_MISSING_ENTITLEMENT, true, DeviceState::Ready), Outcome::Unavailable);
    assert_eq!(after_unlock(ERR_SEC_NOT_AVAILABLE, DeviceState::Ready, false), Outcome::Unavailable);
}

#[test]
fn an_unknown_status_is_an_error_whatever_the_device_says() {
    assert_eq!(after_unlock(-9999, DeviceState::Ready, true), Outcome::Error);
    assert_eq!(after_plain_read(-9999, false), Outcome::Error);
    assert_eq!(after_store(-9999, false, DeviceState::Ready), Outcome::Error);
}

// ---- the write ------------------------------------------------------------------------------------------------------

/// Never a silently weaker store: where the device cannot do the check, a protected store is refused.
#[test]
fn a_protected_store_the_device_cannot_bind_is_refused_not_downgraded() {
    assert_eq!(after_store(ERR_SEC_PARAM, true, DeviceState::NotEnrolled), Outcome::NotEnrolled);
    assert_eq!(after_store(ERR_SEC_PARAM, true, DeviceState::Ready), Outcome::NotEnrolled);
    assert_eq!(after_store(ERR_SEC_AUTH_FAILED, true, DeviceState::NotEnrolled), Outcome::NotEnrolled);
    assert_eq!(after_store(ERR_SEC_PARAM, true, DeviceState::Unavailable), Outcome::Unavailable);
    // an unprotected store has no such reading of -50: it is a plain error
    assert_eq!(after_store(ERR_SEC_PARAM, false, DeviceState::Ready), Outcome::Error);
    assert_eq!(after_store(0, true, DeviceState::Ready), Outcome::Ok);
}

// ---- what an app learns before it asks ------------------------------------------------------------------------------

#[test]
fn needs_prompt_reads_the_meta_record_and_the_attributes_never_the_secret() {
    assert_eq!(need_of(None, ItemState::Present), Need::Nothing);
    assert_eq!(need_of(None, ItemState::Absent), Need::NotFound);
    assert_eq!(need_of(Some(Level::UserPresence), ItemState::Present), Need::UserPresence);
    assert_eq!(need_of(Some(Level::Biometric), ItemState::Present), Need::Biometric);
    assert_eq!(need_of(Some(Level::Biometric), ItemState::Absent), Need::Invalidated);
    assert_eq!(need_of(Some(Level::UserPresence), ItemState::Absent), Need::Invalidated);
}

// ---- the gate ("confirm it is you", no secret) ----------------------------------------------------------------------

#[test]
fn the_gate_tells_the_causes_apart() {
    assert_eq!(gate_outcome(LA_USER_CANCEL), GateOutcome::Cancelled);
    assert_eq!(gate_outcome(LA_SYSTEM_CANCEL), GateOutcome::Cancelled);
    assert_eq!(gate_outcome(LA_APP_CANCEL), GateOutcome::Cancelled);
    assert_eq!(gate_outcome(LA_USER_FALLBACK), GateOutcome::FellBackToPasscode);
    assert_eq!(gate_outcome(LA_AUTHENTICATION_FAILED), GateOutcome::Failed);
    assert_eq!(gate_outcome(LA_BIOMETRY_LOCKOUT), GateOutcome::LockedOut);
    assert_eq!(gate_outcome(LA_BIOMETRY_NOT_ENROLLED), GateOutcome::NotEnrolled);
    assert_eq!(gate_outcome(LA_PASSCODE_NOT_SET), GateOutcome::NotEnrolled);
    assert_eq!(gate_outcome(LA_BIOMETRY_NOT_AVAILABLE), GateOutcome::Unavailable);
    assert_eq!(gate_outcome(LA_NOT_INTERACTIVE), GateOutcome::Error);
    assert_eq!(gate_outcome(-424242), GateOutcome::Error);
}

/// The old mapping said Unavailable for "not enrolled" and "no passcode", and Failed for a lockout: an app could not
/// tell a missing enrolment (explain how to set it up) from a failed match (try again) from a lockout (use the passcode).
#[test]
fn a_lockout_and_a_missing_enrolment_are_not_a_failed_match() {
    assert_ne!(gate_outcome(LA_BIOMETRY_LOCKOUT), GateOutcome::Failed);
    assert_ne!(gate_outcome(LA_BIOMETRY_NOT_ENROLLED), GateOutcome::Unavailable);
}

// ---- the meta record ------------------------------------------------------------------------------------------------

#[test]
fn a_meta_record_round_trips_with_and_without_a_stamp() {
    for level in [Level::UserPresence, Level::Biometric] {
        assert_eq!(decode_meta(&encode_meta(level, None)), Some((level, None)));
        assert_eq!(decode_meta(&encode_meta(level, Some("ab12"))), Some((level, Some(String::from("ab12")))));
        assert_eq!(decode_meta(&encode_meta(level, Some(""))), Some((level, None)), "an empty stamp is none");
    }
    assert_eq!(encode_meta(Level::Biometric, Some("ff")), b"AZM1:B:ff".to_vec());
    assert_eq!(encode_meta(Level::UserPresence, None), b"AZM1:U".to_vec());
}

#[test]
fn what_is_no_meta_record_is_not_read_as_one() {
    assert_eq!(decode_meta(b""), None);
    assert_eq!(decode_meta(b"hunter2"), None, "a secret is no record");
    assert_eq!(decode_meta(b"AZM1:X"), None, "an unknown level");
    assert_eq!(decode_meta(b"AZM2:U"), None, "another version");
    assert_eq!(decode_meta(&[0xff, 0xfe, 0xfd]), None);
}

#[test]
fn the_meta_item_sits_in_a_service_of_its_own() {
    assert_eq!(meta_service("com.example.app"), "com.example.app.azul-meta");
    assert_ne!(meta_service("a"), "a", "the meta item is never the item");
}
