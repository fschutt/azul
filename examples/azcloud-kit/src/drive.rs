//! An Azlin drive in two halves (ARCH29 C6d): its [`AzlinAccount`] - the session, the token server, the
//! keyring entry, the node list, the grant - and the bytes, an azul-storage [`Drive`] over an S3 bucket
//! whose temporary credentials (12 h) the account refreshes with the drive token before they run out,
//! and once more when the bucket refuses them (a clock that is off, a revoked key). [`open_azlin`] opens
//! both: the account is handed out as it is, the bytes only as the drive the app's gate made of them,
//! so a job that holds the account has nothing to write with. Every refresh spends the drive token and
//! answers the next one; spending a spent one makes the token server revoke this device.
//!
//! So a refresh is one at a time across every process of the user (two AzDrive windows, two
//! apps over one drive): it holds the drive's lock (the lock of its keyring entry,
//! [`SharedKeyring::lock`]) and reads the keyring first - another process may have refreshed
//! already: its session is the newest then, and is taken as it is unless its credentials need
//! a refresh too (which spends ITS token) - and it stores the rotated session in the keyring
//! before it lets go. [`AzlinAccount`] hands every session it switches to to the app's
//! `on_rotated`, with whether the keyring has it. Within the process the session is behind a
//! lock as well, so two calls that find the credentials running out at once refresh ONCE - the
//! second sees the first's answer. Blocking, like every drive: call it from an azul `Thread`.
//!
//! Every request goes through the drive's [`Failover`] (iroh to the nodes when the app plugs in
//! a dialer - [`AzlinOpen::iroh`] -, then the block endpoint, a node's hint, the node
//! list, the nodes' addresses; retries by the class of the answer): each refresh hands it
//! the node list it answered with, and with a nodes file ([`AzlinOpen::nodes_file`]) the
//! list outlives the app - the next start fails over before its first refresh.

use std::{
    fmt,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use azlin_contract::IdempotentScope;
use azul_storage::{
    config::{DriveAuth, DriveEntry, DriveLocation},
    ops,
    time::now_unix,
    ui_thread::NetworkLock,
    AzlinError, BatchDelete, BatchMove, BatchPut, BatchStream, BucketSpace, ByteRange, Ctl, Drive,
    DriveError, ListPage, ListRequest, ObjectInfo, Precondition, S3Drive, ServiceError, Transport,
};

#[cfg(feature = "encryption")]
use azul_storage::AutoEncrypted;

use crate::{
    bundle::DriveBundle,
    failover::{cached_nodes, read_nodes, write_nodes, Failover, Node, TrustedEndpoint},
    outbox,
    period::GrantChanged,
    session::{AzlinSession, Grant},
    shared::{SharedKeyring, SpendOnce},
    token::{TokenError, TokenServer},
    transport::{IrohDialer, IrohLane},
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

/// An Azlin drive's ACCOUNT: its session and the keyring entry it lives in, the token server it is
/// refreshed at, the node list its requests fail over to, the grant and the restore announcement the
/// token server last told - what a lockdown, a count of the stored bytes, a month bought and a sync
/// session's restore notice are about. It refreshes the credentials of its bucket by itself and hands
/// them to the bytes ([`AzlinDrive`], the store [`open_azlin`] gates). It has no `put` and no `get`: a
/// job that holds one cannot write into the drive's bucket, which is how a price notice reached an
/// encrypted drive's bucket in the clear, below the gate (X-DELTA-AZDRIVE-KEYS-4). `Debug` shows no
/// secret.
///
/// What the account does not show: the session (it holds the drive token and the bucket's credentials), the
/// failover that routes the drive's requests and the node list; the iroh lane has no reader at all. A job that
/// holds the account - which is the point of the split - had them in public methods nobody called
/// (X-DELTA-STRUCT3-14):
///
/// ```compile_fail,E0599
/// use azcloud_kit::AzlinAccount;
/// fn look(account: &AzlinAccount) {
///     let _ = account.with_session(|session| session.drive_id.clone());
/// }
/// ```
///
/// ```compile_fail,E0599
/// use azcloud_kit::AzlinAccount;
/// fn route(account: &AzlinAccount) {
///     let _ = account.failover();
/// }
/// ```
///
/// ```compile_fail,E0599
/// use azcloud_kit::AzlinAccount;
/// fn nodes(account: &AzlinAccount) {
///     let _ = account.nodes();
/// }
/// ```
///
/// What it shows, the control of the four above:
///
/// ```no_run
/// use azcloud_kit::AzlinAccount;
/// fn look(account: &AzlinAccount) {
///     let _ = (account.drive_id(), account.token_url(), account.restore_notice());
/// }
/// ```
pub struct AzlinAccount {
    entry: DriveEntry,
    /// The drive's id at the token server, as [`azlin_target`] resolved it: what its keys, its vault and its
    /// grants are kept under.
    drive_id: String,
    token_url: String,
    /// Held across a refresh's requests, so the UI thread never takes it.
    current: NetworkLock<Current>,
    /// Where the drive's session lives between processes, and the lock its refreshes hold.
    keyring: SharedKeyring,
    transports: TransportFactory,
    on_rotated: OnRotated,
    clock: Box<dyn Fn() -> u64 + Send + Sync>,
    /// Where every request of the bucket goes (the node list of the last refresh).
    failover: Arc<Failover>,
    /// Where the node list is kept between starts, if anywhere.
    nodes_file: Option<PathBuf>,
    /// When (by `clock`) a refusal without a body last made the drive ask for fresh credentials:
    /// such a refusal could be expired credentials, or anything else - asked once in a while,
    /// never on every request.
    bodyless_refresh: Mutex<Option<u64>>,
    /// The token server changed the drive's grant (a month bought, a voucher): read-only
    /// credentials are refreshed before the next request ([`AzlinAccount::apply`]).
    grant_changed: AtomicBool,
    /// When (by `clock`) an answer naming an endpoint or a map the drive's view lacks last made
    /// it ask the token server for its view.
    view_refresh: Mutex<Option<u64>>,
    /// The restore announcement of the last refresh ([`AzlinAccount::restore_notice`]).
    restore: Mutex<Option<crate::sync::RestoreNotice>>,
}

/// Why credentials are refreshed: what makes another process's newer session good enough.
#[derive(Clone, Copy)]
enum Why<'a> {
    /// They run out soon.
    Expiring,
    /// The bucket refused the credentials of this access key.
    Refused(&'a str),
    /// The drive's grant changed: read-only ones are of before.
    GrantChanged,
    /// An answer named an endpoint or a map the view lacks: the token server's view (nodes,
    /// map) is wanted, so the token is spent even when another process refreshed.
    View,
}

/// How long the drive waits before an answer may make it ask for its view again (seconds): a
/// node that names strangers on every answer does not make every request spend a token.
const VIEW_REFRESH_EVERY: u64 = 300;

/// How long a refusal without a body waits before it makes the drive ask for fresh
/// credentials again (seconds).
const BODYLESS_REFRESH_EVERY: u64 = 600;

/// Whether `e` is a 400 or 403 that says nothing more (a HEAD's answer has no body): expired
/// credentials look like that too.
fn bodyless_refusal(e: &DriveError) -> bool {
    matches!(e, DriveError::Service(service)
        if matches!(service.status, 400 | 403)
            && service.azlin_error.is_none()
            && matches!(service.code.as_str(), "BadRequest" | "Forbidden"))
}

impl fmt::Debug for AzlinAccount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AzlinAccount")
            .field("entry", &self.entry)
            .field("token_url", &self.token_url)
            .finish_non_exhaustive()
    }
}

