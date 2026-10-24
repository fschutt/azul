//! A drive in a local folder, in the layout of the sync test harness (azul-apps
//! `local/sync-chaos`): the side the `azsync` command line syncs against, so every scenario of
//! the harness - its server faults, its tampering as another device, a share participant or a
//! malicious server - runs against this engine as it runs against the harness's reference.
//!
//! The layout: `index.json` (`generation`, `files`, `deleted`; `features` and
//! `restore_epoch` when a writer set them), `index-history/<generation>.json`,
//! `blobs/<2 hex>/<sha256 of the stored bytes>`, `versions.jsonl` (every entry replaced or
//! deleted), `requests.jsonl` (every request and its answer), `faults.json` (the faults the
//! harness switches on: errors, rate limits, a partition, a quota, ...), `state/` (counters
//! per device), `lock` (flock: one writer at a time).
//!
//! The engine names contents by BLAKE3; the harness by SHA-256. An entry this engine writes
//! carries both (`hash` is the SHA-256 the harness checks, `blake3` the engine's); an entry
//! another party wrote is hashed from its blob as it is read. Everything in the folder is
//! untrusted: an entry that cannot be read, a link, an unknown codec or a name that can be no
//! key is never taken for a delete - it is shown to the engine as a version it cannot fetch
//! (so the file here stays as it is) and listed, and a commit leaves it on the drive as it
//! was: a commit writes only the entries the pass changed.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use super::{
    local,
    problems::{Code, Problem},
    remote::{check_key, RemoteIndex},
    retry::{random_jitter, with_retries, Gate, RetryPolicy},
    LocalRoot, RunHooks, SyncOptions, SyncRemote, SyncReport, Uploads,
};
use crate::error::{CloudError, CloudResult};

/// Bytes that are no UTF-8 in the harness's files (a hostile key) travel through the engine as
/// these private-use characters, one per byte, and are written back as the bytes they were.
const RAW_BASE: u32 = 0x10_FF00;

/// The harness's drive folder, as one device sees it.
pub struct FolderRemote {
    dir: PathBuf,
    /// This device's name: the requests' `device`, the counters', the writer of its entries.
    device: String,
    /// The pass's clock (seconds), the harness's windows and logs use it too.
    now: Box<dyn Fn() -> f64 + Sync>,
    /// After a 429 no request of this pass goes out before the drive's wait.
    pub gate: Gate,
    policy: RetryPolicy,
    /// BLAKE3 -> the blob's SHA-256 name, for every content this pass read or wrote.
    blobs: Mutex<BTreeMap<String, String>>,
    /// The stand-in hashes of entries this engine cannot take, and why each cannot be.
    unreadable: Mutex<BTreeMap<String, (String, Problem)>>,
    /// The harness entries as read, by key: what a commit compares with and keeps.
    entries: Mutex<BTreeMap<String, Value>>,
    /// Problems found while reading, given to the pass's report.
    found: Mutex<Vec<Problem>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The SHA-256 of `bytes` in hex.
fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The harness's text: UTF-8, with every byte that is not UTF-8 as a private-use character.
fn decode_raw(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    let mut rest = bytes;
    loop {
        match std::str::from_utf8(rest) {
            Ok(text) => {
                out.push_str(text);
                return out;
            }
            Err(e) => {
                let (good, bad) = rest.split_at(e.valid_up_to());
                out.push_str(std::str::from_utf8(good).unwrap_or_default());
                let n = e.error_len().unwrap_or(bad.len()).max(1);
                for b in &bad[..n] {
                    out.push(char::from_u32(RAW_BASE + u32::from(*b)).unwrap_or('\u{FFFD}'));
                }
                rest = &bad[n..];
            }
        }
    }
}

/// [`decode_raw`] backwards.
fn encode_raw(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        let o = u32::from(c);
        if (RAW_BASE..=RAW_BASE + 0xFF).contains(&o) {
            out.push((o - RAW_BASE) as u8);
        } else {
            let mut buf = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        }
    }
    out
}

fn has_raw(text: &str) -> bool {
    text.chars()
        .any(|c| (RAW_BASE..=RAW_BASE + 0xFF).contains(&u32::from(c)))
}

