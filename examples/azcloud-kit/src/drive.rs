//! An Azlin drive as an azul-storage [`Drive`]: an S3 bucket whose temporary credentials (12 h)
//! are refreshed with the drive token before they run out, and once more when the bucket
//! refuses them (a clock that is off, a revoked key). Every refresh spends the drive token and
//! answers the next one; spending a spent one makes the token server revoke this device.
//!
//! So a refresh is one at a time across every process of the user (two AzDrive windows, two
//! apps over one drive): it holds the drive's lock (the lock of its keyring entry,
//! [`SharedKeyring::lock`]) and reads the keyring first - another process may have refreshed
//! already: its session is the newest then, and is taken as it is unless its credentials need
//! a refresh too (which spends ITS token) - and it stores the rotated session in the keyring
//! before it lets go. [`AzlinDrive`] hands every session it switches to to the app's
//! `on_rotated`, with whether the keyring has it. Within the process the session is behind a
//! lock as well, so two calls that find the credentials running out at once refresh ONCE - the
//! second sees the first's answer. Blocking, like every drive: call it from an azul `Thread`.

use std::{
    fmt,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use azul_storage::{
    config::{keyring_key, DriveEntry},
    time::now_unix,
    ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, S3Drive, Transport,
};

use crate::{
    session::AzlinSession,
    shared::SharedKeyring,
    token::{TokenError, TokenServer},
};

/// Makes a transport: one for every S3 drive the credentials open, one for every refresh.
pub type TransportFactory = Arc<dyn Fn() -> Box<dyn Transport> + Send + Sync>;
/// Hears every session the drive switches to (the app keeps it as the drive's secret in
/// memory), and whether the keyring has it: `Ok` it was stored there by this refresh or read
/// from there (another process refreshed); `Err` why it could not be stored - it lives in this
/// process only, and the app should say so.
pub type OnRotated = Box<dyn Fn(&AzlinSession, Result<(), String>) + Send + Sync>;

/// The S3 error codes that mean "these credentials no longer work".
const REFUSED_CREDENTIALS: &[&str] = &[
    "ExpiredToken",
    "InvalidToken",
    "TokenRefreshRequired",
    "InvalidAccessKeyId",
];

/// The session and the bucket its credentials open.
struct Current {
    session: AzlinSession,
    drive: Option<Arc<S3Drive>>,
}

/// An Azlin drive that refreshes its credentials by itself. `Debug` shows no secret.
pub struct AzlinDrive {
    entry: DriveEntry,
    token_url: String,
    current: Mutex<Current>,
    /// Where the drive's session lives between processes, and the lock its refreshes hold.
    keyring: SharedKeyring,
    transports: TransportFactory,
    on_rotated: OnRotated,
    clock: Box<dyn Fn() -> u64 + Send + Sync>,
}

impl fmt::Debug for AzlinDrive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AzlinDrive")
            .field("entry", &self.entry)
            .field("token_url", &self.token_url)
            .finish_non_exhaustive()
    }
}

/// A token server's refusal as the drive's error.
fn drive_error_of(e: &TokenError) -> DriveError {
    match e {
        TokenError::Connect(why) => DriveError::Transport(why.clone()),
        TokenError::SignIn(_) => DriveError::Denied {
            message: e.to_string(),
        },
        TokenError::Config(why) => DriveError::InvalidConfig(why.clone()),
        TokenError::Refused { .. } | TokenError::Protocol(_) => {
            DriveError::Protocol(e.to_string())
        }
    }
}

/// Whether `e` says the credentials no longer work.
fn refused_credentials(e: &DriveError) -> bool {
    matches!(e, DriveError::Service(service) if REFUSED_CREDENTIALS.contains(&service.code.as_str()))
}

impl AzlinDrive {
    /// The Azlin drive `entry` (its location: the bucket), with the keyring's `session`. Its
    /// refreshes go to the token server the entry names, else `token_url`, and read and write
    /// the drive's entry of `keyring` under its lock. Sends nothing.
    pub fn new(
        entry: &DriveEntry,
        session: AzlinSession,
        token_url: &str,
        keyring: SharedKeyring,
        transports: TransportFactory,
        on_rotated: OnRotated,
    ) -> Result<AzlinDrive, DriveError> {
        if entry.s3_config().is_none() {
            return Err(DriveError::InvalidConfig(format!(
                "\"{}\" is not a bucket",
                entry.name
            )));
        }
        let named = entry.azlin().map(|(_, url)| url.trim()).unwrap_or_default();
        let token_url = if named.is_empty() {
            token_url.trim()
        } else {
            named
        };
        if token_url.is_empty() {
            return Err(DriveError::InvalidConfig(format!(
                "\"{}\" names no token server to refresh its credentials at",
                entry.name
            )));
        }
        Ok(AzlinDrive {
            entry: entry.clone(),
            token_url: token_url.trim_end_matches('/').to_string(),
            current: Mutex::new(Current {
                session,
                drive: None,
            }),
            keyring,
            transports,
            on_rotated,
            clock: Box::new(now_unix),
        })
    }

    /// Reads the time from `clock` (seconds since 1970) instead of the system's.
    #[must_use]
    pub fn with_clock(mut self, clock: impl Fn() -> u64 + Send + Sync + 'static) -> Self {
        self.clock = Box::new(clock);
        self
    }

