//! The answers of the online services AzMaps asks (routes, public transport journeys, place
//! searches, the address of a place), kept so that the same question is not asked twice: in
//! memory for the session and in a folder across sessions, the folder capped in size with the
//! least recently used answers going first.
//!
//! An answer is fresh for as long as its service allows (`Cache-Control: max-age`, or the
//! caller's own time to live) and stale after that. A stale answer still has its validators
//! (`ETag`, `Last-Modified`), so asking again can cost a `304 Not Modified` instead of the whole
//! answer, and it is what answers when the network is gone: the caller marks it "offline".
//!
//! The key of an answer is the question as the caller words it. A caller that wants a nearby
//! question to find the same answer rounds its places first ([`place_key`]) and puts times into
//! buckets ([`time_bucket`]).

use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};

use serde_json::{json, Value};

/// One answer as the cache keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The answer's bytes as the service sent them.
    pub body: Vec<u8>,
    /// When it was fetched or last confirmed by the service, unix seconds.
    pub stored_at: u64,
    /// How long after `stored_at` it is fresh, seconds.
    pub max_age_s: u64,
    /// The service's `ETag`, sent back as `If-None-Match` when the answer is stale.
    pub etag: Option<String>,
    /// The service's `Last-Modified`, sent back as `If-Modified-Since`.
    pub last_modified: Option<String>,
}

impl Entry {
    /// A new answer, stored `now`, fresh for `max_age_s`.
    #[must_use]
    pub fn new(body: Vec<u8>, now: u64, max_age_s: u64) -> Self {
        Self {
            body,
            stored_at: now,
            max_age_s,
            etag: None,
            last_modified: None,
        }
    }

    /// Whether it may answer without asking the service.
    #[must_use]
    pub fn is_fresh(&self, now: u64) -> bool {
        now < self.stored_at.saturating_add(self.max_age_s)
    }

    /// How old it is, seconds.
    #[must_use]
    pub fn age_s(&self, now: u64) -> u64 {
        now.saturating_sub(self.stored_at)
    }

    /// Whether the service can be asked if it changed (it has a validator).
    #[must_use]
    pub fn can_revalidate(&self) -> bool {
        self.etag.is_some() || self.last_modified.is_some()
    }
}

/// The answers, in memory and in a folder.
pub struct ResponseCache {
    /// The folder of the disk half; `None` keeps the answers in memory only.
    dir: Option<PathBuf>,
    /// How big the folder may grow, bytes ([`ResponseCache::prune`] keeps it under).
    max_disk_bytes: u64,
    /// How many answers memory keeps.
    memory_entries: usize,
    /// The session's answers, least recently used first.
    memory: Mutex<VecDeque<(String, Entry)>>,
}

/// The first line of an answer's file: what kind of file it is.
const MAGIC: &str = "AZC1";

impl ResponseCache {
    /// A cache that keeps `memory_entries` answers in memory and, with a `dir`, every answer on
    /// disk too, at most `max_disk_bytes` of them.
    #[must_use]
    pub fn new(dir: Option<PathBuf>, memory_entries: usize, max_disk_bytes: u64) -> Self {
        Self {
            dir,
            max_disk_bytes,
            memory_entries: memory_entries.max(1),
            memory: Mutex::new(VecDeque::new()),
        }
    }

    /// A cache in memory only.
    #[must_use]
    pub fn in_memory(memory_entries: usize) -> Self {
        Self::new(None, memory_entries, 0)
    }

    /// The folder of the disk half.
    #[must_use]
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// The answer kept for `key`, fresh or stale: from memory, else from the folder (it is
    /// remembered in memory from then on, and its file counts as just used).
    #[must_use]
    pub fn get(&self, key: &str) -> Option<Entry> {
        if let Ok(mut memory) = self.memory.lock() {
            if let Some(at) = memory.iter().position(|(k, _)| k == key) {
                // Used now: it moves to the back, the most recently used end.
                let hit = memory.remove(at)?;
                let entry = hit.1.clone();
                memory.push_back(hit);
                return Some(entry);
            }
        }
        let entry = Self::read_file(self.dir.as_deref()?, key)?;
        self.remember(key, &entry);
        Some(entry)
    }

