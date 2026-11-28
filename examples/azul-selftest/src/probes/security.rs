//! The keyring and the biometric sensor.
//!
//! The keyring round trip uses the blocking calls (no user-check sheet: an unprotected item), so it runs
//! unattended; `security.keyring_support` reads what this device does for each protection level without a
//! sheet. `security.keyring_protected` is the model of `azul_core::keyring` end to end on one item: a
//! protected store, `needs_prompt` and a `get` that must NOT show a sheet, then `unlock`, the one call that does.
//! Its answer is told by the first word of the detail: `CANCELLED`, `FAILED`, `LOCKED_OUT`, `NOT_ENROLLED`,
//! `INVALIDATED` or `DENIED` (the system refused), next to `PASS`. The biometric prompt needs a person: the
//! row says what to do and unattended runs list it.

use azul::{
    biometric::{BiometricKind, BiometricPrompt, BiometricResult},
    error::KeyringResult,
    misc::{Keyring, KeyringNeed, KeyringProtection, KeyringPrompt, KeyringSupport},
    str::String as AzString,
    window::PlatformCapability,
};

use super::{describe_capability, plain, Ctx, ProbeDef, Step};
use crate::report::Verdict;

/// How long the biometric sheet may stay unanswered.
const SHEET_WAIT_MS: u64 = 60_000;

/// How long a protected store may take to show in the keyring (a sheet of the platform may come first).
const STORE_WAIT_MS: u64 = 15_000;

pub static PROBES: &[ProbeDef] = &[
    plain("security.keyring", "security", "Keyring round trip (unprotected item)", keyring_round_trip),
    plain("security.keyring_support", "security", "Keyring protection levels", keyring_support),
    ProbeDef {
        id: "security.keyring_protected",
        group: "security",
        title: "Protected keyring item (store, probe, unlock)",
        human: Some("when the system sheet appears, touch the fingerprint sensor or look at the phone (cancel it once to see CANCELLED)"),
        note: "",
        widget: None,
        step: keyring_protected,
    },
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
        KeyringResult::Cancelled => "Cancelled",
        KeyringResult::Failed => "Failed",
        KeyringResult::LockedOut => "LockedOut",
        KeyringResult::NotEnrolled => "NotEnrolled",
        KeyringResult::Invalidated => "Invalidated",
        KeyringResult::WouldPrompt => "WouldPrompt",
    }
}

