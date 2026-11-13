//! MVT tile decode + projection helpers for `MapWidget`. Opt-in via the
//! `map-tiles` Cargo feature so default builds don't pay for the
//! `td` / `mvt-reader` / `proj4rs` dep tree.
//!
//! Architecture (see `MOBILE_SESSION_LOG.md` for the full design and
//! the user's "MVT + MapCSS = SVG → DOM" pipeline):
//!
//! 1. `MapWidget`'s `VirtualView` callback computes the visible tiles (Web Mercator XYZ).
//! 2. For each visible tile not in cache, the widget enqueues a fetch. Fetch lands in a follow-up
//!    tick — needs a Thread/async surface.
//! 3. When the fetched PBF bytes arrive, `decode_mvt_tile` parses them via the `td` crate
//!    (`td::parse_mvt_tile`), returning a `Vec` of GeoJSON `Feature`s with WGS-84 coordinates.
//! 4. The next tick maps each Feature → SVG path string by applying the user's `MapCSS` stylesheet
//!    (parsed via the framework's existing CSS parser).
//! 5. The widget's `VirtualView` patches the SVG-as-DOM into the tile `<div>` as a child.
//!
//! This module is the entry point for step 3 only. Steps 1-2 are
//! `MapWidget` internals; steps 4-5 land in later ticks.

#[cfg(feature = "map-tiles")]
mod mvt;
#[cfg(feature = "map-tiles")]
pub use mvt::{parse_mvt_tile, TileCoord};

#[cfg(feature = "map-tiles")]
mod svg;
#[cfg(feature = "map-tiles")]
pub use svg::{features_to_svg, features_to_svg_in};

/// Decode the PBF bytes of a single MVT tile into a `Vec` of GeoJSON
/// `Feature`s. Wraps `td::parse_mvt_tile` with the tile-coord
/// conversion from `azul_layout::widgets::map::MapTileId`.
///
/// Returns an error string when the `map-tiles` feature is disabled or
/// when `td` fails to parse the bytes. Callers shouldn't trust the
/// returned features to be in any particular order — `mvt-reader` walks
/// the layers as it finds them.
#[cfg(feature = "map-tiles")]
pub fn decode_mvt_tile(
    bytes: alloc::vec::Vec<u8>,
    tile: azul_layout::widgets::map::MapTileId,
) -> Result<alloc::vec::Vec<geojson::Feature>, alloc::string::String> {
    let coord = TileCoord {
        z: tile.z as u32,
        x: tile.x,
        y: tile.y,
    };
    mvt::parse_mvt_tile(bytes, &coord).map_err(|e| alloc::format!("{e:?}"))
}

/// Build the `https://host/{z}/{x}/{y}.pbf` URL for a tile, expanding
/// the same `{z}` / `{x}` / `{y}` placeholders Leaflet uses. The
/// substitution is hand-rolled instead of going through
/// `td::tile_coords_to_urls` because we need the result for a single
/// tile, not a batch. Always safe — never returns an error.
pub fn build_tile_url(
    url_template: &str,
    tile: azul_layout::widgets::map::MapTileId,
) -> String {
    use alloc::string::ToString;
    url_template
        .replace("{z}", &tile.z.to_string())
        .replace("{x}", &tile.x.to_string())
        .replace("{y}", &tile.y.to_string())
}

/// Stub used when the `map-tiles` feature is off — decode is a no-op
/// returning an empty `Vec`. Lets the `MapWidget` cache state machine
/// compile and run without dragging in the MVT dep tree.
#[cfg(not(feature = "map-tiles"))]
pub fn decode_mvt_tile(
    _bytes: Vec<u8>,
    _tile: azul_layout::widgets::map::MapTileId,
) -> Result<Vec<()>, String> {
    Err(String::from(
        "azul-dll built without `map-tiles` feature — MVT decode unavailable",
    ))
}