    /// Keeps `entry` as the answer for `key`, in memory and in the folder. A folder that cannot
    /// be written only makes the next session slower.
    pub fn put(&self, key: &str, entry: Entry) {
        if let Some(dir) = &self.dir {
            Self::write_file(dir, key, &entry);
        }
        self.remember(key, &entry);
    }

    /// The service said the answer for `key` did not change (`304`): it is fresh again from
    /// `now` for `max_age_s`.
    pub fn refresh(&self, key: &str, now: u64, max_age_s: u64) {
        if let Some(mut entry) = self.get(key) {
            entry.stored_at = now;
            entry.max_age_s = max_age_s;
            self.put(key, entry);
        }
    }

    /// Deletes the least recently used files of the folder until it holds at most its cap.
    /// Returns how many bytes it holds after.
    pub fn prune(&self) -> u64 {
        let Some(dir) = &self.dir else {
            return 0;
        };
        let mut files = Vec::new();
        crate::source::collect_files(dir, &mut files, 0);
        let mut total: u64 = files.iter().map(|f| f.1).sum();
        // A file's time is when it was last written or read (`read_file` touches it).
        files.sort_by(|a, b| a.0.cmp(&b.0));
        for (_, size, path) in files {
            if total <= self.max_disk_bytes {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(size);
            }
        }
        total
    }

    /// The file of `key` in `dir`: a hash of the key, so any key makes a file name.
    fn file_of(dir: &Path, key: &str) -> PathBuf {
        dir.join(format!("{:016x}.cache", fnv(key)))
    }

    /// Keeps `entry` in memory as the most recently used answer, dropping the least recently
    /// used one when memory is full.
    fn remember(&self, key: &str, entry: &Entry) {
        let Ok(mut memory) = self.memory.lock() else {
            return;
        };
        if let Some(at) = memory.iter().position(|(k, _)| k == key) {
            memory.remove(at);
        }
        while memory.len() >= self.memory_entries {
            memory.pop_front();
        }
        memory.push_back((key.to_string(), entry.clone()));
    }

    /// The answer for `key` in its file: the magic line, the header line naming this very key,
    /// then the body. The file's time is set to now, so pruning sees it as just used.
    fn read_file(dir: &Path, key: &str) -> Option<Entry> {
        let path = Self::file_of(dir, key);
        let bytes = std::fs::read(&path).ok()?;
        let magic_end = bytes.iter().position(|&b| b == b'\n')?;
        if &bytes[..magic_end] != MAGIC.as_bytes() {
            return None;
        }
        let rest = &bytes[magic_end + 1..];
        let header_end = rest.iter().position(|&b| b == b'\n')?;
        let header: Value = serde_json::from_slice(&rest[..header_end]).ok()?;
        if header.get("key").and_then(Value::as_str) != Some(key) {
            return None;
        }
        let text = |name: &str| header.get(name).and_then(Value::as_str).map(str::to_string);
        let entry = Entry {
            body: rest[header_end + 1..].to_vec(),
            stored_at: header.get("stored_at").and_then(Value::as_u64)?,
            max_age_s: header.get("max_age_s").and_then(Value::as_u64)?,
            etag: text("etag"),
            last_modified: text("last_modified"),
        };
        if let Ok(file) = std::fs::File::options().write(true).open(&path) {
            let _ = file.set_modified(SystemTime::now());
        }
        Some(entry)
    }

