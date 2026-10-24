//! The drive side of a synced folder: prefixes, the index's and the blobs' keys, the entries a
//! run never acts on.

use crate::sync::remote::{
    blob_key, index_key, is_hash, normalize_prefix, RemoteFile, RemoteIndex, Tombstone,
    INDEX_VERSION,
};

fn h(c: char) -> String {
    std::iter::repeat(c).take(64).collect()
}

#[test]
fn a_prefix_has_one_trailing_slash_and_never_climbs_out() {
    assert_eq!(normalize_prefix("").unwrap(), "");
    assert_eq!(normalize_prefix("/").unwrap(), "");
    assert_eq!(normalize_prefix("azlin/data").unwrap(), "azlin/data/");
    assert_eq!(normalize_prefix("/azlin/data/").unwrap(), "azlin/data/");
    for bad in ["../x", "a/../b", "a\\b", "x/.azlin/", ".azlin"] {
        assert!(normalize_prefix(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn the_index_and_the_blobs_live_in_the_prefixes_bookkeeping_folder() {
    assert_eq!(index_key("azlin/data/"), "azlin/data/.azlin/index.json");
    assert_eq!(index_key(""), ".azlin/index.json");
    let hash = h('a');
    assert_eq!(blob_key("p/", &hash), format!("p/.azlin/blobs/aa/{hash}"));
    assert!(is_hash(&hash));
    assert!(!is_hash(&h('A')));
    assert!(!is_hash("abc"));
}

#[test]
fn an_index_round_trips_and_its_entries_that_break_a_rule_are_named_never_dropped() {
    let mut index = RemoteIndex::empty();
    index.generation = 3;
    index.files.insert(
        String::from("notes/a.md"),
        RemoteFile {
            hash: h('1'),
            size: 5,
            mtime: 1,
            gen: 3,
            device: String::from("dev-a"),
            exec: false,
        },
    );
    index.deleted.insert(
        String::from("notes/old.md"),
        Tombstone {
            hash: h('2'),
            gen: 2,
            at: 9,
            device: String::from("dev-b"),
            confirmed: false,
        },
    );
    let back = RemoteIndex::parse(&index.to_bytes()).unwrap();
    assert_eq!(back, index);
    assert!(back.has_blob(&h('1')));
    assert!(back.has_blob(&h('2')), "a tombstone keeps its blob");
    assert!(!back.has_blob(&h('3')));

    // M1, D6: another device's or a share participant's bad entry is named, kept as it is
    // (a run that rewrites the index keeps it) and never acted on; the rest of the index is read.
    assert!(back.unreadable().is_empty());
    for bad in [
        "../../etc/passwd",
        "/etc/passwd",
        ".azlin/index.json",
        "a\\b",
        "C:x",
        "tab\there",
    ] {
        let mut evil = index.clone();
        evil.files
            .insert(bad.to_string(), evil.files["notes/a.md"].clone());
        let read = RemoteIndex::parse(&evil.to_bytes()).unwrap();
        let unreadable = read.unreadable();
        assert!(unreadable.contains_key(bad), "{bad:?}: {unreadable:?}");
        assert_eq!(unreadable.len(), 1, "{bad:?}");
        let readable = read.readable(&unreadable);
        assert!(readable.files.contains_key("notes/a.md"));
        assert!(!readable.files.contains_key(bad));
        assert!(read.files.contains_key(bad), "the entry is kept in the index");
    }
    let mut damaged = index.clone();
    damaged.files.get_mut("notes/a.md").unwrap().hash = String::from("zz");
    let read = RemoteIndex::parse(&damaged.to_bytes()).unwrap();
    assert!(read.unreadable().contains_key("notes/a.md"));
    // D43: a newer version, a feature this code does not know, fields it does not know - the
    // index is read and what it does not know named (the run leaves such a drive as it is and
    // asks to be updated, `sync_format.rs`).
    assert!(index.unknown_features().is_empty());
    let mut newer = index.clone();
    newer.version = INDEX_VERSION + 1;
    assert_eq!(
        RemoteIndex::parse(&newer.to_bytes())
            .unwrap()
            .unknown_features(),
        vec![format!("version {}", INDEX_VERSION + 1)]
    );
    let mut flagged = index.clone();
    flagged.features = vec![String::from("chunked-files")];
    assert_eq!(
        RemoteIndex::parse(&flagged.to_bytes())
            .unwrap()
            .unknown_features(),
        vec![String::from("chunked-files")]
    );
    let mut value = serde_json::to_value(&flagged).unwrap();
    value["chunks"] = serde_json::json!({ "notes/a.md": ["x"] });
    assert!(RemoteIndex::parse(value.to_string().as_bytes()).is_ok());
    let mut zero = index;
    zero.version = 0;
    assert!(
        RemoteIndex::parse(&zero.to_bytes()).is_err(),
        "version 0 is no index"
    );
}
