//! The keyring every process of this user shares, and the locks its entries change under: what
//! an app keeps that must outlive it and its other windows - an Azlin drive's session (its drive
//! token rotates with every refresh), the unfinished checkouts. Every change of such an entry
//! is read, decided and written under the entry's lock ([`crate::lock`]), so a second window or
//! a second app never works from a text another one has replaced.
//!
//! Blocking, like the keyring it wraps (azul's in the apps, a map in the tests): call it from an
//! azul `Thread`.

use std::{
    collections::BTreeMap,
    fmt,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex, PoisonError},
    time::Duration,
};

use azul_storage::{
    config::keyring_key,
    keyring::{KeyringNamespace, KeyringStore},
};
use zeroize::Zeroizing;

use crate::{
    bundle::DriveBundle,
    error::CloudResult,
    lock::{HeldLock, LockDir},
    outbox::Outbox,
    session::{AzlinSession, DriveToken},
    token::TokenError,
};

/// How long a change waits for another process's change of the same entry: a refresh holds the
/// lock for one call to the token server, which azul's HTTP client gives up on after 90 s.
pub const LOCK_WAIT: Duration = Duration::from_secs(120);

/// The variable naming azul's keyring stand-in file (a test stack's): its entries are that
/// file's, so their locks are beside it.
pub const KEYRING_FILE_VAR: &str = "AZ_KEYRING_FILE";

/// Sessions a refresh rotated that the keyring did not take, by the lock file of their entry:
/// the newest token this process knows, spent by the next refresh of the entry instead of the
/// spent one the keyring still has (and stored again then).
static UNSAVED: Mutex<BTreeMap<PathBuf, Zeroizing<String>>> = Mutex::new(BTreeMap::new());

fn unsaved() -> std::sync::MutexGuard<'static, BTreeMap<PathBuf, Zeroizing<String>>> {
    UNSAVED.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The keyring and the locks of its entries. `Debug` shows where the locks are, nothing of the
/// keyring.
#[derive(Clone)]
pub struct SharedKeyring {
    keyring: Arc<dyn KeyringStore>,
    namespace: KeyringNamespace,
    locks: LockDir,
}

impl fmt::Debug for SharedKeyring {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SharedKeyring")
            .field("namespace", &self.namespace)
            .field("locks", &self.locks)
            .finish_non_exhaustive()
    }
}

/// The user's configuration folder, as the OS names it (the kit has no azul to ask).
fn user_config_dir() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(windows) {
        return var("APPDATA");
    }
    if cfg!(target_os = "macos") {
        return var("HOME").map(|home| home.join("Library").join("Application Support"));
    }
    var("XDG_CONFIG_HOME").or_else(|| var("HOME").map(|home| home.join(".config")))
}

/// The folder name of `namespace` among the locks: its letters, digits, `.`, `-` and `_`;
/// `app` for an app's own namespace.
fn namespace_folder(namespace: &KeyringNamespace) -> String {
    let name: String = namespace
        .as_str()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() || name.chars().all(|c| c == '.') {
        String::from("app")
    } else {
        name
    }
}

/// Where the locks of the keyring entries of `namespace` are: one folder per keyring and
/// namespace, whichever app or profile asks - `<config dir>/azlin/locks/<namespace>`, or beside
/// azul's keyring stand-in file when [`KEYRING_FILE_VAR`] names one (`<file>.locks/<namespace>`).
#[must_use]
pub fn lock_folder(namespace: &KeyringNamespace) -> PathBuf {
    lock_folder_for(std::env::var_os(KEYRING_FILE_VAR), user_config_dir(), namespace)
}

/// [`lock_folder`] for the stand-in file `keyring_file` (if any) and the configuration folder
/// `config_dir`.
#[must_use]
pub fn lock_folder_for(
    keyring_file: Option<std::ffi::OsString>,
    config_dir: Option<PathBuf>,
    namespace: &KeyringNamespace,
) -> PathBuf {
    let root = match keyring_file.filter(|v| !v.is_empty()) {
        Some(mut file) => {
            file.push(".locks");
            PathBuf::from(file)
        }
        None => config_dir
            .unwrap_or_else(std::env::temp_dir)
            .join("azlin")
            .join("locks"),
    };
    root.join(namespace_folder(namespace))
}

