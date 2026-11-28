//! Platform dispatcher for system-keyring operations
//! (SUPER_PLAN_2 §4 P4.2).
//!
//! Cross-platform state lives in
//! `azul_layout::managers::keyring::KeyringManager`. A callback queues a
//! `KeyringRequest` (`CallbackInfo::keyring_store/get/delete`); the
//! capability pump drains it and calls [`request`] here, which turns each
//! op into the right native keyring call and parks its answer in the result
//! channel (a later frame's `KeyringResult` event):
//!
//! | Platform | Backend | A protected item (`UserPresence` / `Biometric`) |
//! |----------|---------|--------------------------------------------------|
//! | iOS / macOS | Keychain `SecItemAdd` / `SecItemCopyMatching` / `SecItemDelete` (Security.framework) | `SecAccessControl` (`userPresence` / `biometryCurrentSet`), read through an `LAContext`: released by the Secure Enclave after the check |
//! | Android | `KeyStore` keys + ciphertext in the app's storage (JNI helper) | a key per entry that needs the user (`setUserAuthenticationRequired`), opened through a `BiometricPrompt` `CryptoObject` |
//! | Linux | libsecret (`secret_password_store/lookup/clear`) via the secret-service D-Bus | the library's gate: fprintd / polkit first, then the item (`KeyringSupport::Library`) |
//! | Windows | Credential Manager (`CredWriteW` / `CredReadW` / `CredDeleteW`, generic credentials) | a Windows Hello key: the secret is sealed (DPAPI) with the Hello signature of a fixed challenge as entropy |
//!
//! One model on every platform (`azul_core::keyring`): the user-check sheet shows only for an
//! `Unlock` and a protected `Store` ([`Interaction::UserCheck`]); every other op, and every op
//! of a worker thread, answers `WouldPrompt` for what would need one. A protected item has a
//! record readable without the sheet (its level), so [`need_in`] can say what a read needs and
//! `Invalidated` can be told from `NotFound`.
//!
//! A protected `Unlock` parks its outcome in the app's result queue (`KeyringChannels::results`)
//! asynchronously from the OS prompt's reply.
//!
//! [`request_blocking`] (the C API's [`Keyring`]: `Keyring::get_blocking`,
//! `store_blocking`, `delete_blocking` and their `_in` forms) runs one op on the CALLING thread and
//! returns its answer: what an app's worker `Thread` reads and writes the
//! keyring with - it re-reads a secret another process may have changed (an
//! Azlin drive's rotating token) and writes one before its work goes on. Both
//! ways reach the same keyring. It never shows the user-check sheet and reads and writes
//! UNPROTECTED items only: a protected one answers `WouldPrompt`.
//!
//! Every entry lives in a [`KeyringNamespace`] - the keyring service - so azul
//! apps do not read each other's secrets by accident: an app's own (its app
//! id) unless a call names one, which apps that share entries do (the Azlin
//! apps' `io.azlin.shared`). An entry still under the service every app shared
//! before (`com.azul.keyring`) moves into the first namespace that touches its
//! name ([`answer_in`]).
//!
//! # Test hooks (cargo feature `test-hooks`; a shipped build has none)
//!
//! The environment variables below are TEST SWITCHES. They are read ONLY by a build with the
//! cargo feature `test-hooks` (the unit tests of this module and the E2E builds turn it on:
//! `cargo build --release -p azul-dll --features build-dll,test-hooks`). A release or
//! distribution build never looks at them ([`RunFacts::from_env`]): a variable in a shell
//! profile, a launcher's environment or a wrapper script cannot send a user's drive keys to
//! process memory (lost at exit, an encrypted drive locked) or into a plaintext file.
//!
//! In a build with the hooks, headless / E2E runs (`AZ_BACKEND=headless`, `AZ_E2E_TEST`) never
//! touch the REAL host secret store: both ways reach the stand-in of `memory.rs` - one
//! map for the process, or the file `AZ_KEYRING_FILE` names (which keeps it
//! for the next run of a test and shares it between the test's processes).
//! A windowed run that names such a file keeps its keyring there too: an app
//! on a test stack, in a real window, then shares no entry with the same app
//! of the user's own (the system keyring's entries are the app's, not the
//! stack's). The file holds the secrets in plain text - it is made readable by its owner alone
//! (0600) -: test stacks only. `AZ_KEYRING` picks the keyring outright: `system`,
//! `file` (the one `AZ_KEYRING_FILE` names) or `memory` ([`backend_for`]).
//!
//! A debug build of libazul keeps its secrets in memory as well, with or without the hooks:
//! every rebuild is a new code signature, and the macOS keychain asks the user again for each
//! entry such a binary reads.
//!
//! [`request_quietly_blocking`] (the C API's `Keyring::get_quietly_in`,
//! `store_quietly_in` and `delete_quietly_in`) runs
//! an op that never shows a prompt - what an app's background work uses, so a
//! start, a timer or a poll never raises the keychain's dialog. Where the
//! system would have to ask (an item behind Touch ID, another app's item, a
//! locked keychain) it answers `WouldPrompt`, and the app waits for the user's own
//! action to ask. Only the Apple keychain prompts per item; the Credential
//! Manager never asks, and libsecret asks only to unlock a locked collection (the login keyring
//! is unlocked at login): a quiet op looks at the default collection first ([`quiet_verdict`]) and
//! answers `WouldPrompt` while it is locked, and for a store where there is no default collection yet
//! (making one asks for a password). A libsecret too old to say whether a collection is locked
//! (before 0.18) leaves that look out, and its unlock dialog can still come.