    /// Writes the answer's file whole or not at all: into a temporary file, then renamed over
    /// the old one, so a reader never sees half an answer.
    fn write_file(dir: &Path, key: &str, entry: &Entry) {
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        let path = Self::file_of(dir, key);
        let mut bytes = Vec::with_capacity(entry.body.len() + 256);
        bytes.extend_from_slice(MAGIC.as_bytes());
        bytes.push(b'\n');
        bytes.extend_from_slice(header_of(key, entry).to_string().as_bytes());
        bytes.push(b'\n');
        bytes.extend_from_slice(&entry.body);
        let part = path.with_extension(format!("part{}", std::process::id()));
        if std::fs::write(&part, &bytes).is_ok() && std::fs::rename(&part, &path).is_err() {
            let _ = std::fs::remove_file(&part);
        }
    }
}

/// The header line of an answer's file.
fn header_of(key: &str, entry: &Entry) -> Value {
    json!({
        "key": key,
        "stored_at": entry.stored_at,
        "max_age_s": entry.max_age_s,
        "etag": entry.etag,
        "last_modified": entry.last_modified,
    })
}

/// Seconds since 1970.
#[must_use]
pub fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// How long a `Cache-Control` header lets an answer be fresh: its `max-age`, `0` for
/// `no-cache` / `no-store`, `None` when it says nothing about it.
#[must_use]
pub fn max_age_of(cache_control: Option<&str>) -> Option<u64> {
    let mut found = None;
    for directive in cache_control?.split(',') {
        let directive = directive.trim().to_ascii_lowercase();
        if directive == "no-cache" || directive == "no-store" {
            return Some(0);
        }
        if let Some(value) = directive.strip_prefix("max-age=") {
            found = value.trim_matches('"').parse().ok().or(found);
        }
    }
    found
}

/// A place as a key: rounded to `decimals` (3 is about 100 m, 4 about 10 m), so places that
/// close together ask the same question.
#[must_use]
pub fn place_key((lat, lon): (f64, f64), decimals: usize) -> String {
    format!("{lat:.decimals$},{lon:.decimals$}")
}

/// The bucket of `unix` seconds `bucket_s` wide (its start): times in one bucket ask the same
/// question.
#[must_use]
pub fn time_bucket(unix: i64, bucket_s: i64) -> i64 {
    if bucket_s <= 0 {
        return unix;
    }
    unix.div_euclid(bucket_s) * bucket_s
}