impl SharedKeyring {
    /// `keyring`, whose entries live in `namespace`, changed under the locks of that keyring and
    /// namespace ([`lock_folder`]): every app and profile of this user that reaches the same
    /// entries takes turns at the same locks.
    #[must_use]
    pub fn new(keyring: Arc<dyn KeyringStore>, namespace: KeyringNamespace) -> SharedKeyring {
        let locks = LockDir::new(lock_folder(&namespace));
        SharedKeyring {
            keyring,
            namespace,
            locks,
        }
    }

    /// [`SharedKeyring::new`] with its locks under `root` instead of the user's folder: a test's
    /// keyring of its own (a map) with locks of its own.
    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn with_lock_root(
        keyring: Arc<dyn KeyringStore>,
        namespace: KeyringNamespace,
        root: &std::path::Path,
    ) -> SharedKeyring {
        let locks = LockDir::new(root.join(namespace_folder(&namespace)));
        SharedKeyring {
            keyring,
            namespace,
            locks,
        }
    }

    /// The namespace the entries live in.
    #[must_use]
    pub fn namespace(&self) -> &KeyringNamespace {
        &self.namespace
    }

    /// Where the locks are.
    #[must_use]
    pub fn locks(&self) -> &LockDir {
        &self.locks
    }

    /// The open grant calls of this keyring's credentials ([`crate::outbox`]): with its locks,
    /// so every app and profile that spends the same tokens asks again with the same keys.
    #[must_use]
    pub fn outbox(&self) -> Outbox {
        Outbox::new(self.locks.path().join(crate::outbox::OUTBOX_DIR))
    }

    /// Takes the lock of the entry `key`: one holder in every process of this user at a time.
    ///
    /// # Errors
    ///
    /// When another holder keeps it longer than [`LOCK_WAIT`], or it cannot be taken.
    pub fn lock(&self, key: &str) -> CloudResult<HeldLock> {
        self.locks.lock(key, LOCK_WAIT)
    }

    /// The text of the entry `key`; `None` when there is none.
    ///
    /// # Errors
    ///
    /// When the keyring cannot be read.
    pub fn get(&self, key: &str) -> CloudResult<Option<String>> {
        if let Some(text) = unsaved().get(&self.locks.file_of(key)) {
            return Ok(Some(text.to_string()));
        }
        Ok(self.keyring.get(key)?)
    }

    /// The entry `key` under its lock: a session this process rotated that the keyring did not
    /// take is stored again first, and is the entry's text either way.
    fn read_locked(&self, key: &str) -> CloudResult<Option<Zeroizing<String>>> {
        let file = self.locks.file_of(key);
        let pending = unsaved().get(&file).cloned();
        if let Some(text) = pending {
            if self.keyring.set(key, &text).is_ok() {
                unsaved().remove(&file);
            }
            return Ok(Some(text));
        }
        Ok(self.keyring.get(key)?.map(Zeroizing::new))
    }

    /// Stores `text` under `key`, held under its lock; when the keyring does not take it, this
    /// process keeps it as the entry's text ([`SharedKeyring::get`] and the next spend read it).
    fn keep_locked(&self, key: &str, text: &str) -> Result<(), String> {
        let file = self.locks.file_of(key);
        match self.keyring.set(key, text) {
            Ok(()) => {
                unsaved().remove(&file);
                Ok(())
            }
            Err(e) => {
                unsaved().insert(file, Zeroizing::new(text.to_string()));
                Err(e.to_string())
            }
        }
    }

    /// `held` (the session an app opened a drive with) as the entry of its drive when the
    /// keyring has no session of the drive or cannot be read (a system without a keyring):
    /// this process keeps it then, the next spend takes it and stores it where it can. A
    /// session the keyring has stays the one spent.
    ///
    /// # Errors
    ///
    /// When the lock cannot be taken.
    pub fn adopt(&self, held: &AzlinSession) -> CloudResult<()> {
        let drive_id = held.drive_id.as_str();
        self.adopt_entry(&keyring_key(drive_id), &held.to_keyring_secret(), |text| {
            AzlinSession::from_keyring_secret_without_credentials(text)
                .is_ok_and(|s| s.drive_id == drive_id && !s.drive_token.is_empty())
        })
    }

    /// [`SharedKeyring::adopt`] of an entry in another format: `held` becomes the text of `key`
    /// in this process unless the keyring has a text `ours` takes.
    ///
    /// # Errors
    ///
    /// When the lock cannot be taken.
    pub fn adopt_entry(
        &self,
        key: &str,
        held: &str,
        ours: impl Fn(&str) -> bool,
    ) -> CloudResult<()> {
        let _lock = self.lock(key)?;
        let file = self.locks.file_of(key);
        if unsaved().contains_key(&file) {
            return Ok(());
        }
        let has = self.keyring.get(key).ok().flatten().is_some_and(|text| ours(&text));
        if !has && ours(held) {
            unsaved().insert(file, Zeroizing::new(held.to_string()));
        }
        Ok(())
    }