/// What the keyring says about a level, as the word the report uses.
fn support_word(support: KeyringSupport) -> &'static str {
    match support {
        KeyringSupport::Hardware => "hardware",
        KeyringSupport::Library => "library",
        KeyringSupport::NotEnrolled => "not_enrolled",
        KeyringSupport::Unavailable => "unavailable",
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
    match Keyring::store_blocking(AzString::from(NAME), AzString::from(SECRET)) {
        KeyringResult::Stored => {}
        KeyringResult::Unavailable => {
            return Step::Done(Verdict::Unavailable(format!("the keyring answered Unavailable (capability said {backend})")));
        }
        KeyringResult::Denied | KeyringResult::WouldPrompt => {
            return Step::Done(Verdict::Denied("the keyring refused the store".into()));
        }
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

/// What this device does for each protection level (`Keyring::support`: no sheet, no prompt).
fn keyring_support(_ctx: &mut Ctx<'_>) -> Step {
    let (available, backend) = describe_capability(&PlatformCapability::keyring());
    let plain = Keyring::support(KeyringProtection::DeviceUnlocked);
    if !available || plain == KeyringSupport::Unavailable {
        return Step::Done(Verdict::Unavailable(format!("the keyring capability is not available: {backend}")));
    }
    let presence = Keyring::support(KeyringProtection::UserPresence);
    let biometric = Keyring::support(KeyringProtection::Biometric);
    Step::Done(Verdict::Pass(format!(
        "device_unlocked={} user_presence={} biometric={} via {backend}",
        support_word(plain),
        support_word(presence),
        support_word(biometric)
    )))
}

/// The strongest level this device can hold an item at, if any.
fn protected_level() -> Option<KeyringProtection> {
    [KeyringProtection::Biometric, KeyringProtection::UserPresence]
        .into_iter()
        .find(|level| matches!(Keyring::support(*level), KeyringSupport::Hardware | KeyringSupport::Library))
}

fn level_word(level: KeyringProtection) -> &'static str {
    if level == KeyringProtection::Biometric {
        "biometric"
    } else {
        "user_presence"
    }
}

/// A protected item through the whole model: store (behind the user), `needs_prompt` says which sheet will
/// come, a `get` shows no sheet and answers WouldPrompt, `unlock` shows the sheet and gives the secret.
fn keyring_protected(ctx: &mut Ctx<'_>) -> Step {
    const NAME: &str = "azselftest.protected";
    const SECRET: &str = "az-selftest-protected-1";
    let (available, backend) = describe_capability(&PlatformCapability::keyring());
    if !available {
        return Step::Done(Verdict::Unavailable(format!("the keyring capability is not available: {backend}")));
    }
    let Some(level) = protected_level() else {
        let presence = Keyring::support(KeyringProtection::UserPresence);
        let biometric = Keyring::support(KeyringProtection::Biometric);
        return Step::Done(Verdict::Unavailable(if presence == KeyringSupport::NotEnrolled || biometric == KeyringSupport::NotEnrolled {
            "NOT_ENROLLED: this device has the means but nothing is set up (enrol a fingerprint or face, set a passcode)".into()
        } else {
            format!("no protected level here: user_presence={} biometric={} via {backend}", support_word(presence), support_word(biometric))
        }));
    };
    let expected = if level == KeyringProtection::Biometric { KeyringNeed::Biometric } else { KeyringNeed::UserPresence };
    match ctx.phase {
        0 => {
            // A clean start: nothing of an earlier run stays, and no answer of the manager is taken for this run's.
            let _ = Keyring::delete_blocking(AzString::from(NAME));
            ctx.info.keyring_store_protected(
                AzString::from(NAME),
                AzString::from(SECRET),
                level,
                KeyringPrompt::create(AzString::from("AzSelfTest: keep a test secret behind the sensor")),
            );
            ctx.phase = 1;
            Step::Wait
        }
        1 => {
            // The store is asynchronous: it is done when the item shows as protected AND the manager says Stored.
            let need = Keyring::needs_prompt(AzString::from(NAME));
            let answer = ctx.info.get_keyring_result().into_option();
            if need == expected && matches!(answer, Some(KeyringResult::Stored)) {
                ctx.phase = 2;
                return Step::Wait;
            }
            if need == KeyringNeed::NotFound || need == expected {
                match answer {
                    Some(KeyringResult::NotEnrolled) => {
                        return Step::Done(Verdict::Unavailable("NOT_ENROLLED: the store was refused, nothing is enrolled for this level".into()));
                    }
                    Some(KeyringResult::Cancelled) => return Step::Done(Verdict::Denied("CANCELLED: the sheet of the store was cancelled".into())),
                    Some(KeyringResult::Unavailable) => {
                        return Step::Done(Verdict::Unavailable(format!("the keyring answered Unavailable for a {} store", level_word(level))));
                    }
                    Some(KeyringResult::Denied) => return Step::Done(Verdict::Denied("DENIED: the system refused the store".into())),
                    Some(KeyringResult::Error | KeyringResult::Failed | KeyringResult::LockedOut) => {
                        return Step::Done(Verdict::Fail(format!("the {} store failed", level_word(level))));
                    }
                    _ => {}
                }
            }
            if ctx.elapsed_ms > STORE_WAIT_MS {
                return Step::Done(Verdict::NeedsHuman(format!("the {} store did not finish in {} s (answer the sheet of the store)", level_word(level), STORE_WAIT_MS / 1000)));
            }
            Step::Wait
        }
        2 => {
            // No sheet from here until the unlock: a get of a protected item must answer WouldPrompt.
            match Keyring::get_blocking(AzString::from(NAME)) {
                KeyringResult::WouldPrompt => {}
                other => {
                    let _ = Keyring::delete_blocking(AzString::from(NAME));
                    return Step::Done(Verdict::Fail(format!("a get of a {} item answered {} (wanted WouldPrompt: no sheet)", level_word(level), name_of(&other))));
                }
            }
            ctx.info.keyring_unlock(
                AzString::from(NAME),
                KeyringPrompt::create(AzString::from("AzSelfTest: unlock the test secret")).with_cancel_label(AzString::from("Cancel")),
            );
            ctx.scratch.push(ctx.elapsed_ms);
            ctx.phase = 3;
            // A harness that feeds the platform answers this line (iOS Simulator: a face that matches).
            super::await_feed("biometric.match");
            Step::Wait
        }
        _ => {
            let asked_at = ctx.scratch.first().copied().unwrap_or(0);
            let waited = ctx.elapsed_ms.saturating_sub(asked_at);
            // `Stored` is the store's answer still standing: the unlock has not answered yet.
            let verdict = match ctx.info.get_keyring_result().into_option() {
                Some(KeyringResult::Retrieved(secret)) if secret.as_str() == SECRET => {
                    let _ = Keyring::delete_blocking(AzString::from(NAME));
                    Verdict::Pass(format!("{} item: store, needs_prompt, get=WouldPrompt, unlock ok after {waited} ms via {backend}", level_word(level)))
                }
                Some(KeyringResult::Retrieved(_)) => Verdict::Fail("unlock returned another secret than the one stored".into()),
                Some(KeyringResult::Cancelled) => Verdict::Denied("CANCELLED: the sheet was cancelled".into()),
                Some(KeyringResult::Failed) => Verdict::Denied("FAILED: the finger or face did not match".into()),
                Some(KeyringResult::LockedOut) => Verdict::Denied("LOCKED_OUT: too many tries, the biometric is locked until the passcode is entered".into()),
                Some(KeyringResult::NotEnrolled) => Verdict::Unavailable("NOT_ENROLLED: nothing is enrolled for this level any more".into()),
                Some(KeyringResult::Invalidated) => Verdict::Denied("INVALIDATED: the enrolled biometrics changed, the secret is gone: store it again".into()),
                Some(KeyringResult::Denied) => Verdict::Denied("DENIED: the system refused this app".into()),
                Some(KeyringResult::WouldPrompt) => Verdict::Fail("unlock answered WouldPrompt: it must show the sheet".into()),
                Some(KeyringResult::Stored) | None if waited <= SHEET_WAIT_MS => return Step::Wait,
                Some(KeyringResult::Stored) | None => {
                    Verdict::NeedsHuman("nobody answered the sheet in 60 s (on the emulator: adb emu finger touch 1)".into())
                }
                Some(other) => Verdict::Fail(format!("unlock answered {}", name_of(&other))),
            };
            if !matches!(verdict, Verdict::Pass(_)) {
                let _ = Keyring::delete_blocking(AzString::from(NAME));
            }
            Step::Done(verdict)
        }
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
        // A harness that feeds the platform answers this line (iOS Simulator: a face that matches).
        super::await_feed("biometric.match");
        return Step::Wait;
    }
    match ctx.info.get_biometric_result().into_option() {
        Some(BiometricResult::Authenticated) => Step::Done(Verdict::Pass(format!("authenticated after {} ms", ctx.elapsed_ms))),
        Some(BiometricResult::FellBackToPasscode) => Step::Done(Verdict::Pass("the passcode was used".into())),
        Some(BiometricResult::Cancelled) => Step::Done(Verdict::Denied("CANCELLED: the sheet was cancelled".into())),
        Some(BiometricResult::Failed) => Step::Done(Verdict::Denied("FAILED: the finger or face did not match".into())),
        Some(BiometricResult::LockedOut) => Step::Done(Verdict::Denied("LOCKED_OUT: too many tries, the biometric is locked".into())),
        Some(BiometricResult::NotEnrolled) => Step::Done(Verdict::Unavailable("NOT_ENROLLED: nothing is enrolled to check with".into())),
        Some(BiometricResult::Unavailable) => Step::Done(Verdict::Unavailable("the platform says the biometric is unavailable".into())),
        Some(BiometricResult::Error) => Step::Done(Verdict::Fail("the biometric prompt answered Error".into())),
        None if ctx.elapsed_ms > SHEET_WAIT_MS => {
            Step::Done(Verdict::NeedsHuman("nobody answered the sheet in 60 s (on the emulator: adb emu finger touch 1)".into()))
        }
        None => Step::Wait,
    }
}
