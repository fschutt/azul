//! The presence check before an action that reveals or changes a drive's keys (the gated tier):
//! Touch ID or the computer's password, then the action - only for those actions, never for a
//! drive's sync keys. A computer with no way to check asks for the drive's name instead; it never
//! lets the action through (AZ-W9, X-AR2-O5).

use azul::biometric::BiometricResult;

use crate::{
    popup_slot::PopupSlot,
    presence::{
        confirm_name, decide, names_match, open_name_check, test_stand_in_in, Decision, Gated,
        Named,
    },
    Popup,
};

/// The user's finger or password lets the action run; a cancel or a failure stops it and says
/// why - in a test run too.
#[test]
fn touch_id_or_the_password_lets_a_gated_action_run_and_a_cancel_stops_it() {
    for test_stand_in in [false, true] {
        assert_eq!(
            decide(BiometricResult::Authenticated, test_stand_in),
            Decision::Run
        );
        assert_eq!(
            decide(BiometricResult::FellBackToPasscode, test_stand_in),
            Decision::Run
        );
        assert_eq!(
            decide(BiometricResult::Cancelled, test_stand_in),
            Decision::Stop("azdrive-presence-cancelled")
        );
        assert_eq!(
            decide(BiometricResult::Failed, test_stand_in),
            Decision::Stop("azdrive-presence-failed")
        );
        assert_eq!(
            decide(BiometricResult::Error, test_stand_in),
            Decision::Stop("azdrive-presence-failed")
        );
    }
}

/// AZ-W9, X-AR2-O5: a system with no way to check (azul answers Unavailable: no fingerprint
/// reader and no password that locks the computer - Windows without Hello, Linux without a
/// fingerprint) does NOT run the action: the old gate opened for exactly the machines it could
/// not check. The name of the drive stands in.
#[test]
fn a_computer_that_cannot_check_asks_for_the_drives_name_and_never_runs_the_action() {
    assert_eq!(
        decide(BiometricResult::Unavailable, false),
        Decision::Unchecked
    );
}

/// Only a TEST run - whose libazul never shows a real prompt and answers Unavailable for it -
/// runs the action at once.
#[test]
fn only_a_test_run_runs_the_action_of_an_unavailable_sensor() {
    assert_eq!(decide(BiometricResult::Unavailable, true), Decision::Run);
}

/// What makes a run a test run: the cargo feature `test-hooks` AND a headless backend or a
/// scenario file. A shipped AzDrive is never one, whatever its environment says.
#[test]
fn a_run_is_a_test_run_only_in_a_test_build_and_only_when_headless_or_scripted() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_string())
        }
    };
    let headless = env(&[("AZ_BACKEND", "headless")]);
    let scenario = env(&[("AZ_E2E_TEST", "/tmp/scenario.json")]);
    let empty_scenario = env(&[("AZ_E2E_TEST", "")]);
    let windowed = env(&[("AZ_BACKEND", "metal")]);
    let nothing = env(&[]);

    assert!(test_stand_in_in(true, &headless));
    assert!(test_stand_in_in(true, &scenario));
    assert!(
        test_stand_in_in(true, &empty_scenario),
        "any value, as libazul's hooks"
    );
    assert!(!test_stand_in_in(true, &windowed));
    assert!(!test_stand_in_in(true, &nothing));
    // The same environments in a shipped build: none of them is a test run.
    for var in [&headless, &scenario, &empty_scenario] {
        assert!(
            !test_stand_in_in(false, var),
            "a shipped build ran as a test"
        );
    }
}

/// The right name: the words of the drive's name, spaces around them aside, exactly - and a
/// blank is never the name of a drive with none.
#[test]
fn the_name_is_the_drives_exactly_and_a_blank_is_never_a_name() {
    assert!(names_match("Photos", "Photos"));
    assert!(names_match("  Photos \t", "Photos"));
    assert!(names_match("Photos", " Photos "));
    assert!(
        !names_match("photos", "Photos"),
        "the case is part of the name"
    );
    assert!(!names_match("Photo", "Photos"));
    assert!(!names_match("Photos 2", "Photos"));
    assert!(!names_match("", "Photos"));
    assert!(
        !names_match("", ""),
        "a drive with no name is confirmed by nothing"
    );
    assert!(!names_match("  ", "  "));
}

/// The name of a drive by its id, for the tests.
fn name_of(id: &str) -> String {
    match id {
        "d1" => String::from("Photos"),
        other => other.to_string(),
    }
}

fn type_name(slot: &mut PopupSlot, text: &str) {
    let Some(Popup::Presence(check)) = slot.as_mut() else {
        panic!("no name check is open");
    };
    check.typed = text.to_string();
}

fn shows_name_check(slot: &PopupSlot) -> bool {
    matches!(slot.as_ref(), Some(Popup::Presence(_)))
}

/// "Remove drive" on a computer with no sensor: the name check opens over the question, a wrong
/// name changes nothing and says so, the right name brings the question back and runs the
/// action - which finds its question as it left it.
#[test]
fn removing_a_drive_without_a_sensor_waits_for_the_drives_name() {
    let mut slot = PopupSlot::default();
    slot.open(Popup::ConfirmForget {
        drive_id: String::from("d1"),
    });

    assert!(open_name_check(&mut slot, Gated::Forget));
    assert!(shows_name_check(&slot), "the name is asked");

    // Nothing typed, then the wrong name.
    assert_eq!(confirm_name(&mut slot, &name_of), Named::Wrong);
    assert!(shows_name_check(&slot), "the question stays");
    type_name(&mut slot, "Videos");
    assert_eq!(confirm_name(&mut slot, &name_of), Named::Wrong);
    let Some(Popup::Presence(check)) = slot.as_ref() else {
        panic!("the name check closed on a wrong name");
    };
    assert!(
        !check.error.is_empty(),
        "the dialog says it is not the name"
    );

    // The right name (spaces around it do not matter).
    type_name(&mut slot, " Photos ");
    assert_eq!(
        confirm_name(&mut slot, &name_of),
        Named::Right(Gated::Forget)
    );
    assert!(
        matches!(slot.as_ref(), Some(Popup::ConfirmForget { drive_id }) if drive_id == "d1"),
        "the action's own question is back, as forget_drive finds it"
    );
}