/// The tile-fetch worker thread. [`ensure_map_tile_fetcher`] registers it, a
/// map picks it up when it mounts and runs it once per visible tile, on a
/// thread of its own or on the `ThreadPool` from the map's `MapSetup`.
///
/// Flow (all on the background thread, blocking is fine):
/// 1. Read `TileFetchInit { tile, url, client, cache_dir, .. }` from the init `RefAny`.
/// 2. Find the tile's bytes: the cache's own when it handed them over (a
///    restyle, no I/O at all), else the map's disk cache when it holds the
///    tile fresh (no request), else a GET of `url` through `client`. A tile
///    the disk holds stale is drawn from the disk at once and then asked
///    about (`If-None-Match` / `If-Modified-Since`): a `304` only renews the
///    file, a `200` is drawn again. Offline, the stale tile stays.
/// 3. `decode_mvt_tile(bytes, tile)` → GeoJSON features.
/// 4. `features_to_svg(&features, tile)` → SVG string.
/// 5. `draw_tile_svg(&svg)` → the tile DRAWN: its geometry rasterised, its
///    labels parsed. This is the expensive step, so it runs here and not in
///    the map's render.
/// 6. `sender.send(ThreadReceiveMsg::WriteBack(...))` a `TileReadyMsg` pointed at
///    `azul_layout::widgets::map::map_tile_writeback`, which files the drawing and
///    re-renders the map's view in place.
///
/// Cancellation: before the fetch, between the fetch and the decode and
/// between the decode and the draw we poll `recv.recv()` for
/// `ThreadSendMsg::TerminateThread`, so a job whose `Thread` was dropped while
/// it waited in a pool's queue never downloads, and one dropped mid-download
/// doesn't waste a decode or a draw.
///
/// With `AZ_MAP_STATS` (AzMaps' `--stats`) every tile prints
/// `AZ_MAP_TILE <z>/<x>/<y> <fetch_ms> <decode_ms> <draw_ms> <bytes> <source>`
/// from here, `<source>` saying where the bytes came from: `memory` (a
/// restyle), `disk` (fresh on disk, no request), `stale` (from the disk, a
/// request follows), `net` (downloaded); a failed fetch prints `- - 0 failed`.
/// A revalidation that the server answers with `304` prints
/// `AZ_MAP_TILE_REVALIDATED <z>/<x>/<y> <ms>`.
#[cfg(feature = "map-tiles")]
pub extern "C" fn tile_fetch_worker(
    mut init: azul_core::refany::RefAny,
    mut sender: azul_layout::thread::ThreadSender,
    mut recv: azul_core::task::ThreadReceiver,
) {
    use azul_core::refany::{OptionRefAny, RefAny};
    use azul_css::AzString;
    use azul_layout::{
        http::{HttpRequestConfig, HttpResponse, OptionHttpClient},
        thread::{ThreadReceiveMsg, ThreadWriteBackMsg, WriteBackCallback},
        widgets::map::{
            disk_cache::{cache_policy, unix_now, TileDiskCache},
            draw_tile_svg, map_stats_enabled, map_tile_writeback, DrawnTile, TileFetchInit,
            TileReadyMsg,
        },
    };

    fn millis(since: std::time::Instant) -> f64 {
        since.elapsed().as_secs_f64() * 1000.0
    }

    let Some((tile, url, mapcss, look, cached_bytes, client, disk, language)) =
        init.downcast_ref::<TileFetchInit>().map(|i| {
            (
                i.tile,
                i.url.as_str().to_string(),
                i.style_css.as_str().to_string(),
                i.look,
                i.bytes.as_ref().to_vec(),
                i.client.clone(),
                TileDiskCache::open(i.cache_dir.as_str(), i.cache_max_bytes),
                i.language.as_str().to_string(),
            )
        })
    else {
        return;
    };

    // Whether the map has let go of this job: a `TerminateThread` ANYWHERE in
    // the queue - the UI thread sends a `Tick` every frame, and reading one
    // message only saw the first of those.
    let mut cancelled = || loop {
        match recv.recv().into_option() {
            Some(azul_core::task::ThreadSendMsg::TerminateThread) => break true,
            Some(_) => continue,
            None => break false,
        }
    };

    // A job that waited in a thread pool's queue may belong to a map that has
    // since gone away.
    if cancelled() {
        return;
    }

    // `bytes` travels back to the main thread only when it did not have them
    // (a restyle sends nothing back). A drawn tile sends no SVG: the drawing
    // is all the render needs.
    let send_back = |sender: &mut azul_layout::thread::ThreadSender,
                     error: AzString,
                     bytes: Vec<u8>,
                     drawn: Option<DrawnTile>| {
        sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
            WriteBackCallback {
                cb: map_tile_writeback,
                ctx: OptionRefAny::None,
            },
            RefAny::new(TileReadyMsg {
                tile,
                svg: AzString::from(""),
                error,
                look,
                bytes: azul_css::U8Vec::from_vec(bytes),
                drawn,
            }),
        )));
    };

    let dbg = std::env::var("AZ_MAP_DEBUG").is_ok();
    let (z, x, y) = (tile.z, tile.x, tile.y);
    let fail = |sender: &mut azul_layout::thread::ThreadSender, why: String, since| {
        if dbg {
            eprintln!("[map] worker FAILED tile=({z},{x},{y}): {why}");
        }
        if map_stats_enabled() {
            println!("AZ_MAP_TILE {z}/{x}/{y} {:.1} - - 0 failed", millis(since));
        }
        send_back(sender, AzString::from(why), Vec::new(), None);
    };

    // Decode, style and draw `bytes`, and send the drawing back; `false` when
    // the map let go of the job on the way.
    let finish = |sender: &mut azul_layout::thread::ThreadSender,
                      cancelled: &mut dyn FnMut() -> bool,
                      bytes: Vec<u8>,
                      give_back: bool,
                      source: &str,
                      fetch_ms: f64|
     -> bool {
        if cancelled() {
            return false;
        }
        let returned = if give_back { bytes.clone() } else { Vec::new() };
        let payload_len = bytes.len();
        let decode_started = std::time::Instant::now();
        let svg = match decode_mvt_tile(bytes, tile) {
            Ok(features) => {
                let svg = features_to_svg_in(&features, tile, &mapcss, &language);
                if dbg {
                    eprintln!(
                        "[map] worker decoded tile=({z},{x},{y}) look={look:?} {} features \
                         svg_len={} from {source}",
                        features.len(),
                        svg.len()
                    );
                }
                svg
            }
            Err(e) => {
                if dbg {
                    eprintln!("[map] worker decode FAILED tile=({z},{x},{y}): {e}");
                }
                send_back(
                    sender,
                    AzString::from(alloc::format!("decode failed: {e}")),
                    returned,
                    None,
                );
                return true;
            }
        };
        let decode_ms = millis(decode_started);
        // The draw (the rasterisation) is the most expensive step of all.
        if cancelled() {
            return false;
        }
        let draw_started = std::time::Instant::now();
        let drawn = draw_tile_svg(&svg);
        let draw_ms = millis(draw_started);
        if dbg {
            eprintln!(
                "[map] worker drew tile=({z},{x},{y}) image={} labels={} in {draw_ms:.1} ms",
                drawn.image.is_some(),
                drawn.labels.len()
            );
        }
        if map_stats_enabled() {
            println!(
                "AZ_MAP_TILE {z}/{x}/{y} {fetch_ms:.1} {decode_ms:.1} {draw_ms:.1} {payload_len} \
                 {source}"
            );
        }
        send_back(sender, AzString::from(""), returned, Some(drawn));
        true
    };

    // A restyle: the geometry is in hand, only the palette changed.
    if !cached_bytes.is_empty() {
        if dbg {
            eprintln!(
                "[map] worker restyle tile=({z},{x},{y}) look={look:?} - {} cached bytes, no fetch",
                cached_bytes.len()
            );
        }
        finish(&mut sender, &mut cancelled, cached_bytes, false, "memory", 0.0);
        return;
    }

    let get = |validators: &[(String, String)]| {
        let mut config = match &client {
            OptionHttpClient::Some(client) => {
                HttpRequestConfig::default().with_client(client.clone())
            }
            OptionHttpClient::None => HttpRequestConfig::default(),
        };
        for (name, value) in validators {
            config = config.with_header(name.clone(), value.clone());
        }
        azul_layout::http::http_get_with_config(&url, &config)
    };
    let policy_of = |resp: &HttpResponse| {
        cache_policy(
            resp.headers
                .as_ref()
                .iter()
                .map(|h| (h.name.as_str(), h.value.as_str())),
            unix_now(),
        )
    };
    // 200, or 204 for a tile with nothing in it.
    let is_tile = |resp: &HttpResponse| matches!(resp.status_code, 200 | 204);

    let held = disk.as_ref().and_then(|d| d.lookup(&url, unix_now()));
    let fetch_started = std::time::Instant::now();
    match held {
        // Fresh on disk: no request at all.
        Some(found) if found.fresh => {
            if dbg {
                eprintln!("[map] worker disk hit tile=({z},{x},{y}) fresh, no request");
            }
            finish(
                &mut sender,
                &mut cancelled,
                found.bytes,
                true,
                "disk",
                millis(fetch_started),
            );
        }
        // Stale on disk: shown at once, then asked about.
        Some(found) => {
            let validators = found.revalidation_headers();
            if !finish(
                &mut sender,
                &mut cancelled,
                found.bytes,
                true,
                "stale",
                millis(fetch_started),
            ) {
                return;
            }
            let asked = std::time::Instant::now();
            match get(&validators) {
                Ok(resp) if resp.status_code == 304 => {
                    if let Some(disk) = &disk {
                        // A tile the folder lost meanwhile is fetched again next time.
                        let _ = disk.renew(&url, &policy_of(&resp));
                    }
                    if dbg {
                        eprintln!("[map] worker revalidated tile=({z},{x},{y}): unchanged");
                    }
                    if map_stats_enabled() {
                        println!("AZ_MAP_TILE_REVALIDATED {z}/{x}/{y} {:.1}", millis(asked));
                    }
                }
                Ok(resp) if is_tile(&resp) => {
                    if let Some(disk) = &disk {
                        disk.store(&url, resp.body.as_ref(), &policy_of(&resp));
                    }
                    let body = resp.body.as_ref().to_vec();
                    finish(&mut sender, &mut cancelled, body, true, "net", millis(asked));
                }
                // An error answer or no network: the stale tile stays shown.
                Ok(resp) => {
                    if dbg {
                        eprintln!(
                            "[map] worker revalidation of tile=({z},{x},{y}) got HTTP {}, the \
                             stale tile stays",
                            resp.status_code
                        );
                    }
                }
                Err(e) => {
                    if dbg {
                        eprintln!(
                            "[map] worker revalidation of tile=({z},{x},{y}) failed ({e:?}), \
                             the stale tile stays"
                        );
                    }
                }
            }
        }
        None => match get(&[]) {
            Ok(resp) if is_tile(&resp) => {
                if let Some(disk) = &disk {
                    disk.store(&url, resp.body.as_ref(), &policy_of(&resp));
                }
                if dbg {
                    eprintln!(
                        "[map] worker fetched tile=({z},{x},{y}) {} bytes",
                        resp.body.as_ref().len()
                    );
                }
                let body = resp.body.as_ref().to_vec();
                finish(
                    &mut sender,
                    &mut cancelled,
                    body,
                    true,
                    "net",
                    millis(fetch_started),
                );
            }
            Ok(resp) => fail(
                &mut sender,
                alloc::format!("fetch failed: HTTP {}", resp.status_code),
                fetch_started,
            ),
            Err(e) => fail(
                &mut sender,
                alloc::format!("fetch failed: {e:?}"),
                fetch_started,
            ),
        },
    }
}

