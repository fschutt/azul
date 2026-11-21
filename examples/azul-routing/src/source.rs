//! Where the routing tiles come from: a folder (a build's tiles next to its
//! manifest), a download (feature `http`), and a cache in front of either -
//! in memory for the session and in a folder across sessions, so a second
//! route through the same area costs no network.
//!
//! Roads change slowly, the big ones slowest, so the cache keeps a tile for
//! weeks: by default 8 weeks for zoom 8 (motorways, trunk and primary
//! roads), 4 for zoom 11 and 2 for zoom 14 ([`KeepPolicy`]). The manifest
//! may suggest other numbers and the user's settings win over both
//! ([`SETTINGS_KEY`]). Tiles of different builds can be used together,
//! because node and edge keys do not change between builds. A tile past its
//! time is fetched again; when that fails (no network), the old tile is used
//! anyway, so a downloaded area keeps working offline. Pruning the folder
//! goes oldest first and never deletes a tile of an offline area
//! ([`crate::offline`]).

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use serde_json::Value;

use crate::{
    manifest::{fill, manifest_path, Manifest, DEFAULT_TILES},
    offline,
    router::TileSource,
    tile::TileId,
};

/// The settings key the user sets: a number of days for every zoom, or an
/// object `{"8": 56, "11": 28, "14": 14}` (zooms left out keep their value).
/// The settings page shows it as "Keep downloaded roads for ... days".
pub const SETTINGS_KEY: &str = "routing_tiles_keep_days";

/// How many days a tile of zoom 8, 11 and 14 is kept before it is fetched
/// again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeepPolicy {
    /// The days for zoom 8, 11 and 14, in that order.
    pub days: [u32; 3],
}

impl Default for KeepPolicy {
    fn default() -> Self {
        Self { days: [56, 28, 14] }
    }
}

impl KeepPolicy {
    /// What the hosting suggests.
    #[must_use]
    pub const fn from_manifest(manifest: &Manifest) -> Self {
        Self { days: manifest.keep_days }
    }

    /// This policy with the user's settings (the whole `settings.json`)
    /// applied over it.
    #[must_use]
    pub fn with_settings(mut self, settings: &Value) -> Self {
        let days = |v: &Value| v.as_u64().and_then(|d| u32::try_from(d).ok());
        match &settings[SETTINGS_KEY] {
            Value::Number(_) => {
                if let Some(d) = days(&settings[SETTINGS_KEY]) {
                    self.days = [d; 3];
                }
            }
            Value::Object(per_zoom) => {
                for (slot, z) in self.days.iter_mut().zip(["8", "11", "14"]) {
                    if let Some(d) = per_zoom.get(z).and_then(days) {
                        *slot = d;
                    }
                }
            }
            _ => {}
        }
        self
    }

    /// How long a tile of zoom `z` is kept.
    #[must_use]
    pub fn keep_for(&self, z: u8) -> Duration {
        let days = match z {
            0..=8 => self.days[0],
            9..=11 => self.days[1],
            _ => self.days[2],
        };
        Duration::from_secs(u64::from(days) * 86_400)
    }

    /// Whether a tile of zoom `z` fetched at `fetched` is still good at `now`.
    #[must_use]
    pub fn is_fresh(&self, z: u8, fetched: SystemTime, now: SystemTime) -> bool {
        now.duration_since(fetched).map_or(true, |age| age < self.keep_for(z))
    }
}

/// The file of `tile` in a tile folder: `<z>/<x>/<y>.pbf`.
#[must_use]
pub fn tile_file(root: &Path, tile: TileId) -> PathBuf {
    root.join(tile.z.to_string())
        .join(tile.x.to_string())
        .join(format!("{}.pbf", tile.y))
}

/// Tiles from a folder: a build's `manifest.json` (or the folder holding
/// it), whose tile template names the files, or else a folder with its
/// tiles at `tiles/<z>/<x>/<y>.pbf`. A missing file is a tile without roads.
#[derive(Debug, Clone)]
pub struct FolderTiles {
    /// The manifest, or the folder holding it or the tiles.
    pub root: PathBuf,
}

