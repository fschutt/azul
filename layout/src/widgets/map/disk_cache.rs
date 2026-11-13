//! The map's tiles on disk, so a pan back or a restart shows them at once.
//!
//! The map's workers keep every tile they download in a folder the app names
//! (`MapSetup::with_disk_cache`), one file per tile, named by a hash of the
//! tile's URL: the URL names the server, the planet version and the tile, so
//! two servers or two versions never share a file. A file starts with a short
//! text header - the URL, until when the tile is fresh, and the validators
//! the server sent (`ETag`, `Last-Modified`) - then the tile's bytes as they
//! were downloaded.
//!
//! How long a tile is fresh is the server's to say (RFC 9111, read as a
//! private cache): `Cache-Control: no-store` keeps nothing, `no-cache` keeps
//! the tile but asks every time, `max-age` (less the `Age` the response
//! already had) or `Expires` (against the server's `Date`) say how long, and
//! with neither a tenth of the time since `Last-Modified`, at most a week. A
//! fresh tile is shown without a request. A stale one is shown at once and
//! asked about with `If-None-Match` / `If-Modified-Since`; a `304 Not
//! Modified` only renews its header.
//!
//! The folder is held under its cap: using a tile touches its file, and when
//! the files add up to more than the cap the least recently used go first,
//! until the folder is under 90% of it.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    time::SystemTime,
};

/// The cap of a disk cache whose setup names none.
pub const DEFAULT_DISK_CACHE_BYTES: u64 = 256 * 1024 * 1024;

/// The first line of every tile file: what it is, and the layout's version.
const MAGIC: &str = "AZTILE 1";

/// The longest a tile is taken as fresh on `Last-Modified` alone.
const MAX_HEURISTIC_SECS: u64 = 7 * 24 * 60 * 60;

/// What a response's headers allow ([`cache_policy`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachePolicy {
    /// Whether the tile may be kept at all (`no-store` says no).
    pub store: bool,
    /// Until when (seconds since 1970) it is used without asking the server.
    pub fresh_until: u64,
    /// The validators to ask with once it is stale.
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

/// A tile the folder holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedTile {
    /// The tile as downloaded.
    pub bytes: Vec<u8>,
    /// Whether it may be used without asking the server.
    pub fresh: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

impl CachedTile {
    /// The headers that ask the server whether this tile changed.
    #[must_use]
    pub fn revalidation_headers(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if let Some(etag) = &self.etag {
            out.push(("If-None-Match".to_string(), etag.clone()));
        }
        if let Some(date) = &self.last_modified {
            out.push(("If-Modified-Since".to_string(), date.clone()));
        }
        out
    }
}

/// The tile folder of one map (cheap: a path and a cap; every worker opens
/// its own).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileDiskCache {
    dir: PathBuf,
    max_bytes: u64,
}

impl TileDiskCache {
    /// The folder at `dir`, held under `max_bytes` (0: the default cap).
    /// `None` for an empty `dir`: the map keeps its tiles in memory only. The
    /// folder is created when the first tile is stored.
    #[must_use]
    pub fn open(dir: &str, max_bytes: u64) -> Option<Self> {
        if dir.trim().is_empty() {
            return None;
        }
        Some(Self {
            dir: PathBuf::from(dir),
            max_bytes: if max_bytes == 0 {
                DEFAULT_DISK_CACHE_BYTES
            } else {
                max_bytes
            },
        })
    }

    /// The tile `url` names, if the folder holds it, fresh or stale at `now`
    /// (seconds since 1970). Touches its file: it was just used.
    #[must_use]
    pub fn lookup(&self, url: &str, now: u64) -> Option<CachedTile> {
        self.lookup_at(url, now, SystemTime::now())
    }

    /// [`Self::lookup`], recording the use at `used`.
    #[must_use]
    pub fn lookup_at(&self, url: &str, now: u64, used: SystemTime) -> Option<CachedTile> {
        let path = self.path_of(url);
        let data = fs::read(&path).ok()?;
        let (header, body) = split_file(&data)?;
        if header.url != url {
            return None;
        }
        touch(&path, used);
        Some(CachedTile {
            bytes: body.to_vec(),
            fresh: now < header.fresh_until,
            etag: header.etag,
            last_modified: header.last_modified,
        })
    }

    /// Keeps `bytes` as the tile `url` names, as `policy` allows; a policy
    /// that forbids keeping it removes what was kept.
    pub fn store(&self, url: &str, bytes: &[u8], policy: &CachePolicy) {
        self.store_at(url, bytes, policy, SystemTime::now());
    }

    /// [`Self::store`], recording the use at `used`.
    pub fn store_at(&self, url: &str, bytes: &[u8], policy: &CachePolicy, used: SystemTime) {
        let path = self.path_of(url);
        let before = fs::metadata(&path).map_or(0, |m| m.len());
        if !policy.store {
            if fs::remove_file(&path).is_ok() {
                self.account(before, 0);
            }
            return;
        }
        let mut data = header_text(url, policy).into_bytes();
        data.extend_from_slice(bytes);
        if write_atomically(&path, &data, used).is_ok() {
            self.account(before, data.len() as u64);
        }
    }