/// Install the built-in tile-fetch worker as the framework-owned fetcher every
/// map picks up when it mounts, once. Called from the shared per-frame layout
/// pass (like the file-picker backend) so it is in place before any map mounts,
/// and again defensively from [`map_widget_dom`].
pub fn ensure_map_tile_fetcher() {
    #[cfg(feature = "map-tiles")]
    {
        static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        DONE.get_or_init(|| {
            let _ = azul_layout::widgets::map::register_map_tile_fetcher(
                azul_layout::thread::ThreadCallback {
                    cb: tile_fetch_worker,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
            );
        });
    }
}

/// Build the `MapWidget`'s rendered `Dom`, making sure the built-in tile-fetch
/// worker is registered for the map to pick up when it mounts.
///
/// This is the single entry point the FFI `MapWidget::dom()` shims to (see
/// api.json). The worker (`tile_fetch_worker`) lives here in `azul-dll` — NOT in
/// `azul-layout::widgets::map` — because it pulls the MVT/Mercator dep tree
/// (`mvt-reader`, `geo-types`, `proj4rs`, `geojson`) that we deliberately keep out
/// of the layout crate's (mobile) build. The layout crate only knows the registered
/// `ThreadCallback`: the map installs it into its tile cache on `AfterMount`, the
/// cache's merge callback keeps it across relayout, and it is freed when the cache
/// `RefAny` drops.
///
/// When the `map-tiles` feature is off the worker doesn't exist, so the map renders
/// placeholders — keeping default/mobile builds free of the dep tree.
pub fn map_widget_dom(widget: azul_layout::widgets::map::MapWidget) -> azul_core::dom::Dom {
    #[cfg(feature = "map-tiles")]
    {
        ensure_map_tile_fetcher();
        widget.dom()
    }
    #[cfg(not(feature = "map-tiles"))]
    {
        // The widget renders its placeholder DOM and NO tile will ever load —
        // indistinguishable from a slow network unless we say so.
        static ANNOUNCE: std::sync::Once = std::sync::Once::new();
        ANNOUNCE.call_once(|| {
            eprintln!(
                "[azul][map] MapWidget: this build has no `map-tiles` feature — no tile fetch \
                 worker exists, the map shows its placeholder forever. Rebuild with: cargo build \
                 -p azul-dll --features build-dll,map-tiles"
            );
        });
        widget.dom()
    }
}
