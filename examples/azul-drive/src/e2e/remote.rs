//! The E2E commands' "cloud": a folder in the sandbox that answers like a bucket (listings,
//! entity tags, compare-and-swap writes), and the fault layer every device reaches it through.
//!
//! Objects are files in `<remote>/objects/`, one per key, named by [`file_name`] so that keys a
//! case-insensitive or normalizing disk would merge stay apart. A write goes through a temporary
//! file in `<remote>/tmp/` and a rename, so a reader never sees half an object. One lock per
//! remote makes a conditional write atomic against every device of the process.
//!
//! [`FaultDrive`] is one device's connection: before a request reaches the folder it is
//! counted, and a fault armed for the device (or for every device) may refuse it the way a
//! real service or network would: a connection error, a 503, a 429 with `Retry-After`, a
//! quota error, a 401.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard, PoisonError,
    },
    time::{Duration, Instant},
};

use azul_storage::{
    keyring::{KeyringError, KeyringStore},
    AzlinError, ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition,
    ServiceError, DEFAULT_PAGE_SIZE,
};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a fault layer does to a device's requests (sync-chaos's `fault` step).
#[derive(Clone, Debug, Default)]
pub(crate) struct Faults {
    /// Every request fails with a connection error.
    pub partition: bool,
    /// This share of requests answers 503, chosen by `seed` (repeatable).
    pub error_rate: f64,
    pub seed: u64,
    /// Every n-th request answers 429 with `Retry-After: retry_after` seconds.
    pub rate_limit_every: Option<u64>,
    pub retry_after: u64,
    /// Every request after the n-th from when it was set fails with a connection error.
    pub cut_after_ops: Option<u64>,
    /// A write that would make the remote hold more than this many bytes answers "quota".
    pub quota_bytes: Option<u64>,
    /// Requests for keys matching one of these globs answer 500.
    pub fail_keys: Vec<String>,
    /// Requests of these kinds (`get`, `put`, `put_if`, `head`, `list`, `delete`) answer 500.
    pub refuse_ops: Vec<String>,
    /// Every request answers 401.
    pub auth_failed: bool,
    /// Every request waits this long first.
    pub latency_ms: u64,
}

impl Faults {
    /// The faults with `fields` (a `drive_fault` step's object) laid over them.
    pub(crate) fn with(mut self, fields: &serde_json::Map<String, serde_json::Value>) -> Faults {
        let num = |k: &str| fields.get(k).and_then(serde_json::Value::as_u64);
        let strings = |k: &str| {
            fields.get(k).and_then(serde_json::Value::as_array).map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
        };
        if let Some(v) = fields.get("partition") {
            // `true`, or a list of device names (the layer of a named device is cut).
            self.partition = v.as_bool().unwrap_or_else(|| v.is_array());
        }
        if let Some(v) = fields.get("error_rate").and_then(serde_json::Value::as_f64) {
            self.error_rate = v.clamp(0.0, 1.0);
        }
        if let Some(v) = num("seed") {
            self.seed = v;
        }
        if fields.contains_key("rate_limit_every") {
            self.rate_limit_every = num("rate_limit_every").filter(|n| *n > 0);
        }
        if let Some(v) = num("retry_after") {
            self.retry_after = v;
        }
        if fields.contains_key("cut_after_ops") {
            self.cut_after_ops = num("cut_after_ops");
        }
        if fields.contains_key("quota_bytes") {
            self.quota_bytes = num("quota_bytes");
        }
        if let Some(v) = strings("fail_keys") {
            self.fail_keys = v;
        }
        if let Some(v) = strings("refuse_ops") {
            self.refuse_ops = v;
        }
        if let Some(v) = fields.get("auth_failed").and_then(serde_json::Value::as_bool) {
            self.auth_failed = v;
        }
        if let Some(v) = num("latency_ms") {
            self.latency_ms = v;
        }
        self
    }
}