use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{Mutex, PoisonError},
};

use azul_core::keyring::{
    KeyringNamespace, KeyringNeed, KeyringProtection, KeyringPrompt, KeyringRequest, KeyringResult,
    KeyringSupport,
};
use azul_css::AzString;
use azul_layout::managers::{shared_queue::SharedQueue, stand_in_user::StandInUser};

#[cfg(target_os = "android")]
pub mod android;
#[cfg(any(target_os = "ios", target_os = "macos"))]
pub mod apple;
#[cfg(target_os = "linux")]
pub mod linux;
mod memory;
pub(crate) mod record;
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(test)]
mod tests;

/// The variable naming the file that keeps a run's keyring instead of the
/// system's (a test stack's runs, windowed or headless). Read by builds with the cargo feature
/// `test-hooks` only.
pub const KEYRING_FILE_VAR: &str = azul_core::run_mode::KEYRING_FILE_VAR;

/// The variable that picks a run's keyring: `system`, `file` (the one
/// [`KEYRING_FILE_VAR`] names) or `memory`. Read by builds with the cargo feature `test-hooks`
/// only.
pub const KEYRING_CHOICE_VAR: &str = azul_core::run_mode::KEYRING_CHOICE_VAR;

/// Present only in a libazul built with the cargo feature `test-hooks`: the scripts that need the
/// keyring's test switches look for it in the library file (`grep -a`) before they run an app
/// against it, so a test run never falls back to the user's own keychain by mistake.
#[cfg(feature = "test-hooks")]
#[no_mangle]
#[used]
pub static AZUL_KEYRING_TEST_HOOKS: [u8; 32] = *b"azul-keyring-test-hooks-enabled\0";

/// What a keyring op may show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Interaction {
    /// The op of a user's action, through the request channel: the user-check sheet (biometric /
    /// passcode / Hello / polkit) and the system's own dialogs (a keychain unlock, an "allow
    /// this app" dialog) may show.
    UserCheck,
    /// A worker thread's blocking call: the system's own dialogs may show, the user-check sheet
    /// never (an op that needs it answers `WouldPrompt`).
    Dialogs,
    /// Background work: nothing shows. An op the system would have to ask for, or that needs the
    /// user-check sheet, answers `WouldPrompt`.
    Quiet,
}

impl Interaction {
    /// Whether the user-check sheet may show.
    pub(crate) const fn user_check(self) -> bool {
        matches!(self, Self::UserCheck)
    }

    /// Whether nothing at all may show.
    #[cfg_attr(
        not(any(target_os = "linux", target_os = "macos", target_os = "ios")),
        allow(dead_code) // only the Apple keychain and libsecret have a dialog to leave out
    )]
    pub(crate) const fn quiet(self) -> bool {
        matches!(self, Self::Quiet)
    }
}

/// Where a run's keyring ops go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Backend {
    /// The platform's keyring.
    Platform,
    /// The headless stand-in in this process's memory.
    Memory,
    /// The headless stand-in in a file ([`KEYRING_FILE_VAR`]).
    File(PathBuf),
}

/// What decides which keyring a run uses.
#[derive(Debug, Clone, Default)]
pub(crate) struct RunFacts {
    /// `AZ_KEYRING`: `system`, `file` or `memory` (any case); another word
    /// chooses nothing.
    pub choice: Option<std::ffi::OsString>,
    /// `AZ_KEYRING_FILE`: the stand-in's file; a blank name is none.
    pub file: Option<std::ffi::OsString>,
    /// A headless or E2E run: it never reaches the system keyring.
    pub headless: bool,
    /// libazul is a debug build, whose secrets stay in memory unless the run
    /// asks for the system keyring.
    pub debug_build: bool,
}

