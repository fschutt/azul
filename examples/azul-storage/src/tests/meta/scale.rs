//! C4's measure: a folder of 100,000 files. It is stored in 256 hidden shards, one change in it
//! writes four small objects instead of a 5 MB tree, and a device lists it page by page from
//! its copy without one List request to the bucket.

use crate::meta::{
    shard::is_sharded,
    tree::{apply, entry_at, folder_at},
    Change, Kind, ObjectId, Objects,
};

const FILES: usize = 100_000;

/// 100,000 pointer-sized files in `photos/`.
fn big_folder(objects: &mut Objects, blob: impl Fn(usize) -> Vec<u8>) -> ObjectId {
    let changes: Vec<Change> = (0..FILES)
        .map(|i| Change::Put {
            path: format!("photos/{i:06}.jpg"),
            id: objects.write_blob(&blob(i)),
        })
        .collect();
    apply(objects, None, &changes).unwrap()
}

/// The size git stores for the tree `id`.
fn tree_bytes(objects: &Objects, id: &ObjectId) -> usize {
    let (kind, body) = objects.get(id).unwrap();
    assert_eq!(kind, Kind::Tree);
    body.len()
}

#[test]
fn a_folder_of_100000_files_is_256_shards_and_one_change_writes_four_small_objects() {
    let mut objects = Objects::new();
    let root = big_folder(&mut objects, |i| format!("pointer {i}\n").into_bytes());
    let photos = entry_at(&objects, &root, "photos").unwrap().unwrap();
    let raw = objects.tree(&photos.id).unwrap();
    assert!(is_sharded(&raw));
    assert_eq!(raw.entries().len(), 256);
    let largest = raw
        .entries()
        .iter()
        .map(|shard| objects.tree(&shard.id).unwrap().entries().len())
        .max()
        .unwrap();
    // ~390 a shard on average.
    assert!(largest < 600, "the largest shard holds {largest}");
    assert_eq!(
        folder_at(&objects, &root, "photos").unwrap().unwrap().entries().len(),
        FILES
    );
    assert!(entry_at(&objects, &root, "photos/054321.jpg").unwrap().is_some());

    let before = objects.len();
    let blob = objects.write_blob(b"a new pointer\n");
    let next = apply(
        &mut objects,
        Some(&root),
        &[Change::Put {
            path: "photos/new.jpg".to_string(),
            id: blob,
        }],
    )
    .unwrap();
    // The blob, one shard, the folder's shard list, the root.
    assert_eq!(objects.len(), before + 4);
    let photos = entry_at(&objects, &next, "photos").unwrap().unwrap();
    let shard_list = tree_bytes(&objects, &photos.id);
    let shard = objects
        .tree(&photos.id)
        .unwrap()
        .entries()
        .iter()
        .map(|s| tree_bytes(&objects, &s.id))
        .max()
        .unwrap();
    // A whole tree of 100,000 entries would be ~5 MB; a change writes well under 64 KiB.
    assert!(shard_list + shard + tree_bytes(&objects, &next) < 64 * 1024);
}

/// A device lists 100,000 files from its copy, page by page, and never lists the bucket.
#[cfg(feature = "encryption")]
#[test]
fn a_device_browses_100000_files_without_listing_the_bucket() {
    use crate::{
        crypto::DriveKey,
        encrypted::{IndexEntry, NameIndex},
        meta::{merge::keep_both, pointer, MemoryBucket, MetaIndex, MetaRepo},
        ListRequest,
    };

    let bucket = MemoryBucket::new();
    let key = DriveKey::generate().unwrap();
    let mut laptop = MetaRepo::create(bucket.clone(), key.clone(), "laptop", "Laptop").unwrap();
    let changes: Vec<Change> = (0..FILES)
        .map(|i| Change::Put {
            path: format!("photos/{i:06}.jpg"),
            id: laptop.write_blob(&pointer::encode(&IndexEntry {
                size: i as u64,
                modified: Some(1_760_000_000),
                object: None,
            })),
        })
        .collect();
    laptop.commit(&changes, "import", &mut keep_both).unwrap();

    let phone = MetaRepo::open(bucket.clone(), key, "phone", "Phone").unwrap();
    let index = MetaIndex::new(phone);
    let first = index
        .list(&ListRequest::folder("photos/").with_max_keys(1000))
        .unwrap();
    assert_eq!(first.entries.len(), 1000);
    let next = first.next.clone().unwrap();
    let second = index
        .list(
            &ListRequest::folder("photos/")
                .with_max_keys(1000)
                .with_continuation(next.clone()),
        )
        .unwrap();
    assert_eq!(second.entries.len(), 1000);
    assert!(second.entries[0].0 > next);
    assert_eq!(
        index.get("photos/054321.jpg").unwrap().map(|e| e.size),
        Some(54_321)
    );
    assert_eq!(bucket.counts().lists, 0);
}