/// What one device sent and was answered.
#[derive(Clone, Debug, Default)]
pub(crate) struct Counters {
    /// Requests by kind.
    pub ops: BTreeMap<String, u64>,
    pub bytes_up: u64,
    pub bytes_down: u64,
    /// Requests a fault refused.
    pub refused: u64,
    /// Successful writes by key.
    pub puts: BTreeMap<String, u64>,
    /// Requests answered 429.
    pub rate_limited: u64,
    /// Requests sent before a 429's `Retry-After` ran out.
    pub early: u64,
    /// The cut's count of requests since `cut_after_ops` was set.
    cut_count: u64,
    /// The rate limit's count of requests.
    limit_count: u64,
    /// The error rate's draws.
    draws: u64,
    /// Until when a 429 asked this device to wait.
    blocked_until: Option<Instant>,
}

impl Counters {
    pub(crate) fn to_json(&self) -> serde_json::Value {
        // Every kind of request is named, 0 when none was made (a check reads `ops.delete`).
        let mut ops = self.ops.clone();
        for kind in ["get", "put", "put_if", "head", "list", "delete"] {
            ops.entry(kind.to_string()).or_insert(0);
        }
        serde_json::json!({
            "ops": ops,
            "bytes_up": self.bytes_up,
            "bytes_down": self.bytes_down,
            "refused": self.refused,
            "puts": self.puts,
            "rate_limited": self.rate_limited,
            "early": self.early,
        })
    }

    pub(crate) fn add(&mut self, other: &Counters) {
        for (k, n) in &other.ops {
            *self.ops.entry(k.clone()).or_default() += n;
        }
        for (k, n) in &other.puts {
            *self.puts.entry(k.clone()).or_default() += n;
        }
        self.bytes_up += other.bytes_up;
        self.bytes_down += other.bytes_down;
        self.refused += other.refused;
        self.rate_limited += other.rate_limited;
        self.early += other.early;
    }
}

/// A keyring in memory: the drive keys of an encrypted remote, which every device of the remote
/// shares (they are "already joined"). Never the system's keyring.
#[derive(Default)]
pub(crate) struct MemKeyring {
    entries: Mutex<BTreeMap<String, String>>,
}

impl KeyringStore for MemKeyring {
    fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
        Ok(lock(&self.entries).get(key).cloned())
    }

    fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
        lock(&self.entries).insert(key.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), KeyringError> {
        lock(&self.entries).remove(key);
        Ok(())
    }
}

/// A remote: the folder, its devices' faults and counters, its keys.
pub(crate) struct Remote {
    pub name: String,
    /// `<run>/remote/<name>`.
    pub dir: PathBuf,
    /// Made encrypted (the drive key in [`Self::keyring`]).
    pub encrypted: bool,
    /// The folder of the drive the devices sync with (`""`: the whole drive).
    pub prefix: String,
    /// The drive id the encryption keys its keyring entries and index by: the same for every
    /// device, as for one drive opened on several computers.
    pub drive_name: String,
    pub keyring: Arc<MemKeyring>,
    /// Serializes writes: a conditional write is atomic against every device.
    writes: Mutex<()>,
    /// Held from a write's quota check to its end while a quota is armed: the check and the
    /// write are one step, as on a server that counts what it holds, so two writes at once
    /// cannot both take the last room.
    room: Mutex<()>,
    /// Faults by device name; `""` holds the ones for every device.
    faults: Mutex<BTreeMap<String, Faults>>,
    counters: Mutex<BTreeMap<String, Counters>>,
    /// Every version of the sync index written, oldest first (for `rollback`).
    pub index_history: Mutex<Vec<Vec<u8>>>,
    /// Named copies of the index (`drive_snapshot`).
    pub snapshots: Mutex<BTreeMap<String, Vec<u8>>>,
    /// Names temporary files apart.
    temp_serial: AtomicU64,
}