impl RunFacts {
    /// The facts of this process: the hooks of its build (`azul_core::run_mode::test_hooks()`:
    /// the environment's test switches, read only by a build with the cargo feature
    /// `test-hooks`) and how libazul was built.
    fn of_this_run() -> Self {
        Self::from_hooks(azul_core::run_mode::test_hooks(), cfg!(debug_assertions))
    }

    /// The facts of a run with the hooks `hooks`. The test switches (`AZ_KEYRING`,
    /// `AZ_KEYRING_FILE`, `AZ_BACKEND=headless`, `AZ_E2E_TEST`) are `azul_core::run_mode`'s: a
    /// release or distribution build has no hook, so nothing in its environment reaches the
    /// facts.
    pub(crate) fn from_hooks(hooks: &azul_core::run_mode::TestHooks, debug_build: bool) -> Self {
        Self {
            choice: hooks.keyring_choice.clone(),
            file: hooks.keyring_file.clone(),
            headless: hooks.keyring_headless,
            debug_build,
        }
    }

    /// The facts of a run whose environment is `var`, in a build that has the test hooks (`hooks`)
    /// or not: a build without them reads nothing of `var`.
    pub(crate) fn from_env(
        hooks: bool,
        var: impl Fn(&str) -> Option<std::ffi::OsString>,
        debug_build: bool,
    ) -> Self {
        let asked = if hooks {
            azul_core::run_mode::TestHooks::from_env(&var)
        } else {
            azul_core::run_mode::TestHooks::NONE
        };
        Self::from_hooks(&asked, debug_build)
    }
}

/// The keyring of this run ([`backend_for`] of its facts). A debug build that
/// keeps its secrets in memory says so once on stderr, so a developer knows
/// why a drive added in another build is not there.
fn backend_of_run() -> Backend {
    static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let run = RunFacts::of_this_run();
    let backend = backend_for(&run);
    if backend == Backend::Memory
        && run.debug_build
        && !run.headless
        && !SAID.swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        eprintln!(
            "[azul] keyring: this debug build keeps its secrets in memory, not in the system \
             keyring (a build with the cargo feature test-hooks reads {KEYRING_CHOICE_VAR}=system \
             and {KEYRING_FILE_VAR}=<file> to choose another)"
        );
    }
    backend
}

/// The keyring of a run. `AZ_KEYRING` decides first: `memory`; `file`, the
/// file `AZ_KEYRING_FILE` names (memory without one); `system`, the platform's
/// - except in a headless or E2E run, which never reaches it. Without a choice:
/// the named file, windowed or headless; else memory in a headless / E2E run
/// (the dispatcher is keyed on `target_os`, so a headless test on a dev Mac
/// would otherwise write to the real login Keychain) and in a debug build;
/// else the platform's. The facts come from [`RunFacts::from_env`]: a build without the cargo
/// feature `test-hooks` has no choice, no file and no headless run in them, so it uses the
/// platform's keyring (memory in a debug build) whatever its environment says.
pub(crate) fn backend_for(run: &RunFacts) -> Backend {
    let file = run
        .file
        .clone()
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);
    let choice = run
        .choice
        .as_ref()
        .and_then(|choice| choice.to_str())
        .map(|choice| choice.trim().to_ascii_lowercase());
    match choice.as_deref() {
        Some("memory") => return Backend::Memory,
        Some("file") => return file.map_or(Backend::Memory, Backend::File),
        Some("system") if !run.headless => return Backend::Platform,
        _ => {}
    }
    match file {
        Some(path) => Backend::File(path),
        None if run.headless || run.debug_build => Backend::Memory,
        None => Backend::Platform,
    }
}

/// The service `namespace` names: its own, or the app's id for the app's own
/// namespace (`AppConfig::app_id`, or what the bundle / package declares).
#[must_use]
pub fn service_of(namespace: &KeyringNamespace) -> String {
    if namespace.is_app() {
        crate::desktop::app_identity::current().id.clone()
    } else {
        namespace.service.as_str().trim().to_string()
    }
}

/// Dispatch one keyring op in `namespace` to the native keyring. Called from
/// the capability pump for each request drained from the channel; the answer
/// is parked in `results` (the app's `KeyringChannels::results`, a clone of which
/// the op's thread holds), which `CallbackInfo::get_keyring_result()` reads on a
/// later frame.
///
/// iOS/macOS → Keychain; Windows → Credential Manager; Linux → libsecret,
/// each op on a thread of its own (an `Unlock` waits for the OS
/// prompt); Android → KeyStore (JNI, answered by its Java callback). Targets
/// without any backend resolve to `Unavailable`. The stand-ins answer from `user`, the
/// scripted user of the app (`BiometricChannels::user`): what is enrolled and what they do at
/// each sheet.
pub fn request(
    namespace: &KeyringNamespace,
    req: &KeyringRequest,
    results: &SharedQueue<KeyringResult>,
    user: &StandInUser,
) {
    let service = service_of(namespace);
    match backend_of_run() {
        Backend::Platform => platform_request(service, req, results),
        stand_in => results.push(answer_with(
            &service,
            req,
            &stand_in,
            Interaction::UserCheck,
            user,
        )),
    }
}

