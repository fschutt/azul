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
    // Now the writer swaps. The pack the sweep retired is on its way out, so the writer does not
    // take it for its own (a collection could delete it the moment the swap lands, AZ-M9): its
    // objects go up again under a name of their own, and the manifest names that.
    publish(&mut laptop, "c1");
    let named = laptop.state().packs[0].name.clone();
    assert_ne!(
        named, sealed.name,
        "another name than the pack on its way out"
    );
    assert_eq!(
        laptop.collect_garbage(&guard, 0).unwrap(),
        2,
        "the orphan goes"
    );
    assert!(!present(&bucket, &keys::pack(&sealed.name)));
    assert!(
        present(&bucket, &keys::pack(&named)),
        "the writer's pack stays"
    );
    assert!(laptop.manifest().unwrap().retired.is_empty());
    let phone = MetaStore::open(bucket, sealer(), "phone").unwrap();
    let mut objects = Objects::new();
    phone
        .fetch_pack(&phone.state().packs[0].clone(), &mut objects)
        .unwrap();
    assert_eq!(objects.len(), 1);
}

/// AZ-M9. A writer comes back after a long stall and republishes the very objects of a pack its
/// own crash left behind: the same objects make the same name. A sweep has retired that pack and
/// a collection is deleting it. The writer used to find the pack there (its create answered
/// "exists"), take it for its own and name it - and the delete that was already under way removed
/// a pack the manifest names.
#[test]
fn az_m9_a_writer_republishing_a_pack_that_is_being_deleted_is_not_left_with_a_manifest_naming_nothing(
) {
    let bucket = MemoryBucket::new();
    let mut laptop = MetaStore::create(bucket.clone(), sealer(), "laptop").unwrap();
    let sealed = pack_of("c1").seal(&sealer()).unwrap();
    bucket
        .create(&keys::pack(&sealed.name), &sealed.pack)
        .unwrap();
    bucket
        .create(&keys::idx(&sealed.name), &sealed.idx)
        .unwrap();
    let guard = laptop.acquire_lease(MAINTENANCE, 600).unwrap();
    assert_eq!(laptop.sweep_orphans(&guard, 0).unwrap(), 2);

    // Just before the collection deletes the first of the two, the writer comes back.
    let mut writer = MetaStore::open(bucket.clone(), sealer(), "writer").unwrap();
    let comes_back: Box<dyn FnOnce() + Send> = Box::new(move || publish(&mut writer, "c1"));
    let once = std::sync::Arc::new(std::sync::Mutex::new(Some(comes_back)));
    for key in [keys::pack(&sealed.name), keys::idx(&sealed.name)] {
        let once = std::sync::Arc::clone(&once);
        bucket.before_next_remove(&key, move || {
            if let Some(run) = once.lock().unwrap().take() {
                run();
            }
        });
    }
    assert_eq!(laptop.collect_garbage(&guard, 0).unwrap(), 2);

    // A device that opens the repository afterwards finds every pack its manifest names.
    let phone = MetaStore::open(bucket.clone(), sealer(), "phone").unwrap();
    assert_eq!(phone.state().packs.len(), 1);
    let mut objects = Objects::new();
    phone
        .fetch_pack(&phone.state().packs[0].clone(), &mut objects)
        .expect("the manifest names a pack that is in the bucket");
    assert_eq!(objects.len(), 1);
}

/// AZ-M9. A delete the storage does not take (here: the pack, once) leaves its name listed as
/// retired for the next round, where the collection used to drop every due name from the list
/// before it deleted - and an object it could not delete was nobody's to find again until a sweep
/// listed the whole folder.
#[test]
fn az_m9_an_object_the_storage_would_not_delete_stays_listed_as_retired_for_the_next_round() {
    use crate::{meta::MetaError, DriveError};

    let bucket = MemoryBucket::new();
    let mut laptop = MetaStore::create(bucket.clone(), sealer(), "laptop").unwrap();
    let sealed = pack_of("c1").seal(&sealer()).unwrap();
    bucket
        .create(&keys::pack(&sealed.name), &sealed.pack)
        .unwrap();
    bucket
        .create(&keys::idx(&sealed.name), &sealed.idx)
        .unwrap();
    let guard = laptop.acquire_lease(MAINTENANCE, 600).unwrap();
    assert_eq!(laptop.sweep_orphans(&guard, 0).unwrap(), 2);

    bucket.fail_next_write(
        &keys::pack(&sealed.name),
        MetaError::Drive(DriveError::Transport("no answer".to_string())),
    );
    assert_eq!(laptop.collect_garbage(&guard, 0).unwrap(), 2);
    assert!(bucket.read(&keys::pack(&sealed.name)).unwrap().is_some());
    assert!(bucket.read(&keys::idx(&sealed.name)).unwrap().is_none());
    let listed: Vec<String> = laptop
        .manifest()
        .unwrap()
        .retired
        .iter()
        .map(|retired| retired.key.clone())
        .collect();
    assert_eq!(
        listed,
        [keys::pack(&sealed.name)],
        "the pack stays listed for the next round"
    );

    // The next round deletes it and the list is empty.
    assert_eq!(laptop.collect_garbage(&guard, 0).unwrap(), 1);
    assert!(bucket.read(&keys::pack(&sealed.name)).unwrap().is_none());
    assert!(laptop.manifest().unwrap().retired.is_empty());
}

#[test]
fn the_sweep_needs_the_lease() {
    let bucket = MemoryBucket::new();
    let mut laptop = MetaStore::create(bucket, sealer(), "laptop").unwrap();
    let guard = laptop.acquire_lease(MAINTENANCE, 600).unwrap();
    laptop.release_lease(guard.clone()).unwrap();
    assert!(laptop.sweep_orphans(&guard, 0).is_err());
}