/// The family a drive token names (`dt_<family>.<generation>.<random>`): not secret, the
/// random part is.
fn family_of(token: &str) -> Option<&str> {
    token.trim().strip_prefix("dt_")?.split('.').next()
}

/// A token server's refusal as the drive's error.
fn drive_error_of(e: &TokenError) -> DriveError {
    match e {
        TokenError::Connect(why) => DriveError::Transport(why.clone()),
        // The token is refused (this device was removed, or its token spent elsewhere): only
        // signing in again helps. That is the errors table's credentials_revoked, as a node's
        // refusal of revoked credentials is; `Denied` would read as the drive's own refusal
        // (a read-only grant), which keeps its own words.
        TokenError::SignIn(_) => DriveError::Service(ServiceError {
            status: 401,
            code: String::from("Unauthorized"),
            message: e.to_string(),
            azlin_error: Some(AzlinError::CredentialsRevoked),
            ..ServiceError::default()
        }),
        TokenError::Config(why) => DriveError::InvalidConfig(why.clone()),
        // A refusal keeps its status and code: a busy token server is a busy service to the
        // user (user_errors), not a broken answer - and a code of the errors table (a banned
        // drive past its end: drive_banned) stays that code.
        TokenError::Refused {
            status,
            code,
            message,
        } => DriveError::Service(ServiceError {
            status: *status,
            code: code.clone(),
            message: message.clone(),
            azlin_error: AzlinError::parse(code).filter(|known| *known != AzlinError::Internal),
            ..ServiceError::default()
        }),
        // A token server that asked to wait keeps its pause: the sync's gate and its next pass
        // do not ask before it is over.
        TokenError::Busy {
            status,
            code,
            message,
            retry_after,
            ..
        } => DriveError::Service(ServiceError {
            status: *status,
            code: code.clone(),
            message: message.clone(),
            azlin_error: AzlinError::parse(code).filter(|known| *known != AzlinError::Internal),
            retry_after: *retry_after,
            ..ServiceError::default()
        }),
        TokenError::Protocol(_) => DriveError::Protocol(e.to_string()),
    }
}

/// Whether `e` says the credentials no longer work: an S3 code for it, a node's
/// `session_expired` (all a HEAD's answer can say is its header), or its `credentials_stale`
/// (read-only credentials of before the drive took writes again).
fn refused_credentials(e: &DriveError) -> bool {
    matches!(e, DriveError::Service(service)
        if REFUSED_CREDENTIALS.contains(&service.code.as_str())
            || matches!(
                service.azlin_error,
                Some(AzlinError::SessionExpired | AzlinError::CredentialsStale)
            ))
}

impl AzlinAccount {
    /// The Azlin drive `entry` (its location: the bucket), with the keyring's `session`. Its
    /// refreshes go to the token server of `target` - [`azlin_target`]'s answer, the one place that
    /// chooses it (in a TEST build the server the entry names, if it names one: `drives.json` is a file any
    /// process of the user can write, and the rotated drive token goes where the refresh goes, so a shipped
    /// build never takes the address from it, REVIEW K20; X-DELTA-STRUCT3-11: the account chose it again
    /// itself, and a later change of the one choice did not reach the refresh) - and read and write the
    /// drive's entry of `keyring` under its lock. Sends nothing.
    ///
    /// The drive's id has two sources here: the entry's (`target`, the drives file's `drive_id`: what the
    /// keys, the vault and the grants are kept under) and the session's (the keyring's text: what the
    /// credentials are spent, refreshed and locked down under). The account takes the first as its own and
    /// spends the second, so a drives file that names the id of ANOTHER drive this computer holds a key for
    /// - any process of the user can write it - got the other drive's key looked up under this bucket, and for
    /// a bucket with no index yet its first files sealed under it. The two are compared before anything is
    /// made (X-DELTA-STRUCT4-2).
    ///
    /// # Errors
    ///
    /// An entry that is no bucket, and a session of another drive than the entry's: nothing is made and
    /// nothing is sent.
    pub(crate) fn new(
        entry: &DriveEntry,
        session: AzlinSession,
        target: &AzlinTarget,
        keyring: SharedKeyring,
        transports: TransportFactory,
        on_rotated: OnRotated,
    ) -> Result<AzlinAccount, DriveError> {
        if session.drive_id != target.drive_id {
            return Err(DriveError::InvalidConfig(format!(
                "\"{}\" is the drive {} in the drives file, but the session the keyring holds for it is the \
                 drive {}'s: the file and the keyring disagree, so nothing is opened with either",
                entry.name, target.drive_id, session.drive_id
            )));
        }
        let Some(config) = entry.s3_config() else {
            return Err(DriveError::InvalidConfig(format!(
                "\"{}\" is not a bucket",
                entry.name
            )));
        };
        Ok(AzlinAccount {
            entry: entry.clone(),
            drive_id: target.drive_id.clone(),
            token_url: target.token_url.trim_end_matches('/').to_string(),
            current: NetworkLock::new(Current {
                session,
                drive: None,
            }),
            keyring,
            transports,
            on_rotated,
            clock: Box::new(now_unix),
            failover: Arc::new(Failover::new(&config.endpoint)),
            nodes_file: None,
            bodyless_refresh: Mutex::new(None),
            grant_changed: AtomicBool::new(false),
            view_refresh: Mutex::new(None),
            restore: Mutex::new(None),
        })
    }

