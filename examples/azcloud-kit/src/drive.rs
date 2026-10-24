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
//!
//! Every request goes through the drive's [`Failover`] (iroh to the nodes when the app plugs in
//! a dialer - [`AzlinDrive::with_iroh`] -, then the block endpoint, a node's hint, the node
//! list, the nodes' addresses; retries by the class of the answer): each refresh hands it
//! the node list it answered with, and with a nodes file ([`AzlinDrive::with_nodes_file`]) the
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
    config::DriveEntry,
    time::now_unix,
    ui_thread::NetworkLock,
    ops, AzlinError, BatchDelete, BatchMove, BatchStream, BucketSpace, ByteRange, Drive, DriveError,
    ListPage, ListRequest, ObjectInfo, Precondition, S3Drive, ServiceError, Transport,
};

use crate::{
    bundle::DriveBundle,
    failover::{read_nodes, write_nodes, Failover, Node, TrustedEndpoint},
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

/// An Azlin drive that refreshes its credentials by itself. `Debug` shows no secret.
pub struct AzlinDrive {
    entry: DriveEntry,
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
    /// credentials are refreshed before the next request ([`AzlinDrive::apply`]).
    grant_changed: AtomicBool,
    /// When (by `clock`) an answer naming an endpoint or a map the drive's view lacks last made
    /// it ask the token server for its view.
    view_refresh: Mutex<Option<u64>>,
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

impl fmt::Debug for AzlinDrive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AzlinDrive")
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
        let Some(config) = entry.s3_config() else {
            return Err(DriveError::InvalidConfig(format!(
                "\"{}\" is not a bucket",
                entry.name
            )));
        };
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
        })
    }

    /// Keeps the node list of every refresh in `path` (not secret: the app's state or cache
    /// folder), and starts with the list kept there.
    #[must_use]
    pub fn with_nodes_file(mut self, path: &Path) -> Self {
        let kept = read_nodes(path);
        if !kept.is_empty() {
            self.failover.set_nodes(kept);
        }
        self.nodes_file = Some(path.to_path_buf());
        self
    }

    /// The node list the requests fail over to (of the last refresh, or the nodes file).
    #[must_use]
    pub fn nodes(&self) -> Vec<Node> {
        self.failover.nodes()
    }

    /// The failover every request of the drive goes through.
    #[must_use]
    pub fn failover(&self) -> &Arc<Failover> {
        &self.failover
    }

    /// Sends every request over iroh first: to each ready node the node list names with an iroh
    /// id, dialed through `dialer` (the app's: azul's iroh endpoint) at its iroh sockets and
    /// relayed through `relay` (`off`, `default` or an address; `None`: the dialer's own); a node
    /// that fails rests for five minutes and the request goes on over HTTPS
    /// ([`crate::transport::IrohLane`]).
    #[must_use]
    pub fn with_iroh(self, dialer: Arc<dyn IrohDialer>, relay: Option<&str>) -> Self {
        self.failover
            .set_lane(Some(Arc::new(IrohLane::new(dialer, relay))));
        self
    }

    /// The iroh lane, when the drive has one: what it did last.
    #[must_use]
    pub fn lane(&self) -> Option<Arc<IrohLane>> {
        self.failover.lane()
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
        self.failover.set_map_epoch(bundle.map_epoch);
    }

    /// Reads the time from `clock` (seconds since 1970) instead of the system's.
    #[must_use]
    pub fn with_clock(mut self, clock: impl Fn() -> u64 + Send + Sync + 'static) -> Self {
        self.clock = Box::new(clock);
        self
    }

    /// Runs `look` on the session as it is now (after any refresh).
    pub fn with_session<T>(&self, look: impl FnOnce(&AzlinSession) -> T) -> T {
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
        let ours = self
            .entry
            .azlin()
            .map_or(self.entry.id.as_str(), |(drive_id, _)| drive_id);
        if changed.drive_id() == ours {
            self.grant_changed.store(true, Ordering::SeqCst);
        }
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
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.with_bucket(|b| b.put_if(key, bytes, condition))
    }
    /// The bucket's streamed (multipart) upload. A body is read once, so a refusal of the
    /// credentials half way is not tried again: the credentials are refreshed for the next
    /// call and the error is the caller's to retry.
    fn put_from(&self, key: &str, body: &mut dyn Read) -> Result<u64, DriveError> {
        let (bucket, access_key) = self.bucket()?;
        match bucket.put_from(key, body) {
            Err(e) if refused_credentials(&e) => {
                self.after_refusal(&access_key)?;
                Err(e)
            }
            other => other,
        }
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.with_bucket(|b| b.delete(key))
    }
    fn delete_many(&self, items: &[BatchDelete<'_>]) -> Vec<Result<(), DriveError>> {
        self.batch_with_bucket(items, |b, items| b.delete_many(items))
    }
    fn move_many(&self, items: &[BatchMove<'_>]) -> Vec<Result<(), DriveError>> {
        self.batch_with_bucket(items, |b, items| b.move_many(items))
    }
    /// [`azul_storage::PARALLEL_FILES`] uploads at once, each through this drive's own
    /// streamed upload (a refusal of the credentials refreshes them for the next one).
    fn put_from_many(
        &self,
        items: &mut [BatchStream<'_>],
    ) -> Vec<Result<Option<String>, DriveError>> {
        ops::put_from_parallel(self, items)
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
    /// The bucket's resumable upload of a file: after a refusal of the credentials it is sent
    /// again with fresh ones (and resumes with the parts already up).
    fn put_file(
        &self,
        key: &str,
        path: &Path,
        progress: &(dyn Fn(u64) + Sync),
    ) -> Result<u64, DriveError> {
        self.with_bucket(|b| b.put_file(key, path, progress))
    }
    /// The bucket's resumable upload: a refusal of the credentials half way refreshes them and
    /// the upload goes on from its state file with the parts it lacks.
    fn put_file_checked(
        &self,
        key: &str,
        path: &Path,
        check: &azul_storage::checked::FileCheck,
        progress: &(dyn Fn(u64) + Sync),
    ) -> Result<azul_storage::checked::CheckedUpload, DriveError> {
        self.with_bucket(|b| b.put_file_checked(key, path, check, progress))
    }
    /// The bucket's streamed conditional upload; like [`AzlinDrive::put_from`], a refusal of the
    /// credentials half way refreshes them for the next call and is the caller's to retry.
    fn put_from_if(
        &self,
        key: &str,
        body: &mut dyn Read,
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        let (bucket, access_key) = self.bucket()?;
        match bucket.put_from_if(key, body, condition) {
            Err(e) if refused_credentials(&e) => {
                self.after_refusal(&access_key)?;
                Err(e)
            }
            other => other,
        }
    }
}
