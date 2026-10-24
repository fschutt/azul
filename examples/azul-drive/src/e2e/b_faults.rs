//! The hostile server and network faults of `drive_fault`, beyond the basic set the fault layer
//! keeps itself ([`super::remote::Faults`]): a drive gone read-only, a server that moved, a
//! connection that never answers, a certificate the device does not trust, a body cut short,
//! the requests of chosen files failing, another remote answering, and the server's caps on the
//! number of files and the size of one object.
//!
//! They are kept per remote (its folder, which no other run shares) and device name (`""`: every
//! device of the remote), the way the basic faults are: a device's own entry replaces the one for
//! every device. The fault layer ([`super::remote::FaultDrive`]) asks them on every request.
//!
//! The requests of a file (`fail_files`, globs over the files' keys) are found two ways. On a
//! plain remote a file's content is the blob its BLAKE3 names, so a request for a blob fails when
//! a matching file (in the drive's index or in a device's folder) has that BLAKE3. An encrypted
//! remote names its objects at random; there [`FileFaults`] sits above the encryption, where the
//! drive is still asked for files by their keys, and fails those calls the same way.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use azcloud_kit::sync::remote::{blobs_prefix, index_key, RemoteIndex};
use azul_storage::{
    checked::{CheckedUpload, FileCheck},
    AzlinError, BatchDelete, BatchMove, BatchPut, BatchStream, ByteRange, Drive, DriveError,
    ListPage, ListRequest, ObjectInfo, Precondition, ServiceError,
};

use super::{
    bool_arg,
    files::{hash, user_files},
    remote::{glob, Remote},
    str_arg, Args, Harness,
};

/// The fields of a `drive_fault` step this module takes.
pub(crate) const FIELDS: &[&str] = &[
    "read_only",
    "moved",
    "hang",
    "hang_ms",
    "tls_untrusted",
    "short_body_after_bytes",
    "report_ok",
    "fail_files",
    "answer_from",
    "max_files",
    "max_object_bytes",
];

/// What the stand-in server and the network do to one device's requests.
#[derive(Clone, Default)]
pub(crate) struct ServerFaults {
    /// Writes of new bytes are refused with this read-only code; reads and the owner's deletes
    /// still work (rule V7: an unpaid drive goes read-only, nothing is deleted).
    pub read_only: Option<AzlinError>,
    /// Every request answers 301 to this address.
    pub moved: Option<String>,
    /// Every request is taken and never answered: the transport's timeout ends it after this.
    pub hang: Option<Duration>,
    /// Every connection meets a certificate the device does not trust (an intercepting proxy).
    pub tls_untrusted: bool,
    /// A GET's body ends after this many bytes: as an error, or (`report_ok`) as a short
    /// success the way a client that ignores Content-Length takes it.
    pub short_body_after: Option<u64>,
    pub report_ok: bool,
    /// The requests for these files' objects answer 500.
    pub fail_files: Vec<String>,
    /// This remote answers every request instead (a wrong or hijacked address).
    pub answer_from: Option<Arc<Remote>>,
    /// A commit of the index that would name more files than this is refused (plain remotes).
    pub max_files: Option<u64>,
    /// A content object over this many bytes is refused with 413.
    pub max_object_bytes: Option<u64>,
    /// The folders of the remote's devices when the faults were set: `fail_files` finds a
    /// file that is not on the drive yet by its BLAKE3 there.
    pub roots: Vec<PathBuf>,
}