    /// Keeps the node list of every refresh in `path` (not secret: the app's state or cache
    /// folder), and starts with what a start may take from the list kept there: in a shipped
    /// build the HINTS of it - the addresses the block host is tried at - not the nodes' URLs
    /// or their iroh ids and sockets, which the file (any process of the user can write it) does
    /// not decide ([`cached_nodes`]); the first refresh of this process brings the list
    /// itself.
    #[must_use]
    pub(crate) fn with_nodes_file(self, path: &Path) -> Self {
        self.with_nodes_file_for(azul_appkit::anchors::layers_honoured(), path)
    }

    /// [`AzlinAccount::with_nodes_file`] with the build's choice as an argument (the tests set
    /// both).
    #[must_use]
    pub(crate) fn with_nodes_file_for(mut self, honour: bool, path: &Path) -> Self {
        let kept = cached_nodes(read_nodes(path), honour);
        if !kept.is_empty() {
            self.failover.set_nodes(kept);
        }
        self.nodes_file = Some(path.to_path_buf());
        self
    }

    /// The node list the requests fail over to (of the last refresh, or the nodes file). Only the kit's
    /// tests look at it: no app has a use for the failover of an account it holds, and a public reader of
    /// it is a hand on the drive's routing with no gate involved (X-DELTA-STRUCT3-14).
    #[cfg(test)]
    #[must_use]
    pub(crate) fn nodes(&self) -> Vec<Node> {
        self.failover.nodes()
    }