/// A request the drive refused, as the kit's errors say it.
fn service(status: u16, message: &str, retry_after: Option<f64>) -> CloudError {
    CloudError::Drive(azul_storage::DriveError::Service(azul_storage::ServiceError {
        status,
        code: String::from(match status {
            429 => "SlowDown",
            401 => "Unauthorized",
            507 => "QuotaExceeded",
            _ => "ServiceUnavailable",
        }),
        message: message.to_string(),
        retry_after: retry_after.map(|s| s.ceil().max(1.0) as u64),
        ..azul_storage::ServiceError::default()
    }))
}

fn offline(message: &str) -> CloudError {
    CloudError::Drive(azul_storage::DriveError::Transport(message.to_string()))
}

fn io(e: impl std::fmt::Display) -> CloudError {
    CloudError::Io(e.to_string())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> CloudResult<()> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    })()
    .map_err(|e| {
        let _ = fs::remove_file(&tmp);
        io(format!("{}: {e}", path.display()))
    })
}

/// The folder's `lock`, held while it lives (flock, as the harness takes it).
struct Locked(#[allow(dead_code)] fs::File);

impl Locked {
    fn take(dir: &Path) -> CloudResult<Locked> {
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("lock"))
            .map_err(io)?;
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            // SAFETY: a descriptor of a file this function opened.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
                return Err(io(std::io::Error::last_os_error()));
            }
        }
        Ok(Locked(file))
    }
}

/// The index as the harness keeps it.
struct HarnessIndex {
    generation: u64,
    files: Map<String, Value>,
    deleted: Map<String, Value>,
    features: Vec<String>,
    restore_epoch: u64,
}

impl FolderRemote {
    /// The drive in `dir` (made when missing), as the device `device` sees it, on the clock
    /// `now`.
    ///
    /// # Errors
    ///
    /// When the folder cannot be made.
    pub fn new(dir: &Path, device: &str, now: Box<dyn Fn() -> f64 + Sync>) -> CloudResult<Self> {
        for sub in ["blobs", "index-history", "state"] {
            fs::create_dir_all(dir.join(sub)).map_err(io)?;
        }
        Ok(FolderRemote {
            dir: dir.to_path_buf(),
            device: device.to_string(),
            now,
            gate: Gate::new(),
            policy: RetryPolicy::REQUEST,
            blobs: Mutex::new(BTreeMap::new()),
            unreadable: Mutex::new(BTreeMap::new()),
            entries: Mutex::new(BTreeMap::new()),
            found: Mutex::new(Vec::new()),
        })
    }

    fn p(&self, rel: &str) -> PathBuf {
        self.dir.join(rel)
    }