impl FolderTiles {
    /// The file of `tile`. The manifest is read for every tile: it is small,
    /// and this keeps the folder a plain path.
    ///
    /// # Errors
    /// The folder has a manifest that does not read.
    pub fn tile_path(&self, tile: TileId) -> Result<PathBuf, String> {
        let manifest = manifest_path(&self.root);
        let (base, template) = if manifest.is_file() {
            let base = manifest.parent().unwrap_or_else(|| Path::new("")).to_path_buf();
            (base, Manifest::read_file(&manifest)?.tiles)
        } else {
            (self.root.clone(), DEFAULT_TILES.to_string())
        };
        Ok(base.join(fill(&template, tile)))
    }

    /// The folder's manifest: its attribution goes next to every route.
    ///
    /// # Errors
    /// There is none, or it does not read.
    pub fn manifest(&self) -> Result<Manifest, String> {
        Manifest::read_file(&self.root)
    }
}

impl TileSource for FolderTiles {
    fn fetch(&self, tile: TileId) -> Result<Option<Vec<u8>>, String> {
        let path = self.tile_path(tile)?;
        match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }
}

/// Tiles held in memory: the tests' source.
#[derive(Debug, Clone, Default)]
pub struct MemoryTiles {
    /// The tiles' bytes; a tile not in here has no roads.
    pub tiles: BTreeMap<TileId, Vec<u8>>,
}

impl TileSource for MemoryTiles {
    fn fetch(&self, tile: TileId) -> Result<Option<Vec<u8>>, String> {
        Ok(self.tiles.get(&tile).cloned())
    }
}

/// The marker file of a tile the hosting does not have, next to where its
/// bytes would be: the cache remembers "no roads here" too.
fn none_file(root: &Path, tile: TileId) -> PathBuf {
    tile_file(root, tile).with_extension("none")
}

/// The tile a file of a tile folder holds (`<z>/<x>/<y>.pbf` or `.none`);
/// `None` for any other file.
fn tile_of_file(root: &Path, path: &Path) -> Option<TileId> {
    let parts: Vec<&str> = path.strip_prefix(root).ok()?.iter().filter_map(|p| p.to_str()).collect();
    let [z, x, file] = parts.as_slice() else {
        return None;
    };
    let (y, extension) = file.split_once('.')?;
    if !matches!(extension, "pbf" | "none") {
        return None;
    }
    Some(TileId::new(z.parse().ok()?, x.parse().ok()?, y.parse().ok()?))
}

/// Writes a cache file. A cache that cannot be written is only slower: the
/// route goes on.
fn store(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, bytes);
}

