//! The flows of a protected secret through the whole engine path of a headless run, on every
//! host: a callback's request in the app's channel, the pump handing it to the keyring (the
//! stand-in of a debug or test build), the answer folded into the manager the callback reads
//! with `get_keyring_result`, and the biometric gate answered by the scripted user - the same
//! path a phone's e2e scenario runs, with no sheet and no sensor.

use azul_core::keyring::{
    KeyringNamespace, KeyringProtection, KeyringPrompt, KeyringRequest, KeyringResult,
};
use azul_css::AzString;
use azul_layout::{
    managers::{biometric::BiometricPrompt, stand_in_user::ScriptedOutcome},
    window::LayoutWindow,
};
use rust_fontconfig::FcFontCache;

use super::pump;

fn window() -> LayoutWindow {
    LayoutWindow::new(FcFontCache::default()).expect("a layout window")
}

/// Whether the keyring of this run is the stand-in (a debug build, or a run of a test build): a
/// test never touches the machine's own keychain.
fn keyring_is_the_stand_in() -> bool {
    cfg!(debug_assertions) || azul_core::run_mode::test_hooks().keyring_headless
}

/// A key no other test of this process uses (the stand-in's store is the process's one).
fn key(what: &str) -> AzString {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    AzString::from(format!("pump-flow/{what}/{}", NEXT.fetch_add(1, Ordering::Relaxed)))
}

/// What a callback does and the next frame sees: queue the op, run the pump, read the answer.
fn ask(lw: &mut LayoutWindow, request: KeyringRequest) -> KeyringResult {
    lw.app_services
        .keyring
        .push_request_in(KeyringNamespace::create("org.example.PumpFlow".into()), request);
    pump(lw);
    lw.keyring_manager
        .last_result()
        .cloned()
        .expect("the pump answered the request")
}

fn prompt() -> KeyringPrompt {
    KeyringPrompt::create("Unlock your vault".into())
}

fn store(key: &AzString, secret: &str, protection: KeyringProtection) -> KeyringRequest {
    KeyringRequest::Store {
        key: key.clone(),
        secret: secret.into(),
        protection,
        prompt: prompt(),
    }
}

fn unlock(key: &AzString) -> KeyringRequest {
    KeyringRequest::Unlock {
        key: key.clone(),
        prompt: prompt(),
    }
}

fn get(key: &AzString) -> KeyringRequest {
    KeyringRequest::Get { key: key.clone() }
}

#[test]
fn a_protected_secret_goes_through_the_whole_engine_path_with_every_answer_told_apart() {
    if !keyring_is_the_stand_in() {
        return;
    }
    let mut lw = window();
    let user = lw.app_services.biometric.user.clone();
    let vault = key("vault");

    // Nothing enrolled: the level cannot be met and nothing is stored.
    assert_eq!(
        ask(&mut lw, store(&vault, "s3cr3t", KeyringProtection::Biometric)),
        KeyringResult::NotEnrolled
    );
    assert_eq!(ask(&mut lw, get(&vault)), KeyringResult::NotFound);

    // The user enrols a finger and a passcode; now it is stored, and a get never opens it.
    user.enroll(true, true);
    assert_eq!(
        ask(&mut lw, store(&vault, "s3cr3t", KeyringProtection::Biometric)),
        KeyringResult::Stored
    );
    assert_eq!(ask(&mut lw, get(&vault)), KeyringResult::WouldPrompt);

    // The sheet, with the user failing, cancelling, locking the sensor out, and then matching.
    user.script([
        ScriptedOutcome::Mismatch,
        ScriptedOutcome::Cancel,
        ScriptedOutcome::Lockout,
        ScriptedOutcome::Match,
    ]);
    assert_eq!(ask(&mut lw, unlock(&vault)), KeyringResult::Failed);
    assert_eq!(ask(&mut lw, unlock(&vault)), KeyringResult::Cancelled);
    assert_eq!(ask(&mut lw, unlock(&vault)), KeyringResult::LockedOut);
    assert_eq!(
        ask(&mut lw, unlock(&vault)),
        KeyringResult::Retrieved("s3cr3t".into())
    );

    // A finger is added: the biometric item is invalidated, store it again to go on.
    user.enrollment_changed();
    assert_eq!(ask(&mut lw, unlock(&vault)), KeyringResult::Invalidated);
    assert_eq!(ask(&mut lw, get(&vault)), KeyringResult::Invalidated);
    assert_eq!(
        ask(&mut lw, store(&vault, "again", KeyringProtection::Biometric)),
        KeyringResult::Stored
    );
    assert_eq!(
        ask(&mut lw, unlock(&vault)),
        KeyringResult::Retrieved("again".into())
    );

    // Gone for good.
    assert_eq!(ask(&mut lw, KeyringRequest::Delete { key: vault.clone() }), KeyringResult::Deleted);
    assert_eq!(ask(&mut lw, unlock(&vault)), KeyringResult::NotFound);
}