    fn faults(&self) -> Map<String, Value> {
        fs::read(self.p("faults.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default()
    }

    fn log(&self, op: &str, key: &str, status: u16, early: bool) {
        let mut line = json!({"t": (self.now)(), "device": self.device, "op": op, "key": key, "status": status});
        if early {
            line["early"] = Value::Bool(true);
        }
        if let Ok(mut f) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.p("requests.jsonl"))
        {
            let _ = writeln!(f, "{line}");
        }
    }

    /// A counter in `state/` that survives the process (a pass after a crash continues the
    /// same fault sequence).
    fn counter(&self, name: &str) -> u64 {
        let _held = Locked::take(&self.dir);
        let path = self.p(&format!("state/{name}"));
        let n = fs::read_to_string(&path)
            .ok()
            .and_then(|t| t.trim().parse::<u64>().ok())
            .unwrap_or(0)
            + 1;
        let _ = fs::write(&path, n.to_string());
        n
    }

    /// The faults applied to one request, as the harness's server answers them.
    fn gate_once(&self, op: &str, key: &str) -> CloudResult<()> {
        let f = self.faults();
        if let Some(ms) = f.get("latency_ms").and_then(Value::as_f64) {
            std::thread::sleep(std::time::Duration::from_secs_f64(ms / 1000.0));
        }
        if f.get("auth_failed").and_then(Value::as_bool) == Some(true) {
            self.log(op, key, 401, false);
            return Err(service(401, "the credentials are no longer valid", None));
        }
        let refused = f
            .get("refuse_ops")
            .and_then(Value::as_array)
            .is_some_and(|ops| ops.iter().any(|o| o.as_str() == Some(op)));
        if refused {
            self.log(op, key, 503, false);
            return Err(service(503, &format!("the server refuses {op} requests for now"), None));
        }
        let cut_off = match f.get("partition") {
            Some(Value::Bool(true)) => true,
            Some(Value::Array(names)) => names.iter().any(|n| n.as_str() == Some(&self.device)),
            _ => false,
        };
        if cut_off {
            self.log(op, key, 0, false);
            return Err(offline("the server cannot be reached"));
        }
        if let Some(cut) = f.get("cut_after_ops").and_then(Value::as_u64) {
            if self.counter("ops-all") > cut {
                self.log(op, key, 0, false);
                return Err(offline("the connection dropped"));
            }
        }
        if let Some(every) = f.get("rate_limit_every").and_then(Value::as_u64).filter(|n| *n > 0) {
            let window = self.p(&format!("state/rl-{}.json", self.device));
            let until = fs::read(&window)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .and_then(|v| v.get("until").and_then(Value::as_f64))
                .unwrap_or(0.0);
            let wait = f.get("retry_after").and_then(Value::as_f64).unwrap_or(1.0);
            let now = (self.now)();
            if now < until {
                self.log(op, key, 429, true);
                return Err(service(429, "slow down", Some(until - now)));
            }
            if self.counter(&format!("rl-{}", self.device)) % every == 0 {
                let _ = write_atomic(&window, json!({"until": now + wait}).to_string().as_bytes());
                self.log(op, key, 429, false);
                return Err(service(429, "slow down", Some(wait)));
            }
        }
        if let Some(rate) = f.get("error_rate").and_then(Value::as_f64) {
            let n = self.counter(&format!("err-{}", self.device));
            let seed = f.get("seed").map(Value::to_string).unwrap_or_default();
            let roll = blake3::hash(format!("{seed}:{}:{n}", self.device).as_bytes());
            let x = f64::from(u32::from_le_bytes(roll.as_bytes()[..4].try_into().unwrap_or([0; 4])))
                / f64::from(u32::MAX);
            if x < rate {
                self.log(op, key, 503, false);
                return Err(service(503, "the server failed", None));
            }
        }
        let failing = f
            .get("fail_keys")
            .and_then(Value::as_array)
            .is_some_and(|keys| keys.iter().any(|k| k.as_str() == Some(key)));
        if failing && !key.is_empty() {
            self.log(op, key, 503, false);
            return Err(service(503, "the server keeps failing for this file", None));
        }
        self.log(op, key, 200, false);
        Ok(())
    }

    /// `request` after the faults, with the pass's retries around both.
    fn call<T>(&self, op: &str, key: &str, mut request: impl FnMut() -> CloudResult<T>) -> CloudResult<T> {
        with_retries(&self.policy, &self.gate, &random_jitter, || {
            self.gate_once(op, key)?;
            request()
        })
    }

    fn blob_path(&self, sha: &str) -> PathBuf {
        self.dir
            .join("blobs")
            .join(sha.get(..2).unwrap_or("00"))
            .join(sha)
    }

    fn read_harness(&self) -> CloudResult<Option<HarnessIndex>> {
        let raw = match fs::read(self.p("index.json")) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io(e)),
        };
        let text = decode_raw(&raw);
        let unreadable =
            |why: String| CloudError::failed(format!("the drive's sync index cannot be read: {why}"));
        let doc: Value = serde_json::from_str(&text).map_err(|e| unreadable(e.to_string()))?;
        let generation = doc
            .get("generation")
            .and_then(Value::as_u64)
            .ok_or_else(|| unreadable(String::from("it has no generation")))?;
        let files = doc
            .get("files")
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| unreadable(String::from("it has no files")))?;
        let deleted = doc
            .get("deleted")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let features = doc
            .get("features")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|f| f.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        let restore_epoch = doc.get("restore_epoch").and_then(Value::as_u64).unwrap_or(0);
        Ok(Some(HarnessIndex {
            generation,
            files,
            deleted,
            features,
            restore_epoch,
        }))
    }

    /// The BLAKE3 of the blob `sha` (read and checked against its name), cached.
    fn blake3_of(&self, sha: &str) -> Option<String> {
        if let Some((b3, _)) = lock(&self.blobs).iter().find(|(_, s)| s.as_str() == sha) {
            return Some(b3.clone());
        }
        let bytes = fs::read(self.blob_path(sha)).ok()?;
        if sha256_hex(&bytes) != sha {
            return None;
        }
        let b3 = local::hash_bytes(&bytes);
        lock(&self.blobs).insert(b3.clone(), sha.to_string());
        Some(b3)
    }

    /// A stand-in content for an entry this engine cannot take: the file here stays as it
    /// is, the fetch says why, the problem is listed.
    fn stand_in(&self, key: &str, entry: &Value, problem: Problem) -> String {
        let hash = blake3::hash(format!("unreadable entry\n{key}\n{entry}").as_bytes())
            .to_hex()
            .to_string();
        lock(&self.found).push(problem.clone());
        lock(&self.unreadable).insert(hash.clone(), (key.to_string(), problem));
        hash
    }

    /// One harness entry as the engine's index entry (`None`: the key is no key at all).
    fn translate(&self, key: &str, entry: &Value) -> Option<Value> {
        if has_raw(key) {
            lock(&self.found).push(Problem::new(
                Code::BadRemoteName,
                &key.chars()
                    .map(|c| if has_raw(&c.to_string()) { '\u{FFFD}' } else { c })
                    .collect::<String>(),
                "never written here: its name is not UTF-8",
            ));
            return None;
        }
        if let Err(e) = check_key(key) {
            lock(&self.found).push(Problem::new(
                Code::BadRemoteName,
                key,
                format!("never written here: {e}"),
            ));
            return None;
        }
        let gen = entry.get("gen").and_then(Value::as_u64).unwrap_or(0);
        let device = entry
            .get("device")
            .and_then(Value::as_str)
            .unwrap_or("peer")
            .to_string();
        let mtime = entry.get("mtime").and_then(Value::as_i64).unwrap_or(0) / 1_000_000_000;
        let size = entry.get("size").and_then(Value::as_u64);
        let make = |hash: String, size: u64| {
            let mut out = json!({"hash": hash, "size": size, "mtime": mtime, "gen": gen, "device": device});
            if let Some(exec) = entry.get("exec").and_then(Value::as_bool) {
                out["exec"] = Value::Bool(exec);
            }
            out
        };
        let Some(obj) = entry.as_object() else {
            let p = Problem::new(Code::RemoteEntryUnreadable, key, "the drive's entry for it cannot be read; it is left as it is here");
            return Some(make(self.stand_in(key, entry, p), 0));
        };
        if obj.get("kind").and_then(Value::as_str) == Some("link") {
            let target = obj.get("target").and_then(Value::as_str).unwrap_or("");
            let p = Problem::new(Code::LinkRefused, key, format!("a link to {target:?} from another device is not created"));
            return Some(make(self.stand_in(key, entry, p), 0));
        }
        let codec = obj.get("codec").and_then(Value::as_str).unwrap_or("raw");
        if codec != "raw" {
            let p = Problem::new(Code::RemoteEntryUnreadable, key, format!("it is compressed with {codec}, which this engine does not read; it is left on the drive"));
            return Some(make(self.stand_in(key, entry, p), size.unwrap_or(0)));
        }
        let sha = obj.get("hash").and_then(Value::as_str).unwrap_or("");
        let blob = obj.get("blob").and_then(Value::as_str).unwrap_or(sha);
        let well_formed = sha.len() == 64 && blob.len() == 64 && size.is_some();
        if !well_formed {
            let p = Problem::new(Code::RemoteEntryUnreadable, key, "the drive's entry for it cannot be read; it is left as it is here");
            return Some(make(self.stand_in(key, entry, p), 0));
        }
        let size = size.unwrap_or(0);
        if let Some(b3) = obj.get("blake3").and_then(Value::as_str).filter(|h| super::remote::is_hash(h)) {
            lock(&self.blobs).insert(b3.to_string(), blob.to_string());
            return Some(make(b3.to_string(), size));
        }
        match self.blake3_of(blob) {
            Some(b3) => Some(make(b3, size)),
            None => {
                let p = Problem::new(Code::HashMismatch, key, "the drive's bytes for it are missing or do not match their hash; the old version stays");
                Some(make(self.stand_in(key, entry, p), size))
            }
        }
    }

    /// Whether the content `b3` is stored already (put by this pass before a lost race).
    fn has_stored(&self, b3: &str) -> bool {
        let sha = lock(&self.blobs).get(b3).cloned();
        sha.is_some_and(|sha| self.blob_path(&sha).exists())
    }

    fn etag(index: &HarnessIndex) -> String {
        format!("g{}", index.generation)
    }

    /// Bytes of the content `hash`, checked against it.
    fn blob_bytes(&self, key: &str, hash: &str) -> CloudResult<Vec<u8>> {
        if let Some((_, problem)) = lock(&self.unreadable).get(hash) {
            return Err(CloudError::failed(problem.detail.clone()));
        }
        let sha = lock(&self.blobs)
            .get(hash)
            .cloned()
            .ok_or_else(|| CloudError::failed(format!("the drive has no blob for {key}")))?;
        let bytes = fs::read(self.blob_path(&sha))
            .map_err(|_| service(503, &format!("the blob {} is missing", &sha[..12.min(sha.len())]), None))?;
        if local::hash_bytes(&bytes) != hash {
            return Err(CloudError::Drive(azul_storage::DriveError::Corrupt {
                key: key.to_string(),
                reason: String::from("its bytes do not match their hash"),
            }));
        }
        Ok(bytes)
    }

    /// Stores what `source` reads as a blob, hashing both ways as it goes: `(sha256, bytes)`,
    /// `None` when its BLAKE3 is not `expect` (it changed while read; nothing is stored).
    fn store_blob(&self, source: &mut dyn Read, expect: &str) -> CloudResult<Option<(String, u64)>> {
        let tmp = self.dir.join("blobs").join(format!(
            "in.tmp-{}-{}",
            std::process::id(),
            &blake3::hash(format!("{expect}{}", (self.now)()).as_bytes()).to_hex()[..8]
        ));
        let mut b3 = blake3::Hasher::new();
        let mut sha = Sha256::new();
        let mut n = 0u64;
        let written = (|| {
            let mut out = fs::File::create(&tmp)?;
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let got = source.read(&mut buf)?;
                if got == 0 {
                    break;
                }
                b3.update(&buf[..got]);
                sha.update(&buf[..got]);
                out.write_all(&buf[..got])?;
                n += got as u64;
            }
            out.sync_all()
        })();
        if let Err(e) = written {
            let _ = fs::remove_file(&tmp);
            return Err(io(e));
        }
        if b3.finalize().to_hex().as_str() != expect {
            let _ = fs::remove_file(&tmp);
            return Ok(None);
        }
        let sha = hex(&sha.finalize());
        let quota = self.faults().get("quota_bytes").and_then(Value::as_u64);
        if let Some(quota) = quota {
            if used_bytes(&self.dir.join("blobs")) > quota {
                let _ = fs::remove_file(&tmp);
                return Err(service(507, "the drive is full", None));
            }
        }
        let dest = self.blob_path(&sha);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(io)?;
        }
        fs::rename(&tmp, &dest).map_err(io)?;
        lock(&self.blobs).insert(expect.to_string(), sha.clone());
        Ok(Some((sha, n)))
    }
}