/// When a cache file was written, which is when its tile was fetched.
fn written(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// How many tiles the memory cache keeps before it starts over: a long
/// trip's few hundred tiles a few times over.
const MEMORY_TILES: usize = 2048;

/// A tile in the memory cache: its bytes (`None`: no roads there) and when
/// they were fetched.
type Kept = (Option<Arc<Vec<u8>>>, SystemTime);

/// A cache in front of another source: in memory for the session, and in a
/// folder (`dir`) across sessions, each tile kept as long as [`KeepPolicy`]
/// says and then fetched again - unless the network is gone, then the old
/// one serves. The folder is capped at `max_disk_bytes`, the oldest tiles
/// going first ([`CachedTiles::prune`]).
pub struct CachedTiles<S: TileSource> {
    /// Where the tiles come from when the cache has none fresh.
    pub inner: S,
    /// The folder of the disk cache; `None` keeps the tiles in memory only.
    pub dir: Option<PathBuf>,
    /// How big the tiles of no offline area may grow on disk, bytes.
    pub max_disk_bytes: u64,
    /// How long each zoom is kept.
    pub policy: KeepPolicy,
    /// The tiles of this session. Shared: the cache is read from all fetch
    /// threads at once.
    memory: Mutex<HashMap<TileId, Kept>>,
}

impl<S: TileSource> CachedTiles<S> {
    /// A cache over `inner`, on disk in `dir` when there is one, keeping
    /// tiles as long as the default [`KeepPolicy`] says.
    pub fn new(inner: S, dir: Option<PathBuf>, max_disk_bytes: u64) -> Self {
        Self {
            inner,
            dir,
            max_disk_bytes,
            policy: KeepPolicy::default(),
            memory: Mutex::new(HashMap::new()),
        }
    }

    /// This cache, keeping tiles as long as `policy` says.
    #[must_use]
    pub fn with_policy(mut self, policy: KeepPolicy) -> Self {
        self.policy = policy;
        self
    }

    fn remember(&self, tile: TileId, bytes: Option<&Vec<u8>>, fetched: SystemTime) {
        if let Ok(mut memory) = self.memory.lock() {
            if memory.len() >= MEMORY_TILES {
                memory.clear();
            }
            memory.insert(tile, (bytes.map(|b| Arc::new(b.clone())), fetched));
        }
    }

    fn recall(&self, tile: TileId) -> Option<Kept> {
        self.memory.lock().ok()?.get(&tile).cloned()
    }

    /// Deletes the oldest tiles of the disk cache until the tiles of no
    /// offline area take at most `max_disk_bytes`; an offline area's tiles
    /// count apart and stay. Files of the old JSON tiles go too. Returns
    /// how many bytes the other tiles take after.
    pub fn prune(&self) -> u64 {
        let Some(dir) = &self.dir else {
            return 0;
        };
        let pinned = offline::pinned(dir);
        let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
        collect_files(dir, &mut files, 4);
        files.retain(|(_, _, path)| match tile_of_file(dir, path) {
            Some(tile) => !pinned.contains(&tile),
            None => {
                if path.to_string_lossy().ends_with(".json.gz") {
                    let _ = std::fs::remove_file(path);
                }
                false
            }
        });
        let mut total: u64 = files.iter().map(|f| f.1).sum();
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
}

/// Every file under `dir`, `depth` folders deep at most, with its time and
/// size.
pub(crate) fn collect_files(dir: &Path, out: &mut Vec<(std::time::SystemTime, u64, PathBuf)>, depth: usize) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_dir() {
            if depth > 0 {
                collect_files(&entry.path(), out, depth - 1);
            }
        } else {
            let time = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
            out.push((time, meta.len(), entry.path()));
        }
    }
}

impl<S: TileSource> TileSource for CachedTiles<S> {
    fn fetch(&self, tile: TileId) -> Result<Option<Vec<u8>>, String> {
        let now = SystemTime::now();
        let in_memory = self.recall(tile);
        if let Some((bytes, fetched)) = &in_memory {
            if self.policy.is_fresh(tile.z, *fetched, now) {
                return Ok(bytes.as_deref().cloned());
            }
        }
        // What the folder has: when it was fetched, and whether it is a tile
        // rather than "no roads here".
        let files = self.dir.as_ref().map(|dir| (tile_file(dir, tile), none_file(dir, tile)));
        let on_disk = files.as_ref().and_then(|(have, none)| match (written(have), written(none)) {
            (Some(at), _) => Some((at, true)),
            (None, Some(at)) => Some((at, false)),
            (None, None) => None,
        });
        let read_disk = |exists: bool| match (&files, exists) {
            (Some((have, _)), true) => std::fs::read(have).ok().map(Some),
            _ => Some(None),
        };
        if let Some((fetched, exists)) = on_disk.filter(|(at, _)| self.policy.is_fresh(tile.z, *at, now)) {
            if let Some(bytes) = read_disk(exists) {
                self.remember(tile, bytes.as_ref(), fetched);
                return Ok(bytes);
            }
        }
        match self.inner.fetch(tile) {
            Ok(fetched) => {
                if let Some((have, none)) = &files {
                    let (write, gone) = if fetched.is_some() { (have, none) } else { (none, have) };
                    store(write, fetched.as_deref().unwrap_or(&[]));
                    let _ = std::fs::remove_file(gone);
                }
                self.remember(tile, fetched.as_ref(), now);
                Ok(fetched)
            }
            // The network is gone: an old tile is better than none.
            Err(e) => match (on_disk, in_memory) {
                (Some((_, exists)), _) => read_disk(exists).ok_or(e),
                (None, Some((bytes, _))) => Ok(bytes.as_deref().cloned()),
                (None, None) => Err(e),
            },
        }
    }
}

