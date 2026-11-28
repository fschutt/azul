//! The keyring and the biometric sensor.
//!
//! The keyring round trip uses the blocking calls (no user-check sheet: an unprotected item), so it runs
//! unattended. The biometric prompt needs a person: the row says what to do and unattended runs list it.
//! When the keyring and biometric model is changed (protection levels, `needs_prompt`, `unlock`), the
//! probes of this file are the ones to extend: one row per protection level and outcome.

use azul::{
    biometric::{BiometricKind, BiometricPrompt, BiometricResult},
    error::KeyringResult,
    misc::Keyring,
    str::String as AzString,
    window::PlatformCapability,
};

use super::{describe_capability, plain, Ctx, ProbeDef, Step};
use crate::report::Verdict;

/// How long the biometric sheet may stay unanswered.
const SHEET_WAIT_MS: u64 = 60_000;

pub static PROBES: &[ProbeDef] = &[
    plain("security.keyring", "security", "Keyring round trip (unprotected item)", keyring_round_trip),
    plain("security.biometric_kind", "security", "Biometric sensor kind", biometric_kind),
    ProbeDef {
        id: "security.biometric_prompt",
        group: "security",
        title: "Biometric prompt",
        human: Some("touch the fingerprint sensor or look at the phone when the system sheet appears (cancel it once to see DENIED)"),
        note: "",
        widget: None,
        step: biometric_prompt,
    },
];

fn name_of(result: &KeyringResult) -> &'static str {
    match result {
        KeyringResult::Stored => "Stored",
        KeyringResult::Retrieved(_) => "Retrieved",
        KeyringResult::Deleted => "Deleted",
        KeyringResult::NotFound => "NotFound",
        KeyringResult::Denied => "Denied",
        KeyringResult::Unavailable => "Unavailable",
        KeyringResult::Error => "Error",
    }
}

/// store, get, delete, get again: the secret comes back as it went in and is gone after the delete.
fn keyring_round_trip(_ctx: &mut Ctx<'_>) -> Step {
    let (available, backend) = describe_capability(&PlatformCapability::keyring());
    if !available {
        return Step::Done(Verdict::Unavailable(format!("the keyring capability is not available: {backend}")));
    }
    const NAME: &str = "azselftest.roundtrip";
    const SECRET: &str = "az-selftest-secret-1";
    match Keyring::store_blocking(AzString::from(NAME), AzString::from(SECRET), false) {
        KeyringResult::Stored => {}
        KeyringResult::Unavailable => {
            return Step::Done(Verdict::Unavailable(format!("the keyring answered Unavailable (capability said {backend})")));
        }
        KeyringResult::Denied => return Step::Done(Verdict::Denied("the keyring refused the store".into())),
        other => return Step::Done(Verdict::Fail(format!("store answered {}", name_of(&other)))),
    }
    match Keyring::get_blocking(AzString::from(NAME)) {
        KeyringResult::Retrieved(secret) if secret.as_str() == SECRET => {}
        KeyringResult::Retrieved(_) => return Step::Done(Verdict::Fail("get returned another secret than the one stored".into())),
        other => return Step::Done(Verdict::Fail(format!("get after store answered {}", name_of(&other)))),
    }
    match Keyring::delete_blocking(AzString::from(NAME)) {
        KeyringResult::Deleted => {}
        other => return Step::Done(Verdict::Fail(format!("delete answered {}", name_of(&other)))),
    }
    match Keyring::get_blocking(AzString::from(NAME)) {
        KeyringResult::NotFound => Step::Done(Verdict::Pass(format!("store, get, delete, get-after-delete ok via {backend}"))),
        other => Step::Done(Verdict::Fail(format!("get after delete answered {} (wanted NotFound)", name_of(&other)))),
    }
}

fn biometric_kind(ctx: &mut Ctx<'_>) -> Step {
    let (available, backend) = describe_capability(&PlatformCapability::biometric());
    let kind = match ctx.info.get_biometric_kind() {
        BiometricKind::NotAvailable => None,
        BiometricKind::Fingerprint => Some("fingerprint"),
        BiometricKind::Face => Some("face"),
        BiometricKind::Iris => Some("iris"),
    };
    Step::Done(match (kind, available) {
        (Some(kind), _) => Verdict::Pass(format!("{kind} via {backend}")),
        (None, false) => Verdict::Unavailable(format!("the biometric capability is not available: {backend}")),
        (None, true) => Verdict::Unavailable(format!("no biometric enrolled on this device (capability via {backend})")),
    })
}

fn biometric_prompt(ctx: &mut Ctx<'_>) -> Step {
    if matches!(ctx.info.get_biometric_kind(), BiometricKind::NotAvailable) {
        return Step::Done(Verdict::Unavailable("no biometric sensor enrolled: enrol one, then run the row".into()));
    }
    if ctx.phase == 0 {
        ctx.info.request_biometric_auth(BiometricPrompt {
            reason: AzString::from("AzSelfTest: confirm that it is you"),
            cancel_label: AzString::from("Cancel"),
            allow_device_credential: false,
        });
        ctx.phase = 1;
        return Step::Wait;
    }
    match ctx.info.get_biometric_result().into_option() {
        Some(BiometricResult::Authenticated) => Step::Done(Verdict::Pass(format!("authenticated after {} ms", ctx.elapsed_ms))),
        Some(BiometricResult::FellBackToPasscode) => Step::Done(Verdict::Pass("the passcode was used".into())),
        Some(BiometricResult::Cancelled) => Step::Done(Verdict::Denied("the sheet was cancelled".into())),
        Some(BiometricResult::Failed) => Step::Done(Verdict::Denied("the finger or face did not match".into())),
        Some(BiometricResult::Unavailable) => Step::Done(Verdict::Unavailable("the platform says the biometric is unavailable".into())),
        Some(BiometricResult::Error) => Step::Done(Verdict::Fail("the biometric prompt answered Error".into())),
        None if ctx.elapsed_ms > SHEET_WAIT_MS => {
            Step::Done(Verdict::NeedsHuman("nobody answered the sheet in 60 s (on the emulator: adb emu finger touch 1)".into()))
        }
        None => Step::Wait,
    }
}