/// [`request`] on the platform's keyring: answered through the result queue.
fn platform_request(service: String, req: &KeyringRequest, results: &SharedQueue<KeyringResult>) {
    #[cfg(target_os = "android")]
    {
        // The app's own storage holds every entry it keeps: the namespace changes nothing.
        let _ = service;
        android::request(req, results);
    }
    #[cfg(any(
        target_os = "ios",
        target_os = "macos",
        target_os = "windows",
        target_os = "linux"
    ))]
    {
        let req = req.clone();
        let results = results.clone();
        std::thread::spawn(move || {
            results.push(answer_with(
                &service,
                &req,
                &Backend::Platform,
                Interaction::UserCheck,
                &StandInUser::new(),
            ));
        });
    }
    #[cfg(not(any(
        target_os = "ios",
        target_os = "macos",
        target_os = "android",
        target_os = "windows",
        target_os = "linux"
    )))]
    {
        let _ = (service, req);
        results.push(KeyringResult::Unavailable);
    }
}

/// Runs one keyring op in `namespace` on the calling thread and answers it -
/// blocking: a locked keychain waits for its unlock dialog. For worker threads; a UI callback
/// queues its op with `CallbackInfo::keyring_*` instead. Never the user-check sheet: an op that
/// needs it (an `Unlock`, a protected `Store`) answers `WouldPrompt`, and so does a `Get` of a
/// protected item.
#[must_use]
pub fn request_blocking(namespace: &KeyringNamespace, req: &KeyringRequest) -> KeyringResult {
    answer_with(
        &service_of(namespace),
        req,
        &backend_of_run(),
        Interaction::Dialogs,
        &process_user(),
    )
}

/// [`request_blocking`] that never shows anything: an op the system would have
/// to ask the user for answers `WouldPrompt` (see the module). For background work,
/// which must not raise a dialog the user did not ask for.
#[must_use]
pub fn request_quietly_blocking(namespace: &KeyringNamespace, req: &KeyringRequest) -> KeyringResult {
    answer_with(
        &service_of(namespace),
        req,
        &backend_of_run(),
        Interaction::Quiet,
        &process_user(),
    )
}

/// The scripted user of the stand-ins for a call that has no context (a worker thread's
/// static): the one of the app this process runs, else a device with nothing set up. A real
/// backend never reads it.
fn process_user() -> StandInUser {
    crate::desktop::process_app::services()
        .map_or_else(StandInUser::new, |services| services.biometric.user)
}

/// What a read of `key` in `namespace` needs, asked quietly: the item's record only, never a
/// sheet. See `azul_core::keyring::KeyringNeed`.
#[must_use]
pub fn need_for(namespace: &KeyringNamespace, key: &AzString) -> KeyringNeed {
    need_in(&service_of(namespace), key.as_str(), &backend_of_run(), &process_user())
}

/// What this device does for `protection`, asked quietly. See
/// `azul_core::keyring::KeyringSupport`.
#[must_use]
pub fn support_for(protection: KeyringProtection) -> KeyringSupport {
    support_in(protection, &backend_of_run(), &process_user())
}

/// One op in `service` on `backend`, answered on this thread, with the system's own dialogs
/// allowed and no user-check sheet ([`Interaction::Dialogs`]). The entry of the
/// same name in the service every azul app shared before namespaces moves into
/// `service` first ([`fold_legacy`]), so an app's entries survive the change
/// and no other app finds them there any more.
#[cfg(test)]
pub(crate) fn answer_in(service: &str, req: &KeyringRequest, backend: &Backend) -> KeyringResult {
    answer_with(service, req, backend, Interaction::Dialogs, &StandInUser::new())
}

