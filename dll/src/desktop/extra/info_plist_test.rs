use super::*;

/// A bundle without the purpose string must not ask; a bundle with it, and a bare binary (the
/// system attributes the request to the terminal), may.
#[test]
fn only_a_bundle_without_the_purpose_string_is_stopped_from_asking() {
    assert!(usage_key_blocks_request(true, false), "bundled, no key: the system would stop it");
    assert!(!usage_key_blocks_request(true, true), "bundled, with the key: ask");
    assert!(!usage_key_blocks_request(false, false), "a bare binary has no Info.plist to ask of");
    assert!(!usage_key_blocks_request(false, true));
}

/// The diagnostic names what was not requested and the key to add: never a silent no-op.
#[test]
fn the_diagnostic_names_the_missing_key() {
    let message = missing_key_message("The step counter", keys::MOTION);
    assert!(message.contains("The step counter was not requested"), "{message}");
    assert!(message.contains("NSMotionUsageDescription"), "{message}");
    assert!(message.contains("AZ_IOS_USAGE"), "the way the iOS build takes it: {message}");
}

/// The three keys are Apple's spelling (a typo here would let the check pass for the wrong key).
#[test]
fn the_keys_are_the_documented_purpose_strings() {
    assert_eq!(keys::MOTION, "NSMotionUsageDescription");
    assert_eq!(keys::CAMERA, "NSCameraUsageDescription");
    assert_eq!(keys::MICROPHONE, "NSMicrophoneUsageDescription");
}