impl ServerFaults {
    /// The faults with the fields of a `drive_fault` step laid over them.
    fn with(mut self, args: &Args, h: &Harness, remote: &Remote) -> Result<ServerFaults, String> {
        let num = |k: &str| args.get(k).and_then(serde_json::Value::as_u64);
        if let Some(v) = args.get("read_only") {
            self.read_only = read_only_code(v)?;
        }
        if let Some(v) = args.get("moved") {
            self.moved = match v {
                serde_json::Value::Bool(true) => {
                    Some(format!("https://moved.{}.e2e.invalid", remote.name))
                }
                serde_json::Value::String(to) => Some(to.clone()),
                _ => None,
            };
        }
        if let Some(v) = args.get("hang") {
            // The transport gives up on an answer that never comes after its overall timeout.
            self.hang = v
                .as_bool()
                .unwrap_or(false)
                .then_some(azul_storage::transport::DEFAULT_TIMEOUT);
        }
        if let Some(ms) = num("hang_ms") {
            self.hang = (ms > 0).then(|| Duration::from_millis(ms));
        }
        if let Some(v) = args.get("tls_untrusted").and_then(serde_json::Value::as_bool) {
            self.tls_untrusted = v;
        }
        if args.contains_key("short_body_after_bytes") {
            self.short_body_after = num("short_body_after_bytes");
        }
        if let Some(v) = args.get("report_ok").and_then(serde_json::Value::as_bool) {
            self.report_ok = v;
        }
        if let Some(globs) = args.get("fail_files").and_then(serde_json::Value::as_array) {
            self.fail_files = globs
                .iter()
                .filter_map(|g| g.as_str().map(str::to_string))
                .collect();
        }
        if let Some(v) = args.get("answer_from") {
            self.answer_from = match v.as_str() {
                Some(name) if name == remote.name => {
                    return Err(String::from("a remote cannot answer for itself"))
                }
                Some(name) => Some(
                    h.remotes
                        .get(name)
                        .cloned()
                        .ok_or_else(|| format!("no remote '{name}' to answer from"))?,
                ),
                None => None,
            };
        }
        if args.contains_key("max_files") {
            if remote.encrypted && num("max_files").is_some() {
                return Err(String::from(
                    "max_files counts the files a plain index commit names; an encrypted \
                     remote's commits are sealed: make the remote with \"encrypted\": false",
                ));
            }
            self.max_files = num("max_files");
        }
        if args.contains_key("max_object_bytes") {
            self.max_object_bytes = num("max_object_bytes");
        }
        self.roots = h
            .devices
            .iter()
            .filter(|d| d.remote == remote.name)
            .map(|d| d.root.clone())
            .collect();
        Ok(self)
    }
}

/// The read-only code `v` names: `true` (unpaid), or why: `payment-failed` / `unpaid`,
/// `lockdown`, `operator`, `banned`.
fn read_only_code(v: &serde_json::Value) -> Result<Option<AzlinError>, String> {
    Ok(match v {
        serde_json::Value::Bool(false) | serde_json::Value::Null => None,
        serde_json::Value::Bool(true) => Some(AzlinError::ReadOnlyUnpaid),
        serde_json::Value::String(why) => Some(match why.as_str() {
            "payment-failed" | "unpaid" => AzlinError::ReadOnlyUnpaid,
            "lockdown" => AzlinError::ReadOnlyLockdown,
            "operator" => AzlinError::ReadOnlyOperator,
            "banned" => AzlinError::DriveBanned,
            other => {
                return Err(format!(
                    "read_only '{other}' is none of payment-failed, unpaid, lockdown, operator, \
                     banned"
                ))
            }
        }),
        other => return Err(format!("read_only {other} is no bool or reason")),
    })
}

/// Every remote's faults, by (remote folder, device name).
type Table = BTreeMap<(PathBuf, String), ServerFaults>;

static SERVER: Mutex<Table> = Mutex::new(BTreeMap::new());
/// Wakes a hanging request when its fault is cleared.
static WAKE: Condvar = Condvar::new();

fn table() -> MutexGuard<'static, Table> {
    SERVER.lock().unwrap_or_else(PoisonError::into_inner)
}

fn entry_of<'t>(all: &'t Table, remote: &Path, device: &str) -> Option<&'t ServerFaults> {
    all.get(&(remote.to_path_buf(), device.to_string()))
        .or_else(|| all.get(&(remote.to_path_buf(), String::new())))
}

/// The faults one device of `remote` meets now.
pub(crate) fn faults_of(remote: &Remote, device: &str) -> Option<ServerFaults> {
    entry_of(&table(), &remote.dir, device).cloned()
}

/// A `drive_fault` step's fields of this module, for `remote` (and the step's `device`, else
/// every device). `clear` starts from nothing; `clear` alone with neither device nor field ends
/// every fault of the remote.
pub(crate) fn set(h: &Harness, remote: &Arc<Remote>, args: &Args) -> Result<(), String> {
    let device = match str_arg(args, "device") {
        Some(_) => h.device(args)?.name.clone(),
        None => String::new(),
    };
    let clear = bool_arg(args, "clear");
    let touched = FIELDS.iter().any(|f| args.contains_key(*f));
    if !clear && !touched {
        return Ok(());
    }
    let key = (remote.dir.clone(), device.clone());
    if clear && !touched && device.is_empty() {
        clear_remote(remote);
        return Ok(());
    }
    let before = if clear {
        ServerFaults::default()
    } else {
        table().get(&key).cloned().unwrap_or_default()
    };
    let next = before.with(args, h, remote)?;
    table().insert(key, next);
    WAKE.notify_all();
    Ok(())
}

/// Every fault of `remote` gone; a request that hangs gives up at once.
pub(crate) fn clear_remote(remote: &Remote) {
    table().retain(|(dir, _), _| dir != &remote.dir);
    WAKE.notify_all();
}