/// [`answer_in`] with what the op may show (`interaction`) and, for the stand-ins, the scripted
/// user that answers a sheet. On the platform's keyring the move and the op run as one turn
/// ([`platform_turn`]).
pub(crate) fn answer_with(
    service: &str,
    req: &KeyringRequest,
    backend: &Backend,
    interaction: Interaction,
    user: &StandInUser,
) -> KeyringResult {
    // The same on every system: libsecret kept a secret with a NUL as "" (a C string ends there).
    if has_nul(service, req) {
        return KeyringResult::Error;
    }
    // The same on every system: the user-check sheet shows only where the user asked for it. An
    // op that needs it anywhere else answers WouldPrompt before any keyring is touched.
    if req.may_show_sheet() && !interaction.user_check() {
        return KeyringResult::WouldPrompt;
    }
    let run = || {
        if service != KeyringNamespace::LEGACY_SERVICE && namespaces_apply(backend) {
            fold_legacy(service, req.key(), backend, interaction);
        }
        op_in(service, req, backend, interaction, user)
    };
    match backend {
        Backend::Platform => platform_turn(interaction, run),
        _ => run(),
    }
}

/// Whether `backend` keeps namespaces apart. Android's keyring is the app's
/// own storage, which no other app reads: there is nothing to move.
fn namespaces_apply(backend: &Backend) -> bool {
    !(cfg!(target_os = "android") && *backend == Backend::Platform)
}

/// Whether the service, the key or the secret of `req` holds a NUL. Keys and secrets are text; a C
/// string ends at a NUL, so a backend built on them would store and look up something other than
/// what was named (AZ-W16: libsecret kept the secret `a\0b` as the empty one). Refused alike on every
/// system and every stand-in, so a secret that works on one computer does not become nothing on another.
pub(crate) fn has_nul(service: &str, req: &KeyringRequest) -> bool {
    service.contains('\0')
        || req.key().as_str().contains('\0')
        || matches!(req, KeyringRequest::Store { secret, .. } if secret.as_str().contains('\0'))
}

/// The most one Windows Credential Manager entry holds: `CRED_MAX_CREDENTIAL_BLOB_SIZE`, 5 * 512 bytes.
pub(crate) const CREDENTIAL_BLOB_MAX: usize = 2560;

/// Why a secret of `len` bytes cannot be kept in one Credential Manager entry; `None` when it fits.
/// `CredWriteW` refuses a longer blob with an error the keyring reports as a bare "error"; the
/// sentence says what the size was and what the most is (AZ-W16). The callers keep their entries under
/// the limit (a list of pending checkouts is bounded by it); nothing is split over several entries.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn blob_refusal(len: usize) -> Option<String> {
    (len > CREDENTIAL_BLOB_MAX).then(|| {
        format!(
            "a Credential Manager entry holds at most {CREDENTIAL_BLOB_MAX} bytes; this secret has {len}"
        )
    })
}

/// What a quiet op finds out about the system's default collection (libsecret) before it touches
/// it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) enum Collection {
    /// It is there and open.
    Unlocked,
    /// It is there and locked: touching it asks the user for the keyring's password.
    Locked,
    /// There is none yet: a store would make one, which asks for a password; a read finds nothing.
    Missing,
    /// libsecret did not say (an old one, a refused call): the op goes on.
    Unknown,
}

/// What a quiet op answers when the default collection's `state` settles it without touching the
/// collection; `None`: go on and ask libsecret. A locked collection is `WouldPrompt` for every op (the
/// unlock would show a dialog nobody asked for); with no collection a read finds nothing, a delete
/// has nothing to delete, and only a store - which would make one - is `WouldPrompt` (AZ-W16).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn quiet_verdict(state: Collection, req: &KeyringRequest) -> Option<KeyringResult> {
    match (state, req) {
        (Collection::Locked, _) | (Collection::Missing, KeyringRequest::Store { .. }) => {
            Some(KeyringResult::WouldPrompt)
        }
        (Collection::Missing, KeyringRequest::Get { .. } | KeyringRequest::Unlock { .. }) => {
            Some(KeyringResult::NotFound)
        }
        (Collection::Missing, KeyringRequest::Delete { .. }) => Some(KeyringResult::Deleted),
        (Collection::Unlocked | Collection::Unknown, _) => None,
    }
}

/// One op in `service` on `backend`, nothing moved.
fn op_in(
    service: &str,
    req: &KeyringRequest,
    backend: &Backend,
    interaction: Interaction,
    user: &StandInUser,
) -> KeyringResult {
    match backend {
        Backend::Platform => platform_blocking(service, req, interaction),
        Backend::Memory => memory::answer(service, req, interaction, user),
        Backend::File(path) => memory::answer_in_file(service, req, path, interaction, user),
    }
}

/// What a read of `key` in `service` needs on `backend` (see [`need_for`]).
pub(crate) fn need_in(
    service: &str,
    key: &str,
    backend: &Backend,
    user: &StandInUser,
) -> KeyringNeed {
    if service.contains('\0') || key.contains('\0') {
        return KeyringNeed::Error;
    }
    match backend {
        Backend::Platform => platform_need(service, key),
        Backend::Memory => memory::need(service, key, user),
        Backend::File(path) => memory::need_in_file(service, key, path, user),
    }
}