impl Remote {
    pub(crate) fn create(
        name: &str,
        dir: PathBuf,
        encrypted: bool,
        prefix: &str,
    ) -> std::io::Result<Remote> {
        fs::create_dir_all(dir.join("objects"))?;
        fs::create_dir_all(dir.join("tmp"))?;
        Ok(Remote {
            name: name.to_string(),
            dir,
            encrypted,
            prefix: prefix.to_string(),
            drive_name: format!("e2e-remote-{name}"),
            keyring: Arc::new(MemKeyring::default()),
            writes: Mutex::new(()),
            room: Mutex::new(()),
            faults: Mutex::new(BTreeMap::new()),
            counters: Mutex::new(BTreeMap::new()),
            index_history: Mutex::new(Vec::new()),
            snapshots: Mutex::new(BTreeMap::new()),
            temp_serial: AtomicU64::new(0),
        })
    }

    fn objects(&self) -> PathBuf {
        self.dir.join("objects")
    }

    fn path_of(&self, key: &str) -> PathBuf {
        self.objects().join(file_name(key))
    }

    /// Lays `fields` over the faults of `device` (`""`: every device); `clear` starts empty.
    pub(crate) fn set_faults(
        &self,
        device: &str,
        clear: bool,
        fields: &serde_json::Map<String, serde_json::Value>,
    ) {
        let mut all = lock(&self.faults);
        let before = if clear {
            Faults::default()
        } else {
            all.get(device).cloned().unwrap_or_default()
        };
        all.insert(device.to_string(), before.with(fields));
        if fields.contains_key("cut_after_ops") {
            // The cut counts requests from now on.
            for (name, c) in lock(&self.counters).iter_mut() {
                if device.is_empty() || name == device {
                    c.cut_count = 0;
                }
            }
        }
    }

    /// Every fault gone.
    pub(crate) fn clear_faults(&self) {
        lock(&self.faults).clear();
    }

    /// A request of `device` a fault refused above the fault layer (a failing file's call to
    /// an encrypted drive).
    pub(crate) fn note_refused(&self, device: &str) {
        lock(&self.counters).entry(device.to_string()).or_default().refused += 1;
    }

    /// The counters of `device`, or (`None`) of every device together.
    pub(crate) fn counters(&self, device: Option<&str>) -> Counters {
        let all = lock(&self.counters);
        match device {
            Some(d) => all.get(d).cloned().unwrap_or_default(),
            None => {
                let mut sum = Counters::default();
                for c in all.values() {
                    sum.add(c);
                }
                sum
            }
        }
    }

    // ==== The folder, past every fault ====