#[test]
fn the_pass_of_a_user_presence_secret_survives_a_change_of_the_fingers() {
    if !keyring_is_the_stand_in() {
        return;
    }
    let mut lw = window();
    let user = lw.app_services.biometric.user.clone();
    user.enroll(true, true);
    let token = key("token");
    assert_eq!(
        ask(&mut lw, store(&token, "t0ken", KeyringProtection::UserPresence)),
        KeyringResult::Stored
    );
    user.enrollment_changed();
    user.script([ScriptedOutcome::Passcode]);
    assert_eq!(
        ask(&mut lw, unlock(&token)),
        KeyringResult::Retrieved("t0ken".into()),
        "the passcode opens a user-presence item whatever the fingers did"
    );
}

#[test]
fn an_unprotected_secret_is_read_without_the_user_and_nothing_of_the_script_is_used() {
    if !keyring_is_the_stand_in() {
        return;
    }
    let mut lw = window();
    let user = lw.app_services.biometric.user.clone();
    user.enroll(true, true);
    user.script([ScriptedOutcome::Cancel]);
    let plain = key("plain");
    assert_eq!(
        ask(&mut lw, store(&plain, "x", KeyringProtection::DeviceUnlocked)),
        KeyringResult::Stored
    );
    assert_eq!(ask(&mut lw, get(&plain)), KeyringResult::Retrieved("x".into()));
    assert_eq!(ask(&mut lw, unlock(&plain)), KeyringResult::Retrieved("x".into()));
    assert_eq!(user.pending_outcomes(), 1, "no sheet came up");
}

#[test]
fn the_standalone_gate_and_the_availability_follow_the_same_scripted_user() {
    use azul_core::biometric::{BiometricKind, BiometricResult};
    let mut lw = window();
    let user = lw.app_services.biometric.user.clone();

    // Nothing enrolled: the sensor is not there and the prompt answers Unavailable. Only a run
    // with the test hooks asks the stand-in; any other run would ask the machine's own sensor
    // (and show its sheet), which a test never does.
    let in_a_test_run = azul_core::run_mode::test_hooks().no_biometric;
    if in_a_test_run {
        lw.app_services.biometric.requests.push(BiometricPrompt::default());
        pump(&mut lw);
        assert_eq!(lw.biometric_manager.last_result(), Some(BiometricResult::Unavailable));
        assert_eq!(lw.biometric_manager.availability(), BiometricKind::NotAvailable);

        user.enroll(true, true);
        user.script([ScriptedOutcome::Mismatch, ScriptedOutcome::Match]);
        for want in [BiometricResult::Failed, BiometricResult::Authenticated] {
            lw.app_services.biometric.requests.push(BiometricPrompt::default());
            pump(&mut lw);
            assert_eq!(lw.biometric_manager.last_result(), Some(want));
        }
        assert_eq!(lw.biometric_manager.availability(), BiometricKind::Fingerprint);
    }
}

#[test]
fn a_permission_that_may_ask_waits_for_the_users_gesture_and_a_decided_one_is_not_asked() {
    use azul_layout::managers::permission::{Admission, Capability, PermissionState};
    let mut lw = window();
    // The pump folds the platform's quiet read in at its first pass; nothing in it prompts.
    pump(&mut lw);
    let manager = &mut lw.permission_manager;
    manager.set_status(Capability::Camera, PermissionState::Denied);
    assert_eq!(manager.admit(Capability::Camera), Admission::Skip, "denied is not asked again");
    assert_eq!(
        manager.admit(Capability::Microphone),
        Admission::Hold,
        "an undecided one waits for a gesture"
    );
    manager.note_gesture();
    assert_eq!(manager.admit(Capability::Microphone), Admission::Go);
}