#[cfg(feature = "http")]
impl CachedTiles<HttpTiles> {
    /// The tile set's manifest (its attribution goes next to every route):
    /// from the network, with a copy kept in the cache folder for when the
    /// network is gone.
    ///
    /// # Errors
    /// Neither the network nor the copy has it.
    pub fn manifest(&self) -> Result<Manifest, String> {
        let copy = self.dir.as_ref().map(|dir| dir.join("manifest.json"));
        match self.inner.manifest() {
            Ok(manifest) => {
                if let Some(copy) = &copy {
                    store(copy, manifest.to_json().as_bytes());
                }
                Ok(manifest)
            }
            Err(e) => copy.and_then(|copy| Manifest::read_file(&copy).ok()).ok_or(e),
        }
    }

    /// The hosting's tile list (`tiles.txt`, which the manifest names):
    /// every routing tile there is and its size, what an offline area is
    /// planned with. From the network, with a copy kept in the cache folder
    /// for when the network is gone.
    ///
    /// # Errors
    /// The tile set lists no tiles, or neither the network nor the copy has
    /// the list.
    pub fn index(&self) -> Result<BTreeMap<TileId, u64>, String> {
        let copy = self.dir.as_ref().map(|dir| dir.join("tiles.txt"));
        match self.inner.index() {
            Ok(index) => {
                if let Some(copy) = &copy {
                    store(copy, offline::write_index(&index).as_bytes());
                }
                Ok(index)
            }
            Err(e) => copy
                .and_then(|copy| std::fs::read_to_string(copy).ok())
                .map(|text| offline::parse_index(&text))
                .filter(|index| !index.is_empty())
                .ok_or(e),
        }
    }
}

impl CachedTiles<FolderTiles> {
    /// The folder's manifest: its attribution goes next to every route.
    ///
    /// # Errors
    /// There is none, or it does not read.
    pub fn manifest(&self) -> Result<Manifest, String> {
        self.inner.manifest()
    }
}

/// The connection pool every download of this crate uses: rustls with the
/// pure-Rust crypto provider and the Mozilla roots, an HTTP error status
/// handed back as a response, 30 s at most per request.
#[cfg(feature = "http")]
pub(crate) fn http_agent() -> ureq::Agent {
    agent_trusting(ureq::tls::RootCerts::WebPki)
}

/// [`http_agent`] trusting exactly the roots in `pems` (PEM text, one or more
/// certificates each): the app's set, as azul's `TrustRoots::get_certificates_pem`
/// hands it over, so the downloads trust what the app's other connections trust.
///
/// # Errors
/// An entry that does not parse or holds no certificate, named by its place.
#[cfg(feature = "http")]
pub(crate) fn http_agent_trusting(pems: &[String]) -> Result<ureq::Agent, String> {
    let mut certs = Vec::new();
    for (i, pem) in pems.iter().enumerate() {
        let before = certs.len();
        for item in ureq::tls::parse_pem(pem.as_bytes()) {
            match item {
                Ok(ureq::tls::PemItem::Certificate(cert)) => certs.push(cert),
                Ok(_) => {}
                Err(e) => return Err(format!("trusted root {i} does not parse: {e}")),
            }
        }
        if certs.len() == before {
            return Err(format!("trusted root {i} holds no certificate"));
        }
    }
    Ok(agent_trusting(ureq::tls::RootCerts::Specific(Arc::new(certs))))
}

#[cfg(feature = "http")]
fn agent_trusting(roots: ureq::tls::RootCerts) -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::Rustls)
        .unversioned_rustls_crypto_provider(Arc::new(rustls_rustcrypto::provider()))
        .root_certs(roots)
        .build();
    let config = ureq::Agent::config_builder()
        .tls_config(tls)
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build();
    ureq::Agent::new_with_config(config)
}

/// Tiles downloaded from the hosting a manifest URL names (its tile
/// template is relative to it), e.g.
/// `https://example.github.io/azul-routing-tiles/berlin/manifest.json`, or
/// from a bare tile URL template with `{z}`, `{x}` and `{y}`. A 404 is a
/// tile without roads.
#[cfg(feature = "http")]
pub struct HttpTiles {
    /// What the routing value named: the manifest's URL, or a tile URL
    /// template.
    pub url: String,
    /// What the requests name themselves: the app and its version, nothing
    /// about the user.
    pub user_agent: String,
    /// The manifest once read, read on the first tile: the tile template
    /// comes from it. Stays `None` for a bare template.
    manifest: Mutex<Option<Manifest>>,
    /// The connection pool, shared by the fetch threads.
    agent: ureq::Agent,
    /// Why the roots it was told to trust did not load ([`HttpTiles::trusting`]): every
    /// download fails with it.
    trust_error: Option<String>,
}