    /// The failover every request of the drive goes through (the kit's tests only, see
    /// [`AzlinAccount::nodes`]): its setters change where the drive's requests go.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn failover(&self) -> &Arc<Failover> {
        &self.failover
    }

    /// Sends every request over iroh first: to each ready node the node list names with an iroh
    /// id, dialed through `dialer` (the app's: azul's iroh endpoint) at its iroh sockets and
    /// relayed through `relay` (`off`, `default` or an address; `None`: the dialer's own); a node
    /// that fails rests for five minutes and the request goes on over HTTPS
    /// ([`crate::transport::IrohLane`]).
    #[must_use]
    pub(crate) fn with_iroh(self, dialer: Arc<dyn IrohDialer>, relay: Option<&str>) -> Self {
        self.failover
            .set_lane(Some(Arc::new(IrohLane::new(dialer, relay))));
        self
    }

    /// Takes a bundle's view: its node list and failover URLs (the list kept in the nodes file),
    /// the endpoints an answer may send the requests to, the cluster map's epoch.
    fn learn_nodes(&self, bundle: &DriveBundle) {
        let nodes = Node::list(&bundle.nodes);
        if let Some(path) = &self.nodes_file {
            write_nodes(path, &nodes);
        }
        self.failover.set_nodes(nodes);
        self.failover.set_alternatives(bundle.failover.clone());
        self.failover.trust(TrustedEndpoint::from_bundle(bundle));
        self.failover.saw_frozen(bundle.read_only);
        self.failover.set_map_epoch(bundle.map_epoch);
        *self.restore.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
            bundle.restore.clone();
    }

    /// The drive's newest restore announcement as the last refresh answered it: what an app's
    /// sync session takes as its restore notice ([`crate::sync::session::SyncSession::with_restore`]).
    #[must_use]
    pub fn restore_notice(&self) -> Option<crate::sync::RestoreNotice> {
        self.restore
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Reads the time from `clock` (seconds since 1970) instead of the system's.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_clock(mut self, clock: impl Fn() -> u64 + Send + Sync + 'static) -> Self {
        self.clock = Box::new(clock);
        self
    }

    /// Runs `look` on the session as it is now (after any refresh). The kit's tests only: the session
    /// holds the drive token and the bucket's credentials, and a job that holds the account has no
    /// business reading them (X-DELTA-STRUCT3-14).
    #[cfg(test)]
    pub(crate) fn with_session<T>(&self, look: impl FnOnce(&AzlinSession) -> T) -> T {
        look(&self.lock().session)
    }

    #[track_caller]
    fn lock(&self) -> std::sync::MutexGuard<'_, Current> {
        self.current.lock()
    }

    /// The token server changed the drive's grant (`changed`: a month bought, a voucher's
    /// days): credentials made while the drive took no writes are refreshed before the next
    /// request, so the writes that follow a payment are not refused. Sends nothing; a grant of
    /// another drive changes nothing here.
    pub fn apply(&self, changed: GrantChanged) {
        if changed.drive_id() == self.drive_id() {
            self.grant_changed.store(true, Ordering::SeqCst);
        }
    }

    /// The drive's id at the token server: what its keys, its vault and its grants are kept under. The
    /// resolver's ([`azlin_target`]), kept by the account and read from nowhere else - a caller never names it
    /// for the account (X-DELTA-STRUCT3-16).
    #[must_use]
    pub fn drive_id(&self) -> &str {
        &self.drive_id
    }

    /// The bucket with credentials that work now (refreshed first when they run out, or when
    /// they are read-only ones from before the drive's grant changed), and the access key it
    /// signs with.
    fn bucket(&self) -> Result<(Arc<S3Drive>, String), DriveError> {
        let mut current = self.lock();
        let changed = self.grant_changed.swap(false, Ordering::SeqCst);
        if changed && current.session.grant == Grant::ReadOnly {
            if let Err(e) = self.refresh(&mut current, Why::GrantChanged) {
                // asked again by the next request
                self.grant_changed.store(true, Ordering::SeqCst);
                return Err(e);
            }
        } else if current.session.needs_refresh((self.clock)()) {
            self.refresh(&mut current, Why::Expiring)?;
        } else if self.failover.take_view_refresh() && self.view_refresh_due() {
            // An answer named an endpoint or a map the view lacks: the token server's view
            // first. Without an answer the request goes on with the view the drive has.
            let _ = self.refresh(&mut current, Why::View);
        }
        let drive = self.opened(&mut current)?;
        Ok((drive, current.session.access_key_id.clone()))
    }

    /// Whether an answer may make the drive ask for its view now (not within
    /// [`VIEW_REFRESH_EVERY`] of the last time); takes the turn when it may.
    fn view_refresh_due(&self) -> bool {
        let now = (self.clock)();
        let mut last = self
            .view_refresh
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if last.is_some_and(|at| now < at.saturating_add(VIEW_REFRESH_EVERY)) {
            return false;
        }
        *last = Some(now);
        true
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
        let drive = Arc::new(
            S3Drive::new(config, current.session.credentials(), (self.transports)())?
                .with_router(self.failover.clone()),
        );
        current.drive = Some(drive.clone());
        Ok(drive)
    }

    /// Fresh credentials: under the drive's lock, the session another process refreshed (the
    /// keyring's, when its token is not the one `current` holds - and its credentials are not
    /// running out, nor the ones the bucket refused: `refused`), else the keyring's drive token
    /// spent for new ones, which go into the keyring before the lock is let go. The new session
    /// takes the old one's place and goes to `on_rotated`. When the outcome is unknown (no
    /// answer), the keyring keeps the token and the next refresh sends it again: the token
    /// server answers it while the new one was never used. Called with `current` held: one
    /// refresh at a time in this process too (`on_rotated` must not call back into this drive).
    fn refresh(&self, current: &mut Current, why: Why<'_>) -> Result<(), DriveError> {
        let spend = match self.keyring.spend(&current.session.drive_id) {
            Ok(spend) => spend,
            // The keyring has no session of the drive, or none can be read: the one this
            // process holds is the newest it knows.
            Err(_) => {
                self.keyring
                    .adopt(&current.session)
                    .map_err(|e| DriveError::Io(e.to_string()))?;
                self.keyring
                    .spend(&current.session.drive_id)
                    .map_err(|e| DriveError::Io(e.to_string()))?
            }
        };
        let newer = spend
            .entry()
            .and_then(|text| AzlinSession::from_keyring_secret(text).ok())
            .filter(|stored| {
                stored.drive_id == current.session.drive_id
                    && stored.drive_token != current.session.drive_token
            });
        if let Some(newer) = newer {
            let usable = match why {
                Why::Refused(refused) => newer.access_key_id != refused,
                Why::Expiring => !newer.needs_refresh((self.clock)()),
                Why::GrantChanged => {
                    newer.grant == Grant::ReadWrite && !newer.needs_refresh((self.clock)())
                }
                Why::View => false,
            };
            if usable {
                // Another process refreshed: its session is the newest, nothing is spent.
                drop(spend);
                current.session = newer;
                current.drive = None;
                (self.on_rotated)(&current.session, Ok(()));
                return Ok(());
            }
        }
        let family = family_of(spend.token().as_str()).unwrap_or_default().to_string();
        let transport = (self.transports)();
        let server =
            TokenServer::new(&self.token_url, transport.as_ref()).map_err(|e| drive_error_of(&e))?;
        let rotation = match server.refresh(spend, None).rotated() {
            Ok(rotation) => rotation,
            // A lockdown of this device whose answer was lost revoked the token: asked again
            // with its key it hands over its grant, where signing in again would be all that
            // is left.
            Err(TokenError::SignIn(_))
                if self.lockdown_open(&current.session.drive_id, &family) =>
            {
                let spend = self
                    .keyring
                    .spend(&current.session.drive_id)
                    .map_err(|e| DriveError::Io(e.to_string()))?;
                return self.lock_down(current, spend);
            }
            Err(e) => return Err(drive_error_of(&e)),
        };
        self.learn_nodes(rotation.bundle());
        // In the keyring BEFORE the lock is let go: the token just spent is dead, and the next
        // process to refresh must find this one.
        let kept = rotation.keep();
        (self.on_rotated)(&kept.session, kept.saved);
        current.session = kept.session;
        current.drive = None;
        Ok(())
    }

    /// Locks the drive down from this device ("I was hacked"): every other device, key and
    /// public link of the drive loses access at once; this drive continues with the new grant
    /// the answer carries - stored in the keyring and handed to `on_rotated` like a refresh's,
    /// under the drive's lock (the drive token spent here is dead afterwards). A lockdown whose
    /// answer was lost is asked again with the same idempotency key - within this call, by the
    /// next one, after a restart or by the next refresh - and gets the grant it made.
    ///
    /// # Errors
    ///
    /// No answer, the token server's refusal, a lock or keyring that cannot be used.
    pub fn lockdown(&self) -> Result<(), DriveError> {
        let mut current = self.lock();
        let spend = self
            .keyring
            .spend(&current.session.drive_id)
            .map_err(|e| DriveError::Io(e.to_string()))?;
        self.lock_down(&mut current, spend)
    }

    /// Whether a lockdown sent with a token of `family` is open: its answer was never read.
    fn lockdown_open(&self, drive_id: &str, family: &str) -> bool {
        let open = self.keyring.outbox().pending(IdempotentScope::Lockdown, drive_id);
        open.is_some_and(|(_, sent_by)| !family.is_empty() && sent_by == family)
    }

    /// The lockdown with the token of `spend`, named by the open lockdown call of that token's
    /// family ([`crate::outbox`]): sent until its outcome is final, the grant kept like a
    /// refresh's before the call is closed. Called with `current` held.
    fn lock_down(&self, current: &mut Current, spend: SpendOnce<'_>) -> Result<(), DriveError> {
        let family = family_of(spend.token().as_str()).unwrap_or_default().to_string();
        let call = self
            .keyring
            .outbox()
            .open(IdempotentScope::Lockdown, spend.drive_id(), &family)
            .map_err(|e| drive_error_of(&e))?;
        let transport = (self.transports)();
        let server =
            TokenServer::new(&self.token_url, transport.as_ref()).map_err(|e| drive_error_of(&e))?;
        let outcome = server.until_final(|| {
            server.lockdown(spend.drive_id(), spend.token().as_str(), call.key())
        });
        let answer = match outcome {
            Ok(answer) => answer,
            Err(e) => {
                if outbox::is_final_error(&e) {
                    call.close();
                }
                return Err(drive_error_of(&e));
            }
        };
        // an answer that does not read leaves the call open: the grant is asked for again
        let bundle = DriveBundle::from_value(&answer).map_err(|e| drive_error_of(&e))?;
        // The answer is for the drive the token was spent for, as a refresh's is (`TokenServer::refresh`): one for
        // another drive would be kept as this drive's session and the account would go on under another id than
        // the one it was opened with (X-DELTA-STRUCT4-2). It is no grant of this drive, so the call stays open.
        if bundle.drive_id() != spend.drive_id() {
            return Err(drive_error_of(&TokenError::Protocol(format!(
                "the lockdown's answer is for the drive {}, not {}",
                bundle.drive_id(),
                spend.drive_id()
            ))));
        }
        self.learn_nodes(&bundle);
        let kept = spend.answered(bundle).keep();
        call.close();
        (self.on_rotated)(&kept.session, kept.saved);
        current.session = kept.session;
        current.drive = None;
        Ok(())
    }

    /// The bucket after the bucket refused the credentials of `refused` (an access key): a
    /// refresh, unless another call refreshed meanwhile.
    fn after_refusal(&self, refused: &str) -> Result<Arc<S3Drive>, DriveError> {
        let mut current = self.lock();
        if current.session.access_key_id == refused {
            self.refresh(&mut current, Why::Refused(refused))?;
        }
        self.opened(&mut current)
    }

    /// The token server this drive's credentials go to, as [`open_azlin`] resolved it
    /// ([`azlin_target`]).
    #[must_use]
    pub fn token_url(&self) -> &str {
        &self.token_url
    }

    /// THIS drive - the account's own, [`AzlinAccount::drive_id`] - with the key `keyring` holds (or this
    /// device's member wrap in the bucket opens), for the vault in the drive: its bucket below the
    /// encryption and the drive key. The vault is sealed under the drive key and lies where no listing of
    /// the drive shows it, so it is written below the gate on purpose - by the vault's own code, in this
    /// crate, and nowhere else: the bucket's bytes are handed to nothing but [`crate::UnlockedDrive`]. The
    /// drive's id is not an argument: a caller that named another drive's id for this account's bucket got
    /// that drive's key over this bucket, and the other drive's sealed vault was written into it
    /// (X-DELTA-STRUCT3-16):
    ///
    /// ```compile_fail,E0061
    /// // The id comes from the account: a second argument is refused.
    /// use std::sync::Arc;
    /// use azcloud_kit::AzlinAccount;
    /// use azul_storage::keyring::KeyringStore;
    /// fn another_drives_vault(account: &Arc<AzlinAccount>, keyring: &dyn KeyringStore) {
    ///     let _ = account.unlocked("d_other", keyring);
    /// }
    /// ```
    ///
    /// ```no_run
    /// // The control of the one above: the same call without the id.
    /// use std::sync::Arc;
    /// use azcloud_kit::AzlinAccount;
    /// use azul_storage::keyring::KeyringStore;
    /// fn this_drives_vault(account: &Arc<AzlinAccount>, keyring: &dyn KeyringStore) {
    ///     let _ = account.unlocked(keyring);
    /// }
    /// ```
    ///
    /// `None` when this device has no key of the drive.
    ///
    /// # Errors
    ///
    /// The bucket's or the keyring's refusal, a damaged wrap.
    #[cfg(feature = "encryption")]
    pub fn unlocked(
        self: &Arc<Self>,
        keyring: &dyn azul_storage::keyring::KeyringStore,
    ) -> crate::CloudResult<Option<crate::UnlockedDrive>> {
        crate::UnlockedDrive::open(
            self.drive_id(),
            Arc::new(AzlinDrive::over(Arc::clone(self))),
            keyring,
        )
    }

    /// What the drive's node counts of its bucket (one HeadBucket, SRV17): the stored bytes -
    /// what the quota counts - and the quota of the drive's tier ([`S3Drive::space`]); `None`
    /// what an older node does not say.
    ///
    /// # Errors
    ///
    /// The bucket's refusal or no answer.
    pub fn space(&self) -> Result<BucketSpace, DriveError> {
        self.with_bucket(S3Drive::space)
    }

    /// The drive's node's signed count of the bytes its bucket holds ([`S3Drive::usage_proof`]):
    /// what a move to a smaller tier hands the token server.
    ///
    /// # Errors
    ///
    /// The bucket's refusal or no answer.
    pub fn usage_proof(&self) -> Result<Option<String>, DriveError> {
        self.with_bucket(S3Drive::usage_proof)
    }

    /// Runs the batch `call` on the bucket; the items the bucket refused for their credentials
    /// (they expired half way) run once more with fresh ones. A batch always answers per item,
    /// so even a bucket that cannot be reached answers every item with that.
    fn batch_with_bucket<T, I: Copy>(
        &self,
        items: &[I],
        call: impl Fn(&S3Drive, &[I]) -> Vec<Result<T, DriveError>>,
    ) -> Vec<Result<T, DriveError>> {
        let (bucket, key) = match self.bucket() {
            Ok(found) => found,
            Err(e) => return items.iter().map(|_| Err(e.clone())).collect(),
        };
        let mut results = call(&bucket, items);
        let refused: Vec<usize> = (0..results.len())
            .filter(|&i| matches!(&results[i], Err(e) if refused_credentials(e)))
            .collect();
        if refused.is_empty() {
            return results;
        }
        let Ok(bucket) = self.after_refusal(&key) else {
            return results;
        };
        let again: Vec<I> = refused.iter().map(|&i| items[i]).collect();
        for (&i, result) in refused.iter().zip(call(&bucket, &again)) {
            results[i] = result;
        }
        results
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
            // Expired credentials cannot say so in a HEAD's answer: fresh ones are asked for
            // once in a while, and the call goes again.
            Err(e) if bodyless_refusal(&e) && self.bodyless_refresh_due() => {
                let bucket = self.after_refusal(&key)?;
                call(&bucket)
            }
            other => other,
        }
    }

    /// Whether a refusal without a body may make the drive ask for fresh credentials now (not
    /// within [`BODYLESS_REFRESH_EVERY`] of the last time); takes the turn when it may.
    fn bodyless_refresh_due(&self) -> bool {
        let now = (self.clock)();
        let mut last = self
            .bodyless_refresh
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if last.is_some_and(|at| now < at.saturating_add(BODYLESS_REFRESH_EVERY)) {
            return false;
        }
        *last = Some(now);
        true
    }
}