/// What `backend` does for `protection` (see [`support_for`]).
pub(crate) fn support_in(
    protection: KeyringProtection,
    backend: &Backend,
    user: &StandInUser,
) -> KeyringSupport {
    match backend {
        Backend::Platform => platform_support(protection),
        Backend::Memory | Backend::File(_) => memory::support(protection, user),
    }
}

/// The entries of the platform's keyring this process has moved already (or
/// found nothing to move for), by service and name: each is looked up in the
/// old service once per process. The stand-ins are cheap and a test may seed
/// their file at any time, so they look every time. Held while a move runs, so
/// an op of another thread on the same entry waits for the move to finish.
static FOLDED: Mutex<BTreeSet<(String, String)>> = Mutex::new(BTreeSet::new());

/// Moves the entry `key` of the old shared service ([`KeyringNamespace::LEGACY_SERVICE`])
/// into `service`, unless `service` has one of its own (then the old one is the stale copy and
/// goes). Before a store too, so a stale secret never stays readable there, and before a
/// delete, so a deleted entry never comes back from it. A move that fails (a locked keyring)
/// is tried again by the next op.
fn fold_legacy(service: &str, key: &AzString, backend: &Backend, interaction: Interaction) {
    let marker = (service.to_string(), key.as_str().to_string());
    let mut folded = FOLDED.lock().unwrap_or_else(PoisonError::into_inner);
    let platform = *backend == Backend::Platform;
    if platform && folded.contains(&marker) {
        return;
    }
    let moved = match backend {
        Backend::Platform => platform_move(key, service, interaction),
        stand_in => move_by_copy(key, service, stand_in, interaction),
    };
    if moved && platform {
        folded.insert(marker);
    }
}

/// [`fold_legacy`] by reading the old entry, storing it in `service` and deleting it: for the
/// keyrings with no other way (and no biometry binding to keep). `true` when nothing is left
/// in the old service.
fn move_by_copy(
    key: &AzString,
    service: &str,
    backend: &Backend,
    interaction: Interaction,
) -> bool {
    let legacy = KeyringNamespace::LEGACY_SERVICE;
    // The old shared service held unprotected entries only: nothing here shows a sheet, and
    // the move needs no scripted user.
    let user = StandInUser::new();
    let op = |service: &str, req: &KeyringRequest| op_in(service, req, backend, interaction, &user);
    let secret = match op(legacy, &KeyringRequest::Get { key: key.clone() }) {
        KeyringResult::Retrieved(secret) => secret,
        KeyringResult::NotFound => return true,
        _ => return false,
    };
    match op(service, &KeyringRequest::Get { key: key.clone() }) {
        KeyringResult::Retrieved(_) => {}
        KeyringResult::NotFound => {
            let store = KeyringRequest::Store {
                key: key.clone(),
                secret,
                protection: KeyringProtection::DeviceUnlocked,
                prompt: KeyringPrompt::default(),
            };
            if op(service, &store) != KeyringResult::Stored {
                return false;
            }
        }
        _ => return false,
    }
    op(legacy, &KeyringRequest::Delete { key: key.clone() }) == KeyringResult::Deleted
}

#[cfg(any(target_os = "ios", target_os = "macos"))]
fn platform_blocking(service: &str, req: &KeyringRequest, interaction: Interaction) -> KeyringResult {
    apple::handle(service, req, interaction)
}

/// The Windows keyring: the Credential Manager never asks; a protected item behind the Hello
/// gesture shows the Hello sheet for an `Unlock` ([`Interaction::UserCheck`]) only.
#[cfg(target_os = "windows")]
fn platform_blocking(service: &str, req: &KeyringRequest, interaction: Interaction) -> KeyringResult {
    windows::handle(service, req, interaction)
}

/// libsecret asks only to unlock a locked collection: a quiet op looks first (see the module); a
/// protected item runs the fprintd / polkit check first for an `Unlock` only.
#[cfg(target_os = "linux")]
fn platform_blocking(service: &str, req: &KeyringRequest, interaction: Interaction) -> KeyringResult {
    linux::handle(service, req, interaction)
}

/// Android's KeyStore: a worker thread's call reaches the helper's synchronous entry (an
/// unprotected item needs no sheet, so no callback); the user-check sheet comes through
/// [`android::request`] only.
#[cfg(target_os = "android")]
fn platform_blocking(
    _service: &str,
    req: &KeyringRequest,
    interaction: Interaction,
) -> KeyringResult {
    android::blocking(req, interaction)
}

