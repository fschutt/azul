//! The online services of a session - the routing server, the public transport server and
//! its place search - and the one cache of their answers in the data folder
//! (`maps/cache/`, capped, the least recently used answers going first). Every worker gets
//! them from here, so a question is asked once per session and once more only when its
//! answer grew stale; offline (`--offline`, or no network) the cache answers, marked so.

use std::{path::Path, sync::Arc};

use azul_routing::{cache::ResponseCache, transit::transitous::Transitous};

/// How big the cache of answers may grow on disk.
pub const CACHE_BYTES: u64 = 128 * 1024 * 1024;

/// How many answers the session keeps in memory.
pub const MEMORY_ANSWERS: usize = 512;

/// The online services.
pub struct Services {
    /// The answers of every service, in memory and in `maps/cache/`.
    pub cache: Arc<ResponseCache>,
    /// The public transport server and place search; `None` with `--transit off`.
    pub transit: Option<Arc<Transitous>>,
    /// Nothing is sent (`--offline`).
    pub offline: bool,
}

impl Services {
    /// The services of a session whose data folder is `data_root`.
    #[must_use]
    pub fn new(data_root: &Path, transit_url: Option<&str>, offline: bool) -> Self {
        let cache = Arc::new(ResponseCache::new(
            Some(data_root.join("maps").join("cache")),
            MEMORY_ANSWERS,
            CACHE_BYTES,
        ));
        let transit = transit_url.map(|url| {
            let server = Transitous::new(url, &crate::route::user_agent(), Arc::clone(&cache))
                .trusting(&azul_appkit::ui::trusted_roots_pem());
            server.fetcher().set_offline(offline);
            Arc::new(server)
        });
        Self {
            cache,
            transit,
            offline,
        }
    }
}
