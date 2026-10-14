//! Where the routing tiles come from: a folder (what the generator wrote),
//! a download (`http` feature), and a cache in front of either - in memory
//! for the session and in a folder across sessions, so a second route
//! through the same area costs no network.

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{router::TileSource, tile::TileId};

/// The file of `tile` under a tile folder: `<z>/<x>/<y>.json.gz`.
#[must_use]
pub fn tile_file(root: &Path, tile: TileId) -> PathBuf {
    root.join(tile.z.to_string())
        .join(tile.x.to_string())
        .join(format!("{}.json.gz", tile.y))
}

/// Tiles from a folder as the generator writes it; a missing file is a
/// tile without roads.
#[derive(Debug, Clone)]
pub struct FolderTiles {
    /// The folder holding `<z>/<x>/<y>.json.gz`.
    pub root: PathBuf,
}

impl TileSource for FolderTiles {
    fn fetch(&self, tile: TileId) -> Result<Option<Vec<u8>>, String> {
        let path = tile_file(&self.root, tile);
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

/// How many tiles the memory cache keeps before it starts over: a long
/// trip's few hundred tiles a few times over.
const MEMORY_TILES: usize = 2048;

/// A cache in front of another source: in memory for the session, and in a
/// folder (`dir`) across sessions, capped at `max_disk_bytes` (the oldest
/// files go first, see [`CachedTiles::prune`]).
pub struct CachedTiles<S: TileSource> {
    /// Where the tiles come from when the cache does not have them.
    pub inner: S,
    /// The folder of the disk cache; `None` keeps the tiles in memory only.
    pub dir: Option<PathBuf>,
    /// How big the disk cache may grow, bytes.
    pub max_disk_bytes: u64,
    /// The tiles of this session, `None` for a tile without roads. Shared:
    /// the cache is read from all fetch threads at once.
    memory: Mutex<HashMap<TileId, Option<Arc<Vec<u8>>>>>,
}

impl<S: TileSource> CachedTiles<S> {
    /// A cache over `inner`, on disk in `dir` when there is one.
    pub fn new(inner: S, dir: Option<PathBuf>, max_disk_bytes: u64) -> Self {
        Self {
            inner,
            dir,
            max_disk_bytes,
            memory: Mutex::new(HashMap::new()),
        }
    }

    fn remember(&self, tile: TileId, bytes: Option<Arc<Vec<u8>>>) {
        if let Ok(mut memory) = self.memory.lock() {
            if memory.len() >= MEMORY_TILES {
                memory.clear();
            }
            memory.insert(tile, bytes);
        }
    }

    /// Deletes the oldest files of the disk cache until it holds at most
    /// `max_disk_bytes`. Returns how many bytes it holds after.
    pub fn prune(&self) -> u64 {
        let Some(dir) = &self.dir else {
            return 0;
        };
        let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = Vec::new();
        collect_files(dir, &mut files, 4);
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
fn collect_files(dir: &Path, out: &mut Vec<(std::time::SystemTime, u64, PathBuf)>, depth: usize) {
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
        if let Some(hit) = self.memory.lock().ok().and_then(|m| m.get(&tile).cloned()) {
            return Ok(hit.map(|bytes| bytes.as_ref().clone()));
        }
        if let Some(dir) = &self.dir {
            if let Ok(bytes) = std::fs::read(tile_file(dir, tile)) {
                let bytes = Arc::new(bytes);
                self.remember(tile, Some(Arc::clone(&bytes)));
                return Ok(Some(bytes.as_ref().clone()));
            }
            if none_file(dir, tile).exists() {
                self.remember(tile, None);
                return Ok(None);
            }
        }
        let fetched = self.inner.fetch(tile)?;
        if let Some(dir) = &self.dir {
            // A cache that cannot be written is only slower: the route goes on.
            let path = match &fetched {
                Some(_) => tile_file(dir, tile),
                None => none_file(dir, tile),
            };
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&path, fetched.as_deref().unwrap_or(&[]));
        }
        self.remember(tile, fetched.clone().map(Arc::new));
        Ok(fetched)
    }
}

/// Tiles downloaded from a URL template (`{z}`, `{x}`, `{y}`), e.g.
/// `https://example.org/routing/2026-10/{z}/{x}/{y}.json.gz`. A 404 is a
/// tile without roads.
#[cfg(feature = "http")]
pub struct HttpTiles {
    /// The URL of a tile, with `{z}`, `{x}` and `{y}` in it.
    pub template: String,
    /// What the requests name themselves: the app and its version, nothing
    /// about the user.
    pub user_agent: String,
    /// The connection pool, shared by the fetch threads.
    agent: ureq::Agent,
}

#[cfg(feature = "http")]
impl HttpTiles {
    /// The largest tile it reads, bytes: a city's zoom 14 tile is well under.
    const MAX_TILE: u64 = 32 * 1024 * 1024;

    /// Downloads from `template`, naming itself `user_agent`.
    #[must_use]
    pub fn new(template: impl Into<String>, user_agent: impl Into<String>) -> Self {
        let tls = ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::Rustls)
            .unversioned_rustls_crypto_provider(Arc::new(rustls_rustcrypto::provider()))
            .root_certs(ureq::tls::RootCerts::WebPki)
            .build();
        let config = ureq::Agent::config_builder()
            .tls_config(tls)
            .http_status_as_error(false)
            .timeout_global(Some(std::time::Duration::from_secs(30)))
            .build();
        Self {
            template: template.into(),
            user_agent: user_agent.into(),
            agent: ureq::Agent::new_with_config(config),
        }
    }

    /// The URL of `tile`.
    #[must_use]
    pub fn url(&self, tile: TileId) -> String {
        self.template
            .replace("{z}", &tile.z.to_string())
            .replace("{x}", &tile.x.to_string())
            .replace("{y}", &tile.y.to_string())
    }
}

#[cfg(feature = "http")]
impl TileSource for HttpTiles {
    fn fetch(&self, tile: TileId) -> Result<Option<Vec<u8>>, String> {
        let url = self.url(tile);
        let response = self
            .agent
            .get(&url)
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
