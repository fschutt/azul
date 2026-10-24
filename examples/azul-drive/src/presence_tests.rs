//! The presence check before an action that reveals or changes a drive's keys (the gated tier):
//! Touch ID or the computer's password, then the action - only for those actions, never for a
//! drive's sync keys.

use azul::biometric::BiometricResult;

use crate::presence::{decide, Decision, Gated};

/// The user's finger or password lets the action run; a cancel or a failure stops it and says
/// why. A system with no way to check (azul answers Unavailable: no sensor and no password
/// policy, a headless or E2E run) runs it after its own question, as before.
#[test]
fn touch_id_or_the_password_lets_a_gated_action_run_and_a_cancel_stops_it() {
    assert_eq!(decide(BiometricResult::Authenticated), Decision::Run);
    assert_eq!(decide(BiometricResult::FellBackToPasscode), Decision::Run);
    assert_eq!(decide(BiometricResult::Unavailable), Decision::Run);
    assert_eq!(
        decide(BiometricResult::Cancelled),
        Decision::Stop("azdrive-presence-cancelled")
    );
    assert_eq!(decide(BiometricResult::Failed), Decision::Stop("azdrive-presence-failed"));
    assert_eq!(decide(BiometricResult::Error), Decision::Stop("azdrive-presence-failed"));
}

/// Exactly the actions that reveal or change keys are gated - a drive removed, new keys after a
/// hack, another device, another recovery code, a recovery code removed - and each says why in
/// the system's prompt, in both languages.
#[test]
fn every_gated_action_says_why_in_the_prompt() {
    let gated = [
        Gated::Forget,
        Gated::Rotate,
        Gated::AddDevice {
            drive_id: String::from("d"),
        },
        Gated::AddCode {
            drive_id: String::from("d"),
        },
        Gated::RemoveCode {
            drive_id: String::from("d"),
            key_id: String::from("k"),
            label: String::from("Kit"),
        },
    ];
    let ftl = |lang: &str| {
        std::fs::read_to_string(format!("{}/resources/{lang}.ftl", env!("CARGO_MANIFEST_DIR")))
            .unwrap()
    };
    let (en, de) = (ftl("en"), ftl("de"));
    let mut reasons = std::collections::BTreeSet::new();
    for action in &gated {
        let key = action.reason();
        reasons.insert(key);
        for (lang, text) in [("en", &en), ("de", &de)] {
            assert!(
                text.lines().any(|line| line.starts_with(&format!("{key} ="))),
                "{lang}.ftl lacks {key}"
            );
        }
    }
    assert_eq!(reasons.len(), gated.len(), "each action its own reason");
    for key in ["azdrive-presence-cancelled", "azdrive-presence-failed"] {
        assert!(en.contains(&format!("{key} =")) && de.contains(&format!("{key} =")), "{key}");
    }
}
