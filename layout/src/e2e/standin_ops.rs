//! The ops a scenario drives a protected secret's whole flow with, on any host.
//!
//! The engine answers three op names itself, before any command of the app: `standin_user`
//! scripts the pretend user of the stand-in keyring and biometric gate (what is enrolled, what
//! they do at each sheet, a change of the enrolled fingers), `keyring` asks the keyring for one
//! thing (the way a callback's `keyring_store_protected` / `keyring_unlock` does) and waits for
//! the answer, and `biometric_prompt` does the same for the standalone gate. A scenario of the
//! flows of `azul_core::keyring` therefore runs unchanged on a desktop, in a headless run and, once
//! a phone's build is the stand-in too, on an emulator.
//!
//! An op that asks something waits for its answer by saying `{"pending": true}`: the runner
//! calls it again after one turn of the event loop (the capability pump answers meanwhile) with
//! `_attempt` counting the calls. `expect` names the answer the step requires: it fails with
//! what it got otherwise.
//!
//! ```json
//! { "op": "standin_user", "enrolled": true, "passcode": true, "script": ["mismatch", "match"] },
//! { "op": "keyring", "action": "store_protected", "key": "vault", "secret": "s3cr3t",
//!   "protection": "biometric", "reason": "Turn on", "expect": "stored" },
//! { "op": "keyring", "action": "unlock", "key": "vault", "reason": "Unlock", "expect": "failed" },
//! { "op": "keyring", "action": "unlock", "key": "vault", "reason": "Unlock",
//!   "expect": "retrieved", "expect_secret": "s3cr3t" }
//! ```

use azul_core::{
    biometric::{BiometricPrompt, BiometricResult},
    events::CustomE2eOpResult,
    keyring::{
        KeyringNamespace, KeyringProtection, KeyringPrompt, KeyringRequest, KeyringResult,
    },
};
use azul_css::AzString;

use crate::{managers::stand_in_user::ScriptedOutcome, window::LayoutWindow};

/// What a `keyring` step says when the run's keyring is the system's.
const NOT_A_STAND_IN_KEYRING: &str = "keyring: this run's keyring is the system's, not the stand-in \
    a scenario drives (a debug build, or a build with the cargo feature test-hooks in a headless \
    run): the step is not made, the user's own keychain stays untouched";

/// What a `biometric_prompt` step says when the run's gate is the machine's sensor.
const NOT_A_STAND_IN_GATE: &str = "biometric_prompt: this run's biometric gate is the machine's \
    sensor, not the stand-in a scenario drives (a build with the cargo feature test-hooks in a \
    headless run): the step is not made, no sheet is shown";

/// The op names the engine answers itself.
pub(crate) const ENGINE_OPS: [&str; 3] = ["standin_user", "keyring", "biometric_prompt"];

/// The engine's answer to the op `name` with `args`, or `None` when the op is not one of the
/// engine's own ([`ENGINE_OPS`]): then an app's command (or its catch-all handler) is asked.
#[must_use]
pub(crate) fn engine_op(
    lw: &LayoutWindow,
    name: &str,
    args: &serde_json::Value,
) -> Option<CustomE2eOpResult> {
    let answer = match name {
        "standin_user" => standin_user(lw, args),
        "keyring" => keyring(lw, args),
        "biometric_prompt" => biometric_prompt(lw, args),
        _ => return None,
    };
    let json = match answer {
        Ok(value) => value,
        Err(error) => serde_json::json!({ "success": false, "error": error }),
    };
    Some(CustomE2eOpResult {
        handled: true,
        json: AzString::from(json.to_string()),
    })
}

/// The name a scenario uses for a keyring answer (`stored`, `retrieved`, `would_prompt`, ...).
#[must_use]
pub(crate) const fn keyring_result_name(result: &KeyringResult) -> &'static str {
    match result {
        KeyringResult::Stored => "stored",
        KeyringResult::Retrieved(_) => "retrieved",
        KeyringResult::Deleted => "deleted",
        KeyringResult::NotFound => "not_found",
        KeyringResult::Denied => "denied",
        KeyringResult::Unavailable => "unavailable",
        KeyringResult::Error => "error",
        KeyringResult::Cancelled => "cancelled",
        KeyringResult::Failed => "failed",
        KeyringResult::LockedOut => "locked_out",
        KeyringResult::NotEnrolled => "not_enrolled",
        KeyringResult::Invalidated => "invalidated",
        KeyringResult::WouldPrompt => "would_prompt",
    }
}