// ==== The bytes (ARCH29 C6d) ====

/// The bytes of an Azlin drive: its bucket, asked for every request with the credentials the
/// [`AzlinAccount`] holds now (refreshed when they run out, once more when the bucket refuses them). The
/// store [`open_azlin`] hands to the app's gate and to nothing else: crate-private, so no job, no window
/// and no other app can name it, and nobody outside this crate can `put` or `get` on the plain store of an
/// encrypted drive - the price notice that reached a bucket in the clear did so through this handle, below
/// the gate (X-DELTA-AZDRIVE-KEYS-4). The drive's own vault is the one exception, and it is written by the
/// vault's code in this crate ([`AzlinAccount::unlocked`]).
pub(crate) struct AzlinDrive {
    account: Arc<AzlinAccount>,
}

impl AzlinDrive {
    /// The bytes of `account`'s bucket.
    pub(crate) fn over(account: Arc<AzlinAccount>) -> AzlinDrive {
        AzlinDrive { account }
    }
}

impl fmt::Debug for AzlinDrive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AzlinDrive")
            .field("account", &self.account)
            .finish()
    }
}

/// An account and its bytes in one object, for the tests that drive both the way they did before the split:
/// the account's methods are the drive's through `Deref`, and the builders change the account the drive is
/// made over (it is not shared yet).
#[cfg(test)]
impl AzlinDrive {
    pub(crate) fn new(
        entry: &DriveEntry,
        session: AzlinSession,
        token_url: &str,
        keyring: SharedKeyring,
        transports: TransportFactory,
        on_rotated: OnRotated,
    ) -> Result<AzlinDrive, DriveError> {
        let target = target_of(entry, token_url)?;
        Ok(AzlinDrive::over(Arc::new(AzlinAccount::new(
            entry, session, &target, keyring, transports, on_rotated,
        )?)))
    }