/// Other targets have no keyring.
#[cfg(not(any(
    target_os = "ios",
    target_os = "macos",
    target_os = "windows",
    target_os = "linux",
    target_os = "android"
)))]
fn platform_blocking(
    _service: &str,
    _req: &KeyringRequest,
    _interaction: Interaction,
) -> KeyringResult {
    KeyringResult::Unavailable
}

/// What a read needs on the platform's keyring (the item's record, never a sheet).
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn platform_need(service: &str, key: &str) -> KeyringNeed {
    apple::needs_prompt(service, key)
}

#[cfg(target_os = "windows")]
fn platform_need(service: &str, key: &str) -> KeyringNeed {
    windows::needs_prompt(service, key)
}

#[cfg(target_os = "linux")]
fn platform_need(service: &str, key: &str) -> KeyringNeed {
    linux::needs_prompt(service, key)
}

#[cfg(target_os = "android")]
fn platform_need(_service: &str, key: &str) -> KeyringNeed {
    android::needs_prompt(key)
}

#[cfg(not(any(
    target_os = "ios",
    target_os = "macos",
    target_os = "windows",
    target_os = "linux",
    target_os = "android"
)))]
fn platform_need(_service: &str, _key: &str) -> KeyringNeed {
    KeyringNeed::Unavailable
}

/// What the platform's keyring does for `protection`.
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn platform_support(protection: KeyringProtection) -> KeyringSupport {
    apple::support(protection)
}

#[cfg(target_os = "windows")]
fn platform_support(protection: KeyringProtection) -> KeyringSupport {
    windows::support(protection)
}

#[cfg(target_os = "linux")]
fn platform_support(protection: KeyringProtection) -> KeyringSupport {
    linux::support(protection)
}

#[cfg(target_os = "android")]
fn platform_support(protection: KeyringProtection) -> KeyringSupport {
    android::support(protection)
}

#[cfg(not(any(
    target_os = "ios",
    target_os = "macos",
    target_os = "windows",
    target_os = "linux",
    target_os = "android"
)))]
fn platform_support(_protection: KeyringProtection) -> KeyringSupport {
    KeyringSupport::Unavailable
}

/// One turn at the Apple keychain: the ops of this process take turns there, as
/// a quiet op turns the keychain's prompts off for the whole process while it
/// runs ([`apple::in_turn`]).
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn platform_turn<T>(interaction: Interaction, op: impl FnOnce() -> T) -> T {
    apple::in_turn(interaction.quiet(), op)
}

/// The other keyrings need no turns.
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
fn platform_turn<T>(_interaction: Interaction, op: impl FnOnce() -> T) -> T {
    op()
}

/// The Keychain moves an item by changing its service: the secret is not read
/// and a biometry-bound item stays bound. A quiet move runs with the prompts
/// off already ([`platform_turn`]).
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn platform_move(key: &AzString, service: &str, _interaction: Interaction) -> bool {
    apple::move_entry(key.as_str(), KeyringNamespace::LEGACY_SERVICE, service)
}

/// libsecret and the Credential Manager have no protected item in the old service to keep.
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
fn platform_move(key: &AzString, service: &str, interaction: Interaction) -> bool {
    move_by_copy(key, service, &Backend::Platform, interaction)
}

/// The system keyring as a worker thread uses it: every call runs its op
/// where it is made and returns the answer (blocking) - unlike
/// `CallbackInfo::keyring_*`, whose answer arrives on a later frame as the
/// window's `KeyringResult` event. What an app's `Thread` re-reads a secret
/// with that another process may have changed, and writes one with before
/// its work goes on. Both reach the same keyring (in a headless / E2E run the
/// same stand-in).
///
/// These calls NEVER show the user-check sheet (biometric / passcode / Hello) and read and write
/// UNPROTECTED items only: a protected item answers `WouldPrompt` - the app asks the user with
/// `CallbackInfo::keyring_unlock` from the callback of their action. The system keychain's own
/// dialogs (a locked keychain, an unsigned build's access list) can still show for the blocking
/// calls; the `*_quietly*` ones never show anything.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(clippy::pub_underscore_fields)] // _reserved: FFI/api.json static-namespace placeholder
                                        // field
pub struct Keyring {
    pub _reserved: u8,
}

impl Default for Keyring {
    fn default() -> Self {
        Self::new()
    }
}

fn plain_store(key: AzString, secret: AzString) -> KeyringRequest {
    let mut secret = secret;
    secret.set_secret(true);
    KeyringRequest::Store {
        key,
        secret,
        protection: KeyringProtection::DeviceUnlocked,
        prompt: KeyringPrompt::default(),
    }
}

