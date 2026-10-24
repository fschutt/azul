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
//! | Platform | Backend |
//! |----------|---------|
//! | iOS / macOS | Keychain `SecItemAdd` / `SecItemCopyMatching` / `SecItemDelete` (objc2 / Security.framework), `kSecAttrAccessControl = biometryCurrentSet` for biometry-bound items |
//! | Android | `KeyStore` + `setUserAuthenticationRequired(true)` (JNI helper) |
//! | Linux | libsecret (`secret_password_store/lookup/clear`) via the secret-service D-Bus |
//! | Windows | Credential Manager (`CredWriteW` / `CredReadW` / `CredDeleteW`, generic credentials) |
//!
//! MWA-C-keyring: ALL FOUR desktop backends are real (the stub-era note
//! that used to live here claimed Windows/Linux resolve to Unavailable —
//! stale). A biometry-bound `Get` parks its outcome back through
//! `push_keyring_result` asynchronously from the OS prompt's reply.
//!
//! [`request_blocking`] (the C API's [`Keyring`]: `Keyring::get_blocking`,
//! `store_blocking`, `delete_blocking` and their `_in` forms) runs one op on the CALLING thread and
//! returns its answer: what an app's worker `Thread` reads and writes the
//! keyring with - it re-reads a secret another process may have changed (an
//! Azlin drive's rotating token) and writes one before its work goes on. Both
//! ways reach the same keyring. Android's helper answers through its Java
//! callback only: a blocking call answers `Unavailable` there.
//!
//! Every entry lives in a [`KeyringNamespace`] - the keyring service - so azul
//! apps do not read each other's secrets by accident: an app's own (its app
//! id) unless a call names one, which apps that share entries do (the Azlin
//! apps' `io.azlin.shared`). An entry still under the service every app shared
//! before (`com.azul.keyring`) moves into the first namespace that touches its
//! name ([`answer_in`]).
//!
//! Headless / E2E runs (`AZ_BACKEND=headless`, `AZ_E2E_TEST`) never touch the
//! REAL host secret store: both ways reach the stand-in of `memory.rs` - one
//! map for the process, or the file `AZ_KEYRING_FILE` names (which keeps it
//! for the next run of a test and shares it between the test's processes).
//! A windowed run that names such a file keeps its keyring there too: an app
//! on a test stack, in a real window, then shares no entry with the same app
//! of the user's own (the system keyring's entries are the app's, not the
//! stack's). The file holds the secrets in plain text: test stacks only.
//!
//! A debug build of libazul keeps its secrets in memory as well: every rebuild
//! is a new code signature, and the macOS keychain asks the user again for each
//! entry such a binary reads. `AZ_KEYRING` picks the keyring outright: `system`,
//! `file` (the one `AZ_KEYRING_FILE` names) or `memory` ([`backend_for`]).
//!
//! [`request_quietly_blocking`] (the C API's `Keyring::get_quietly_in`,
//! `store_quietly_in` and `delete_quietly_in`) runs
//! an op that never shows a prompt - what an app's background work uses, so a
//! start, a timer or a poll never raises the keychain's dialog. Where the
//! system would have to ask (an item behind Touch ID, another app's item, a
//! locked keychain) it answers `Denied`, and the app waits for the user's own
//! action to ask. Only the Apple keychain prompts per item; the Credential
//! Manager never asks, and libsecret asks only to unlock a locked collection
//! (the login keyring is unlocked at login), which a quiet op cannot leave out.

use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{Mutex, PoisonError},
};

use azul_core::keyring::{KeyringNamespace, KeyringRequest, KeyringResult};
use azul_css::AzString;

#[cfg(target_os = "android")]
pub mod android;
#[cfg(any(target_os = "ios", target_os = "macos"))]
pub mod apple;
#[cfg(target_os = "linux")]
pub mod linux;
mod memory;
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(test)]
mod tests;

/// The variable naming the file that keeps a run's keyring instead of the
/// system's (a test stack's runs, windowed or headless).
pub const KEYRING_FILE_VAR: &str = "AZ_KEYRING_FILE";

/// The variable that picks a run's keyring: `system`, `file` (the one
/// [`KEYRING_FILE_VAR`] names) or `memory`.
pub const KEYRING_CHOICE_VAR: &str = "AZ_KEYRING";