fn service(status: u16, code: &str, message: String, azlin: Option<AzlinError>) -> DriveError {
    DriveError::Service(ServiceError {
        status,
        code: code.to_string(),
        message,
        azlin_error: azlin,
        ..ServiceError::default()
    })
}

/// The answer every refused request of a failing file gets.
fn failed_file() -> DriveError {
    service(500, "InternalError", String::from("the service failed"), None)
}

/// Before a request of `device` for `key` reaches the remote: the network's and the server's
/// refusals that do not depend on the request's body. Waits while the connection hangs.
pub(crate) fn gate(remote: &Remote, device: &str, key: &str) -> Result<(), DriveError> {
    let Some(f) = faults_of(remote, device) else {
        return Ok(());
    };
    if f.tls_untrusted {
        return Err(DriveError::Transport(String::from(
            "invalid peer certificate: UnknownIssuer (the connection is intercepted: its \
             certificate is not one this device trusts)",
        )));
    }
    if let Some(to) = &f.moved {
        return Err(service(
            301,
            "PermanentRedirect",
            format!("the drive moved permanently to {to}"),
            None,
        ));
    }
    if let Some(wait) = f.hang {
        hang(remote, device, wait);
        return Err(DriveError::Transport(format!(
            "the request timed out after {} s without an answer",
            wait.as_secs().max(1)
        )));
    }
    if !remote.encrypted && !f.fail_files.is_empty() && is_failing_blob(remote, &f, key) {
        return Err(failed_file());
    }
    Ok(())
}

/// Blocks until `wait` passed or the hang of `device` was cleared, whichever comes first.
fn hang(remote: &Remote, device: &str, wait: Duration) {
    let deadline = Instant::now() + wait;
    let mut all = table();
    loop {
        let still = entry_of(&all, &remote.dir, device).is_some_and(|f| f.hang.is_some());
        let now = Instant::now();
        if !still || now >= deadline {
            return;
        }
        all = WAKE
            .wait_timeout(all, deadline - now)
            .unwrap_or_else(PoisonError::into_inner)
            .0;
    }
}

/// Whether `key` is an object of a file's content (a plain remote's blob, an encrypted
/// remote's data object, a plaintext file), not the drive's bookkeeping (its index, its
/// encrypted metadata, its keys): a read-only drive refuses new content only, so the owner's
/// deletes still commit.
fn is_content(key: &str) -> bool {
    // `data/` is where an encrypted drive keeps its AZL1 objects (azul-storage's DATA_PREFIX).
    if key.starts_with("data/") || key.contains(".azlin/blobs/") {
        return true;
    }
    !(key.starts_with(".azlin/") || key.contains("/.azlin/"))
}

/// Before a write of `bytes` under `key` lands: the read-only drive, the object cap, the
/// file-count cap.
pub(crate) fn check_put(remote: &Remote, device: &str, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
    let Some(f) = faults_of(remote, device) else {
        return Ok(());
    };
    let content = is_content(key);
    if let (true, Some(code)) = (content, f.read_only) {
        return Err(service(
            403,
            "AccessDenied",
            String::from("the drive is read-only: it takes no new files"),
            Some(code),
        ));
    }
    if let (true, Some(max)) = (content, f.max_object_bytes) {
        if bytes.len() as u64 > max {
            return Err(service(
                413,
                "EntityTooLarge",
                format!("an object of {} bytes is over the {max} this server takes", bytes.len()),
                Some(AzlinError::ObjectTooLarge),
            ));
        }
    }
    if let Some(max) = f.max_files {
        if key == index_key(&remote.prefix) {
            let named = serde_json::from_slice::<serde_json::Value>(bytes)
                .ok()
                .and_then(|index| index.get("files").and_then(|f| f.as_object()).map(|f| f.len()))
                .unwrap_or(0) as u64;
            if named > max {
                return Err(service(
                    507,
                    "QuotaExceeded",
                    format!("the drive holds at most {max} files; this commit names {named}"),
                    Some(AzlinError::QuotaExceeded),
                ));
            }
        }
    }
    Ok(())
}

/// A GET's body as the device receives it: whole, cut short as a success (`report_ok`), or an
/// error after the bytes that arrived (the count comes back with it).
pub(crate) fn short_body(remote: &Remote, device: &str, bytes: Vec<u8>) -> Result<Vec<u8>, (u64, DriveError)> {
    let Some(n) = faults_of(remote, device).and_then(|f| f.short_body_after.map(|n| (n, f.report_ok)))
    else {
        return Ok(bytes);
    };
    let (n, report_ok) = n;
    let cut = usize::try_from(n).unwrap_or(usize::MAX);
    if bytes.len() <= cut {
        return Ok(bytes);
    }
    if report_ok {
        let mut bytes = bytes;
        bytes.truncate(cut);
        return Ok(bytes);
    }
    Err((
        n,
        DriveError::Transport(format!(
            "the connection closed after {n} of the body's {} bytes",
            bytes.len()
        )),
    ))
}