    /// The same drive over the account `change` made of its own.
    fn rebuilt(self, change: impl FnOnce(AzlinAccount) -> AzlinAccount) -> AzlinDrive {
        match Arc::try_unwrap(self.account) {
            Ok(account) => AzlinDrive::over(Arc::new(change(account))),
            Err(_) => panic!("a test changed an account that was shared already"),
        }
    }

    pub(crate) fn with_clock(self, clock: impl Fn() -> u64 + Send + Sync + 'static) -> AzlinDrive {
        self.rebuilt(|account| account.with_clock(clock))
    }

    pub(crate) fn with_nodes_file_for(self, honour: bool, path: &Path) -> AzlinDrive {
        self.rebuilt(|account| account.with_nodes_file_for(honour, path))
    }

    pub(crate) fn with_iroh(self, dialer: Arc<dyn IrohDialer>, relay: Option<&str>) -> AzlinDrive {
        self.rebuilt(|account| account.with_iroh(dialer, relay))
    }
}

#[cfg(test)]
impl std::ops::Deref for AzlinDrive {
    type Target = AzlinAccount;

    fn deref(&self) -> &AzlinAccount {
        &self.account
    }
}

/// The drive below the bytes of an Azlin drive: none. The bucket they ask changes with the credentials
/// (the account refreshes them), so a `pass` method of the trait that the bytes did not write has no
/// drive to be handed on to; what it gets is this one, which refuses every call. A method added to
/// [`Drive`] is a request (it goes through [`AzlinDrive`]'s `send`, with the credentials renewed) or a
/// refusal here - never an answer of the trait's default for a drive nobody looked at.
struct NoDriveBelow;

fn nothing_below() -> DriveError {
    DriveError::Unsupported(String::from(
        "an Azlin drive has no drive below it: this call is not one its store was written for",
    ))
}

impl Drive for NoDriveBelow {
    fn list(&self, _: &ListRequest) -> Result<ListPage, DriveError> {
        Err(nothing_below())
    }
    fn get(&self, _: &str) -> Result<Vec<u8>, DriveError> {
        Err(nothing_below())
    }
    fn get_range(&self, _: &str, _: ByteRange) -> Result<Vec<u8>, DriveError> {
        Err(nothing_below())
    }
    fn put(&self, _: &str, _: &[u8]) -> Result<(), DriveError> {
        Err(nothing_below())
    }
    fn delete(&self, _: &str) -> Result<(), DriveError> {
        Err(nothing_below())
    }
    fn head(&self, _: &str) -> Result<ObjectInfo, DriveError> {
        Err(nothing_below())
    }
}

impl azul_storage::DriveDelegate for AzlinDrive {
    fn below(&self) -> &dyn Drive {
        &NoDriveBelow
    }

    /// A request goes to the bucket with the credentials the account holds now; when the bucket refuses
    /// them it asks for fresh ones and sends the request once more. What a bucket refuses a HEAD for is
    /// said by [`AzlinAccount::with_bucket`].
    fn send<T>(
        &self,
        request: impl Fn(&dyn Drive) -> Result<T, DriveError>,
    ) -> Result<T, DriveError> {
        self.account.with_bucket(|bucket| request(bucket))
    }