/// The name a scenario uses for a biometric answer.
#[must_use]
pub(crate) const fn biometric_result_name(result: BiometricResult) -> &'static str {
    match result {
        BiometricResult::Authenticated => "authenticated",
        BiometricResult::Failed => "failed",
        BiometricResult::Cancelled => "cancelled",
        BiometricResult::FellBackToPasscode => "fell_back_to_passcode",
        BiometricResult::Unavailable => "unavailable",
        BiometricResult::Error => "error",
        BiometricResult::LockedOut => "locked_out",
        BiometricResult::NotEnrolled => "not_enrolled",
    }
}

/// A name as a scenario may spell it (`Not-Enrolled`, `would prompt`): lower case, `_` between words.
fn normalised(name: &str) -> String {
    name.trim().to_ascii_lowercase().replace(['-', ' '], "_")
}

/// The params an op takes; one it does not know is an error (a typo must not pass).
fn only_params(op: &str, args: &serde_json::Value, allowed: &[&str]) -> Result<(), String> {
    let Some(map) = args.as_object() else {
        return Ok(());
    };
    for key in map.keys() {
        // `_attempt` and `_test` are the runner's own.
        if !key.starts_with('_') && !allowed.contains(&key.as_str()) {
            return Err(format!(
                "{op}: unknown param '{key}' (takes: {})",
                allowed.join(", ")
            ));
        }
    }
    Ok(())
}

fn text<'a>(args: &'a serde_json::Value, name: &str) -> Option<&'a str> {
    args.get(name).and_then(serde_json::Value::as_str)
}

fn required<'a>(op: &str, args: &'a serde_json::Value, name: &str) -> Result<&'a str, String> {
    match text(args, name) {
        Some(value) if !value.is_empty() => Ok(value),
        _ => Err(format!("{op}: missing param '{name}'")),
    }
}

/// How often the runner has called this op for this step (0 the first time).
fn attempt(args: &serde_json::Value) -> u64 {
    args.get("_attempt")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

/// `{"pending": true}`: the answer is not there yet, call again after a turn of the event loop.
fn pending() -> serde_json::Value {
    serde_json::json!({ "success": true, "pending": true })
}

/// `standin_user`: script the pretend user (`enrolled`, `passcode`, `script`, `enrollment_changed`).
fn standin_user(lw: &LayoutWindow, args: &serde_json::Value) -> Result<serde_json::Value, String> {
    only_params(
        "standin_user",
        args,
        &["enrolled", "passcode", "script", "enrollment_changed"],
    )?;
    let user = &lw.app_services.biometric.user;
    let enrolled = args.get("enrolled").and_then(serde_json::Value::as_bool);
    let passcode = args.get("passcode").and_then(serde_json::Value::as_bool);
    if enrolled.is_some() || passcode.is_some() {
        user.enroll(
            enrolled.unwrap_or_else(|| user.enrolled()),
            passcode.unwrap_or_else(|| user.passcode_set()),
        );
    }
    if let Some(script) = args.get("script") {
        let names = script
            .as_array()
            .ok_or_else(|| "standin_user: 'script' is a list of outcome names".to_string())?;
        let mut outcomes = Vec::with_capacity(names.len());
        for name in names {
            let name = name
                .as_str()
                .ok_or_else(|| "standin_user: an outcome of 'script' is a name".to_string())?;
            let outcome = ScriptedOutcome::from_name(name).ok_or_else(|| {
                let valid: Vec<&str> = ScriptedOutcome::ALL.iter().map(|o| o.name()).collect();
                format!(
                    "standin_user: unknown outcome '{name}' (one of: {})",
                    valid.join(", ")
                )
            })?;
            outcomes.push(outcome);
        }
        user.script(outcomes);
    }
    if args
        .get("enrollment_changed")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        user.enrollment_changed();
    }
    Ok(serde_json::json!({
        "success": true,
        "enrolled": user.enrolled(),
        "passcode": user.passcode_set(),
        "epoch": user.epoch(),
        "pending_outcomes": user.pending_outcomes(),
    }))
}

/// The protection a scenario names.
fn protection_named(name: &str) -> Option<KeyringProtection> {
    match normalised(name).as_str() {
        "device_unlocked" => Some(KeyringProtection::DeviceUnlocked),
        "user_presence" => Some(KeyringProtection::UserPresence),
        "biometric" => Some(KeyringProtection::Biometric),
        _ => None,
    }
}

