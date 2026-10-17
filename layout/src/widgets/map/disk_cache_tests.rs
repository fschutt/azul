//! The map's tiles on disk: kept as long as the server says they are fresh,
//! revalidated when they are not, never kept when the server forbids it, and
//! the folder held under its cap by dropping the least recently used.

use std::{
    path::PathBuf,
    time::{Duration, SystemTime},
};

use super::disk_cache::{cache_policy, parse_http_date, CachePolicy, TileDiskCache};

/// A fresh, empty folder of this test's own.
fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("azul-map-tiles-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn at(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
}

fn policy(fresh_until: u64, etag: Option<&str>) -> CachePolicy {
    CachePolicy {
        store: true,
        fresh_until,
        etag: etag.map(str::to_string),
        last_modified: None,
    }
}

const URL: &str = "https://tiles.invalid/planet/1/0/0/0.pbf";

#[test]
fn an_http_date_is_read_as_seconds_since_1970() {
    assert_eq!(parse_http_date("Sun, 06 Nov 1994 08:49:37 GMT"), Some(784_111_777));
    assert_eq!(parse_http_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
    assert_eq!(parse_http_date("0"), None);
    assert_eq!(parse_http_date("Sun, 06 Foo 1994 08:49:37 GMT"), None);
}

#[test]
fn a_max_age_keeps_a_tile_fresh_for_that_long_less_the_age_it_already_has() {
    // What OpenFreeMap sends: two Cache-Control headers, an Age, a weak ETag.
    let headers = [
        ("Cache-Control", "max-age=86400"),
        ("cache-control", "public"),
        ("Age", "858"),
        ("ETag", "W/\"6acb8f05-bb74\""),
        ("Last-Modified", "Sun, 11 Oct 2026 13:28:37 GMT"),
        ("Expires", "Thu, 01 Jan 1970 00:00:00 GMT"),
    ];
    let p = cache_policy(headers.iter().copied(), 1_000_000);
    assert!(p.store);
    assert_eq!(p.fresh_until, 1_000_000 + 86_400 - 858, "max-age wins over Expires");
    assert_eq!(p.etag.as_deref(), Some("W/\"6acb8f05-bb74\""));
    assert_eq!(p.last_modified.as_deref(), Some("Sun, 11 Oct 2026 13:28:37 GMT"));
}

#[test]
fn no_store_keeps_nothing_and_no_cache_asks_the_server_every_time() {
    let p = cache_policy([("Cache-Control", "no-store, max-age=600")].iter().copied(), 50);
    assert!(!p.store);
    let p = cache_policy([("Cache-Control", "no-cache, max-age=600")].iter().copied(), 50);
    assert!(p.store, "kept, to be revalidated");
    assert_eq!(p.fresh_until, 50, "but never fresh");
}

#[test]
fn expires_counts_from_the_servers_date_and_last_modified_alone_gives_a_tenth_of_its_age() {
    let p = cache_policy(
        [
            ("Date", "Wed, 14 Oct 2026 19:49:55 GMT"),
            ("Expires", "Thu, 15 Oct 2026 19:49:55 GMT"),
        ]
        .iter()
        .copied(),
        7,
    );
    assert_eq!(p.fresh_until, 7 + 86_400, "a day after the server's clock, on ours");
    let p = cache_policy(
        [
            ("Date", "Wed, 14 Oct 2026 00:00:00 GMT"),
            ("Last-Modified", "Sun, 04 Oct 2026 00:00:00 GMT"),
        ]
        .iter()
        .copied(),
        7,
    );
    assert_eq!(p.fresh_until, 7 + 86_400, "a tenth of ten days");
    let p = cache_policy(
        [
            ("Date", "Wed, 14 Oct 2026 00:00:00 GMT"),
            ("Last-Modified", "Thu, 01 Jan 2015 00:00:00 GMT"),
        ]
        .iter()
        .copied(),
        7,
    );
    assert_eq!(p.fresh_until, 7 + 7 * 86_400, "at most a week");
    let p = cache_policy(core::iter::empty(), 7);
    assert_eq!(p.fresh_until, 7, "nothing said: stale at once, revalidated");
}

#[test]
fn an_empty_folder_name_means_no_disk_cache() {
    assert!(TileDiskCache::open("", 1 << 20).is_none());
}

#[test]
fn a_stored_tile_comes_back_fresh_until_its_time_and_stale_with_its_validators_after() {
    let dir = folder("fresh");
    let cache = TileDiskCache::open(dir.to_str().expect("utf-8 temp dir"), 1 << 20).expect("open");
    cache.store_at(URL, &[1, 2, 3], &policy(2_000, Some("\"v1\"")), at(1_000));
    let hit = cache.lookup_at(URL, 1_999, at(1_001)).expect("stored");
    assert!(hit.fresh);
    assert_eq!(hit.bytes, vec![1, 2, 3]);
    let stale = cache.lookup_at(URL, 2_000, at(1_002)).expect("still stored");
    assert!(!stale.fresh);
    assert_eq!(stale.etag.as_deref(), Some("\"v1\""));
    assert!(
        cache
            .lookup_at("https://tiles.invalid/planet/1/0/0/1.pbf", 0, at(1_003))
            .is_none(),
        "another tile's url never reads this file"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_304_renews_a_stale_tile_and_keeps_its_bytes_and_its_etag() {
    let dir = folder("renew");
    let cache = TileDiskCache::open(dir.to_str().expect("utf-8 temp dir"), 1 << 20).expect("open");
    cache.store_at(URL, &[9, 9], &policy(100, Some("\"v1\"")), at(10));
    assert!(cache.renew_at(URL, &policy(5_000, None), at(20)));
    let hit = cache.lookup_at(URL, 4_999, at(21)).expect("renewed");
    assert!(hit.fresh);
    assert_eq!(hit.bytes, vec![9, 9]);
    assert_eq!(hit.etag.as_deref(), Some("\"v1\""), "a 304 without an ETag keeps the old one");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_no_store_answer_removes_what_was_kept() {
    let dir = folder("nostore");
    let cache = TileDiskCache::open(dir.to_str().expect("utf-8 temp dir"), 1 << 20).expect("open");
    cache.store_at(URL, &[1], &policy(100, None), at(10));
    let forbidden = CachePolicy {
        store: false,
        ..policy(0, None)
    };
    cache.store_at(URL, &[2], &forbidden, at(11));
    assert!(cache.lookup_at(URL, 0, at(12)).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_folder_stays_under_its_cap_by_dropping_the_least_recently_used_tiles() {
    let dir = folder("lru");
    let cache = TileDiskCache::open(dir.to_str().expect("utf-8 temp dir"), 3_500).expect("open");
    let url = |n: u32| format!("https://tiles.invalid/planet/1/3/{n}/0.pbf");
    let body = vec![7u8; 1024];
    cache.store_at(&url(1), &body, &policy(10_000, None), at(1));
    cache.store_at(&url(2), &body, &policy(10_000, None), at(2));
    cache.store_at(&url(3), &body, &policy(10_000, None), at(3));
    // The first tile is looked at again: it is the most recently used now.
    assert!(cache.lookup_at(&url(1), 0, at(4)).is_some());
    cache.store_at(&url(4), &body, &policy(10_000, None), at(5));
    let kept = |n: u32| cache.lookup_at(&url(n), 0, at(6)).is_some();
    assert!(kept(1), "used last but one: kept");
    assert!(kept(4), "just stored: kept");
    assert!(!kept(2), "the least recently used goes first");
    assert!(!kept(3), "then the next, until the folder is under 90% of its cap");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_fetch_of_a_map_carries_the_disk_cache_its_setup_names() {
    use azul_css::AzString;

    use super::{
        autotest_generated::cache_at, tile_fetch_init, MapSetup, MapTileId, TileStyleKey,
    };

    let mut cache = cache_at(0.0, 0.0, 4.0);
    let look = cache.current_look();
    let key = TileStyleKey {
        tile: MapTileId { z: 4, x: 8, y: 8 },
        look,
    };
    let none = tile_fetch_init(&cache, key);
    assert!(none.cache_dir.as_str().is_empty(), "no folder named: memory only");
    cache.setup = MapSetup::new().with_disk_cache(AzString::from("/data/maps/tiles"), 64 << 20);
    let init = tile_fetch_init(&cache, key);
    assert_eq!(init.cache_dir.as_str(), "/data/maps/tiles");
    assert_eq!(init.cache_max_bytes, 64 << 20);
    assert_eq!(init.url.as_str(), "https://tiles.invalid/4/8/8.pbf");
}
