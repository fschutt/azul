use rust_fontconfig::FcFontCache;
use serde_json::json;

use super::*;

/// The op names the engine answers itself (the arms of `engine_op`).
static ENGINE_OPS: [&str; 3] = ["standin_user", "keyring", "biometric_prompt"];

fn window() -> LayoutWindow {
    let lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    // The run of these tests has stand-ins: what the pump of a headless or debug run says.
    lw.app_services.biometric.user.set_driven(true, true);
    lw
}

/// A window of a run whose keyring and gate are the system's.
fn real_window() -> LayoutWindow {
    LayoutWindow::new(FcFontCache::default()).expect("a layout window")
}

/// What the op answers, as the JSON the scenario sees.
fn answer(lw: &LayoutWindow, name: &str, args: serde_json::Value) -> serde_json::Value {
    let out = engine_op(lw, name, &args).expect("an op of the engine");
    assert!(out.handled);
    serde_json::from_str(out.json.as_str()).expect("an answer in JSON")
}

#[test]
fn an_op_that_is_not_the_engines_is_left_to_the_app() {
    let lw = window();
    assert!(engine_op(&lw, "load_document", &json!({})).is_none());
    for name in ENGINE_OPS {
        assert!(engine_op(&lw, name, &json!({})).is_some(), "{name}");
    }
}

#[test]
fn the_pretend_user_is_scripted_by_a_scenario() {
    let lw = window();
    let said = answer(
        &lw,
        "standin_user",
        json!({ "enrolled": true, "passcode": true, "script": ["mismatch", "Cancel", "not-enrolled"] }),
    );
    assert_eq!(said["success"], true);
    assert_eq!(said["enrolled"], true);
    assert_eq!(said["passcode"], true);
    assert_eq!(said["pending_outcomes"], 3);
    let user = &lw.app_services.biometric.user;
    assert!(user.enrolled() && user.passcode_set());
    assert_eq!(
        user.remaining_script(),
        vec![
            ScriptedOutcome::Mismatch,
            ScriptedOutcome::Cancel,
            ScriptedOutcome::NotEnrolled
        ]
    );
    let epoch = user.epoch();
    let changed = answer(&lw, "standin_user", json!({ "enrollment_changed": true }));
    assert_eq!(changed["epoch"], epoch + 1);
    // A param that is not given leaves what was set alone.
    assert!(user.enrolled());
    assert_eq!(user.pending_outcomes(), 3);
}

#[test]
fn the_pretend_user_op_refuses_what_it_does_not_know() {
    let lw = window();
    let unknown_param = answer(&lw, "standin_user", json!({ "fingers": 3 }));
    assert_eq!(unknown_param["success"], false);
    assert!(unknown_param["error"].as_str().unwrap().contains("fingers"));
    let unknown_outcome = answer(&lw, "standin_user", json!({ "script": ["thumb"] }));
    assert_eq!(unknown_outcome["success"], false);
    assert!(unknown_outcome["error"].as_str().unwrap().contains("thumb"));
    let not_a_list = answer(&lw, "standin_user", json!({ "script": "match" }));
    assert_eq!(not_a_list["success"], false);
    assert!(!lw.app_services.biometric.user.enrolled(), "a refused step changed nothing");
}

#[test]
fn a_keyring_step_asks_once_waits_and_then_checks_the_answer() {
    let mut lw = window();
    let first = answer(&lw, "keyring", json!({ "action": "get", "key": "k", "_attempt": 0 }));
    assert_eq!(first["pending"], true);
    assert!(lw.app_services.keyring.has_queued_requests());
    // The runner calls the op again while the pump has not taken the request yet: still waiting,
    // and nothing is asked twice.
    let again = answer(&lw, "keyring", json!({ "action": "get", "key": "k", "_attempt": 1 }));
    assert_eq!(again["pending"], true);
    let asked = lw.app_services.keyring.drain_requests();
    assert_eq!(asked, vec![KeyringRequest::Get { key: "k".into() }]);

    // The pump answered.
    lw.keyring_manager.set_last_result(KeyringResult::WouldPrompt);
    let done = answer(
        &lw,
        "keyring",
        json!({ "action": "get", "key": "k", "_attempt": 2, "expect": "would_prompt" }),
    );
    assert_eq!(done["success"], true);
    assert_eq!(done["result"], "would_prompt");
    let wrong = answer(
        &lw,
        "keyring",
        json!({ "action": "get", "key": "k", "_attempt": 2, "expect": "Retrieved" }),
    );
    assert_eq!(wrong["success"], false);
    let error = wrong["error"].as_str().unwrap();
    assert!(error.contains("retrieved") && error.contains("would_prompt"), "{error}");
}

#[test]
fn a_protected_store_and_an_unlock_are_asked_the_way_a_callback_asks_them() {
    let lw = window();
    drop(answer(
        &lw,
        "keyring",
        json!({ "action": "store_protected", "key": "vault", "secret": "s3cr3t",
                "protection": "Biometric", "reason": "Turn on", "cancel_label": "Use password" }),
    ));
    drop(answer(
        &lw,
        "keyring",
        json!({ "action": "unlock", "key": "vault", "reason": "Unlock", "namespace": "org.example.Flow" }),
    ));
    let asked = lw.app_services.keyring.drain_requests_in();
    assert_eq!(asked.len(), 2);
    assert!(asked[0].0.is_app());
    match &asked[0].1 {
        KeyringRequest::Store {
            key,
            secret,
            protection,
            prompt,
        } => {
            assert_eq!(key.as_str(), "vault");
            assert!(secret.is_secret(), "the secret is marked as one");
            assert_eq!(*protection, KeyringProtection::Biometric);
            assert_eq!(prompt.reason.as_str(), "Turn on");
            assert_eq!(prompt.cancel_label.as_str(), "Use password");
        }
        other => panic!("a store was asked, not {other:?}"),
    }
    assert_eq!(asked[1].0.service.as_str(), "org.example.Flow");
    assert!(matches!(asked[1].1, KeyringRequest::Unlock { .. }));
}