/// Whether a keyring op may show the system's prompt (a keychain unlock, an
/// "allow this app" dialog, Touch ID).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Prompts {
    /// The user asked for what the op serves: the system may ask them.
    Allowed,
    /// Background work: an op the system would have to ask for answers `Denied`.
    Never,
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
    /// The facts of this process: its environment and how libazul was built.
    fn of_this_run() -> Self {
        Self {
            choice: std::env::var_os(KEYRING_CHOICE_VAR),
            file: std::env::var_os(KEYRING_FILE_VAR),
            headless: std::env::var("AZ_BACKEND").as_deref() == Ok("headless")
                || std::env::var("AZ_E2E_TEST").is_ok(),
            debug_build: cfg!(debug_assertions),
        }
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
             keyring ({KEYRING_CHOICE_VAR}=system uses it, {KEYRING_FILE_VAR}=<file> keeps them in a file)"
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
/// else the platform's.
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
/// is parked in the result channel, which `CallbackInfo::get_keyring_result()`
/// reads on a later frame.
///
/// iOS/macOS → Keychain; Windows → Credential Manager; Linux → libsecret,
/// each op on a thread of its own (a biometry-bound read waits for the OS
/// prompt); Android → KeyStore (JNI, answered by its Java callback). Targets
/// without any backend resolve to `Unavailable`.
pub fn request(namespace: &KeyringNamespace, req: &KeyringRequest) {
    let service = service_of(namespace);
    match backend_of_run() {
        Backend::Platform => platform_request(service, req),
        stand_in => {
            azul_layout::managers::keyring::push_keyring_result(answer_in(
                &service, req, &stand_in,
            ));
        }
    }
}

/// [`request`] on the platform's keyring: answered through the result channel.
fn platform_request(service: String, req: &KeyringRequest) {
    #[cfg(target_os = "android")]
    {
        // The app's own storage holds every entry it keeps: the namespace changes nothing.
        let _ = service;
        android::request(req);
    }
    #[cfg(any(
        target_os = "ios",
        target_os = "macos",
        target_os = "windows",
        target_os = "linux"
    ))]
    {
        let req = req.clone();
        std::thread::spawn(move || {
            azul_layout::managers::keyring::push_keyring_result(answer_in(
                &service,
                &req,
                &Backend::Platform,
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
        azul_layout::managers::keyring::push_keyring_result(KeyringResult::Unavailable);
    }
}

/// Runs one keyring op in `namespace` on the calling thread and answers it -
/// blocking: a biometry-bound read waits for the OS prompt, a locked keychain
/// for its unlock. For worker threads; a UI callback queues its op with
/// `CallbackInfo::keyring_*` instead.
#[must_use]
pub fn request_blocking(namespace: &KeyringNamespace, req: &KeyringRequest) -> KeyringResult {
    answer_in(&service_of(namespace), req, &backend_of_run())
}

/// [`request_blocking`] that never shows a prompt: an op the system would have
/// to ask the user for answers `Denied` (see the module). For background work,
/// which must not raise a dialog the user did not ask for.
#[must_use]
pub fn request_quietly_blocking(namespace: &KeyringNamespace, req: &KeyringRequest) -> KeyringResult {
    answer_with(&service_of(namespace), req, &backend_of_run(), Prompts::Never)
}

/// One op in `service` on `backend`, answered on this thread. The entry of the
/// same name in the service every azul app shared before namespaces moves into
/// `service` first ([`fold_legacy`]), so an app's entries survive the change
/// and no other app finds them there any more.
pub(crate) fn answer_in(service: &str, req: &KeyringRequest, backend: &Backend) -> KeyringResult {
    answer_with(service, req, backend, Prompts::Allowed)
}

/// [`answer_in`], the system's prompts allowed or not. On the platform's keyring
/// the move and the op run as one turn ([`platform_turn`]).
pub(crate) fn answer_with(
    service: &str,
    req: &KeyringRequest,
    backend: &Backend,
    prompts: Prompts,
) -> KeyringResult {
    let run = || {
        if service != KeyringNamespace::LEGACY_SERVICE && namespaces_apply(backend) {
            fold_legacy(service, key_of(req), backend, prompts);
        }
        op_in(service, req, backend, prompts)
    };
    match backend {
        Backend::Platform => platform_turn(prompts, run),
        _ => run(),
    }
}

/// Whether `backend` keeps namespaces apart. Android's keyring is the app's
/// own storage, which no other app reads: there is nothing to move.
fn namespaces_apply(backend: &Backend) -> bool {
    !(cfg!(target_os = "android") && *backend == Backend::Platform)
}

fn key_of(req: &KeyringRequest) -> &AzString {
    match req {
        KeyringRequest::Store { key, .. }
        | KeyringRequest::Get { key }
        | KeyringRequest::Delete { key } => key,
    }
}

/// One op in `service` on `backend`, nothing moved.
fn op_in(service: &str, req: &KeyringRequest, backend: &Backend, prompts: Prompts) -> KeyringResult {
    match backend {
        Backend::Platform => platform_blocking(service, req, prompts),
        Backend::Memory => memory::answer(service, req),
        Backend::File(path) => memory::answer_in_file(service, req, path),
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
fn fold_legacy(service: &str, key: &AzString, backend: &Backend, prompts: Prompts) {
    let marker = (service.to_string(), key.as_str().to_string());
    let mut folded = FOLDED.lock().unwrap_or_else(PoisonError::into_inner);
    let platform = *backend == Backend::Platform;
    if platform && folded.contains(&marker) {
        return;
    }
    let moved = match backend {
        Backend::Platform => platform_move(key, service, prompts),
        stand_in => move_by_copy(key, service, stand_in, prompts),
    };
    if moved && platform {
        folded.insert(marker);
    }
}

/// [`fold_legacy`] by reading the old entry, storing it in `service` and deleting it: for the
/// keyrings with no other way (and no biometry binding to keep). `true` when nothing is left
/// in the old service.
fn move_by_copy(key: &AzString, service: &str, backend: &Backend, prompts: Prompts) -> bool {
    let legacy = KeyringNamespace::LEGACY_SERVICE;
    let secret = match op_in(legacy, &KeyringRequest::Get { key: key.clone() }, backend, prompts) {
        KeyringResult::Retrieved(secret) => secret,
        KeyringResult::NotFound => return true,
        _ => return false,
    };
    match op_in(service, &KeyringRequest::Get { key: key.clone() }, backend, prompts) {
        KeyringResult::Retrieved(_) => {}
        KeyringResult::NotFound => {
            let store = KeyringRequest::Store {
                key: key.clone(),
                secret,
                require_biometry: false,
            };
            if op_in(service, &store, backend, prompts) != KeyringResult::Stored {
                return false;
            }
        }
        _ => return false,
    }
    op_in(legacy, &KeyringRequest::Delete { key: key.clone() }, backend, prompts)
        == KeyringResult::Deleted
}

#[cfg(any(target_os = "ios", target_os = "macos"))]
fn platform_blocking(service: &str, req: &KeyringRequest, prompts: Prompts) -> KeyringResult {
    apple::handle(service, req, prompts == Prompts::Never)
}

/// The Credential Manager never asks.
#[cfg(target_os = "windows")]
fn platform_blocking(service: &str, req: &KeyringRequest, _prompts: Prompts) -> KeyringResult {
    windows::handle(service, req)
}

/// libsecret asks only to unlock a locked collection (see the module).
#[cfg(target_os = "linux")]
fn platform_blocking(service: &str, req: &KeyringRequest, _prompts: Prompts) -> KeyringResult {
    linux::handle(service, req)
}

/// Android's KeyStore helper answers through its Java callback only; other
/// targets have no keyring.
#[cfg(not(any(
    target_os = "ios",
    target_os = "macos",
    target_os = "windows",
    target_os = "linux"
)))]
fn platform_blocking(_service: &str, _req: &KeyringRequest, _prompts: Prompts) -> KeyringResult {
    KeyringResult::Unavailable
}

/// One turn at the Apple keychain: the ops of this process take turns there, as
/// a quiet op turns the keychain's prompts off for the whole process while it
/// runs ([`apple::in_turn`]).
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn platform_turn<T>(prompts: Prompts, op: impl FnOnce() -> T) -> T {
    apple::in_turn(prompts == Prompts::Never, op)
}

/// The other keyrings need no turns.
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
fn platform_turn<T>(_prompts: Prompts, op: impl FnOnce() -> T) -> T {
    op()
}

/// The Keychain moves an item by changing its service: the secret is not read
/// and a biometry-bound item stays bound. A quiet move runs with the prompts
/// off already ([`platform_turn`]).
#[cfg(any(target_os = "ios", target_os = "macos"))]
fn platform_move(key: &AzString, service: &str, _prompts: Prompts) -> bool {
    apple::move_entry(key.as_str(), KeyringNamespace::LEGACY_SERVICE, service)
}

/// libsecret and the Credential Manager have no biometry binding to keep.
#[cfg(not(any(target_os = "ios", target_os = "macos")))]
fn platform_move(key: &AzString, service: &str, prompts: Prompts) -> bool {
    move_by_copy(key, service, &Backend::Platform, prompts)
}

/// The system keyring as a worker thread uses it: every call runs its op
/// where it is made and returns the answer (blocking) - unlike
/// `CallbackInfo::keyring_*`, whose answer arrives on a later frame as the
/// window's `KeyringResult` event. What an app's `Thread` re-reads a secret
/// with that another process may have changed, and writes one with before
/// its work goes on. Never call it from a callback: a biometry-bound item
/// shows the OS prompt and the call waits for it. Both reach the same
/// keyring (in a headless / E2E run the same stand-in).
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

impl Keyring {
    /// Returns a zero-initialised namespace handle. Static-only - the struct
    /// is just a hook for the FFI layer.
    #[must_use]
    pub const fn new() -> Self {
        Self { _reserved: 0 }
    }

    /// Reads the secret stored under `key` in the app's own namespace (its
    /// app id), on this thread: `Retrieved`, `NotFound`, or why not (`Denied`,
    /// `Unavailable`, `Error`).
    #[must_use]
    pub fn get_blocking(key: AzString) -> KeyringResult {
        Self::get_blocking_in(KeyringNamespace::app(), key)
    }

    /// Stores `secret` under `key` in the app's own namespace (replacing what
    /// was there), on this thread: `Stored`, or why not. `require_biometry` as
    /// in `CallbackInfo::keyring_store`.
    #[must_use]
    pub fn store_blocking(
        key: AzString,
        secret: AzString,
        require_biometry: bool,
    ) -> KeyringResult {
        Self::store_blocking_in(KeyringNamespace::app(), key, secret, require_biometry)
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

    /// Stores `secret` under `key` in `namespace`, on this thread.
    #[must_use]
    pub fn store_blocking_in(
        namespace: KeyringNamespace,
        key: AzString,
        secret: AzString,
        require_biometry: bool,
    ) -> KeyringResult {
        let mut secret = secret;
        secret.set_secret(true);
        request_blocking(
            &namespace,
            &KeyringRequest::Store {
                key,
                secret,
                require_biometry,
            },
        )
    }

    /// Removes the item stored under `key` in `namespace`, on this thread.
    #[must_use]
    pub fn delete_blocking_in(namespace: KeyringNamespace, key: AzString) -> KeyringResult {
        request_blocking(&namespace, &KeyringRequest::Delete { key })
    }

    /// [`Self::get_blocking_in`] without ever showing a prompt: where the system
    /// would have to ask the user (an item behind Touch ID, another app's item,
    /// a locked keychain) the answer is `Denied` and nothing shows. For
    /// background work - a sync at the start, a timer's poll - which must not
    /// raise a dialog the user did not ask for.
    #[must_use]
    pub fn get_quietly_in(namespace: KeyringNamespace, key: AzString) -> KeyringResult {
        request_quietly_blocking(&namespace, &KeyringRequest::Get { key })
    }

    /// [`Self::store_blocking_in`] without ever showing a prompt (`Denied` where
    /// the system would have to ask).
    #[must_use]
    pub fn store_quietly_in(
        namespace: KeyringNamespace,
        key: AzString,
        secret: AzString,
        require_biometry: bool,
    ) -> KeyringResult {
        let mut secret = secret;
        secret.set_secret(true);
        request_quietly_blocking(
            &namespace,
            &KeyringRequest::Store {
                key,
                secret,
                require_biometry,
            },
        )
    }

    /// [`Self::delete_blocking_in`] without ever showing a prompt (`Denied`
    /// where the system would have to ask).
    #[must_use]
    pub fn delete_quietly_in(namespace: KeyringNamespace, key: AzString) -> KeyringResult {
        request_quietly_blocking(&namespace, &KeyringRequest::Delete { key })
    }
}
