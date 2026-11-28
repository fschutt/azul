//! A secret bound to the user's biometric: store it (Biometric level), ask the quiet probe what a read would need, unlock it
//! (the ONLY call that shows the system's sheet), delete it. The probe never decides by itself that the sheet was answered:
//! the harness that feeds the platform (a face that matches on the iOS Simulator, a finger on the Android emulator) answers
//! the `AZ_SELFTEST_AWAIT biometric.match` line, and the secret comes back or it does not.
//!
//! Where the device cannot bind a secret to a biometric (no sensor, nothing enrolled, no passcode) the row says UNAVAILABLE
//! with the reason the platform gave: that is the honest answer of a Simulator whose Face ID was never enrolled.

use azul::{
    error::KeyringResult,
    misc::{Keyring, KeyringNeed, KeyringProtection, KeyringPrompt, KeyringSupport},
    str::String as AzString,
};

use super::{await_feed, Ctx, ProbeDef, Step};
use crate::report::Verdict;

/// How long the sheet may stay unanswered.
const SHEET_WAIT_MS: u64 = 60_000;
const KEY: &str = "azselftest.protected";
const SECRET: &str = "az-selftest-protected-1";

pub static PROBES: &[ProbeDef] = &[ProbeDef {
    id: "security.keyring_protected",
    group: "security",
    title: "Keyring: a secret behind the biometric",
    human: Some("look at the phone / touch the sensor when the system sheet appears (the Simulator and the emulator are fed by the harness)"),
    note: "",
    widget: None,
    step: keyring_protected,
}];

fn prompt() -> KeyringPrompt {
    KeyringPrompt::create(AzString::from("AzSelfTest: unlock the protected secret"))
}

fn name_of(result: &KeyringResult) -> &'static str {
    match result {
        KeyringResult::Stored => "Stored",
        KeyringResult::Retrieved(_) => "Retrieved",
        KeyringResult::Deleted => "Deleted",
        KeyringResult::NotFound => "NotFound",
        KeyringResult::Denied => "Denied",
        KeyringResult::Unavailable => "Unavailable",
        KeyringResult::Error => "Error",
        KeyringResult::Cancelled => "Cancelled",
        KeyringResult::Failed => "Failed",
        KeyringResult::LockedOut => "LockedOut",
        KeyringResult::NotEnrolled => "NotEnrolled",
        KeyringResult::Invalidated => "Invalidated",
        KeyringResult::WouldPrompt => "WouldPrompt",
    }
}

/// The phases: 0 asks the device and stores; 1 waits for `Stored`, probes quietly and unlocks; 2 waits for the secret and
/// deletes; 3 waits for `Deleted`. A result stays readable until the next op replaces it, so each phase waits for the
/// answer of ITS op and ignores the one before it.
fn keyring_protected(ctx: &mut Ctx<'_>) -> Step {
    if ctx.phase == 0 {
        match Keyring::support(KeyringProtection::Biometric) {
            KeyringSupport::Unavailable => {
                return Step::Done(Verdict::Unavailable("this device cannot bind a secret to a biometric (no sensor, or no secure keyring)".into()));
            }
            KeyringSupport::NotEnrolled => {
                return Step::Done(Verdict::Unavailable(
                    "no face or finger is enrolled, or no passcode is set (iOS Simulator: Features > Face ID > Enrolled; emulator: enrol a fingerprint)".into(),
                ));
            }
            KeyringSupport::Hardware | KeyringSupport::Library => {}
        }
        ctx.info.keyring_store_protected(AzString::from(KEY), AzString::from(SECRET), KeyringProtection::Biometric, prompt());
        ctx.phase = 1;
        return Step::Wait;
    }
    let result = ctx.info.get_keyring_result().into_option();
    match (ctx.phase, &result) {
        (1, Some(KeyringResult::Stored)) => {
            // the quiet probe says what a read needs and shows nothing
            match Keyring::needs_prompt(AzString::from(KEY)) {
                KeyringNeed::Biometric => {}
                other => return Step::Done(Verdict::Fail(format!("the quiet probe of a Biometric item says {other:?} (wanted Biometric)"))),
            }
            ctx.info.keyring_unlock(AzString::from(KEY), prompt());
            ctx.phase = 2;
            // the harness shows a matching face / finger to the sheet that is up now
            await_feed("biometric.match");
            return Step::Wait;
        }
        (1, Some(KeyringResult::NotEnrolled | KeyringResult::Unavailable)) => {
            let why = result.as_ref().map_or("", name_of);
            return Step::Done(Verdict::Unavailable(format!("the protected store answered {why}: the device cannot bind the secret")));
        }
        (1, Some(other)) => return Step::Done(Verdict::Fail(format!("the protected store answered {}", name_of(other)))),
        (2, Some(KeyringResult::Retrieved(secret))) => {
            if secret.as_str() != SECRET {
                return Step::Done(Verdict::Fail("the unlock returned another secret than the one stored".into()));
            }
            ctx.info.keyring_delete(AzString::from(KEY));
            ctx.phase = 3;
            return Step::Wait;
        }
        // the answer of the store is still the last one: the sheet is up
        (2, Some(KeyringResult::Stored)) => {}
        (2, Some(KeyringResult::Cancelled)) => return Step::Done(Verdict::Denied("the sheet was cancelled".into())),
        (2, Some(KeyringResult::Failed | KeyringResult::LockedOut)) => {
            return Step::Done(Verdict::Denied(format!("the biometric did not release the secret: {}", result.as_ref().map_or("", name_of))));
        }
        (2, Some(other)) => return Step::Done(Verdict::Fail(format!("the unlock answered {}", name_of(other)))),
        (3, Some(KeyringResult::Deleted)) => {
            return Step::Done(Verdict::Pass(format!(
                "stored behind the biometric, released by the sheet after {} ms, deleted",
                ctx.elapsed_ms
            )));
        }
        // the answer of the unlock is still the last one
        (3, Some(KeyringResult::Retrieved(_))) => {}
        (3, Some(other)) => return Step::Done(Verdict::Fail(format!("the delete answered {}", name_of(other)))),
        _ => {}
    }
    if ctx.elapsed_ms > SHEET_WAIT_MS {
        return Step::Done(Verdict::NeedsHuman("nobody answered the biometric sheet in 60 s".into()));
    }
    Step::Wait
}