/// The remote that answers `device`'s requests to `remote` instead, if one does.
pub(crate) fn answered_by(remote: &Remote, device: &str) -> Option<Arc<Remote>> {
    faults_of(remote, device)
        .and_then(|f| f.answer_from)
        .filter(|other| other.dir != remote.dir)
}

/// Whether `key` is the blob of a file `fail_files` names (plain remotes: a blob is named by
/// its content's BLAKE3).
fn is_failing_blob(remote: &Remote, f: &ServerFaults, key: &str) -> bool {
    let Some(rest) = key.strip_prefix(&blobs_prefix(&remote.prefix)) else {
        return false;
    };
    let blob = rest.rsplit('/').next().unwrap_or(rest);
    failing_hashes(remote, f).contains(blob)
}

/// The BLAKE3 of every file `fail_files` names: as the drive's index has it, and as the
/// devices' folders hold it (a file not sent yet).
fn failing_hashes(remote: &Remote, f: &ServerFaults) -> BTreeSet<String> {
    let names = |key: &str| f.fail_files.iter().any(|g| glob(g, key));
    let mut out = BTreeSet::new();
    if let Some(index) = remote
        .read(&index_key(&remote.prefix))
        .and_then(|bytes| RemoteIndex::parse(&bytes).ok())
    {
        for (key, file) in &index.files {
            if names(key) {
                out.insert(file.hash.clone());
            }
        }
    }
    for root in &f.roots {
        for (rel, path) in user_files(root) {
            if names(&rel) {
                if let Ok(bytes) = std::fs::read(&path) {
                    out.insert(hash(&bytes));
                }
            }
        }
    }
    out
}

/// A device's encrypted drive with the `fail_files` of its remote applied above the
/// encryption, where every call still names a file by its key (under the remote's prefix).
/// Everything else goes to the drive below as it is.
pub(crate) struct FileFaults {
    pub inner: Arc<dyn Drive>,
    pub remote: Arc<Remote>,
    pub device: String,
}

impl FileFaults {
    fn fails(&self, key: &str) -> bool {
        let Some(f) = faults_of(&self.remote, &self.device) else {
            return false;
        };
        let file = key.strip_prefix(self.remote.prefix.as_str()).unwrap_or(key);
        f.fail_files.iter().any(|g| glob(g, file))
    }

    /// The answer of a refused call, counted as refused.
    fn refused(&self) -> DriveError {
        self.remote.note_refused(&self.device);
        failed_file()
    }

    /// `Err` when a call for `keys` must fail.
    fn check(&self, keys: &[&str]) -> Result<(), DriveError> {
        if keys.iter().any(|k| self.fails(k)) {
            return Err(self.refused());
        }
        Ok(())
    }
}

/// Wraps an encrypted device's drive in [`FileFaults`] (a plain one keeps its drive: the fault
/// layer finds a file's blob itself).
pub(crate) fn wrap_files(slot: Option<&mut crate::Slot>, remote: &Arc<Remote>, device: &str) {
    let Some(slot) = slot else {
        return;
    };
    if !remote.encrypted {
        return;
    }
    if let Some(inner) = slot.drive.take() {
        slot.drive = Some(Arc::new(FileFaults {
            inner,
            remote: remote.clone(),
            device: device.to_string(),
        }));
    }
}

