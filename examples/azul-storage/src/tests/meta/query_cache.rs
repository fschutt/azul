//! The drive index's local query cache (feature `index-cache`).

use super::TempDir;
use crate::{
    encrypted::IndexEntry,
    meta::{cache::QueryCache, merge::keep_both, pointer, Change, MemoryBucket, MetaRepo, TestSealer},
};

type Repo = MetaRepo<MemoryBucket, TestSealer>;

fn file(repo: &mut Repo, path: &str, size: u64, modified: u64) -> Change {
    let entry = IndexEntry {
        size,
        modified: Some(modified),
        object: None,
    };
    Change::Put {
        path: path.to_string(),
        id: repo.write_blob(&pointer::encode(&entry)),
    }
}

fn drive() -> Repo {
    let mut repo = Repo::create(MemoryBucket::new(), TestSealer::new([61; 32]), "laptop", "Laptop")
        .unwrap();
    let policy = Change::Put {
        path: ".azlin/policy.toml".to_string(),
        id: repo.write_blob(b"[members]\n"),
    };
    let changes = vec![
        file(&mut repo, "docs/Report 2026.docx", 40_000, 1_760_000_300),
        file(&mut repo, "docs/notes.txt", 120, 1_760_000_100),
        file(&mut repo, "photos/beach.jpg", 3_000_000, 1_760_000_200),
        file(&mut repo, "photos/old/report-scan.pdf", 900_000, 1_700_000_000),
        policy,
    ];
    repo.commit(&changes, "import", &mut keep_both).unwrap();
    repo
}

fn paths(entries: &[crate::meta::cache::CachedEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.path.as_str()).collect()
}

#[test]
fn the_cache_answers_search_largest_recent_and_totals() {
    let mut repo = drive();
    let dir = TempDir::new("meta-query-cache");
    let cache = QueryCache::open(&dir.path().join("index.db")).unwrap();
    assert!(cache.refresh(&mut repo).unwrap());
    assert_eq!(
        cache.built_from().unwrap(),
        repo.head().map(|h| h.to_hex())
    );

    assert_eq!(
        paths(&cache.search("REPORT", 10).unwrap()),
        ["docs/Report 2026.docx", "photos/old/report-scan.pdf"]
    );
    let folders = cache.search("old", 10).unwrap();
    assert_eq!(paths(&folders), ["photos/old"]);
    assert!(folders[0].is_folder);
    assert_eq!(folders[0].folder, "photos");
    assert_eq!(
        paths(&cache.largest(2).unwrap()),
        ["photos/beach.jpg", "photos/old/report-scan.pdf"]
    );
    let recent = cache.recent(1).unwrap();
    assert_eq!(paths(&recent), ["docs/Report 2026.docx"]);
    assert_eq!(recent[0].modified, Some(1_760_000_300));
    assert_eq!(recent[0].name, "Report 2026.docx");
    assert_eq!(cache.totals().unwrap(), (4, 3_940_120));
    // The drive's own folder is not in it.
    assert!(cache.search("policy", 10).unwrap().is_empty());
}

#[test]
fn the_cache_is_rebuilt_only_when_the_head_moved() {
    let mut repo = drive();
    let dir = TempDir::new("meta-query-cache-refresh");
    let path = dir.path().join("index.db");
    let cache = QueryCache::open(&path).unwrap();
    assert!(cache.refresh(&mut repo).unwrap());
    assert!(!cache.refresh(&mut repo).unwrap());

    let gone = Change::Delete {
        path: "photos/beach.jpg".to_string(),
    };
    repo.commit(&[gone], "delete", &mut keep_both).unwrap();
    assert!(cache.refresh(&mut repo).unwrap());
    assert_eq!(cache.totals().unwrap(), (3, 940_120));
    drop(cache);

    // It is a file: opened again, it still knows what it was built from.
    let again = QueryCache::open(&path).unwrap();
    assert!(!again.refresh(&mut repo).unwrap());
    assert_eq!(again.totals().unwrap(), (3, 940_120));
}
