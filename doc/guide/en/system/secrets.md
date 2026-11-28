---
slug: system/secrets
title: Secrets and Biometrics
language: en
canonical_slug: system/secrets
audience: external
maturity: beta
guide_order: 126
topic_only: false
short_desc: Keep a secret in the system keyring behind the user's fingerprint, face or passcode - one model on macOS, iOS, Android, Windows and Linux - and ask for permissions only when needed
prerequisites: [events, events/callbacks]
tracked_files:
  - core/src/keyring.rs
  - core/src/biometric.rs
  - layout/src/managers/keyring.rs
  - layout/src/managers/biometric.rs
  - layout/src/managers/permission.rs
  - layout/src/managers/stand_in_user.rs
  - dll/src/desktop/extra/keyring/mod.rs
last_generated_rev: 754b7f00e088960c14db598f64fa200dacc28bf1
generated_at: 2026-11-28T00:00:00Z
default-search-keys:
  - keyring_store
  - keyring_store_protected
  - keyring_unlock
  - KeyringProtection
  - KeyringResult
  - KeyringSupport
  - needs_prompt
  - request_biometric_auth
  - open_app_settings
---

# Secrets and Biometrics

## Introduction

A secret - a vault key, a token - goes into the platform's own credential store, never
into your files. How strongly it is guarded is a choice you make when you store it, and
the same choice means the same on every platform:

| `KeyringProtection` | Who may read it | Shows a sheet |
|---|---|---|
| `DeviceUnlocked` | the app, while the user's session is unlocked | never |
| `UserPresence` | the app, after the user proved they are there: a biometric **or** the device passcode, PIN or password | on every read |
| `Biometric` | the app, after a biometric match; no passcode fallback; the item is **invalidated** when a finger or a face is added or removed | on every read |

The rules that hold everywhere:

1. The user-check sheet appears **only** for `keyring_unlock` and `keyring_store_protected`,
   both called from the callback of a user's action, with the reason text you give.
   `keyring_get`, every call of a worker thread and every quiet call answer
   `WouldPrompt` for a protected item and show nothing.
2. The answer says why. `Denied` is the system refusing your app and nothing else.
3. An item is never stored weaker than you asked: a device that cannot meet the level
   answers `NotEnrolled` or `Unavailable` and stores nothing.
4. A protected item has a record that tells its level without a sheet, so you can explain
   the sheet *before* it comes (`Keyring::needs_prompt`) and tell an invalidated item from
   one that was never stored.

## What guards the secret, per platform

`Keyring::support(level)` answers at run time what this device does for a level; this is
the table behind it.

| Platform | `UserPresence` | `Biometric` | Enforced by |
|---|---|---|---|
| iOS | `SecAccessControl` userPresence, read through `LAContext` | biometryCurrentSet | **hardware**: the Secure Enclave evaluates the access control and releases the key (type-checked and its decision tables host-tested; the Simulator proof is pending, and the Simulator does not enforce keychain access control the way a device does, so a device is the next proof) |
| macOS, signed with a keychain access group | the same on the data-protection keychain | biometryCurrentSet (Touch ID Macs) | **hardware** |
| macOS, unsigned (development builds) | not available (`Unavailable`) | not available (`Unavailable`) | - (the legacy keychain has no access-control items) |
| Android 9+ | a Keystore key per entry that needs the user (API 30+: biometric or device credential) opened with `BiometricPrompt` and a `CryptoObject` | biometric only, `setInvalidatedByBiometricEnrollment`, StrongBox where the device has one | **hardware** (the TEE or StrongBox releases the key after the auth token) |
| Windows 10+ | a Windows Hello key; the secret is sealed with the Hello signature as entropy | `Unavailable`: Hello takes a PIN as readily as a finger and cannot be told otherwise | **hardware** where the device has a TPM, else a software key behind the Hello gesture. **Windows Hello: not verified on hardware** - written to Microsoft's documented API and built by the Windows CI job, but no machine with Hello has run it yet |
| Linux | the desktop's polkit dialog (action `io.azul.keyring.unlock`; a package installs `scripts/linux/io.azul.keyring.policy` as `/usr/share/polkit-1/actions/io.azul.keyring.policy`, else `Keyring::support` says `NotEnrolled`) | the fingerprint reader (fprintd) | **the library**: azul asks the user, then reads an item the Secret Service holds; another process of the same user can read it without the check |