fn used_bytes(dir: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).into_iter().flatten().flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                total += meta.len();
            }
        }
    }
    total
}

impl SyncRemote for FolderRemote {
    fn read(
        &self,
        _cached: Option<&(String, RemoteIndex)>,
    ) -> CloudResult<(RemoteIndex, Option<String>)> {
        let harness = self.call("index", "", || self.read_harness())?;
        let Some(harness) = harness else {
            lock(&self.entries).clear();
            return Ok((RemoteIndex::empty(), None));
        };
        lock(&self.found).clear();
        let mut files = Map::new();
        let mut entries = BTreeMap::new();
        for (key, entry) in &harness.files {
            entries.insert(key.clone(), entry.clone());
            if let Some(file) = self.translate(key, entry) {
                files.insert(key.clone(), file);
            }
        }
        let now = (self.now)() as i64;
        let mut deleted = Map::new();
        for (key, tomb) in &harness.deleted {
            if has_raw(key) || check_key(key).is_err() {
                continue;
            }
            let sha = tomb.get("hash").and_then(Value::as_str).unwrap_or("");
            let b3 = self.blake3_of(sha).unwrap_or_else(|| "0".repeat(64));
            let mut t = json!({
                "hash": b3,
                "gen": tomb.get("gen").and_then(Value::as_u64).unwrap_or(0),
                "at": now,
                "device": tomb.get("device").and_then(Value::as_str).unwrap_or("peer"),
            });
            if let Some(confirmed) = tomb.get("confirmed").and_then(Value::as_bool) {
                t["confirmed"] = Value::Bool(confirmed);
            }
            deleted.insert(key.clone(), t);
        }
        *lock(&self.entries) = entries;
        let doc = json!({
            "format": super::remote::INDEX_FORMAT,
            "version": super::remote::INDEX_VERSION,
            "features": harness.features,
            "generation": harness.generation,
            "updated_at": now,
            "updated_by": "",
            "files": files,
            "deleted": deleted,
            "restore_epoch": harness.restore_epoch,
        });
        let index: RemoteIndex = serde_json::from_value(doc)
            .map_err(|e| CloudError::failed(format!("the drive's sync index cannot be read: {e}")))?;
        Ok((index, Some(Self::etag(&harness))))
    }