/// The request a `keyring` step asks for.
fn keyring_request(args: &serde_json::Value) -> Result<KeyringRequest, String> {
    let action = required("keyring", args, "action")?;
    let key = AzString::from(required("keyring", args, "key")?);
    let prompt = || {
        let mut prompt = KeyringPrompt::create(AzString::from(text(args, "reason").unwrap_or("")));
        if let Some(label) = text(args, "cancel_label") {
            prompt = prompt.with_cancel_label(AzString::from(label));
        }
        prompt
    };
    let secret = || -> Result<AzString, String> {
        let mut secret = AzString::from(required("keyring", args, "secret")?);
        secret.set_secret(true);
        Ok(secret)
    };
    match normalised(action).as_str() {
        "store" => Ok(KeyringRequest::Store {
            key,
            secret: secret()?,
            protection: KeyringProtection::DeviceUnlocked,
            prompt: prompt(),
        }),
        "store_protected" => {
            let named = required("keyring", args, "protection")?;
            let protection = protection_named(named).ok_or_else(|| {
                format!(
                    "keyring: unknown protection '{named}' (device_unlocked, user_presence, biometric)"
                )
            })?;
            Ok(KeyringRequest::Store {
                key,
                secret: secret()?,
                protection,
                prompt: prompt(),
            })
        }
        "get" => Ok(KeyringRequest::Get { key }),
        "unlock" => Ok(KeyringRequest::Unlock {
            key,
            prompt: prompt(),
        }),
        "delete" => Ok(KeyringRequest::Delete { key }),
        other => Err(format!(
            "keyring: unknown action '{other}' (store, store_protected, get, unlock, delete)"
        )),
    }
}

/// `keyring`: ask the keyring one thing, wait for the answer, check it against `expect`.
fn keyring(lw: &LayoutWindow, args: &serde_json::Value) -> Result<serde_json::Value, String> {
    only_params(
        "keyring",
        args,
        &[
            "action",
            "key",
            "secret",
            "protection",
            "reason",
            "cancel_label",
            "namespace",
            "expect",
            "expect_secret",
        ],
    )?;
    let channels = &lw.app_services.keyring;
    if attempt(args) == 0 {
        // A scenario never reaches the keyring of the user it runs for.
        if !lw.app_services.biometric.user.keyring_driven() {
            return Err(NOT_A_STAND_IN_KEYRING.to_string());
        }
        let request = keyring_request(args)?;
        let namespace = text(args, "namespace")
            .map_or_else(KeyringNamespace::app, |name| {
                KeyringNamespace::create(AzString::from(name))
            });
        channels.push_request_in(namespace, request);
        return Ok(pending());
    }
    if channels.has_queued_requests() || lw.keyring_manager.has_pending_async() {
        return Ok(pending());
    }
    let result = lw
        .keyring_manager
        .last_result()
        .ok_or_else(|| "keyring: the keyring gave no answer".to_string())?;
    let got = keyring_result_name(result);
    if let Some(expect) = text(args, "expect") {
        if normalised(expect) != got {
            return Err(format!("keyring: expected '{}', the keyring said '{got}'", normalised(expect)));
        }
    }
    if let Some(want) = text(args, "expect_secret") {
        match result.secret() {
            Some(secret) if secret.as_str() == want => {}
            Some(_) => return Err("keyring: the secret read is not the expected one".to_string()),
            None => return Err(format!("keyring: expected a secret, the keyring said '{got}'")),
        }
    }
    Ok(serde_json::json!({
        "success": true,
        "result": got,
        "secret_length": result.secret().map(|secret| secret.as_str().len()),
    }))
}

/// `biometric_prompt`: ask the standalone biometric gate, wait, check the answer against `expect`.
fn biometric_prompt(
    lw: &LayoutWindow,
    args: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    only_params(
        "biometric_prompt",
        args,
        &["reason", "cancel_label", "allow_device_credential", "expect"],
    )?;
    let channels = &lw.app_services.biometric;
    if attempt(args) == 0 {
        if !channels.user.gate_driven() {
            return Err(NOT_A_STAND_IN_GATE.to_string());
        }
        channels.requests.push(BiometricPrompt {
            reason: AzString::from(text(args, "reason").unwrap_or("")),
            cancel_label: AzString::from(text(args, "cancel_label").unwrap_or("")),
            allow_device_credential: args
                .get("allow_device_credential")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        });
        return Ok(pending());
    }
    if channels.has_queued_requests() || lw.biometric_manager.has_pending_async() {
        return Ok(pending());
    }
    let result = lw
        .biometric_manager
        .last_result()
        .ok_or_else(|| "biometric_prompt: the gate gave no answer".to_string())?;
    let got = biometric_result_name(result);
    if let Some(expect) = text(args, "expect") {
        if normalised(expect) != got {
            return Err(format!(
                "biometric_prompt: expected '{}', the gate said '{got}'",
                normalised(expect)
            ));
        }
    }
    Ok(serde_json::json!({ "success": true, "result": got }))
}

#[cfg(test)]
#[path = "standin_ops_test.rs"]
mod standin_ops_test;