    /// After a `304 Not Modified`: the tile `url` names keeps its bytes and
    /// takes `policy`'s freshness, and its validators where `policy` names
    /// none. `false` when the folder does not hold it.
    #[must_use]
    pub fn renew(&self, url: &str, policy: &CachePolicy) -> bool {
        self.renew_at(url, policy, SystemTime::now())
    }

    /// [`Self::renew`], recording the use at `used`.
    #[must_use]
    pub fn renew_at(&self, url: &str, policy: &CachePolicy, used: SystemTime) -> bool {
        let Some(old) = self.lookup_at(url, 0, used) else {
            return false;
        };
        let merged = CachePolicy {
            store: true,
            fresh_until: policy.fresh_until,
            etag: policy.etag.clone().or(old.etag),
            last_modified: policy.last_modified.clone().or(old.last_modified),
        };
        self.store_at(url, &old.bytes, &merged, used);
        true
    }

    /// Where the tile `url` names is kept: a hash of the URL, in one of 256
    /// sub-folders so no folder grows huge.
    fn path_of(&self, url: &str) -> PathBuf {
        let name = format!("{:016x}", fnv1a(url.as_bytes()));
        self.dir.join(&name[..2]).join(format!("{name}.tile"))
    }

    /// Books a file's change from `before` to `after` bytes, and evicts the
    /// least recently used tiles once the folder is over its cap.
    fn account(&self, before: u64, after: u64) {
        let Ok(mut usage) = USAGE.lock() else {
            return;
        };
        let used = if let Some(used) = usage.get_mut(&self.dir) {
            *used = used.saturating_sub(before).saturating_add(after);
            *used
        } else {
            // The first store into this folder in this run: what the disk
            // holds already includes it.
            let size = folder_size(&self.dir);
            usage.insert(self.dir.clone(), size);
            size
        };
        if used > self.max_bytes {
            let left = evict(&self.dir, self.max_bytes / 10 * 9);
            usage.insert(self.dir.clone(), left);
        }
    }
}

/// The bytes every tile folder in use holds, as booked by its stores: read
/// from the disk once per folder, then kept up to date, so a store does not
/// walk the folder.
static USAGE: Mutex<BTreeMap<PathBuf, u64>> = Mutex::new(BTreeMap::new());

/// What a response's `headers` (any case, repeated ones combined) allow, at
/// `now` (seconds since 1970). See the module intro for the rules.
#[must_use]
pub fn cache_policy<'a>(headers: impl IntoIterator<Item = (&'a str, &'a str)>, now: u64) -> CachePolicy {
    let mut directives: Vec<String> = Vec::new();
    let (mut age, mut date, mut expires) = (0u64, None, None);
    let (mut etag, mut last_modified) = (None, None);
    for (name, value) in headers {
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "cache-control" => directives.extend(
                value
                    .split(',')
                    .map(|d| d.trim().to_ascii_lowercase())
                    .filter(|d| !d.is_empty()),
            ),
            "age" => age = value.parse().unwrap_or(0),
            "date" => date = parse_http_date(value),
            // An Expires that is not a date ("0", "-1") means already expired.
            "expires" => expires = Some(parse_http_date(value).unwrap_or(0)),
            "etag" if one_line(value) => etag = Some(value.to_string()),
            "last-modified" if one_line(value) => last_modified = Some(value.to_string()),
            _ => {}
        }
    }
    let has = |d: &str| directives.iter().any(|x| x == d);
    let max_age = directives
        .iter()
        .find_map(|d| d.strip_prefix("max-age=")?.trim_matches('"').parse::<u64>().ok());
    let lifetime = if has("no-store") {
        return CachePolicy {
            store: false,
            fresh_until: now,
            etag,
            last_modified,
        };
    } else if has("no-cache") {
        0
    } else if let Some(max_age) = max_age {
        max_age.saturating_sub(age)
    } else if let Some(expires) = expires {
        expires.saturating_sub(date.unwrap_or(now)).saturating_sub(age)
    } else if let Some(modified) = last_modified.as_deref().and_then(parse_http_date) {
        (date.unwrap_or(now).saturating_sub(modified) / 10).min(MAX_HEURISTIC_SECS)
    } else {
        0
    };
    CachePolicy {
        store: true,
        fresh_until: now.saturating_add(lifetime),
        etag,
        last_modified,
    }
}