#[cfg(feature = "http")]
impl HttpTiles {
    /// The largest tile it reads, bytes: a city's zoom 14 tile is well under.
    const MAX_TILE: u64 = 32 * 1024 * 1024;

    /// Downloads from `url` (a manifest's URL or a tile template), naming
    /// itself `user_agent`. Nothing is fetched before the first tile.
    #[must_use]
    pub fn new(url: impl Into<String>, user_agent: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            user_agent: user_agent.into(),
            manifest: Mutex::new(None),
            agent: http_agent(),
            trust_error: None,
        }
    }

    /// The same downloads trusting exactly the roots in `pems` (PEM text: the app's
    /// set) instead of the Mozilla roots. A root that does not parse fails every
    /// download, saying why: they never fall back to the Mozilla roots.
    #[must_use]
    pub fn trusting(mut self, pems: &[String]) -> Self {
        match http_agent_trusting(pems) {
            Ok(agent) => self.agent = agent,
            Err(why) => self.trust_error = Some(why),
        }
        self
    }

    /// The URL of `tile`.
    ///
    /// # Errors
    /// The manifest could not be had.
    pub fn url(&self, tile: TileId) -> Result<String, String> {
        if !crate::manifest::is_manifest(&self.url) {
            return Ok(fill(&self.url, tile));
        }
        self.with_manifest(|m| m.tile_url(&self.url, tile))
    }

    /// The manifest, read once.
    ///
    /// # Errors
    /// The URL is a bare tile template, or the manifest could not be had.
    pub fn manifest(&self) -> Result<Manifest, String> {
        if !crate::manifest::is_manifest(&self.url) {
            return Err(format!("{}: a tile template has no manifest", self.url));
        }
        self.with_manifest(Manifest::clone)
    }

    /// `f` of the manifest, reading it first if it is not read yet. The
    /// lock is held while it is read, so the fetch threads wait for one
    /// download of it instead of each starting their own.
    fn with_manifest<T>(&self, f: impl FnOnce(&Manifest) -> T) -> Result<T, String> {
        let mut slot = self.manifest.lock().map_err(|_| "the manifest's lock is poisoned".to_string())?;
        if slot.is_none() {
            let bytes = self.get(&self.url)?.ok_or_else(|| format!("{}: no manifest there", self.url))?;
            *slot = Some(Manifest::parse(&String::from_utf8_lossy(&bytes))?);
        }
        slot.as_ref().map(f).ok_or_else(|| "no manifest".to_string())
    }

    /// The hosting's tile list (`tiles.txt`, which the manifest names):
    /// every routing tile there is and its size.
    ///
    /// # Errors
    /// The manifest could not be had, names no list, or the list is not
    /// there.
    pub fn index(&self) -> Result<BTreeMap<TileId, u64>, String> {
        let url = self
            .with_manifest(|m| m.index_url(&self.url))?
            .ok_or_else(|| format!("{}: the tile set lists no tiles", self.url))?;
        let bytes = self.get(&url)?.ok_or_else(|| format!("{url}: no tile list there"))?;
        Ok(offline::parse_index(&String::from_utf8_lossy(&bytes)))
    }

    /// The bytes at `url`; `None` on a 404.
    fn get(&self, url: &str) -> Result<Option<Vec<u8>>, String> {
        if let Some(why) = &self.trust_error {
            return Err(format!("{url}: {why}"));
        }
        let response = self
            .agent
            .get(url)
            .header("User-Agent", self.user_agent.as_str())
            .call()
            .map_err(|e| format!("{url}: {e}"))?;
        match response.status().as_u16() {
            200 => response
                .into_body()
                .into_with_config()
                .limit(Self::MAX_TILE)
                .read_to_vec()
                .map(Some)
                .map_err(|e| format!("{url}: {e}")),
            404 => Ok(None),
            status => Err(format!("{url}: HTTP {status}")),
        }
    }
}

#[cfg(feature = "http")]
impl TileSource for HttpTiles {
    fn fetch(&self, tile: TileId) -> Result<Option<Vec<u8>>, String> {
        let url = self.url(tile)?;
        self.get(&url)
    }
}