impl Keyring {
    /// Returns a zero-initialised namespace handle. Static-only - the struct
    /// is just a hook for the FFI layer.
    #[must_use]
    pub const fn new() -> Self {
        Self { _reserved: 0 }
    }

    /// Reads the secret stored under `key` in the app's own namespace (its
    /// app id), on this thread: `Retrieved`, `NotFound`, or why not (`WouldPrompt` for a
    /// protected item, `Invalidated`, `Denied`, `Unavailable`, `Error`).
    #[must_use]
    pub fn get_blocking(key: AzString) -> KeyringResult {
        Self::get_blocking_in(KeyringNamespace::app(), key)
    }

    /// Stores `secret` under `key` in the app's own namespace (replacing what
    /// was there, unprotected), on this thread: `Stored`, or why not.
    #[must_use]
    pub fn store_blocking(key: AzString, secret: AzString) -> KeyringResult {
        Self::store_blocking_in(KeyringNamespace::app(), key, secret)
    }

    /// Removes the item stored under `key` in the app's own namespace, on this
    /// thread: `Deleted` (also when there was none), or why not.
    #[must_use]
    pub fn delete_blocking(key: AzString) -> KeyringResult {
        Self::delete_blocking_in(KeyringNamespace::app(), key)
    }

    /// Reads the secret stored under `key` in `namespace` (the entries apps
    /// share live in the namespace they all name), on this thread.
    #[must_use]
    pub fn get_blocking_in(namespace: KeyringNamespace, key: AzString) -> KeyringResult {
        request_blocking(&namespace, &KeyringRequest::Get { key })
    }

    /// Stores `secret` under `key` in `namespace`, unprotected, on this thread.
    #[must_use]
    pub fn store_blocking_in(
        namespace: KeyringNamespace,
        key: AzString,
        secret: AzString,
    ) -> KeyringResult {
        request_blocking(&namespace, &plain_store(key, secret))
    }

    /// Removes the item stored under `key` in `namespace`, on this thread.
    #[must_use]
    pub fn delete_blocking_in(namespace: KeyringNamespace, key: AzString) -> KeyringResult {
        request_blocking(&namespace, &KeyringRequest::Delete { key })
    }

    /// [`Self::get_blocking_in`] without ever showing anything: where the system
    /// would have to ask the user (a protected item, another app's item,
    /// a locked keychain) the answer is `WouldPrompt` and nothing shows. For
    /// background work - a sync at the start, a timer's poll - which must not
    /// raise a dialog the user did not ask for.
    #[must_use]
    pub fn get_quietly_in(namespace: KeyringNamespace, key: AzString) -> KeyringResult {
        request_quietly_blocking(&namespace, &KeyringRequest::Get { key })
    }

    /// [`Self::store_blocking_in`] without ever showing anything (`WouldPrompt` where
    /// the system would have to ask).
    #[must_use]
    pub fn store_quietly_in(
        namespace: KeyringNamespace,
        key: AzString,
        secret: AzString,
    ) -> KeyringResult {
        request_quietly_blocking(&namespace, &plain_store(key, secret))
    }

    /// [`Self::delete_blocking_in`] without ever showing anything (`WouldPrompt`
    /// where the system would have to ask).
    #[must_use]
    pub fn delete_quietly_in(namespace: KeyringNamespace, key: AzString) -> KeyringResult {
        request_quietly_blocking(&namespace, &KeyringRequest::Delete { key })
    }

    /// What a read of `key` in the app's own namespace needs, asked quietly (the item's record
    /// only, never a sheet): nothing, the user's biometric, the user's biometric or passcode, or
    /// the keyring's own unlock - so an app can explain BEFORE the sheet comes. See
    /// [`Self::needs_prompt_in`].
    #[must_use]
    pub fn needs_prompt(key: AzString) -> KeyringNeed {
        Self::needs_prompt_in(KeyringNamespace::app(), key)
    }

    /// [`Self::needs_prompt`] in `namespace`.
    #[must_use]
    pub fn needs_prompt_in(namespace: KeyringNamespace, key: AzString) -> KeyringNeed {
        need_for(&namespace, &key)
    }

    /// What this device does for `protection`, asked quietly: `Hardware` (the OS or the secure
    /// hardware releases the secret only after the user's check), `Library` (azul runs the
    /// check, then reads an item that is not itself bound to it), `NotEnrolled` (nothing set up:
    /// no biometric, no passcode) or `Unavailable`. For `DeviceUnlocked` it says whether the
    /// platform keyring works at all.
    #[must_use]
    pub fn support(protection: KeyringProtection) -> KeyringSupport {
        support_for(protection)
    }
}