    /// The drive token of the drive `drive_id`'s session in the keyring, to spend once: under the
    /// drive's lock (the one every refresh and grant of the drive holds) until the spend's
    /// answer is kept or given up. The only way to a refresh.
    ///
    /// # Errors
    ///
    /// When the lock cannot be taken, or the keyring holds no session of the drive.
    pub fn spend(&self, drive_id: &str) -> CloudResult<SpendOnce<'_>> {
        let key = keyring_key(drive_id);
        let wanted = drive_id.trim();
        self.spend_entry(&key, wanted, |text| {
            AzlinSession::from_keyring_secret_without_credentials(text)
                .ok()
                .filter(|session| session.drive_id == wanted && !session.drive_token.is_empty())
                .map(|session| session.drive_token)
        })
    }

    /// [`SharedKeyring::spend`] of an entry in another format (AzMail's account sessions): the
    /// token `token_of` reads from the text of the entry `key`, for the drive `drive_id`.
    ///
    /// # Errors
    ///
    /// When the lock cannot be taken, or the entry holds no token.
    pub fn spend_entry(
        &self,
        key: &str,
        drive_id: &str,
        token_of: impl FnOnce(&str) -> Option<DriveToken>,
    ) -> CloudResult<SpendOnce<'_>> {
        let lock = self.lock(key)?;
        let entry = self.read_locked(key)?;
        let token = entry
            .as_deref()
            .and_then(|text| token_of(text))
            .filter(|token| !token.is_empty())
            .ok_or_else(|| {
                TokenError::SignIn(format!(
                    "this device keeps no session of the drive {drive_id}"
                ))
            })?;
        Ok(SpendOnce {
            drive_id: drive_id.trim().to_string(),
            token,
            entry,
            home: Home::Keyring {
                shared: self,
                key: key.to_string(),
            },
            lock: SpendLock::new(lock),
        })
    }



    /// Stores `text` under `key`.
    ///
    /// # Errors
    ///
    /// When the keyring does not take it.
    pub fn set(&self, key: &str, text: &str) -> CloudResult<()> {
        let file = self.locks.file_of(key);
        match self.keyring.set(key, text) {
            Ok(()) => {
                // the keyring has the newest text again
                unsaved().remove(&file);
                Ok(())
            }
            Err(e) => {
                // an entry this process keeps already keeps this text instead
                if let Some(kept) = unsaved().get_mut(&file) {
                    *kept = Zeroizing::new(text.to_string());
                }
                Err(e.into())
            }
        }
    }

    /// Removes the entry `key`.
    ///
    /// # Errors
    ///
    /// When the keyring cannot remove it.
    pub fn delete(&self, key: &str) -> CloudResult<()> {
        unsaved().remove(&self.locks.file_of(key));
        Ok(self.keyring.delete(key)?)
    }

    /// Runs `call` with the newest drive token of `drive_id` the keyring has, under the drive's
    /// lock - the one every refresh of the drive holds: no refresh rotates the token while the
    /// call is out, so a call that takes only the current token (members, keys, lockdown,
    /// restores) never sends a spent one, which the token server would take for a reuse
    /// (AZDRIVE-INTEGRATION §4). One such call at a time per drive.
    ///
    /// # Errors
    ///
    /// When the lock cannot be taken, or the keyring has no session of the drive.
    pub fn with_drive_token<T>(
        &self,
        drive_id: &str,
        call: impl FnOnce(&str) -> T,
    ) -> CloudResult<T> {
        let key = keyring_key(drive_id);
        let _lock = self.lock(&key)?;
        let session = self
            .read_locked(&key)?
            .map(|text| AzlinSession::from_keyring_secret(&text))
            .transpose()?
            .filter(|session| session.drive_id == drive_id && !session.drive_token.is_empty())
            .ok_or_else(|| {
                TokenError::SignIn(format!(
                    "this device keeps no session of the drive {drive_id}"
                ))
            })?;
        Ok(call(session.drive_token.as_str()))
    }

    /// The session of a new drive (`bundle`: a sign-up, a paid checkout's) into the drive's
    /// keyring entry - unless the entry holds a session of that drive already, which stays (it
    /// may have rotated since: the sign-up's token is spent then). Under the drive's lock, as
    /// every refresh of the drive is. The entry's text now, and whether it was there already.
    ///
    /// # Errors
    ///
    /// When the lock cannot be taken or the keyring cannot be read or written.
    pub fn keep_new_drive(&self, bundle: &DriveBundle) -> CloudResult<(String, bool)> {
        let key = keyring_key(bundle.drive_id());
        let _lock = self.lock(&key)?;
        if let Some(text) = self.read_locked(&key)? {
            let ours = AzlinSession::from_keyring_secret(&text)
                .is_ok_and(|session| session.drive_id == bundle.drive_id());
            if ours {
                return Ok((text.to_string(), true));
            }
        }
        let text = bundle.session().to_keyring_secret();
        self.set(&key, &text)?;
        Ok((text, false))
    }
}