    fn fetch(&self, key: &str, hash: &str, _size: u64) -> CloudResult<Vec<u8>> {
        self.call("get", key, || self.blob_bytes(key, hash))
    }

    fn fetch_hash(&self, hash: &str, _size: u64) -> CloudResult<Vec<u8>> {
        self.call("get", "", || self.blob_bytes("", hash))
    }

    fn upload(&self, work: &Uploads<'_>, report: &mut SyncReport) -> CloudResult<BTreeSet<String>> {
        report.problems.extend(lock(&self.found).drain(..));
        let mut abandoned = BTreeSet::new();
        let mut first_failure: Option<CloudError> = None;
        let mut any_sent = false;
        for action in work.actions {
            let Some(hash) = super::uploaded_hash(action, work.scan, work.merged) else {
                continue;
            };
            let key = action.key().to_string();
            if work.remote.has_blob(hash) || self.has_stored(hash) {
                report.blobs_reused += 1;
                continue;
            }
            if work.hooks.cancelled() {
                abandoned.insert(key);
                continue;
            }
            let job = super::blob_job(action, work, hash);
            work.hooks.point("mid-read");
            let outcome = self.call("put", &key, || match &job.source {
                super::BlobSource::File(path) => match fs::File::open(path) {
                    Ok(mut file) => self.store_blob(&mut file, hash),
                    Err(_) => Ok(None),
                },
                _ => match super::job_bytes(&job) {
                    Ok(bytes) => self.store_blob(&mut bytes.as_slice(), hash),
                    Err(_) => Ok(None),
                },
            });
            match outcome {
                Ok(Some((_, n))) => {
                    report.blobs_up += 1;
                    report.bytes_up += n;
                    any_sent = true;
                    work.hooks.point("after-blob");
                }
                Ok(None) => {
                    report
                        .changed_during_sync
                        .push(format!("{key}: changed while it was synced"));
                    abandoned.insert(key);
                }
                Err(e) => {
                    report.errors.push(format!("upload {key}: {e}"));
                    report.problems.push(Problem::of_error(&key, &e));
                    report.file_errors.push((key.clone(), e.to_string()));
                    abandoned.insert(key);
                    first_failure.get_or_insert(e);
                }
            }
        }
        // Nothing went through and something failed: the pass fails with it (the drive is
        // down), as the plain index's upload does.
        if let Some(e) = first_failure {
            let only_offline = matches!(
                e.root(),
                CloudError::Drive(azul_storage::DriveError::Transport(_))
            );
            if !any_sent && only_offline {
                return Err(e);
            }
        }
        Ok(abandoned)
    }

