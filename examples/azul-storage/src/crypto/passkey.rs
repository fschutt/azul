//! A passkey as a drive's recovery method (D51, WebAuthn PRF): DESIGNED, NOT BUILT. Nothing
//! here runs yet; this module holds the design and the two derivation contexts it will use,
//! so that the next step builds against a decided shape.
//!
//! # What it is for
//!
//! The plan's recommended second method beside the recovery code: AzDrive registers a passkey
//! for the drive; the passkey's PRF output (the WebAuthn `prf` extension, CTAP2 `hmac-secret`
//! underneath) wraps the drive key. The passkey syncs through the user's platform account
//! (iCloud Keychain, Google Password Manager) or a password manager, so a new phone or computer
//! signed into that account gets the drive back without paper. The weakness the plan names:
//! the platform account becomes the recovery, and PRF support differs between managers. Like
//! every method it never skips the 48 hour notice (D42).
//!
//! # The platform APIs
//!
//! | platform | registration | assertion with PRF |
//! |---|---|---|
//! | macOS 15, iOS 18 | `ASAuthorizationPlatformPublicKeyCredentialProvider(relyingPartyIdentifier:)` `.createCredentialRegistrationRequest(challenge:name:userID:)` with `.prf = .checkForSupport`; the registration's `prf.isSupported` | `.createCredentialAssertionRequest(challenge:)` with `.prf = .inputValues(.init(saltInput1:))`; the assertion's `prf.first` (a 32-byte `SymmetricKey`). The app needs the `webcredentials:` associated domain of the relying party |
//! | Windows 10 / 11 | `WebAuthNAuthenticatorMakeCredential` (webauthn.dll) with the `hmac-secret` extension (newer API versions also take PRF directly) | `WebAuthNAuthenticatorGetAssertion` with the HMAC secret salts (`WEBAUTHN_HMAC_SECRET_SALT_VALUES`), the result's HMAC secret. Security keys with `hmac-secret` work everywhere; Windows Hello's own passkeys where the system's API version offers PRF (the version and field names to be confirmed against webauthn.h when it is built) |
//! | Android 9+ | Credential Manager (`androidx.credentials`): `CreatePublicKeyCredentialRequest(requestJson)` with `"extensions": {"prf": {}}` | `GetPublicKeyCredentialOption(requestJson)` with `"extensions": {"prf": {"eval": {"first": "<base64url salt>"}}}`; `clientExtensionResults.prf.results.first`. The relying party's Digital Asset Links file names the app |
//! | Linux | none from the system | later: libfido2 for security keys with `hmac-secret`; until then the method is not offered |
//!
//! The relying party is the Azlin account domain (the plan's RP id); `userID` is a random
//! 32-byte handle per drive (never the drive id), `name` the drive's name as the kit shows it.
//!
//! # The azul API it needs (api.json terms; module `passkey`, resumable like `FileDialog`)
//!
//! - `Passkey.is_available() -> bool`: the platform offers passkeys with PRF.
//! - `Passkey.register(rp_id: String, user_id: U8Vec, user_name: String, challenge: U8Vec,
//!   data: RefAny, on_result: ResumeCallback)`: resumes with a `PasskeyRegistration`
//!   (`credential_id: U8Vec`, `prf_supported: bool`, `status: PasskeyStatus`);
//!   `PasskeyRegistration.downcast(result: RefAny) -> OptionPasskeyRegistration`.
//! - `Passkey.prf(rp_id: String, credential_ids: U8VecVec, challenge: U8Vec, salt: U8Vec,
//!   data: RefAny, on_result: ResumeCallback)`: resumes with a `PasskeyPrf`
//!   (`credential_id: U8Vec`, `output: OptionU8Vec` - 32 bytes, `status: PasskeyStatus`);
//!   `PasskeyPrf.downcast(result: RefAny) -> OptionPasskeyPrf`. An empty `credential_ids`
//!   asks for any discoverable passkey of the relying party.
//! - `PasskeyStatus` (`repr(C)` enum): `Ok`, `Cancelled`, `NoPrf` (the authenticator has no
//!   PRF), `NoCredential`, `Failed`; `PasskeyRegistration.create(...)` / `PasskeyPrf.create(...)`
//!   as constructors.
//! - fn_body paths into the dll's `desktop::extra::passkey` (a module beside `biometric` and
//!   `keyring`), one file per platform; the E2E mock store gets a `passkey` key
//!   (`{"prf": "<64 hex>"}` or `null`) so scripts register and assert without a system dialog.
//!
//! # How the PRF output wraps the drive key
//!
//! Registration (on a device that holds the drive key):
//! 1. a random 32-byte `salt` per passkey; the PRF input is `salt` (the authenticator itself
//!    hashes it as `SHA-256("WebAuthn PRF" || 0x00 || salt)`);
//! 2. an assertion right after the registration yields `prf` (32 bytes);
//! 3. the wrapping key is BLAKE3 `derive_key(`[`PASSKEY_WRAP_CONTEXT`]`, prf || credential id
//!    || drive)`: a context of its own, so the PRF output keys nothing else;
//! 4. the drive key is sealed with XChaCha20-Poly1305 under it, a random nonce, the associated
//!    data `"Azlin passkey drive key v1"` with the drive, the credential id, the salt and the
//!    drive key's id (as `keys::associated_data` builds it);
//! 5. the key file `.azlin/keys/passkey-<BLAKE3 of the credential id, 32 hex>.key` (format
//!    `azlin-drive-key` version 1, kind `passkey`): the credential id, the salt, the nonce, the
//!    sealed key, the drive key's id - all in hex, nothing that names a person.
//!
//! The lockdown: the same PRF output derives an Ed25519 seed with
//! [`PASSKEY_LOCKDOWN_CONTEXT`] (over `prf || drive`); its public key is registered at the token
//! server as the drive's passkey recovery key, and signs `lockdown:<drive>:<nonce>` exactly as
//! the recovery code's key does - the server holds it 48 hours for the owner's devices to
//! cancel, then hands the drive over; after that the new computer reads the passkey's key file
//! and opens the drive key. That needs ONE change at the token server: more than one recovery
//! key per drive (`POST /v1/drives/{id}/recovery {"recovery_pubkey", "method": "passkey",
//! "credential": "<hash>"}`), each lockdown naming the key it was signed with - an open
//! question for the server's owners. (The alternative - the passkey sealing the recovery code
//! instead of the drive key, as trusted contacts' shares do - needs the sealed code readable
//! before the bucket is, so it needs a blob at the token server instead.)
//!
//! A rotation (a new drive key) seals the new key to every passkey only with a fresh assertion
//! of each; until then the old passkey files are removed with the old wraps and the methods
//! list shows the passkey as gone.

/// BLAKE3 `derive_key` context of a passkey wrap's wrapping key (from the PRF output).
pub const PASSKEY_WRAP_CONTEXT: &str = "Azlin 2026-10-10 passkey PRF: drive key wrapping";
/// BLAKE3 `derive_key` context of a passkey's lockdown signing seed (from the PRF output).
pub const PASSKEY_LOCKDOWN_CONTEXT: &str = "Azlin 2026-10-10 passkey PRF: lockdown signing key";