    /// The session as it is now (after any refresh).
    #[must_use]
    pub fn session(&self) -> AzlinSession {
        self.lock().session.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Current> {
        self.current
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The bucket with credentials that work now (refreshed first when they run out), and the
    /// access key it signs with.
    fn bucket(&self) -> Result<(Arc<S3Drive>, String), DriveError> {
        let mut current = self.lock();
        if current.session.needs_refresh((self.clock)()) {
            self.refresh(&mut current, None)?;
        }
        let drive = self.opened(&mut current)?;
        Ok((drive, current.session.access_key_id.clone()))
    }

    /// The bucket of the session `current` holds, opened on first use.
    fn opened(&self, current: &mut Current) -> Result<Arc<S3Drive>, DriveError> {
        if let Some(drive) = &current.drive {
            return Ok(drive.clone());
        }
        let config = self
            .entry
            .s3_config()
            .ok_or_else(|| DriveError::InvalidConfig(String::from("not a bucket")))?;
        let drive = Arc::new(S3Drive::new(
            config,
            current.session.credentials(),
            (self.transports)(),
        )?);
        current.drive = Some(drive.clone());
        Ok(drive)
    }

    /// Fresh credentials: under the drive's lock, the session another process refreshed (the
    /// keyring's, when its token is not the one `current` holds - and its credentials are not
    /// running out, nor the ones the bucket refused: `refused`), else the drive token spent for
    /// new ones, which go into the keyring before the lock is let go. The new session takes the
    /// old one's place and goes to `on_rotated`. Called with `current` held: one refresh at a
    /// time in this process too (`on_rotated` must not call back into this drive).
    fn refresh(&self, current: &mut Current, refused: Option<&str>) -> Result<(), DriveError> {
        let key = keyring_key(&current.session.drive_id);
        let _held = self
            .keyring
            .lock(&key)
            .map_err(|e| DriveError::Io(e.to_string()))?;
        if let Some(newer) = self.newer_in_keyring(&key, &current.session) {
            let usable = match refused {
                Some(refused) => newer.access_key_id != refused,
                None => !newer.needs_refresh((self.clock)()),
            };
            // The token this process held is spent either way: the keyring's is the newest.
            current.session = newer;
            current.drive = None;
            if usable {
                (self.on_rotated)(&current.session, Ok(()));
                return Ok(());
            }
        }
        let transport = (self.transports)();
        let server =
            TokenServer::new(&self.token_url, transport.as_ref()).map_err(|e| drive_error_of(&e))?;
        let bundle = server
            .refresh(&current.session.drive_id, &current.session.drive_token)
            .map_err(|e| drive_error_of(&e))?;
        let session = bundle.session();
        // In the keyring BEFORE the lock is let go: the token just spent is dead, and the next
        // process to refresh must find this one.
        let saved = self
            .keyring
            .set(&key, &session.to_keyring_secret())
            .map_err(|e| e.to_string());
        (self.on_rotated)(&session, saved);
        current.session = session;
        current.drive = None;
        Ok(())
    }

    /// The keyring's session of the drive when it is newer than `held` - another process spent
    /// `held`'s token -; `None` when the keyring has none, the same one, or cannot be read (the
    /// session this process holds is the newest it knows then).
    fn newer_in_keyring(&self, key: &str, held: &AzlinSession) -> Option<AzlinSession> {
        let text = self.keyring.get(key).ok()??;
        let stored = AzlinSession::from_keyring_secret(&text).ok()?;
        (stored.drive_id == held.drive_id && stored.drive_token != held.drive_token)
            .then_some(stored)
    }

    /// The bucket after the bucket refused the credentials of `refused` (an access key): a
    /// refresh, unless another call refreshed meanwhile.
    fn after_refusal(&self, refused: &str) -> Result<Arc<S3Drive>, DriveError> {
        let mut current = self.lock();
        if current.session.access_key_id == refused {
            self.refresh(&mut current, Some(refused))?;
        }
        self.opened(&mut current)
    }

    /// Runs `call` on the bucket; when the bucket refuses the credentials, refreshes them and
    /// runs it once more.
    fn with_bucket<T>(
        &self,
        call: impl Fn(&S3Drive) -> Result<T, DriveError>,
    ) -> Result<T, DriveError> {
        let (bucket, key) = self.bucket()?;
        match call(&bucket) {
            Err(e) if refused_credentials(&e) => {
                let bucket = self.after_refusal(&key)?;
                call(&bucket)
            }
            other => other,
        }
    }
}

impl Drive for AzlinDrive {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.with_bucket(|b| b.list(request))
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.with_bucket(|b| b.get(key))
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.with_bucket(|b| b.get_range(key, range))
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.with_bucket(|b| b.put(key, bytes))
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.with_bucket(|b| b.delete(key))
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.with_bucket(|b| b.head(key))
    }
    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.with_bucket(|b| b.copy(from, to))
    }
    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.with_bucket(|b| b.create_folder(prefix))
    }
    fn rename(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.with_bucket(|b| b.rename(from, to))
    }
    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.with_bucket(|b| b.delete_folder(prefix))
    }
    fn local_path(&self, _key: &str) -> Option<PathBuf> {
        None
    }
    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        self.with_bucket(|b| b.metadata(key))
    }
}