    /// The bucket's streamed (multipart) upload. A body is read once, so a refusal of the
    /// credentials half way is not tried again: the credentials are refreshed for the next
    /// call and the error is the caller's to retry.
    fn put_from(&self, key: &str, body: &mut dyn Read) -> Result<u64, DriveError> {
        let (bucket, access_key) = self.account.bucket()?;
        match bucket.put_from(key, body) {
            Err(e) if refused_credentials(&e) => {
                self.account.after_refusal(&access_key)?;
                Err(e)
            }
            other => other,
        }
    }
    /// The place of the bucket needs its credentials (a call to the token server): the drive answers as
    /// the very object it is, which two clones of one `Arc` are and nothing else is.
    fn site(&self) -> azul_storage::xfer::Site {
        azul_storage::xfer::Site::other_of(self)
    }
    fn local_path(&self, _key: &str) -> Option<PathBuf> {
        None
    }
    /// The app's resume folder: the bucket this drive opens keeps its uploads' state there.
    fn resume_folder(&self) -> Option<PathBuf> {
        azul_storage::multipart::resume_folder()
    }
    /// The bucket's resumable upload of a file: after a refusal of the credentials it is sent
    /// again with fresh ones (and resumes with the parts already up).
    fn put_file(&self, key: &str, path: &Path, ctl: &dyn Ctl) -> Result<u64, DriveError> {
        self.account.with_bucket(|b| b.put_file(key, path, ctl))
    }
    /// The bucket's resumable upload: a refusal of the credentials half way refreshes them and
    /// the upload goes on from its state file with the parts it lacks.
    fn put_file_checked(
        &self,
        key: &str,
        path: &Path,
        check: &azul_storage::checked::FileCheck,
        ctl: &dyn Ctl,
    ) -> Result<azul_storage::checked::CheckedUpload, DriveError> {
        self.account
            .with_bucket(|b| b.put_file_checked(key, path, check, ctl))
    }
    /// The bucket's streamed conditional upload; like [`AzlinDrive::put_from`], a refusal of the
    /// credentials half way refreshes them for the next call and is the caller's to retry.
    fn put_from_if(
        &self,
        key: &str,
        body: &mut dyn Read,
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        let (bucket, access_key) = self.account.bucket()?;
        match bucket.put_from_if(key, body, condition) {
            Err(e) if refused_credentials(&e) => {
                self.account.after_refusal(&access_key)?;
                Err(e)
            }
            other => other,
        }
    }
    /// Its bucket is an S3 drive, which asks `If-None-Match` / `If-Match` when an upload completes.
    fn conditional_writes(&self) -> bool {
        true
    }
    /// The bucket's resumable upload, completed only when `condition` holds; after a refusal of
    /// the credentials it is sent again with fresh ones (and resumes with the parts already up).
    fn put_file_if(
        &self,
        key: &str,
        path: &Path,
        condition: &Precondition,
        ctl: &dyn Ctl,
    ) -> Result<Option<String>, DriveError> {
        self.account
            .with_bucket(|b| b.put_file_if(key, path, condition, ctl))
    }
    /// The bucket's batch (an index of small files is written in a few requests, not one each).
    fn put_many(&self, items: &[BatchPut<'_>]) -> Vec<Result<Option<String>, DriveError>> {
        self.account
            .batch_with_bucket(items, |b, items| b.put_many(items))
    }
    fn delete_many(&self, items: &[BatchDelete<'_>]) -> Vec<Result<(), DriveError>> {
        self.account
            .batch_with_bucket(items, |b, items| b.delete_many(items))
    }
    fn move_many(&self, items: &[BatchMove<'_>]) -> Vec<Result<(), DriveError>> {
        self.account
            .batch_with_bucket(items, |b, items| b.move_many(items))
    }
    /// [`azul_storage::PARALLEL_FILES`] uploads at once, each through this drive's own
    /// streamed upload (a refusal of the credentials refreshes them for the next one).
    fn put_from_many(
        &self,
        items: &mut [BatchStream<'_>],
    ) -> Vec<Result<Option<String>, DriveError>> {
        ops::put_from_parallel(self, items)
    }
}

azul_storage::delegate_drive!(AzlinDrive);

// ==== The token server's choice, and the opening of both halves (ARCH29 C6d) ====

/// An Azlin drive's id at the token server and the token server its credentials are SENT to (a drive
/// token, a period token, a recovery key, a voucher, a lockdown): what [`azlin_target`] resolves.
///
/// Made by [`azlin_target`] and by no other crate: the struct is `#[non_exhaustive]`, so its fields are read and
/// never written out, and [`open_azlin`] makes the account over one - the token server of an account is the
/// resolver's answer and no other function's choice (X-DELTA-STRUCT3-11):
///
/// ```compile_fail,E0639
/// use azcloud_kit::AzlinTarget;
/// fn made_by_hand() -> AzlinTarget {
///     AzlinTarget {
///         drive_id: String::from("d_1"),
///         token_url: String::from("https://token.example.test"),
///     }
/// }
/// ```
///
/// ```no_run
/// // The control of the one above: the resolver's answer is read.
/// use azcloud_kit::{azlin_target, AzlinTarget};
/// use azul_storage::config::DriveEntry;
/// fn asked(entry: &DriveEntry) -> Option<AzlinTarget> {
///     let target = azlin_target(entry, Some("https://token.example.test"))?;
///     let (_drive, _server) = (target.drive_id.clone(), target.token_url.clone());
///     Some(target)
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AzlinTarget {
    /// The drive's id at the token server.
    pub drive_id: String,
    /// The token server's address.
    pub token_url: String,
}

/// The Azlin drive of `entry` and the token server its credentials go to - THE resolver of that address
/// (X-DELTA-AZDRIVE-KEYS-9, AZ-K20, REVIEW K20): no other function of the apps reads the address a drive
/// entry carries (`account_url` in `drives.json`), and [`open_azlin`] asks this one. A shipped build
/// answers `fallback` (the run's, the live server's), whatever the entry says: `drives.json` is a file any
/// process of the user can write, and a hostile token server can seed the address of a drive it hands out.
/// A test build answers the entry's, else `fallback` ([`azul_appkit::anchors::token_server_choice`]). `None` for
/// every other drive, and for one with no server to name.
#[must_use]
pub fn azlin_target(entry: &DriveEntry, fallback: Option<&str>) -> Option<AzlinTarget> {
    azlin_target_choice(azul_appkit::anchors::layers_honoured(), entry, fallback)
}

/// [`azlin_target`] with the build's choice as an argument (the tests set both).
#[must_use]
pub fn azlin_target_choice(
    honour_entry: bool,
    entry: &DriveEntry,
    fallback: Option<&str>,
) -> Option<AzlinTarget> {
    let DriveLocation::S3 {
        auth:
            DriveAuth::Azlin {
                drive_id,
                account_url,
                ..
            },
        ..
    } = &entry.location
    else {
        return None;
    };
    let url = azul_appkit::anchors::token_server_choice(
        honour_entry,
        account_url,
        fallback.unwrap_or_default(),
    );
    if url.is_empty() {
        return None;
    }
    Some(AzlinTarget {
        drive_id: drive_id.clone(),
        token_url: url.to_string(),
    })
}

/// The target of the Azlin drive `entry` for a run whose token server is `run`: [`azlin_target`]'s answer, or
/// why there is none - the entry is no Azlin drive (a folder, an S3 bucket of another provider: no token
/// server is theirs to name, and the run's was no business of theirs), or nobody names a server to refresh its
/// credentials at.
///
/// # Errors
///
/// [`DriveError::InvalidConfig`], with the drive's name.
pub(crate) fn target_of(entry: &DriveEntry, run: &str) -> Result<AzlinTarget, DriveError> {
    azlin_target(entry, Some(run)).ok_or_else(|| {
        DriveError::InvalidConfig(if entry.azlin().is_none() {
            format!(
                "\"{}\" is not an Azlin drive: no token server refreshes its credentials",
                entry.name
            )
        } else {
            format!(
                "\"{}\" names no token server to refresh its credentials at",
                entry.name
            )
        })
    })
}

/// What [`open_azlin`] needs to open an Azlin drive.
pub struct AzlinOpen<'a> {
    /// The drive's session as the keyring has it.
    pub session: AzlinSession,
    /// The token server this run resolved: a shipped build uses it whatever the entry names
    /// ([`azlin_target`]); empty when nobody named one.
    pub token_url: &'a str,
    /// Where the drive's session lives between processes, and the lock its refreshes hold.
    pub keyring: SharedKeyring,
    /// Makes a transport: one for every S3 drive the credentials open, one for every refresh.
    pub transports: TransportFactory,
    /// Hears every session the drive switches to.
    pub on_rotated: OnRotated,
    /// Where the node list of every refresh is kept between starts, if anywhere.
    pub nodes_file: Option<&'a Path>,
    /// Sends every request over iroh first (the app's dialer, and the relay: `off`, `default` or an
    /// address); `None`: HTTPS only.
    pub iroh: Option<(Arc<dyn IrohDialer>, Option<&'a str>)>,
}

/// Not nameable outside this file: what makes [`EncryptionGate`] sealed.
mod sealed {
    pub trait Sealed {}
}

/// The encryption's gate: a drive that takes a write only when its drive is encrypted. Only azul-storage's
/// `AutoEncrypted` is one (feature `encryption`), and the trait is SEALED: no type of any other crate - and no
/// `Arc<dyn Drive>` - can be made one, so a function that asks for the gate cannot be handed anything else.
pub trait EncryptionGate: Drive + sealed::Sealed + 'static {}

#[cfg(feature = "encryption")]
impl sealed::Sealed for AutoEncrypted {}

#[cfg(feature = "encryption")]
impl EncryptionGate for AutoEncrypted {}

/// What [`open_azlin`] refuses to compile, and the control of it. They are doc tests of this item and only of it: the
/// encryption's own drive (`azul_storage::AutoEncrypted`) exists with the feature `encryption` only, and without it no
/// type is a gate, so there is nothing to show (and a `compile_fail` example that fails for another reason proves nothing).
///
/// ```compile_fail,E0277
/// // A gate that gives the bucket's bytes back as they are: no encryption in front of the drive.
/// use azcloud_kit::{open_azlin, AzlinOpen};
/// use azul_storage::config::DriveEntry;
/// fn plain(entry: &DriveEntry, open: AzlinOpen<'_>) {
///     let _ = open_azlin(entry, open, |_account, bytes| bytes);
/// }
/// ```
///
/// ```compile_fail,E0277
/// // Nor a drive of any other kind: only the encryption's gate is one.
/// use std::sync::Arc;
/// use azcloud_kit::{open_azlin, AzlinOpen};
/// use azul_storage::{config::DriveEntry, meta::MemoryBucket};
/// fn other(entry: &DriveEntry, open: AzlinOpen<'_>) {
///     let _ = open_azlin(entry, open, |_account, _bytes| Arc::new(MemoryBucket::new()));
/// }
/// ```
///
/// ```no_run
/// // The control of the two above: the same call with a gate that returns the encryption's own drive.
/// use std::sync::Arc;
/// use azcloud_kit::{open_azlin, AzlinOpen};
/// use azul_storage::{config::DriveEntry, keyring::KeyringStore, AutoEncrypted};
/// fn encrypted(entry: &DriveEntry, open: AzlinOpen<'_>, keyring: Arc<dyn KeyringStore>) {
///     let _ = open_azlin(entry, open, |_account, bytes| {
///         Arc::new(AutoEncrypted::new(bytes, "d_1", keyring, None))
///     });
/// }
/// ```
#[cfg(all(doctest, feature = "encryption"))]
struct OpenAzlinGate;

/// Opens the Azlin drive `entry` and hands out its two halves: the [`AzlinAccount`] and the drive of its
/// bytes - which comes out of `gate` and never otherwise. `gate` is given the account and the bucket's
/// bytes (the credentials refreshed by themselves, the requests failing over) and returns the app's
/// drive: the layers it puts over them - the retries, the copies kept on this computer - are its own, and
/// the top of them is the encryption's gate, an [`EncryptionGate`] (azul-storage's `AutoEncrypted`): the
/// return type says so, so a gate that hands the bytes back as they are, or any drive that is not the
/// encryption's, does not compile (it was a closure that returned an `Arc<dyn Drive>`, and one built without
/// the encryption's wrap sent every file of the drive to the bucket in the clear and passed every gate and
/// test: X-DELTA-STRUCT3-10). The bytes are called once, by `gate`, and the bucket's plain store is no value
/// this function returns: a caller that holds the account has nothing to write with, and the drive it holds
/// is the gate's (X-DELTA-AZDRIVE-KEYS-4).
///
/// The examples that show what does not compile - a gate that gives the bytes back, a drive of another kind - are the
/// doc tests of `OpenAzlinGate` below (they need the encryption's own drive).
///
/// The token server is [`azlin_target`]'s choice and no other function's (X-DELTA-AZDRIVE-KEYS-9).
///
/// # Errors
///
/// An entry that is no Azlin drive (a folder; a bucket of another provider), or names no token server to refresh
/// its credentials at, and a session of another drive than the entry's: the drives file's `drive_id` and the
/// keyring's session are compared before the account is made, and nothing is made or sent when they differ
/// (X-DELTA-STRUCT4-2).
pub fn open_azlin<G: EncryptionGate>(
    entry: &DriveEntry,
    open: AzlinOpen<'_>,
    gate: impl FnOnce(&Arc<AzlinAccount>, Arc<dyn Drive>) -> Arc<G>,
) -> Result<(Arc<AzlinAccount>, Arc<G>), DriveError> {
    let target = target_of(entry, open.token_url)?;
    let mut account = AzlinAccount::new(
        entry,
        open.session,
        &target,
        open.keyring,
        open.transports,
        open.on_rotated,
    )?;
    if let Some(path) = open.nodes_file {
        account = account.with_nodes_file(path);
    }
    if let Some((dialer, relay)) = open.iroh {
        account = account.with_iroh(dialer, relay);
    }
    let account = Arc::new(account);
    let bytes: Arc<dyn Drive> = Arc::new(AzlinDrive::over(Arc::clone(&account)));
    let gated = gate(&account, bytes);
    Ok((account, gated))
}