/// An HTTP date (`Sun, 06 Nov 1994 08:49:37 GMT`, RFC 9110's IMF-fixdate)
/// in seconds since 1970.
#[must_use]
pub fn parse_http_date(s: &str) -> Option<u64> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let parts: Vec<&str> = s.split_whitespace().collect();
    let p = match parts.len() {
        6 => &parts[1..],
        5 => &parts[..],
        _ => return None,
    };
    if !p[4].eq_ignore_ascii_case("GMT") && !p[4].eq_ignore_ascii_case("UTC") {
        return None;
    }
    let day: i64 = p[0].parse().ok()?;
    let month = MONTHS.iter().position(|m| p[1].eq_ignore_ascii_case(m))? + 1;
    let year: i64 = p[2].parse().ok()?;
    let mut clock = p[3].split(':').map(str::parse::<i64>);
    let (h, m, sec) = (
        clock.next()?.ok()?,
        clock.next()?.ok()?,
        clock.next()?.ok()?,
    );
    if !(1..=31).contains(&day) || !(0..24).contains(&h) || !(0..60).contains(&m) || !(0..61).contains(&sec) {
        return None;
    }
    #[allow(clippy::cast_possible_wrap)] // a month number
    let days = days_from_civil(year, month as i64, day);
    u64::try_from(days * 86_400 + h * 3_600 + m * 60 + sec).ok()
}

/// Now, in seconds since 1970.
#[must_use]
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Days from 1970-01-01 to `year-month-day` (proleptic Gregorian), after
/// Howard Hinnant's `days_from_civil`.
const fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// A header value that can stand on one line of a tile file.
fn one_line(value: &str) -> bool {
    !value.is_empty() && !value.contains(['\n', '\r'])
}

/// The header of a tile file.
struct FileHeader {
    url: String,
    fresh_until: u64,
    etag: Option<String>,
    last_modified: Option<String>,
}

/// The header text a tile file starts with; a blank line ends it.
fn header_text(url: &str, policy: &CachePolicy) -> String {
    let mut out = format!("{MAGIC}\nurl {url}\nfresh-until {}\n", policy.fresh_until);
    if let Some(etag) = policy.etag.as_deref().filter(|v| one_line(v)) {
        out.push_str("etag ");
        out.push_str(etag);
        out.push('\n');
    }
    if let Some(date) = policy.last_modified.as_deref().filter(|v| one_line(v)) {
        out.push_str("last-modified ");
        out.push_str(date);
        out.push('\n');
    }
    out.push('\n');
    out
}

/// A tile file's header and bytes; `None` for a file that is not one.
fn split_file(data: &[u8]) -> Option<(FileHeader, &[u8])> {
    let end = data.windows(2).position(|w| w == b"\n\n")?;
    let text = core::str::from_utf8(&data[..end]).ok()?;
    let mut lines = text.lines();
    if lines.next()? != MAGIC {
        return None;
    }
    let mut header = FileHeader {
        url: String::new(),
        fresh_until: 0,
        etag: None,
        last_modified: None,
    };
    for line in lines {
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        match key {
            "url" => header.url = value.to_string(),
            "fresh-until" => header.fresh_until = value.parse().unwrap_or(0),
            "etag" => header.etag = Some(value.to_string()),
            "last-modified" => header.last_modified = Some(value.to_string()),
            _ => {}
        }
    }
    Some((header, &data[end + 2..]))
}

/// Writes `data` to `path` through a temporary file and a rename, so another
/// worker never reads half a tile, and records the use at `used`.
fn write_atomically(path: &Path, data: &[u8], used: SystemTime) -> std::io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&tmp, data)?;
    touch(&tmp, used);
    fs::rename(&tmp, path).inspect_err(|_| {
        drop(fs::remove_file(&tmp));
    })
}

/// Marks the file at `path` used at `when` (its modification time, what the
/// eviction orders by).
fn touch(path: &Path, when: SystemTime) {
    if let Ok(file) = fs::File::options().write(true).open(path) {
        drop(file.set_modified(when));
    }
}

/// Every tile file under `dir`: its path, size and last use.
fn tile_files(dir: &Path) -> Vec<(SystemTime, u64, PathBuf)> {
    let mut out = Vec::new();
    let Ok(shards) = fs::read_dir(dir) else {
        return out;
    };
    for shard in shards.flatten() {
        let Ok(files) = fs::read_dir(shard.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("tile") {
                continue;
            }
            if let Ok(meta) = file.metadata() {
                let used = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                out.push((used, meta.len(), path));
            }
        }
    }
    out
}

/// The bytes the tile files under `dir` hold.
fn folder_size(dir: &Path) -> u64 {
    tile_files(dir).iter().map(|(_, len, _)| len).sum()
}

/// Removes the least recently used tile files under `dir` until they hold
/// at most `target` bytes; returns what they hold then.
fn evict(dir: &Path, target: u64) -> u64 {
    let mut files = tile_files(dir);
    let mut total: u64 = files.iter().map(|(_, len, _)| len).sum();
    files.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.2.cmp(&b.2)));
    for (_, len, path) in files {
        if total <= target {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
    total
}

/// FNV-1a over `bytes`: a stable file name for a URL on every run.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}