    fn commit(
        &self,
        remote: &RemoteIndex,
        next: &RemoteIndex,
        etag: Option<&str>,
        report: &mut SyncReport,
    ) -> CloudResult<Option<String>> {
        report.problems.extend(lock(&self.found).drain(..));
        let next_json = serde_json::to_value(next).map_err(io)?;
        let changes: Vec<(String, Option<Value>)> = {
            let mut out = Vec::new();
            for (key, file) in &next.files {
                if remote.files.get(key) != Some(file) {
                    out.push((key.clone(), next_json["files"].get(key).cloned()));
                }
            }
            for key in remote.files.keys().filter(|k| !next.files.contains_key(*k)) {
                out.push((key.clone(), None));
            }
            out
        };
        let device = self.device.clone();
        self.call("commit", "", || {
            let _held = Locked::take(&self.dir)?;
            let current = match self.read_harness() {
                Ok(Some(index)) => index,
                Ok(None) | Err(_) => HarnessIndex {
                    generation: 0,
                    files: Map::new(),
                    deleted: Map::new(),
                    features: Vec::new(),
                    restore_epoch: 0,
                },
            };
            let expected = etag.unwrap_or("g0");
            if Self::etag(&current) != expected {
                return Ok(None);
            }
            let new_gen = current.generation + 1;
            let mut files = current.files;
            let mut deleted = current.deleted;
            let mut versions = Vec::new();
            for (key, change) in &changes {
                if let Some(old) = files.get(key) {
                    versions.push(json!({"key": key, "entry": old, "replaced_at": new_gen}));
                }
                match change {
                    None => {
                        if let Some(old) = files.remove(key) {
                            let confirmed = next_json["deleted"]
                                .get(key)
                                .and_then(|t| t.get("confirmed"))
                                .and_then(Value::as_bool)
                                .unwrap_or(false);
                            deleted.insert(
                                key.clone(),
                                json!({"hash": old.get("hash"), "gen": new_gen, "device": device, "confirmed": confirmed}),
                            );
                        }
                    }
                    Some(file) => {
                        let b3 = file.get("hash").and_then(Value::as_str).unwrap_or("");
                        let Some(sha) = lock(&self.blobs).get(b3).cloned() else {
                            return Err(CloudError::failed(format!(
                                "{key}: its content is not on the drive; nothing was committed"
                            )));
                        };
                        let mut entry = json!({
                            "hash": sha,
                            "blob": sha,
                            "blake3": b3,
                            "size": file.get("size").cloned().unwrap_or(json!(0)),
                            "kind": "file",
                            "exec": file.get("exec").and_then(Value::as_bool).unwrap_or(false),
                            "mtime": file.get("mtime").and_then(Value::as_i64).unwrap_or(0).saturating_mul(1_000_000_000),
                            "gen": new_gen,
                            "device": device,
                        });
                        if let Some(old) = files.get(key) {
                            if old.get("blake3").is_none() && old.get("hash") == entry.get("hash") {
                                entry["mtime"] = old.get("mtime").cloned().unwrap_or(json!(0));
                            }
                        }
                        files.insert(key.clone(), entry);
                        deleted.remove(key);
                    }
                }
            }
            let doc = json!({"generation": new_gen, "files": files, "deleted": deleted});
            let text = serde_json::to_string_pretty(&doc).map_err(io)?;
            let bytes = encode_raw(&text);
            write_atomic(&self.p(&format!("index-history/{new_gen}.json")), &bytes)?;
            write_atomic(&self.p("index.json"), &bytes)?;
            if !versions.is_empty() {
                let mut f = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(self.p("versions.jsonl"))
                    .map_err(io)?;
                for v in versions {
                    let line = encode_raw(&v.to_string());
                    f.write_all(&line).and_then(|()| f.write_all(b"\n")).map_err(io)?;
                }
            }
            Ok(Some(format!("g{new_gen}")))
        })
    }
}

/// One pass of the folder `root` against the harness drive `remote`.
///
/// # Errors
///
/// As [`super::sync_to`].
pub fn sync(
    remote: &FolderRemote,
    root: &LocalRoot,
    index_path: &Path,
    opts: &SyncOptions,
    hooks: &RunHooks<'_>,
) -> CloudResult<SyncReport> {
    super::run(remote, root, index_path, opts, hooks)
}
