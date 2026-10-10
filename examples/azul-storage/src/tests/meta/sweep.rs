//! The orphan sweep: what no manifest names is retired, then collected.

use super::TempDir;
use crate::meta::{
    keys, wal::MAINTENANCE, Bucket, FolderBucket, Kind, MemoryBucket, MetaStore, Objects,
    PackWriter, Publish, RefUpdate, Sealer, TestSealer,
};

const MAIN: &str = "refs/heads/main";

fn sealer() -> TestSealer {
    TestSealer::new([51; 32])
}

fn pack_of(content: &str) -> PackWriter {
    let mut writer = PackWriter::new();
    writer.add(Kind::Blob, content.as_bytes().to_vec());
    writer
}

fn publish<B: Bucket>(store: &mut MetaStore<B, TestSealer>, value: &str) {
    store
        .publish(|state| {
            Ok(Some(Publish {
                pack: Some(pack_of(value)),
                updates: vec![RefUpdate {
                    name: MAIN.to_string(),
                    old: state.refs.get(MAIN).cloned(),
                    new: Some(value.to_string()),
                }],
                message: String::new(),
            }))
        })
        .unwrap()
        .unwrap();
}

/// Leaves what a crashed writer leaves: a pack and its index, a log entry, a
/// checkpoint, none of them named by the manifest.
fn leave_orphans<B: Bucket>(bucket: &B, sealer: &dyn Sealer) -> Vec<String> {
    let sealed = pack_of("never published").seal(sealer).unwrap();
    let orphans = vec![
        keys::pack(&sealed.name),
        keys::idx(&sealed.name),
        keys::log(7, "0123456789abcdef0123456789abcdef"),
        keys::checkpoint(3, "fedcba9876543210fedcba9876543210"),
    ];
    bucket.create(&orphans[0], &sealed.pack).unwrap();
    bucket.create(&orphans[1], &sealed.idx).unwrap();
    bucket.create(&orphans[2], b"sealed log entry").unwrap();
    bucket.create(&orphans[3], b"sealed checkpoint").unwrap();
    orphans
}

fn present<B: Bucket>(bucket: &B, key: &str) -> bool {
    bucket.read(key).unwrap().is_some()
}

fn sweep_and_collect<B: Bucket + Clone>(bucket: B) {
    let mut laptop = MetaStore::create(bucket.clone(), sealer(), "laptop").unwrap();
    publish(&mut laptop, "c1");
    let orphans = leave_orphans(&bucket, &sealer());
    publish(&mut laptop, "c2");

    let guard = laptop.acquire_lease(MAINTENANCE, 600).unwrap();
    assert_eq!(laptop.sweep_orphans(&guard, 0).unwrap(), orphans.len());
    // Retired, not yet deleted; a second sweep finds nothing new.
    assert!(orphans.iter().all(|key| present(&bucket, key)));
    assert_eq!(laptop.sweep_orphans(&guard, 0).unwrap(), 0);
    assert_eq!(laptop.collect_garbage(&guard, 0).unwrap(), orphans.len());
    assert!(orphans.iter().all(|key| !present(&bucket, key)));

    // What the manifest names is all there.
    let phone = MetaStore::open(bucket, sealer(), "phone").unwrap();
    assert_eq!(phone.state().refs.get(MAIN).map(String::as_str), Some("c2"));
    let mut objects = Objects::new();
    for pack in phone.state().packs.clone() {
        phone.fetch_pack(&pack, &mut objects).unwrap();
    }
    assert_eq!(objects.len(), 2);
}

#[test]
fn a_crashed_writers_objects_are_swept_and_collected_in_memory() {
    sweep_and_collect(MemoryBucket::new());
}

#[test]
fn a_crashed_writers_objects_are_swept_and_collected_in_a_folder() {
    let dir = TempDir::new("meta-sweep");
    sweep_and_collect(FolderBucket::new(dir.path()));
}

#[test]
fn a_pack_a_writer_names_after_the_sweep_is_kept() {
    let bucket = MemoryBucket::new();
    let mut laptop = MetaStore::create(bucket.clone(), sealer(), "laptop").unwrap();
    // A slow writer's pack, uploaded before its swap.
    let sealed = pack_of("c1").seal(&sealer()).unwrap();
    bucket.create(&keys::pack(&sealed.name), &sealed.pack).unwrap();
    bucket.create(&keys::idx(&sealed.name), &sealed.idx).unwrap();
    let guard = laptop.acquire_lease(MAINTENANCE, 600).unwrap();
    assert_eq!(laptop.sweep_orphans(&guard, 0).unwrap(), 2);
    // Now the writer swaps: its pack is there (same objects, same name).
    publish(&mut laptop, "c1");
    assert_eq!(laptop.collect_garbage(&guard, 0).unwrap(), 0);
    assert!(present(&bucket, &keys::pack(&sealed.name)));
    assert!(laptop.manifest().unwrap().retired.is_empty());
    let phone = MetaStore::open(bucket, sealer(), "phone").unwrap();
    let mut objects = Objects::new();
    phone
        .fetch_pack(&phone.state().packs[0].clone(), &mut objects)
        .unwrap();
    assert_eq!(objects.len(), 1);
}

#[test]
fn the_sweep_needs_the_lease() {
    let bucket = MemoryBucket::new();
    let mut laptop = MetaStore::create(bucket, sealer(), "laptop").unwrap();
    let guard = laptop.acquire_lease(MAINTENANCE, 600).unwrap();
    laptop.release_lease(guard.clone()).unwrap();
    assert!(laptop.sweep_orphans(&guard, 0).is_err());
}