`Hardware` means the OS or the secure hardware gives the secret up only after the check.
`Library` means a boolean gate: azul runs the check and then reads an item that is not itself
bound to it. Where the table says the library, say so in your own UI ("protected by the app
lock") rather than "secured by hardware". A standalone `request_biometric_auth` is the same
kind of boolean - a yes with no secret behind it - so use it to confirm that the user is
there before a sensitive action, never to release a key.

## The flow

```rust,ignore
// Turning on "unlock with this device" (a button's callback):
match Keyring::support(KeyringProtection::Biometric) {
    KeyringSupport::Hardware | KeyringSupport::Library => {
        info.keyring_store_protected(
            "vault-key".into(),
            vault_key,
            KeyringProtection::Biometric,
            KeyringPrompt::create("Turn on unlocking with this device".into()),
        );
    }
    KeyringSupport::NotEnrolled => { /* "set up a fingerprint or face first" */ }
    KeyringSupport::Unavailable => { /* offer the password only */ }
}

// Before the sheet: say what is about to happen (no sheet shows for this question).
match Keyring::needs_prompt("vault-key".into()) {
    KeyringNeed::Biometric => { /* "Touch ID or Face ID will ask" */ }
    KeyringNeed::UserPresence => { /* "your fingerprint or passcode will ask" */ }
    KeyringNeed::Invalidated => { /* the fingers changed: ask for the password, store the key again */ }
    _ => { /* nothing to explain */ }
}

// Unlocking (the unlock button's callback). The sheet comes now, with this text:
info.keyring_unlock(
    "vault-key".into(),
    KeyringPrompt::create("Unlock your vault".into()),
);

// The answer arrives at a later callback of the KeyringResult event:
match info.get_keyring_result().into_option() {
    Some(KeyringResult::Retrieved(key)) => { /* open the vault */ }
    Some(KeyringResult::Cancelled | KeyringResult::Failed) => { /* offer the password */ }
    Some(KeyringResult::LockedOut) => { /* biometrics are locked: the password */ }
    Some(KeyringResult::Invalidated) => { /* the fingers changed: store the key again */ }
    Some(KeyringResult::NotEnrolled) => { /* nothing is enrolled any more */ }
    Some(KeyringResult::WouldPrompt) => { /* you called keyring_get on a protected item */ }
    _ => { /* NotFound, Denied, Unavailable, Error */ }
}
```

Background work reads without ever asking: `Keyring::get_quietly_in` answers `WouldPrompt` where
a sheet or the system's own dialog would have been needed, and the app waits for the user's own
action.

## Permissions

A permission dialog comes **only when the user triggered a feature that needs it**.

- `get_permission_status` never prompts: it reads the manager, which folds in the platform's
  quiet read once a second.
- A request goes out only after a user gesture (a click, a tap, a key press), one capability
  per gesture - a probe node in the first DOM never asks at launch.
- A denied permission is remembered and **not asked again**. Tell the user and offer
  `open_app_settings(capability)`, which opens the system's settings page for it and answers
  `Opened` or `Unsupported` (Linux has no such page).
- iOS and macOS ask for the Local Network permission when the first local-network announcement
  goes out; `IrohConfig::create` therefore leaves local discovery off, and an app turns it on
  with `with_local_discovery(true)` when the user starts pairing or a call.

## Testing

A test or headless run never reaches the system keyring or a sensor: the stand-in keyring has the
same semantics and a scripted user - what is enrolled, and what they do at each sheet (a match, a
mismatch, a cancel, a lockout, the passcode, a change of the enrolled fingers) - so the flows
above run the same on every host, a phone included.

## Moving from the first version

The first version took a `require_biometry` bool; it is gone. `keyring_store(key, secret, false)` is
`keyring_store(key, secret)` now, and the same for `Keyring::store_blocking`, `store_blocking_in` and
`store_quietly_in`. `keyring_store(key, secret, true)` is `keyring_store_protected(key, secret,
KeyringProtection::Biometric, prompt)`, and the read that showed the system's sheet is
`keyring_unlock(key, prompt)` (a plain `keyring_get` of a protected item answers `WouldPrompt` and shows
nothing). A quiet or blocking call that answered `Denied` because the system would have had to ask now
answers `WouldPrompt`; `Denied` is only the system refusing the app. A `match` over `KeyringResult` needs
the six new arms (`Cancelled`, `Failed`, `LockedOut`, `NotEnrolled`, `Invalidated`, `WouldPrompt`), a
`match` over `BiometricResult` the two new ones (`LockedOut`, `NotEnrolled`).