impl Drive for FileFaults {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.check(&[key])?;
        self.inner.get(key)
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.check(&[key])?;
        self.inner.get_range(key, range)
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.check(&[key])?;
        self.inner.put(key, bytes)
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.check(&[key])?;
        self.inner.delete(key)
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.check(&[key])?;
        self.inner.head(key)
    }

    fn put_from(&self, key: &str, body: &mut dyn Read) -> Result<u64, DriveError> {
        self.check(&[key])?;
        self.inner.put_from(key, body)
    }

    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.check(&[key])?;
        self.inner.put_if(key, bytes, condition)
    }

    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.check(&[from, to])?;
        self.inner.copy(from, to)
    }

    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.inner.create_folder(prefix)
    }

    fn rename(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.check(&[from, to])?;
        self.inner.rename(from, to)
    }

    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.inner.delete_folder(prefix)
    }

    fn local_path(&self, key: &str) -> Option<PathBuf> {
        self.inner.local_path(key)
    }

    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        self.check(&[key])?;
        self.inner.metadata(key)
    }

    fn put_file(
        &self,
        key: &str,
        path: &Path,
        progress: &(dyn Fn(u64) + Sync),
    ) -> Result<u64, DriveError> {
        self.check(&[key])?;
        self.inner.put_file(key, path, progress)
    }

    fn put_file_checked(
        &self,
        key: &str,
        path: &Path,
        check: &FileCheck,
        progress: &(dyn Fn(u64) + Sync),
    ) -> Result<CheckedUpload, DriveError> {
        self.check(&[key])?;
        self.inner.put_file_checked(key, path, check, progress)
    }

    fn put_from_if(
        &self,
        key: &str,
        body: &mut dyn Read,
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.check(&[key])?;
        self.inner.put_from_if(key, body, condition)
    }

    fn put_many(&self, items: &[BatchPut<'_>]) -> Vec<Result<Option<String>, DriveError>> {
        let failing: Vec<bool> = items.iter().map(|i| self.fails(i.key)).collect();
        if !failing.contains(&true) {
            return self.inner.put_many(items);
        }
        let rest: Vec<BatchPut<'_>> = items
            .iter()
            .zip(&failing)
            .filter(|(_, fails)| !**fails)
            .map(|(i, _)| BatchPut {
                key: i.key,
                bytes: i.bytes,
                condition: i.condition,
            })
            .collect();
        let sent = self.inner.put_many(&rest);
        merged(&failing, sent, || self.refused())
    }

    fn delete_many(&self, items: &[BatchDelete<'_>]) -> Vec<Result<(), DriveError>> {
        let failing: Vec<bool> = items.iter().map(|i| self.fails(i.key)).collect();
        if !failing.contains(&true) {
            return self.inner.delete_many(items);
        }
        let rest: Vec<BatchDelete<'_>> = items
            .iter()
            .zip(&failing)
            .filter(|(_, fails)| !**fails)
            .map(|(i, _)| BatchDelete {
                key: i.key,
                version: i.version,
            })
            .collect();
        let sent = self.inner.delete_many(&rest);
        merged(&failing, sent, || self.refused())
    }

    fn move_many(&self, items: &[BatchMove<'_>]) -> Vec<Result<(), DriveError>> {
        let failing: Vec<bool> = items
            .iter()
            .map(|i| self.fails(i.from) || self.fails(i.to))
            .collect();
        if !failing.contains(&true) {
            return self.inner.move_many(items);
        }
        let rest: Vec<BatchMove<'_>> = items
            .iter()
            .zip(&failing)
            .filter(|(_, fails)| !**fails)
            .map(|(i, _)| BatchMove {
                from: i.from,
                to: i.to,
                version: i.version,
            })
            .collect();
        let sent = self.inner.move_many(&rest);
        merged(&failing, sent, || self.refused())
    }

    fn put_from_many(
        &self,
        items: &mut [BatchStream<'_>],
    ) -> Vec<Result<Option<String>, DriveError>> {
        let failing: Vec<bool> = items.iter().map(|i| self.fails(i.key)).collect();
        if !failing.contains(&true) {
            return self.inner.put_from_many(items);
        }
        let mut rest: Vec<BatchStream<'_>> = items
            .iter_mut()
            .zip(&failing)
            .filter(|(_, fails)| !**fails)
            .map(|(i, _)| BatchStream {
                key: i.key,
                body: &mut *i.body,
                condition: i.condition,
            })
            .collect();
        let sent = self.inner.put_from_many(&mut rest);
        merged(&failing, sent, || self.refused())
    }
}

/// The answers of a batch whose `failing` items were held back (each gets `refused()`) and
/// whose others were sent (`sent`, in their order).
fn merged<T>(
    failing: &[bool],
    sent: Vec<Result<T, DriveError>>,
    refused: impl Fn() -> DriveError,
) -> Vec<Result<T, DriveError>> {
    let mut sent = sent.into_iter();
    failing
        .iter()
        .map(|fails| {
            if *fails {
                Err(refused())
            } else {
                sent.next().unwrap_or_else(|| {
                    Err(DriveError::Transport(String::from("the batch lost an answer")))
                })
            }
        })
        .collect()
}

/// Arms `faults` for `device` of `remote` as a `drive_fault` step would (the unit tests).
#[cfg(test)]
pub(crate) fn arm(remote: &Remote, device: &str, faults: ServerFaults) {
    table().insert((remote.dir.clone(), device.to_string()), faults);
    WAKE.notify_all();
}