/// Where the answer of a spend is kept.
enum Home<'k> {
    /// The keyring entry the token was read from.
    Keyring { shared: &'k SharedKeyring, key: String },
    /// The caller keeps it (under the lock it gave).
    Caller,
}

/// How many spends are out in this process (taken, their answer not kept or given up yet), and
/// the waiters for none.
static SPENDS_OUT: (Mutex<usize>, Condvar) = (Mutex::new(0), Condvar::new());

/// One spend out, counted in [`SPENDS_OUT`] while it lives.
struct Out;

impl Out {
    fn new() -> Out {
        *SPENDS_OUT.0.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        Out
    }
}

impl Drop for Out {
    fn drop(&mut self) {
        let mut out = SPENDS_OUT.0.lock().unwrap_or_else(PoisonError::into_inner);
        *out = out.saturating_sub(1);
        SPENDS_OUT.1.notify_all();
    }
}

/// Waits up to `limit` until no spend of this process is out (a refresh that spent a token
/// and has not kept its answer yet): an app that quits calls it, so the session a refresh just
/// rotated reaches the keyring. Whether none is out now.
pub fn wait_for_spends(limit: Duration) -> bool {
    let (out, none) = &SPENDS_OUT;
    let guard = out.lock().unwrap_or_else(PoisonError::into_inner);
    let (guard, _) = none
        .wait_timeout_while(guard, limit, |n| *n > 0)
        .unwrap_or_else(PoisonError::into_inner);
    *guard == 0
}

/// The lock a spend holds (a keyring entry's, or the caller's): dropped, it is released.
pub struct SpendLock {
    _lock: Box<dyn std::any::Any>,
    _out: Out,
}

impl SpendLock {
    fn new(lock: impl std::any::Any) -> SpendLock {
        SpendLock {
            _lock: Box::new(lock),
            _out: Out::new(),
        }
    }
}

impl fmt::Debug for SpendLock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SpendLock")
    }
}

/// A drive token taken out to be spent once, with the lock that keeps every other process of
/// this user from spending it meanwhile. Only [`SharedKeyring::spend`] (and its kin) make one;
/// [`crate::TokenServer::refresh`] takes it by value. `Debug` shows no secret.
pub struct SpendOnce<'k> {
    drive_id: String,
    token: DriveToken,
    /// The text of the entry the token was read from.
    entry: Option<Zeroizing<String>>,
    home: Home<'k>,
    lock: SpendLock,
}

impl fmt::Debug for SpendOnce<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpendOnce")
            .field("drive_id", &self.drive_id)
            .finish_non_exhaustive()
    }
}

impl<'k> SpendOnce<'k> {
    /// The drive the token opens.
    #[must_use]
    pub fn drive_id(&self) -> &str {
        &self.drive_id
    }

    /// The text of the keyring entry the token was read from: a caller that finds a session in
    /// it that is good enough drops the spend, and nothing was spent.
    #[must_use]
    pub fn entry(&self) -> Option<&str> {
        self.entry.as_deref().map(String::as_str)
    }

    /// The entry as the kit's session (with or without credentials).
    #[must_use]
    pub fn session(&self) -> Option<AzlinSession> {
        AzlinSession::from_keyring_secret_without_credentials(self.entry()?).ok()
    }