    /// Every key, in order.
    pub(crate) fn keys(&self) -> Vec<String> {
        let mut out = BTreeSet::new();
        if let Ok(entries) = fs::read_dir(self.objects()) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                // A long key's object is listed through the file beside it that holds the key;
                // any other name is its key, a ".key" file of the drive's own included.
                if name.starts_with(LONG_PREFIX) {
                    if let Some(stem) = name.strip_suffix(LONG_KEY_SUFFIX) {
                        if let Ok(key) = fs::read_to_string(entry.path()) {
                            if self.objects().join(stem).exists() {
                                out.insert(key);
                            }
                        }
                    }
                } else if let Some(key) = key_of(&name) {
                    out.insert(key);
                }
            }
        }
        out.into_iter().collect()
    }

    /// The object's bytes, past every fault.
    pub(crate) fn read(&self, key: &str) -> Option<Vec<u8>> {
        fs::read(self.path_of(key)).ok()
    }

    /// Writes `bytes` under `key`, past every fault (a tamper, a restore).
    pub(crate) fn write(&self, key: &str, bytes: &[u8]) -> std::io::Result<()> {
        let _w = lock(&self.writes);
        self.write_locked(key, bytes)
    }

    /// Removes `key`, past every fault.
    pub(crate) fn remove(&self, key: &str) -> std::io::Result<()> {
        let _w = lock(&self.writes);
        self.remove_locked(key)
    }

    /// Every object's bytes (the `remote_never_has` search).
    pub(crate) fn all_bytes(&self) -> Vec<(String, Vec<u8>)> {
        self.keys()
            .into_iter()
            .filter_map(|k| self.read(&k).map(|b| (k, b)))
            .collect()
    }

    /// Bytes the remote holds.
    fn stored_bytes(&self) -> u64 {
        self.keys()
            .iter()
            .filter_map(|k| fs::metadata(self.path_of(k)).ok())
            .map(|m| m.len())
            .sum()
    }

    fn write_locked(&self, key: &str, bytes: &[u8]) -> std::io::Result<()> {
        let serial = self.temp_serial.fetch_add(1, Ordering::SeqCst);
        let tmp = self.dir.join("tmp").join(format!("{serial}"));
        fs::write(&tmp, bytes)?;
        let name = file_name(key);
        if name.starts_with(LONG_PREFIX) {
            fs::write(self.objects().join(format!("{name}{LONG_KEY_SUFFIX}")), key)?;
        }
        fs::rename(&tmp, self.objects().join(&name))?;
        if is_index_key(key) {
            lock(&self.index_history).push(bytes.to_vec());
        }
        Ok(())
    }

    fn remove_locked(&self, key: &str) -> std::io::Result<()> {
        let name = file_name(key);
        match fs::remove_file(self.objects().join(&name)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        if name.starts_with(LONG_PREFIX) {
            let _ = fs::remove_file(self.objects().join(format!("{name}{LONG_KEY_SUFFIX}")));
        }
        Ok(())
    }

    fn etag_of(bytes: &[u8]) -> String {
        azcloud_kit::sync::local::hash_bytes(bytes)[..32].to_string()
    }

    fn info(&self, key: &str) -> Option<ObjectInfo> {
        let path = self.path_of(key);
        let bytes = fs::read(&path).ok()?;
        let modified = fs::metadata(&path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        Some(ObjectInfo {
            key: key.to_string(),
            size: bytes.len() as u64,
            modified,
            etag: Some(Self::etag_of(&bytes)),
        })
    }
}

/// Whether `key` is a sync index (the plain drive's commit point).
pub(crate) fn is_index_key(key: &str) -> bool {
    key == ".azlin/index.json" || key.ends_with("/.azlin/index.json")
}

/// A file name for long keys (the key itself is in the sidecar `<name>.key`).
const LONG_PREFIX: &str = "~long-";
const LONG_KEY_SUFFIX: &str = ".key";

/// The file that holds `key`: every byte but ASCII lowercase letters, digits and `-_.,=+()[]{} `
/// is `%XX` and an uppercase letter is `^x`, so two keys a case-insensitive or Unicode-normalizing
/// disk would take for one stay two files. A name too long for a disk is a hash with a sidecar.
pub(crate) fn file_name(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for b in key.bytes() {
        match b {
            b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b',' | b'=' | b'+' | b'(' | b')' | b'['
            | b']' | b'{' | b'}' | b' ' => out.push(b as char),
            // A leading dot would hide the file from some tools; inside a name it is plain.
            b'.' if !out.is_empty() => out.push('.'),
            b'A'..=b'Z' => {
                out.push('^');
                out.push((b + 32) as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    if out.len() > 200 || out.starts_with(LONG_PREFIX) {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut h);
        return format!("{LONG_PREFIX}{:016x}{:x}", h.finish(), key.len());
    }
    out
}

/// The key a [`file_name`] stands for (`None`: not one of its names).
fn key_of(name: &str) -> Option<String> {
    let bytes = name.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = name.get(i + 1..i + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'^' => {
                out.push(bytes.get(i + 1)?.to_ascii_uppercase());
                i += 2;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// `*` and `?` over a key (`*` crosses `/`, as sync-chaos's fnmatch does).
pub(crate) fn glob(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// One device's connection to a remote.
pub(crate) struct FaultDrive {
    pub remote: Arc<Remote>,
    pub device: String,
}

impl FaultDrive {
    fn service(status: u16, code: &str, message: &str) -> DriveError {
        DriveError::Service(ServiceError {
            status,
            code: code.to_string(),
            message: message.to_string(),
            ..ServiceError::default()
        })
    }

    /// The faults armed for this device (or for every device).
    fn faults(&self) -> Faults {
        let all = lock(&self.remote.faults);
        all.get(&self.device)
            .or_else(|| all.get(""))
            .cloned()
            .unwrap_or_default()
    }

    /// The remote's room, held for one write while a quota is armed (see [`Remote::room`]).
    fn room(&self) -> Option<std::sync::MutexGuard<'_, ()>> {
        self.faults().quota_bytes.map(|_| lock(&self.remote.room))
    }

    /// Counts the request and lets it through, or answers it as an armed fault says.
    fn basic_gate(&self, op: &str, key: &str, put_bytes: Option<u64>) -> Result<(), DriveError> {
        let faults = self.faults();
        if faults.latency_ms > 0 {
            std::thread::sleep(Duration::from_millis(faults.latency_ms));
        }
        let mut all = lock(&self.remote.counters);
        let c = all.entry(self.device.clone()).or_default();
        *c.ops.entry(op.to_string()).or_default() += 1;
        let now = Instant::now();
        if c.blocked_until.is_some_and(|until| now < until) {
            c.early += 1;
        }
        let refused = |c: &mut Counters, e: DriveError| {
            c.refused += 1;
            Err(e)
        };
        if faults.partition {
            return refused(c, DriveError::Transport(String::from("partitioned from the drive")));
        }
        if let Some(n) = faults.cut_after_ops {
            c.cut_count += 1;
            if c.cut_count > n {
                return refused(c, DriveError::Transport(String::from("the connection was cut")));
            }
        }
        if faults.auth_failed {
            return refused(c, Self::service(401, "InvalidAccessKeyId", "the keys were revoked"));
        }
        if faults.refuse_ops.iter().any(|o| o == op) || faults.fail_keys.iter().any(|g| glob(g, key))
        {
            return refused(c, Self::service(500, "InternalError", "the service failed"));
        }
        if let Some(every) = faults.rate_limit_every {
            c.limit_count += 1;
            if c.limit_count % every == 0 {
                c.rate_limited += 1;
                c.blocked_until = Some(now + Duration::from_secs(faults.retry_after));
                c.refused += 1;
                return Err(DriveError::Service(ServiceError {
                    status: 429,
                    code: String::from("SlowDown"),
                    message: String::from("too many requests"),
                    retry_after: Some(faults.retry_after),
                    ..ServiceError::default()
                }));
            }
        }
        if faults.error_rate > 0.0 {
            c.draws += 1;
            if draw(faults.seed, c.draws) < faults.error_rate {
                return refused(c, Self::service(503, "ServiceUnavailable", "try again later"));
            }
        }
        if let (Some(quota), Some(bytes)) = (faults.quota_bytes, put_bytes) {
            drop(all);
            let held = self.remote.stored_bytes();
            let replaced = fs::metadata(self.remote.path_of(key)).map_or(0, |m| m.len());
            if held.saturating_sub(replaced) + bytes > quota {
                let mut all = lock(&self.remote.counters);
                let c = all.entry(self.device.clone()).or_default();
                c.refused += 1;
                return Err(DriveError::Service(ServiceError {
                    status: 507,
                    code: String::from("QuotaExceeded"),
                    message: String::from("the drive is full"),
                    azlin_error: Some(AzlinError::QuotaExceeded),
                    ..ServiceError::default()
                }));
            }
        }
        Ok(())
    }

    /// [`Self::basic_gate`], then the hostile server's and network's faults (`b_faults`): a
    /// hang waits there, once the basic gate let go of its locks.
    fn gate(&self, op: &str, key: &str, put_bytes: Option<u64>) -> Result<(), DriveError> {
        self.basic_gate(op, key, put_bytes)?;
        super::b_faults::gate(&self.remote, &self.device, key).map_err(|e| self.refuse(e))
    }

    /// `e`, counted as a refused request.
    fn refuse(&self, e: DriveError) -> DriveError {
        self.counted(|c| c.refused += 1);
        e
    }

    /// The connection of this device to the remote that answers it instead (`answer_from`).
    fn redirected(&self) -> Option<FaultDrive> {
        super::b_faults::answered_by(&self.remote, &self.device).map(|other| FaultDrive {
            remote: other,
            device: format!("{}@{}", self.device, self.remote.name),
        })
    }

    /// A GET's body as the device receives it (`short_body_after_bytes`).
    fn received(&self, bytes: Vec<u8>) -> Result<Vec<u8>, DriveError> {
        super::b_faults::short_body(&self.remote, &self.device, bytes).map_err(|(sent, e)| {
            self.counted(|c| {
                c.bytes_down += sent;
                c.refused += 1;
            });
            e
        })
    }

    fn counted(&self, f: impl FnOnce(&mut Counters)) {
        f(lock(&self.remote.counters).entry(self.device.clone()).or_default());
    }

    fn not_found(key: &str) -> DriveError {
        DriveError::NotFound {
            key: key.to_string(),
        }
    }
}

/// A repeatable draw in [0, 1) for request `n` under `seed` (splitmix64).
fn draw(seed: u64, n: u64) -> f64 {
    let mut z = seed.wrapping_add(n.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

impl Drive for FaultDrive {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        if let Some(other) = self.redirected() {
            return other.list(request);
        }
        self.gate("list", &request.prefix, None)?;
        let max = if request.max_keys == 0 {
            DEFAULT_PAGE_SIZE
        } else {
            request.max_keys
        } as usize;
        let after = request.continuation.clone().unwrap_or_default();
        let mut page = ListPage {
            folders: Vec::new(),
            objects: Vec::new(),
            next: None,
        };
        let mut emitted = 0usize;
        let mut last = String::new();
        for key in self.remote.keys() {
            if !key.starts_with(&request.prefix) || (!after.is_empty() && key.as_str() <= after.as_str())
            {
                continue;
            }
            // A folder a previous page ended with is not listed again.
            if !after.is_empty() && after.ends_with('/') && key.starts_with(&after) {
                continue;
            }
            let rest = &key[request.prefix.len()..];
            let folder = request
                .delimiter
                .as_deref()
                .and_then(|d| rest.find(d).map(|i| format!("{}{}", request.prefix, &rest[..i + d.len()])));
            // A key under the folder just listed is that folder.
            if folder.is_some() && page.folders.last() == folder.as_ref() {
                continue;
            }
            if emitted == max {
                page.next = Some(last.clone());
                break;
            }
            match folder {
                Some(folder) => {
                    page.folders.push(folder.clone());
                    emitted += 1;
                    last = folder;
                }
                None => {
                    if let Some(info) = self.remote.info(&key) {
                        page.objects.push(info);
                        emitted += 1;
                        last = key;
                    }
                }
            }
        }
        Ok(page)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        if let Some(other) = self.redirected() {
            return other.get(key);
        }
        self.gate("get", key, None)?;
        let bytes = self.remote.read(key).ok_or_else(|| Self::not_found(key))?;
        let bytes = self.received(bytes)?;
        self.counted(|c| c.bytes_down += bytes.len() as u64);
        Ok(bytes)
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        if let Some(other) = self.redirected() {
            return other.get_range(key, range);
        }
        self.gate("get", key, None)?;
        let bytes = self.remote.read(key).ok_or_else(|| Self::not_found(key))?;
        let start = usize::try_from(range.start).unwrap_or(usize::MAX);
        if start > bytes.len() || (start == bytes.len() && !bytes.is_empty()) {
            return Err(DriveError::InvalidRange {
                key: key.to_string(),
            });
        }
        let end = range
            .end
            .map_or(bytes.len(), |e| (usize::try_from(e).unwrap_or(usize::MAX)).saturating_add(1))
            .min(bytes.len());
        let part = self.received(bytes[start..end.max(start)].to_vec())?;
        self.counted(|c| c.bytes_down += part.len() as u64);
        Ok(part)
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        if let Some(other) = self.redirected() {
            return other.put(key, bytes);
        }
        let _room = self.room();
        self.gate("put", key, Some(bytes.len() as u64))?;
        super::b_faults::check_put(&self.remote, &self.device, key, bytes).map_err(|e| self.refuse(e))?;
        self.remote.write(key, bytes)?;
        self.counted(|c| {
            c.bytes_up += bytes.len() as u64;
            *c.puts.entry(key.to_string()).or_default() += 1;
        });
        Ok(())
    }

    fn put_from(&self, key: &str, body: &mut dyn Read) -> Result<u64, DriveError> {
        let mut bytes = Vec::new();
        body.read_to_end(&mut bytes)?;
        self.put(key, &bytes)?;
        Ok(bytes.len() as u64)
    }

    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        if let Some(other) = self.redirected() {
            return other.put_if(key, bytes, condition);
        }
        let _room = self.room();
        self.gate("put_if", key, Some(bytes.len() as u64))?;
        super::b_faults::check_put(&self.remote, &self.device, key, bytes).map_err(|e| self.refuse(e))?;
        {
            let _w = lock(&self.remote.writes);
            let now = self.remote.read(key).map(|b| Remote::etag_of(&b));
            let holds = match condition {
                Precondition::Absent => now.is_none(),
                Precondition::Matches(want) => now.as_deref() == Some(want.trim_matches('"')),
            };
            if !holds {
                return Err(DriveError::Conflict {
                    key: key.to_string(),
                });
            }
            self.remote.write_locked(key, bytes)?;
        }
        self.counted(|c| {
            c.bytes_up += bytes.len() as u64;
            *c.puts.entry(key.to_string()).or_default() += 1;
        });
        Ok(Some(Remote::etag_of(bytes)))
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        if let Some(other) = self.redirected() {
            return other.delete(key);
        }
        self.gate("delete", key, None)?;
        self.remote.remove(key)?;
        Ok(())
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        if let Some(other) = self.redirected() {
            return other.head(key);
        }
        self.gate("head", key, None)?;
        self.remote.info(key).ok_or_else(|| Self::not_found(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_keep_keys_apart_that_a_case_insensitive_or_normalizing_disk_would_merge() {
        let keys = ["A.txt", "a.txt", "caf\u{e9}", "cafe\u{301}", "dir/x", ".hidden", "50%"];
        let names: BTreeSet<String> = keys.iter().map(|k| file_name(k).to_lowercase()).collect();
        assert_eq!(names.len(), keys.len(), "every key has a file of its own: {names:?}");
        for k in keys {
            assert_eq!(key_of(&file_name(k)).as_deref(), Some(k), "{k} reads back");
        }
    }

    #[test]
    fn glob_star_crosses_folders_and_question_mark_takes_one_character() {
        assert!(glob("f1 (conflict*", "f1 (conflict a 2026-10-24).txt"));
        assert!(glob("*.txt", "dir/a.txt"));
        assert!(glob("a?c", "abc"));
        assert!(!glob("a?c", "ac"));
        assert!(!glob("*.md", "a.txt"));
    }

    #[test]
    fn the_error_rate_is_repeatable_for_a_seed() {
        let a: Vec<bool> = (1..50).map(|n| draw(7, n) < 0.3).collect();
        let b: Vec<bool> = (1..50).map(|n| draw(7, n) < 0.3).collect();
        assert_eq!(a, b);
        assert!(a.iter().any(|x| *x) && a.iter().any(|x| !*x));
    }
}
