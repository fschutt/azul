# 02 — Biometric authentication across 5 platforms

**Sprint:** SUPER_PLAN_2 §1 feature 4 (Security / identity).
**Author:** research-agent, 2026-05-19. **Status:** research / design — no code yet.

This brief inventories the native biometric-authentication APIs on each of the
five Azul-supported platforms (iOS, Android, macOS, Linux, Windows) and
proposes an `App::request_biometric_auth(...)` surface that mirrors the
existing native-injection seam (gesture manager). The W3C `WebAuthn` mapping
for the future web backend is in §9.

**Architecture pattern to mirror:**
*platform backend* → *manager override slot* → *`CallbackInfo` accessor*.
Modelled on `dll/src/desktop/shell2/<plat>/mod.rs::inject(...)` →
`GestureAndDragManager::inject_native_gesture` →
`CallbackInfo::get_gesture_drag_manager`. See
`layout/src/managers/gesture.rs:435` (override slot) and
`layout/src/callbacks.rs:3044` (read-side accessor).

---

## 0. Why biometric is *not* simply "is the user real?"

A correct integration delivers **two** outputs, not one:

1. **Authentication assertion** — boolean-like ("user successfully matched
   their face/finger/iris/passcode within X seconds, for the prompt I asked").
2. **Hardware-bound signed assertion** — a private key stored in the device's
   secure element (Secure Enclave / TrustZone / TPM / TEE) that is only
   unlockable by step 1. The app sends a server-issued challenge; the secure
   element returns a signature; the key never leaves the chip.

Apps shipping only step 1 are vulnerable to rooted devices, replay attacks, and
server impersonation. Step 2 (passkey / WebAuthn) gives hardware-backed
non-repudiation. **Azul's API exposes both** so naive callers ("unlock the
settings panel") get the cheap path and security-sensitive callers ("sign this
banking transaction") get the strong path.

W3C names: `UserVerification` (step 1) and `PublicKeyCredential` (step 2).

---

## 1. iOS — `LocalAuthentication` / `LAContext`

| Layer | Symbol |
|---|---|
| Framework | `LocalAuthentication.framework` (`-framework LocalAuthentication`). |
| Boolean auth | `-[LAContext evaluatePolicy:localizedReason:reply:]` with `LAPolicyDeviceOwnerAuthenticationWithBiometrics` (biometric only) or `LAPolicyDeviceOwnerAuthentication` (also accepts passcode). [^ios-eval] |
| Probe | `-[LAContext canEvaluatePolicy:error:]` → `LAErrorBiometryNotAvailable / NotEnrolled / Lockout`. |
| Biometric kind | `LAContext.biometryType` ∈ `{ .none, .touchID, .faceID, .opticID (visionOS) }`, iOS 11+. [^ios-biotype] |
| Hardware-bound key | `SecAccessControl` + `SecItemAdd` (Keychain) with `kSecAttrAccessControlBiometryCurrentSet` + `kSecAttrTokenIDSecureEnclave`; key gen via `SecKeyCreateRandomKey`; signing via `SecKeyCreateSignature`. Private key never leaves the device. [^ios-secenclave] |
| Passkey | `ASAuthorizationPlatformPublicKeyCredentialProvider` (AuthenticationServices, iOS 16+) — equivalent to WebAuthn from native code. [^ios-passkey] |

**Factors:** Face ID, Touch ID, Optic ID (visionOS), system passcode fallback.

**Async flow:** Callback block. `evaluatePolicy(...)` returns immediately;
reply block fires on an unspecified queue — must marshal to main for UI.
OS draws its own modal sheet; app cannot draw over it.

**Required declaration:** `Info.plist` key **`NSFaceIDUsageDescription`** (one
paragraph, shown in the OS prompt). Missing key → app is killed by TCC on
first attempt. No separate key for Touch ID. [^ios-plist]

**Prompt UI:** OS-dictated. App supplies `localizedReason`, optional
`localizedFallbackTitle`, optional `localizedCancelTitle`.

**Result:** `evaluatePolicy` returns `BOOL success + NSError? error`. Strong
result requires the Keychain item to be gated by the same `LAContext`, so
unlock = "Secure-Enclave key now usable for one signature".

**Risks:**
* Face ID lockout: 5 failed attempts → passcode required.
* `touchIDAuthenticationAllowableReuseDuration` lets one auth serve multiple
  ops within ~30s (likely reset on backgrounding; *TODO: verify*).
* `BiometryCurrentSet` invalidates the key on new enrollment — apps must
  handle re-creation.

---

## 2. macOS — `LocalAuthentication` / `LAContext`

Identical surface to iOS (`LAContext`, `evaluatePolicy:`,
`canEvaluatePolicy:`). [^mac-laguide] Differences:

* Touch ID only on MacBook Pro/Air with the fingerprint key. No Face ID
  on macOS today (*TODO: verify* whether 2026 macOS adds it via continuity).
* Apple Watch unlock is system-level (no app API).
* Secure Enclave on all Apple-silicon Macs; on Intel without T2, key is
  software-backed.

**Async / prompt:** Same callback-block pattern. **Declaration:** No
Info.plist key required for non-sandboxed AppKit apps (*TODO: verify* the
sandboxed case). Hardened-runtime entitlement
`com.apple.security.smartcard` may be needed for some Secure-Enclave flows
via CryptoTokenKit; `com.apple.developer.authentication-services.autofill-credential-provider`
is only needed for passkey *provider* apps. [^mac-entitle]

**Risks:** Mac-Catalyst apps present the *iOS* prompt; native AppKit (Azul's
path) presents the *macOS* prompt — different UX, same API.

---

## 3. Android — `BiometricPrompt` / `BiometricManager` (AndroidX)

| Layer | Symbol |
|---|---|
| Library | `androidx.biometric:biometric` — wraps API 28+ `android.hardware.biometrics.BiometricPrompt` and falls back to legacy `FingerprintManager`. [^and-prompt] |
| Probe | `BiometricManager.from(ctx).canAuthenticate(BIOMETRIC_STRONG \| DEVICE_CREDENTIAL)` → `BIOMETRIC_SUCCESS / NO_HARDWARE / HW_UNAVAILABLE / NONE_ENROLLED / SECURITY_UPDATE_REQUIRED`. [^and-manager] |
| Auth call | `new BiometricPrompt(activity, executor, callback).authenticate(promptInfo[, cryptoObject])` |
| Strength | `Authenticators.BIOMETRIC_STRONG` (Class 3), `BIOMETRIC_WEAK` (Class 2), `DEVICE_CREDENTIAL` (PIN/pattern/password). Only Class 3 may be paired with a `CryptoObject`. [^and-strength] |
| Hardware-bound key | `KeyGenParameterSpec.Builder.setUserAuthenticationRequired(true).setUserAuthenticationParameters(0, BIOMETRIC_STRONG)` → Android Keystore (TrustZone or StrongBox). A `CryptoObject` (Signature/Cipher/Mac) is passed to `authenticate`; auth result authorizes one use. [^and-keystore] |
| Passkey | `androidx.credentials.CredentialManager.createCredential` with `CreatePublicKeyCredentialRequest`. Equivalent to WebAuthn. [^and-passkey] |

**Factors:** Fingerprint, face (often Class 2, *not* Class 3), iris (Samsung
legacy), device credential. Class is set by the OEM during certification.

**Async flow:** Callback-based via `BiometricPrompt.AuthenticationCallback`
(success / error / failure), on the executor supplied at construction.

**Required declaration:**
* `AndroidManifest.xml`:
  `<uses-permission android:name="android.permission.USE_BIOMETRIC" />`
  (API 28+; older `USE_FINGERPRINT` is deprecated). [^and-perm]
* No runtime permission prompt — granted on install.
* AndroidX biometric supports API 23+ but feature availability varies.
  *TODO: verify* the minimum we ship.

**Prompt UI:** OS-dictated. App supplies title/subtitle/description/negative
button. System draws the bottom sheet; no skinning.

**Result:** `AuthenticationResult.getAuthenticationType()` → `BIOMETRIC` or
`DEVICE_CREDENTIAL`; `getCryptoObject()` non-null iff a CryptoObject was
passed in (strong path).

**Risks:**
* Class-2 face unlock is *commonly disabled* for banking apps — apps must
  pass `Authenticators.BIOMETRIC_STRONG` to `canAuthenticate(...)` to
  filter. [^and-strength]
* `setInvalidatedByBiometricEnrollment(true)` rotates the key on enroll —
  same pattern as iOS.

---

## 4. Windows — `UserConsentVerifier` + Windows Hello

| Layer | Symbol |
|---|---|
| Boolean auth | `Windows.Security.Credentials.UI.UserConsentVerifier.RequestVerificationAsync(message)` (WinRT). Returns `UserConsentVerificationResult ∈ { Verified, DeviceNotPresent, NotConfiguredForUser, DisabledByPolicy, DeviceBusy, RetriesExhausted, Canceled }`. [^win-uconsent] |
| Probe | `UserConsentVerifier.CheckAvailabilityAsync()` — same enum minus `Verified`. |
| Strong path | `KeyCredentialManager.RequestCreateAsync(name, KeyCredentialCreationOption.ReplaceExisting)` then `KeyCredential.RequestSignAsync(challenge)`. TPM-backed when present; Hello unlocks the key. [^win-keycred] |
| Modern passkey | Win32 `WebAuthn.dll` (`WebAuthNAuthenticatorMakeCredential` / `…GetAssertion`). [^win-webauthn] |

**Factors:** Face (IR camera), fingerprint, PIN (Hello PIN is hardware-bound,
not a soft fallback), security key (FIDO2 USB).

**Async flow:** WinRT `IAsyncOperation<...>` — the `windows` crate provides
`.await` impls. [^win-rs]

**Required declaration:**
* UWP / packaged: `Package.appxmanifest`:
  `<DeviceCapability Name="userAccountInformation" />`. *TODO: verify*
  exact minimum set for unpackaged Azul.
* Win32 desktop (Azul's realistic target): no manifest capability — but
  `RequestVerificationAsync` returns `NotConfiguredForUser` on systems
  without Hello set up. [^win-desktop]
* `KeyCredentialManager` on unpackaged Win32: callable via the `windows`
  crate; no special entitlement beyond the user being signed in with a
  Microsoft account or having Hello configured.

**Prompt UI:** OS-dictated. `RequestVerificationAsync` accepts only a single
message string; Hello paints its credential picker.

**Result:** `UserConsentVerificationResult` is a 7-variant enum — already the
right shape for Azul's `BiometricError` enum (we map directly).

**Risks:** WinRT-only surface for `UserConsentVerifier` — Win32 C entry
points exist for WebAuthn but not for the boolean-style flow. The `windows`
crate handles this. [^win-rs]

---

## 5. Linux — `polkit` + PAM (degraded experience)

No first-class biometric API. Three partial paths:

### 5a. `polkit` (PolicyKit) — for privileged system actions

* `libpolkit-agent-1` / `polkitd` D-Bus service. [^lin-polkit]
* D-Bus call to `org.freedesktop.PolicyKit1.Authority.CheckAuthorization`
  (Azul already speaks D-Bus; see `dll/src/desktop/shell2/linux/dbus/`).
* When `libpam-fprintd` (Fedora/Debian fingerprint) or `howdy` (face unlock)
  is configured, the polkit agent will route through it. **The app
  doesn't ask for biometric** — it asks polkit "can the user perform
  $action"; polkit decides what to prompt with.
* No way for an app to force a biometric prompt or receive a signed
  assertion. Returns `is_authorized: bool`.

### 5b. PAM + `pam_fprintd`

* `libpam` via `pam_start("login", user, &conv, &handle)` +
  `pam_authenticate(handle, 0)`.
* `pam_fprintd` and `howdy`'s PAM module participate when the stack is
  configured for them. *Synchronous* / *blocking* — must be wrapped in a
  thread (which fits `core/src/task.rs` Threadable pattern). [^lin-pam]
* Boolean result. No hardware-bound key.

### 5c. FIDO2 USB security keys via `libfido2`

* `libfido2` / `webauthn`-stack libraries. [^lin-fido2]
* Hardware-bound key, but only for plugged-in tokens — not the laptop's
  fingerprint reader. The only Linux path that maps cleanly onto W3C
  WebAuthn.

**Factors:** Fingerprint (fprintd), face (howdy, unofficial), FIDO2 token.
No iris. No unified system password fallback.

**Required declaration:** None — but the fingerprint device requires
read/write access (`plugdev` / `input` group + udev rules on most distros).

**Prompt UI:** For polkit: the session's polkit agent paints the prompt
(`polkit-gnome-authentication-agent`, `polkit-kde-authentication-agent`,
…). Apps cannot customize. For raw PAM: the *app* draws its own dialog
via a `pam_conv` callback — Azul has to render a password modal itself.

**Result:** `PolicyResult { is_authorized: bool, is_challenge: bool, ... }`.
Boolean. No signed assertion (unless using libfido2).

**Risks:** Biggest UX gap of the 5 platforms. Realistic Azul story: "we
sometimes route via polkit; we cannot promise a fingerprint prompt; the API
returns `BiometricError::Unsupported { fallback_suggested: PasswordModal }`
on systems where polkit refuses or no PAM modules are configured." Wayland:
the polkit agent must be a Wayland client; some sessions don't start one.

---

## 6. Cross-platform comparison

| | iOS | macOS | Android | Windows | Linux |
|---|---|---|---|---|---|
| Boolean API | `LAContext.evaluatePolicy` | same | `BiometricPrompt.authenticate` | `UserConsentVerifier.RequestVerificationAsync` | `polkit` (degraded) |
| Hardware key | Secure Enclave + Keychain | Secure Enclave (Apple silicon) | Keystore + `setUserAuthenticationRequired` | `KeyCredentialManager` / `WebAuthn.dll` | `libfido2` (tokens only) |
| WebAuthn equiv. | `ASAuthorization…` (iOS 16+) | `ASAuthorization…` (macOS 13+) | `androidx.credentials` | `WebAuthn.dll` | `libfido2` |
| Async style | Block | Block | Java `Executor` | WinRT `IAsyncOperation` | D-Bus / blocking |
| OS-drawn prompt | yes | yes | yes | yes | desktop agent (no app control) |
| Boolean factors | face, finger, optic, passcode | finger, watch, password | finger, face (class-dep), iris (legacy), dev cred | face, finger, PIN, FIDO2 | finger, face (unofficial), token |
| Declaration | `NSFaceIDUsageDescription` | (sandbox-dep) | `USE_BIOMETRIC` | (none for Win32) | (none) |
| Result struct | `BOOL + error` | same | `AuthenticationResult` (rich) | rich enum | `bool` |

---

## 7. Integration sketch for Azul

### 7.1 Architecture seams reused

Following native-gesture injection (`layout/src/managers/gesture.rs:435`,
`dll/src/desktop/shell2/<plat>/mod.rs::inject_native_gesture`), biometric
auth lands as:

1. `BiometricManager` at `layout/src/managers/biometric.rs` — pending
   requests + completion override slot.
2. Platform injection seam:
   `BiometricManager::inject_native_result(req_id, NativeBiometricResult)`.
3. `CallbackInfo` accessor for read-side state.
4. The *request* method lives on `App` (not `CallbackInfo`) because it must
   spawn an OS-side prompt and feed results back — sits at the level of
   `App::create_thread` (`core/src/task.rs:128`).
5. `core/src/events.rs` gains an `EventFilter::Biometric(BiometricEvent)`
   for completion fan-out (callbacks observe completions per node via the
   same event-filter pipeline used everywhere else).

### 7.2 Types (new module `core/src/biometric.rs`)

```rust
#[derive(Debug, Clone)] #[repr(C)]
pub struct BiometricAuthOptions {
    /// Reason shown in the OS prompt (Info.plist parity, iOS).
    pub reason: AzString,
    /// Negative-button / cancel label (Android / Windows).
    pub cancel_label: OptionAzString,
    /// "Use password" fallback label (iOS `localizedFallbackTitle`).
    pub fallback_label: OptionAzString,
    /// Strength: Weak (any) | Strong (Class-3 / Secure-Enclave / TPM).
    pub strength: BiometricStrength,
    /// Challenge for the hardware-bound strong path. None → boolean-only.
    pub challenge: OptionU8Vec,
    /// Allow OS-supplied passcode/PIN fallback if biometrics fail.
    pub allow_device_credential_fallback: bool,
}

#[derive(Debug, Clone)] #[repr(C, u8)]
pub enum BiometricStrength {
    /// Class-2 / face-unlock on Android-Class-2 / no-key on Linux.
    Weak,
    /// Class-3 / Secure-Enclave-bound / TPM-bound. Required when
    /// `challenge` is Some.
    Strong,
}

#[derive(Debug, Clone)] #[repr(C, u8)]
pub enum BiometricAuthResult {
    /// `signed_assertion` is Some iff a challenge was supplied AND the
    /// platform produced a hardware-bound signature.
    Approved {
        signed_assertion: OptionU8Vec,
        biometric_kind: BiometricKind,
        used_fallback: bool, // device credential used
    },
}

#[derive(Debug, Clone)] #[repr(C, u8)]
pub enum BiometricError {
    Cancelled,
    Denied,
    LockedOut,
    NotEnrolled,
    Disabled,
    /// `fallback_suggested` tells the caller what to try next.
    Unsupported { fallback_suggested: BiometricFallback },
    /// Caller asked for Strong but only Weak is available.
    StrengthUnavailable,
    /// Enrollment changed; strong-path key was invalidated.
    KeyInvalidated,
    PlatformError { code: i32, message: AzString },
}

#[derive(Debug, Clone)] #[repr(C, u8)]
pub enum BiometricFallback {
    /// Render an in-app password modal (Linux when polkit absent).
    PasswordModal,
    /// Route via polkit for system-action authorization.
    PolicyKit,
    /// Suggest the user enroll in settings.
    SettingsEnroll,
    None,
}

#[derive(Debug, Clone, Copy)] #[repr(C, u8)]
pub enum BiometricKind { Face, Fingerprint, Iris, DeviceCredential, FidoToken, Unknown }
```

### 7.3 App-level request

Sync vs async: callback-style continuation (matches the rest of azul; see
`core/src/callbacks.rs:542` `CallbackType`). A Rust-only `async fn` is
rejected because (a) bindings to 35 languages can't consume Rust futures;
(b) `azul-doc autofix` doesn't model async fns.

```rust
impl App {
    /// Returns a request id immediately. When the OS prompt completes the
    /// result is delivered to `completion` on the main thread.
    pub fn request_biometric_auth(
        &mut self,
        options: BiometricAuthOptions,
        userdata: RefAny,
        completion: extern "C" fn(RefAny, BiometricAuthRequestId, BiometricAuthResultEnum) -> Update,
    ) -> BiometricAuthRequestId;
}
```

### 7.4 Per-platform shim

Each backend gets a new file `dll/src/desktop/shell2/<plat>/biometric.rs`:

* **iOS / macOS:** `LAContext.evaluatePolicy` on the main queue; capture
  reply block; for Strong path, add Secure-Enclave key gen + sign via
  Keychain.
* **Android:** JNI bridge mirroring `scripts/android/NativeGestureBridge
  .java` — `BiometricBridge.kt` invokes `BiometricPrompt.authenticate`;
  `AuthenticationCallback` calls back into Rust JNI.
* **Windows:** `windows` crate's
  `UserConsentVerifier::RequestVerificationAsync` + `.await`; for Strong,
  `KeyCredentialManager.RequestCreateAsync` + `.RequestSignAsync`.
* **Linux:** Probe via D-Bus to `org.freedesktop.PolicyKit1`. If polkit
  absent or action unregistered, return
  `BiometricError::Unsupported { fallback_suggested: PasswordModal }`
  *synchronously* from the request call. (No background thread.)

Each shim, on completion, calls
`LayoutWindow::biometric_manager.inject_native_result(req_id, ...)`. The
event loop tick drains pending completions and fires the user's
`completion` callback — exactly as native gestures are drained today
(`dll/src/desktop/shell2/common/event.rs:2108`).

### 7.5 Manager shape (mirrors `gesture.rs:410`)

```rust
// layout/src/managers/biometric.rs
#[derive(Debug, Clone, PartialEq)]
pub struct BiometricManager {
    pending: BTreeMap<BiometricAuthRequestId, PendingRequest>,
    /// Native-platform completion override slot. Platforms push results
    /// here; the event loop drains on the next tick.
    pub native_results: Vec<(BiometricAuthRequestId, NativeBiometricResult)>,
    next_id: u64,
}

impl BiometricManager {
    pub fn inject_native_result(&mut self, id: BiometricAuthRequestId,
                                 result: NativeBiometricResult) {
        self.native_results.push((id, result));
    }
    // drain_completions, register, cancel, ...
}
```

### 7.6 CallbackInfo accessor

```rust
impl CallbackInfo {
    pub fn get_biometric_manager(&self) -> &BiometricManager { ... }
    pub fn cancel_biometric_auth(&mut self, id: BiometricAuthRequestId) {
        self.push_change(CallbackChange::CancelBiometric { id });
    }
}
```

### 7.7 Codegen / api.json

`azul-doc autofix add BiometricAuthOptions ...` — all new types flow
through the existing codegen pipeline; 35 bindings get them for free. The
`extern "C" fn` completion signature is already supported.

---

## 8. Linux fallback policy (explicit)

* If `polkit` is available **and** the requested action is registered in
  `/usr/share/polkit-1/actions/`: route through polkit. Return
  `Approved { signed_assertion: None, kind: Unknown }` on success.
* Else if `pam_fprintd` is configured on the user's session: spin a
  worker thread, call `pam_authenticate`, return `Approved { ... }`.
* Else: return
  `BiometricError::Unsupported { fallback_suggested: PasswordModal }`
  synchronously. Caller renders a password modal — *not* the OS
  password modal (Linux doesn't reliably have one); a plain Azul-drawn
  modal that hashes the password against a stored Argon2 hash on the
  app side. *TODO: verify* whether `libsecret` is a better secondary
  store on Linux.

This matches SUPER_PLAN_2 §0's "typed `Unsupported` error rather than
failing silently" rule.

---

## 9. Web backend mapping (WebAuthn)

The browser maps the **strong** path naturally:

```js
// Mapped from App::request_biometric_auth({ challenge, strength: Strong, ... })
const cred = await navigator.credentials.get({
  publicKey: {
    challenge: new Uint8Array(/* options.challenge */),
    userVerification: "required",
    rpId: location.host,
    allowCredentials: [],
  },
});
// cred.response.signature → BiometricAuthResult::Approved { signed_assertion: Some(...) }
```

For the boolean-only path (no challenge), the web has *no exact match*.
`PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable()` is a
*capability probe*, not an authentication. **Recommendation:** when on the
web backend, refuse `BiometricStrength::Weak` without a challenge and emit
`BiometricError::Unsupported { fallback_suggested: None }`. This is
web-correct — the platform genuinely does not have a "is the user real?"
primitive separate from a signed assertion.

---

## 10. Risks & gotchas (cross-cutting)

1. **UX inconsistency.** The OS draws every prompt. Apps cannot match their
   own branding. The API should not pretend otherwise.
2. **Lockout policies vary.** iOS: 5 attempts → passcode. Android:
   vendor-defined. Windows: configurable per-domain via group policy.
   Linux: PAM defines it. Surface this via `BiometricError::LockedOut`
   and do not try to programmatically reset.
3. **Accessibility.** Voice-over / TalkBack users may not realize a prompt
   is up. Apps should announce via `layout/src/managers/a11y.rs` in
   parallel.
4. **Enrollment rotates the key.** iOS + Android invalidate strong-path
   keys when the user enrolls a new face / finger.
   `BiometricError::KeyInvalidated` is a separate variant so apps can
   re-enroll on the server.
5. **No biometric equals identity.** It's "the user has access to a device
   that has someone's biometric enrolled." For *identity*, the strong path
   + server challenge is non-negotiable.
6. **Reuse window.** iOS
   `touchIDAuthenticationAllowableReuseDuration`, Android
   `setUserAuthenticationParameters(timeout, ...)`. *TODO: verify*
   whether these survive backgrounding on each platform.
7. **Threading.** Strong-path Keychain / Keystore APIs are fine off the
   main thread on iOS / Android; Windows WinRT must marshal back to the UI
   thread. The platform shim handles this.
8. **Linux is hard-degraded.** Don't pretend otherwise. Cross-platform
   secret storage on Linux should hash-bind to user password via
   PasswordModal.

---

## 11. Open questions / TODO: verify

* `NSFaceIDUsageDescription` for Catalyst apps — likely same as iOS.
* macOS sandboxed AppKit `NSFaceIDUsageDescription` requirement.
* AndroidX biometric minimum API for `BIOMETRIC_STRONG + CryptoObject`
  (believed API 28+).
* Windows: `UserConsentVerifier` on unpackaged Win32 without manifest
  entries (believed yes since 10 1809).
* Linux: distro-by-distro test matrix for `pam_authenticate` against
  `pam_fprintd` from a non-suid app (Fedora / Ubuntu / Arch).
* visionOS Optic ID — does it report as `LABiometryType.opticID` or alias
  to faceID?
* Whether to expose one `App::request_biometric_auth` or split into
  `request_biometric_simple` (bool) / `request_biometric_assertion`
  (challenge required). Single fn is cleaner for codegen and lets us
  promote any caller from weak → strong by adding a challenge.
* macOS Face ID 2026 — does it exist via continuity?

---

## 12. Suggested implementation ordering for the eventual sprint

1. Define `BiometricAuthOptions / Result / Error / Strength / Kind` in
   `core/src/biometric.rs` + add to api.json. (~0.5 day)
2. Stub `BiometricManager` with override slot + drain method. (~0.5 day)
3. Wire `CallbackInfo` accessor + `App::request_biometric_auth` returning
   `Unsupported { None }` on every platform. Green-light codegen first.
   (~1 day)
4. iOS shim — `LAContext` + Secure Enclave key. (~1.5 days)
5. Android shim — `BiometricPrompt` + Keystore + JNI bridge. (~2 days)
6. macOS shim — same as iOS minus Info.plist key. (~0.5 day)
7. Windows shim — `UserConsentVerifier` + `KeyCredentialManager`.
   (~1.5 days)
8. Linux shim — polkit probe + PasswordModal fallback. (~1 day)
9. WebAuthn mapping for the future web backend — design only this sprint;
   defer impl. (~0 days now)
10. `scripts/mobile/golden/biometric.png` snapshot test for the
    "biometric prompt → callback fires" happy path on iOS + Android.

Total: ~8.5 person-days (cooler estimate ~10–12 with codegen/binding
ripple, lockout / KeyInvalidated edge cases, and the snapshot tests).

---

## References

[^ios-eval]: Apple, *Logging a User into Your App with Face ID or Touch ID*, developer.apple.com/documentation/localauthentication/logging_a_user_into_your_app_with_face_id_or_touch_id (retrieved 2026-05-19).
[^ios-biotype]: Apple, *LABiometryType*, developer.apple.com/documentation/localauthentication/labiometrytype.
[^ios-secenclave]: Apple, *Protecting Keys with the Secure Enclave*, developer.apple.com/documentation/security/protecting_keys_with_the_secure_enclave.
[^ios-passkey]: Apple, *Supporting Passkeys*, developer.apple.com/documentation/authenticationservices/public-private_key_authentication.
[^ios-plist]: Apple, *Information Property List — NSFaceIDUsageDescription*, developer.apple.com/documentation/bundleresources/information_property_list/nsfaceidusagedescription.
[^mac-laguide]: Apple, *Local Authentication — macOS*, LAContext is identical on macOS 10.13+.
[^mac-entitle]: Apple, *Hardened Runtime — Entitlements*, developer.apple.com/documentation/security/hardened_runtime.
[^and-prompt]: Android Developers, *BiometricPrompt*, developer.android.com/reference/androidx/biometric/BiometricPrompt.
[^and-manager]: Android Developers, *BiometricManager*, developer.android.com/reference/androidx/biometric/BiometricManager.
[^and-strength]: Android, *Biometric authentication classes (CDD)*, source.android.com/docs/security/features/biometric/measure.
[^and-keystore]: Android Developers, *Hardware-backed Keystore*, developer.android.com/training/articles/keystore.
[^and-passkey]: Android Developers, *Credential Manager*, developer.android.com/training/sign-in/credential-manager.
[^and-perm]: Android Developers, *USE_BIOMETRIC permission*, developer.android.com/reference/android/Manifest.permission#USE_BIOMETRIC.
[^win-uconsent]: Microsoft Learn, *UserConsentVerifier Class*, learn.microsoft.com/uwp/api/windows.security.credentials.ui.userconsentverifier.
[^win-keycred]: Microsoft Learn, *KeyCredentialManager*, learn.microsoft.com/uwp/api/windows.security.credentials.keycredentialmanager.
[^win-webauthn]: Microsoft Learn, *WebAuthn API*, learn.microsoft.com/windows/win32/api/webauthn.
[^win-rs]: `windows-rs` crate, microsoft.github.io/windows-docs-rs/doc/windows/Security/Credentials/UI/struct.UserConsentVerifier.html.
[^win-desktop]: Microsoft Learn, *Calling WinRT APIs in desktop apps*, learn.microsoft.com/windows/apps/desktop/modernize/desktop-to-uwp-enhance.
[^lin-polkit]: FreeDesktop, *polkit Reference Manual*, www.freedesktop.org/software/polkit/docs/latest/.
[^lin-pam]: linux-pam.org PAM module writers' guide; `pam_fprintd` upstream at fprint.freedesktop.org.
[^lin-fido2]: Yubico, *libfido2*, developers.yubico.com/libfido2/.
[^web-webauthn]: W3C, *Web Authentication: An API for accessing Public Key Credentials, Level 3*, www.w3.org/TR/webauthn-3/.