/// A stable 64-bit hash (FNV-1a): an answer's file name.
#[must_use]
pub fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        // `answers`, not `cache`: the tile cache's tests name their folders `azul-routing-cache-*`,
        // and two tests of one run sharing a folder delete each other's files.
        let dir = std::env::temp_dir().join(format!("azul-routing-answers-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn an_answer_is_fresh_for_its_max_age_and_stale_after() {
        let entry = Entry::new(b"x".to_vec(), 1_000, 60);
        assert!(entry.is_fresh(1_000));
        assert!(entry.is_fresh(1_059));
        assert!(!entry.is_fresh(1_060));
        assert_eq!(entry.age_s(1_030), 30);
        assert!(!entry.can_revalidate(), "no validator, no conditional request");
        let tagged = Entry { etag: Some("\"v1\"".to_string()), ..entry };
        assert!(tagged.can_revalidate());
    }

    #[test]
    fn a_kept_answer_is_found_again_in_memory_and_in_the_folder_of_the_next_session() {
        let dir = folder("sessions");
        let first = ResponseCache::new(Some(dir.clone()), 8, 1 << 20);
        let mut entry = Entry::new(b"{\"route\":1}".to_vec(), 5_000, 3_600);
        entry.etag = Some("\"abc\"".to_string());
        first.put("route car 52.525,13.369 51.345,12.382", entry.clone());
        assert_eq!(first.get("route car 52.525,13.369 51.345,12.382"), Some(entry.clone()));
        assert_eq!(first.get("route car 52.525,13.369 51.346,12.382"), None, "another question");
        // The next session reads the folder.
        let second = ResponseCache::new(Some(dir.clone()), 8, 1 << 20);
        assert_eq!(second.get("route car 52.525,13.369 51.345,12.382"), Some(entry));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn memory_keeps_the_most_recently_used_answers() {
        let cache = ResponseCache::in_memory(2);
        cache.put("a", Entry::new(b"a".to_vec(), 0, 10));
        cache.put("b", Entry::new(b"b".to_vec(), 0, 10));
        assert!(cache.get("a").is_some(), "a used last");
        cache.put("c", Entry::new(b"c".to_vec(), 0, 10));
        assert!(cache.get("a").is_some());
        assert!(cache.get("b").is_none(), "b was used least recently");
        assert!(cache.get("c").is_some());
    }

    #[test]
    fn a_confirmed_answer_is_fresh_again() {
        let cache = ResponseCache::in_memory(4);
        cache.put("q", Entry::new(b"q".to_vec(), 100, 10));
        assert!(!cache.get("q").expect("kept").is_fresh(200));
        cache.refresh("q", 200, 50);
        let entry = cache.get("q").expect("kept");
        assert!(entry.is_fresh(240));
        assert_eq!(entry.body, b"q".to_vec(), "the body stays");
    }

    #[test]
    fn the_folder_is_pruned_least_recently_used_first() {
        let dir = folder("prune");
        let cache = ResponseCache::new(Some(dir.clone()), 1, 2_500);
        for (i, key) in ["one", "two", "three"].into_iter().enumerate() {
            cache.put(key, Entry::new(vec![b'x'; 1_000], i as u64, 10));
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // "one" is read again: it is the most recently used now.
        assert!(ResponseCache::new(Some(dir.clone()), 1, 2_500).get("one").is_some());
        let left = cache.prune();
        assert!(left <= 2_500, "{left} bytes left");
        let fresh = ResponseCache::new(Some(dir.clone()), 1, 2_500);
        assert!(fresh.get("one").is_some(), "used last, kept");
        assert!(fresh.get("two").is_none(), "used least recently, pruned");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_damaged_or_foreign_file_is_no_answer() {
        let dir = folder("damaged");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(ResponseCache::file_of(&dir, "k"), b"not a cache file").expect("written");
        let cache = ResponseCache::new(Some(dir.clone()), 4, 1 << 20);
        assert_eq!(cache.get("k"), None);
        // A file whose header names another key (a hash collision) is not this key's answer.
        cache.put("k2", Entry::new(b"two".to_vec(), 1, 1));
        let _ = std::fs::copy(ResponseCache::file_of(&dir, "k2"), ResponseCache::file_of(&dir, "k3"));
        assert_eq!(ResponseCache::new(Some(dir.clone()), 4, 1 << 20).get("k3"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_control_says_how_long_an_answer_is_fresh() {
        assert_eq!(max_age_of(Some("public, max-age=3600")), Some(3_600));
        assert_eq!(max_age_of(Some("max-age=0, must-revalidate")), Some(0));
        assert_eq!(max_age_of(Some("no-store")), Some(0));
        assert_eq!(max_age_of(Some("no-cache")), Some(0));
        assert_eq!(max_age_of(Some("public")), None);
        assert_eq!(max_age_of(None), None);
    }

    #[test]
    fn nearby_places_and_close_times_ask_the_same_question() {
        assert_eq!(place_key((52.52498, 13.369114), 3), "52.525,13.369");
        assert_eq!(place_key((52.52471, 13.36925), 3), "52.525,13.369");
        assert_ne!(place_key((52.5262, 13.3691), 3), place_key((52.5249, 13.3691), 3));
        assert_eq!(place_key((-33.8688, 151.2093), 2), "-33.87,151.21");
        assert_eq!(time_bucket(1_792_047_600 + 299, 300), 1_792_047_600);
        assert_eq!(time_bucket(1_792_047_600 + 300, 300), 1_792_047_900);
        assert_eq!(time_bucket(-1, 300), -300, "before 1970 rounds down too");
    }
}