    /// A token this device was given, not kept in a keyring yet (a join code's), to spend once
    /// under `lock`, the lock of where the caller keeps the answer.
    pub(crate) fn given(
        drive_id: &str,
        token: DriveToken,
        lock: impl std::any::Any,
    ) -> SpendOnce<'static> {
        SpendOnce {
            drive_id: drive_id.trim().to_string(),
            token,
            entry: None,
            home: Home::Caller,
            lock: SpendLock::new(lock),
        }
    }

    /// The token, for the request that spends it.
    #[must_use]
    pub fn token(&self) -> &DriveToken {
        &self.token
    }

    /// The answer `bundle` of the spend (a refresh's, or a lockdown's new grant), under the same
    /// lock until it is kept. [`crate::TokenServer::refresh`] makes it; a test's token server
    /// too.
    pub fn answered(self, bundle: DriveBundle) -> Rotation<'k> {
        Rotation {
            bundle,
            spend: self,
        }
    }
}

/// A refresh's answer: the next token, still under the drive's lock until it is kept. `Debug`
/// shows no secret.
#[must_use = "the token just spent is dead: keep the new session"]
#[derive(Debug)]
pub struct Rotation<'k> {
    bundle: DriveBundle,
    spend: SpendOnce<'k>,
}

/// A rotated session kept: the session, and whether the keyring took it (`Err` why not: this
/// process keeps it meanwhile and its next refresh spends it).
pub struct Kept {
    pub session: AzlinSession,
    pub saved: Result<(), String>,
}

impl<'k> Rotation<'k> {
    /// The answer.
    #[must_use]
    pub fn bundle(&self) -> &DriveBundle {
        &self.bundle
    }

    /// Keeps the answer's session as the kit's keyring text in the entry the token came from,
    /// before the lock is let go.
    pub fn keep(self) -> Kept {
        let session = self.bundle.session();
        let saved = self.keep_text(&session.to_keyring_secret());
        Kept { session, saved }
    }

    /// Keeps `text` (the caller's format of the answer: AzMail's account session) in the entry
    /// the token came from, before the lock is let go. A spend the caller keeps itself
    /// ([`SpendOnce::given`]) keeps nothing here.
    ///
    /// # Errors
    ///
    /// Why the keyring did not take it (this process keeps it meanwhile).
    pub fn keep_text(self, text: &str) -> Result<(), String> {
        match &self.spend.home {
            Home::Keyring { shared, key } => shared.keep_locked(key, text),
            Home::Caller => Ok(()),
        }
    }

    /// The answer and the lock, for a caller that keeps the session itself.
    #[must_use]
    pub fn into_parts(self) -> (DriveBundle, SpendLock) {
        (self.bundle, self.spend.lock)
    }
}

/// A refresh whose outcome is unknown: the request may have been spent (no answer, an answer
/// cut off, a 503 that does not say it spent nothing). The token server answers the same token
/// again as long as the token it may have rotated to was never used, so the only way on is
/// that same token again: [`crate::TokenServer::retry`], now or by a later refresh (the
/// keyring still holds it).
#[derive(Debug)]
pub struct PendingSpend<'k> {
    spend: SpendOnce<'k>,
    error: TokenError,
}

impl<'k> PendingSpend<'k> {
    pub(crate) fn new(spend: SpendOnce<'k>, error: TokenError) -> PendingSpend<'k> {
        PendingSpend { spend, error }
    }

    /// What happened to the request.
    #[must_use]
    pub fn error(&self) -> &TokenError {
        &self.error
    }

    /// Gives up for now: the lock is let go, the keyring keeps the token, and the next refresh
    /// sends it again.
    #[must_use]
    pub fn into_error(self) -> TokenError {
        self.error
    }

    pub(crate) fn into_spend(self) -> SpendOnce<'k> {
        self.spend
    }
}

/// How a refresh ended.
#[derive(Debug)]
#[must_use]
pub enum RefreshOutcome<'k> {
    /// The token was spent for the next one: keep it.
    Rotated(Rotation<'k>),
    /// The token server refused (the token was not spent, or is dead for good: `error` says
    /// which to the user).
    Refused {
        /// What the token server said (`None`: refused here, or an answer that is no refusal of
        /// the contract).
        refusal: Option<azlin_contract::RefreshRefusal>,
        error: TokenError,
    },
    /// No telling whether the token was spent.
    Unknown(PendingSpend<'k>),
}

impl<'k> RefreshOutcome<'k> {
    /// The rotation, or the error to report (an unknown outcome gives up for now).
    ///
    /// # Errors
    ///
    /// The refusal, or what kept the outcome unknown.
    pub fn rotated(self) -> Result<Rotation<'k>, TokenError> {
        match self {
            RefreshOutcome::Rotated(rotation) => Ok(rotation),
            RefreshOutcome::Refused { error, .. } => Err(error),
            RefreshOutcome::Unknown(pending) => Err(pending.into_error()),
        }
    }
}