/// The actions that carry their drive (another device, another recovery code, a recovery code
/// removed) have no question of their own to bring back: after the name the slot is empty.
#[test]
fn an_action_that_carries_its_drive_asks_the_name_over_nothing() {
    for action in [
        Gated::AddDevice {
            drive_id: String::from("d1"),
        },
        Gated::AddCode {
            drive_id: String::from("d1"),
        },
        Gated::RemoveCode {
            drive_id: String::from("d1"),
            key_id: String::from("k"),
            label: String::from("Kit"),
        },
    ] {
        let mut slot = PopupSlot::default();
        assert!(open_name_check(&mut slot, action.clone()), "{action:?}");
        type_name(&mut slot, "Photos");
        assert_eq!(confirm_name(&mut slot, &name_of), Named::Right(action));
        assert!(slot.is_none(), "nothing was behind the name check");
    }
}

/// "I was hacked: new keys" must stay possible on every computer: without a sensor the name
/// check opens over the dialog with the typed recovery code, and gives it back untouched.
#[cfg(feature = "encryption")]
#[test]
fn i_was_hacked_without_a_sensor_asks_the_name_and_keeps_the_code_that_was_typed() {
    use azul_storage::crypto::Zeroizing;

    use crate::encryption::Dialog;

    let mut slot = PopupSlot::default();
    slot.open(Popup::Encryption(Dialog::ConfirmRotate {
        drive_id: String::from("d1"),
        typed: Zeroizing::new(String::from("ABCDE-FGHIJ-KLMNO-PQRST-UVWXYZ")),
        error: String::new(),
    }));

    assert!(open_name_check(&mut slot, Gated::Rotate));
    assert!(shows_name_check(&slot));
    type_name(&mut slot, "Photos");
    assert_eq!(
        confirm_name(&mut slot, &name_of),
        Named::Right(Gated::Rotate)
    );
    match slot.as_ref() {
        Some(Popup::Encryption(Dialog::ConfirmRotate {
            drive_id, typed, ..
        })) => {
            assert_eq!(drive_id, "d1");
            assert_eq!(typed.as_str(), "ABCDE-FGHIJ-KLMNO-PQRST-UVWXYZ");
        }
        _ => panic!("rotate_confirmed would find no ConfirmRotate dialog"),
    }
}

/// The name check is asked only where the drive is known and nothing that must be seen is in the
/// way: a "Remove drive" with no question open names no drive, and a recovery sheet showing is not
/// replaced. Then nothing changes and the caller says so.
#[test]
fn the_name_check_is_not_opened_without_a_drive_or_over_a_dialog_that_must_be_seen() {
    let mut empty = PopupSlot::default();
    assert!(
        !open_name_check(&mut empty, Gated::Forget),
        "no question, no drive"
    );
    assert!(empty.is_none());
    #[cfg(feature = "encryption")]
    assert!(
        !open_name_check(&mut empty, Gated::Rotate),
        "no dialog, no drive"
    );

    #[cfg(feature = "encryption")]
    {
        use azul_storage::crypto::Zeroizing;

        use crate::encryption::Dialog;

        let mut seen = PopupSlot::default();
        seen.open(Popup::Encryption(Dialog::Invite {
            drive_id: String::from("d1"),
            code: Zeroizing::new(String::from("join-code")),
            note: String::new(),
        }));
        let action = Gated::AddDevice {
            drive_id: String::from("d1"),
        };
        assert!(
            !open_name_check(&mut seen, action),
            "the join code must be seen first"
        );
        assert!(
            matches!(
                seen.as_ref(),
                Some(Popup::Encryption(Dialog::Invite { .. }))
            ),
            "the dialog that must be seen stays"
        );
    }
}

/// With no name check open Confirm does nothing.
#[test]
fn confirm_without_a_name_check_runs_nothing() {
    let mut slot = PopupSlot::default();
    assert_eq!(confirm_name(&mut slot, &name_of), Named::Wrong);
    slot.open(Popup::ConfirmForget {
        drive_id: String::from("d1"),
    });
    assert_eq!(confirm_name(&mut slot, &name_of), Named::Wrong);
    assert!(matches!(slot.as_ref(), Some(Popup::ConfirmForget { .. })));
}

/// Exactly the actions that reveal or change keys are gated - a drive removed, new keys after a
/// hack, another device, another recovery code, a recovery code removed - and each says why in
/// the system's prompt and in the name check, in both languages.
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
    for key in [
        "azdrive-presence-cancelled",
        "azdrive-presence-failed",
        "azdrive-presence-unavailable",
        "azdrive-presence-name-title",
        "azdrive-presence-name-what",
        "azdrive-presence-name-label",
        "azdrive-presence-name-wrong",
        "azdrive-presence-name-confirm",
    ] {
        assert!(en.contains(&format!("{key} =")) && de.contains(&format!("{key} =")), "{key}");
    }
}