#[test]
fn a_secret_is_checked_and_never_echoed() {
    let mut lw = window();
    lw.keyring_manager
        .set_last_result(KeyringResult::Retrieved("s3cr3t".into()));
    let ok = answer(
        &lw,
        "keyring",
        json!({ "action": "unlock", "key": "vault", "_attempt": 1, "expect": "retrieved", "expect_secret": "s3cr3t" }),
    );
    assert_eq!(ok["success"], true);
    assert_eq!(ok["secret_length"], 6);
    assert!(!ok.to_string().contains("s3cr3t"), "{ok}");
    let wrong = answer(
        &lw,
        "keyring",
        json!({ "action": "unlock", "key": "vault", "_attempt": 1, "expect_secret": "other" }),
    );
    assert_eq!(wrong["success"], false);
    assert!(!wrong.to_string().contains("s3cr3t"), "{wrong}");
}

#[test]
fn a_keyring_step_that_cannot_be_made_fails_with_what_is_wrong() {
    let lw = window();
    for (args, mention) in [
        (json!({ "key": "k" }), "action"),
        (json!({ "action": "get" }), "key"),
        (json!({ "action": "store", "key": "k" }), "secret"),
        (json!({ "action": "store_protected", "key": "k", "secret": "s" }), "protection"),
        (
            json!({ "action": "store_protected", "key": "k", "secret": "s", "protection": "sparkly" }),
            "sparkly",
        ),
        (json!({ "action": "fetch", "key": "k" }), "fetch"),
        (json!({ "action": "get", "key": "k", "colour": "red" }), "colour"),
    ] {
        let said = answer(&lw, "keyring", args);
        assert_eq!(said["success"], false, "{said}");
        assert!(said["error"].as_str().unwrap().contains(mention), "{said}");
    }
    assert!(!lw.app_services.keyring.has_queued_requests(), "nothing was asked");
}

#[test]
fn the_standalone_gate_is_asked_and_its_answer_checked() {
    let mut lw = window();
    let first = answer(
        &lw,
        "biometric_prompt",
        json!({ "reason": "Confirm", "allow_device_credential": true, "_attempt": 0 }),
    );
    assert_eq!(first["pending"], true);
    let asked = lw.app_services.biometric.requests.drain();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].reason.as_str(), "Confirm");
    assert!(asked[0].allow_device_credential);
    lw.biometric_manager.set_last_result(BiometricResult::LockedOut);
    let done = answer(
        &lw,
        "biometric_prompt",
        json!({ "_attempt": 1, "expect": "locked_out" }),
    );
    assert_eq!(done["success"], true);
    assert_eq!(done["result"], "locked_out");
    let wrong = answer(&lw, "biometric_prompt", json!({ "_attempt": 1, "expect": "authenticated" }));
    assert_eq!(wrong["success"], false);
}

#[test]
fn every_answer_has_a_name_a_scenario_can_expect() {
    let mut names = std::collections::BTreeSet::new();
    for result in [
        KeyringResult::Stored,
        KeyringResult::Retrieved("x".into()),
        KeyringResult::Deleted,
        KeyringResult::NotFound,
        KeyringResult::Denied,
        KeyringResult::Unavailable,
        KeyringResult::Error,
        KeyringResult::Cancelled,
        KeyringResult::Failed,
        KeyringResult::LockedOut,
        KeyringResult::NotEnrolled,
        KeyringResult::Invalidated,
        KeyringResult::WouldPrompt,
    ] {
        assert!(names.insert(keyring_result_name(&result)), "{result:?}");
    }
    assert_eq!(names.len(), 13);
    let mut gate = std::collections::BTreeSet::new();
    for result in [
        BiometricResult::Authenticated,
        BiometricResult::Failed,
        BiometricResult::Cancelled,
        BiometricResult::FellBackToPasscode,
        BiometricResult::Unavailable,
        BiometricResult::Error,
        BiometricResult::LockedOut,
        BiometricResult::NotEnrolled,
    ] {
        assert!(gate.insert(biometric_result_name(result)), "{result:?}");
    }
    assert_eq!(gate.len(), 8);
}

#[test]
fn a_scenario_never_reaches_the_keyring_or_the_sensor_of_a_real_run() {
    let lw = real_window();
    for args in [
        json!({ "action": "get", "key": "k" }),
        json!({ "action": "store_protected", "key": "k", "secret": "s", "protection": "biometric" }),
    ] {
        let said = answer(&lw, "keyring", args);
        assert_eq!(said["success"], false, "{said}");
        assert!(said["error"].as_str().unwrap().contains("system's"), "{said}");
    }
    assert!(!lw.app_services.keyring.has_queued_requests(), "nothing was asked");
    let gate = answer(&lw, "biometric_prompt", json!({ "reason": "Confirm" }));
    assert_eq!(gate["success"], false, "{gate}");
    assert!(lw.app_services.biometric.requests.is_empty(), "no sheet was asked for");
    // Scripting the pretend user is harmless anywhere: nothing real answers from it.
    assert_eq!(answer(&lw, "standin_user", json!({ "enrolled": true }))["success"], true);
}
