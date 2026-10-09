use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc, Arc,
};

use super::TempDir;
use crate::meta::{
    keys, wal::MAINTENANCE, Bucket, FolderBucket, Kind, MemoryBucket, MetaError, MetaStore, Mode,
    Objects, PackWriter, Publish, Published, RefUpdate, TestSealer, Tree, TreeEntry,
};

const KEY: [u8; 32] = [11; 32];
const MAIN: &str = "refs/heads/main";

fn sealer() -> TestSealer {
    TestSealer::new(KEY)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// A pack with one blob.
fn pack_of(content: &str) -> PackWriter {
    let mut writer = PackWriter::new();
    writer.add(Kind::Blob, content.as_bytes().to_vec());
    writer
}

/// Moves `name` to `value` from whatever it is, with a pack of one blob.
fn set_ref<B: Bucket, S: crate::meta::Sealer>(
    store: &mut MetaStore<B, S>,
    name: &str,
    value: &str,
) -> Published {
    store
        .publish(|state| {
            Ok(Some(Publish {
                pack: Some(pack_of(value)),
                updates: vec![RefUpdate {
                    name: name.to_string(),
                    old: state.refs.get(name).cloned(),
                    new: Some(value.to_string()),
                }],
                message: format!("set {name}"),
            }))
        })
        .unwrap()
        .unwrap()
}

/// A clock the test moves.
fn clock(start: u64) -> (Arc<AtomicU64>, impl Fn() -> u64 + Send + Sync + 'static) {
    let time = Arc::new(AtomicU64::new(start));
    let read = Arc::clone(&time);
    (time, move || read.load(Ordering::SeqCst))
}

#[test]
fn a_new_repository_is_one_sealed_manifest_and_cannot_be_created_twice() {
    let bucket = MemoryBucket::new();
    let store = MetaStore::create(bucket.clone(), sealer(), "laptop").unwrap();
    assert_eq!(store.state().head_seq, 0);
    let objects = bucket.objects();
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0].0, keys::MANIFEST);
    assert!(matches!(
        MetaStore::create(bucket.clone(), sealer(), "phone"),
        Err(MetaError::RepositoryExists)
    ));
    let opened = MetaStore::open(bucket, sealer(), "phone").unwrap();
    assert_eq!(opened.state(), store.state());
}

#[test]
fn opening_a_bucket_without_a_repository_says_so() {
    assert!(matches!(
        MetaStore::open(MemoryBucket::new(), sealer(), "laptop"),
        Err(MetaError::NoRepository)
    ));
    let bucket = MemoryBucket::new();
    let created = MetaStore::open_or_create(bucket.clone(), sealer(), "laptop").unwrap();
    assert_eq!(created.state().revision, 1);
    let opened = MetaStore::open_or_create(bucket, sealer(), "phone").unwrap();
    assert_eq!(opened.state().revision, 1);
}

#[test]
fn a_repository_does_not_open_with_another_drive_key() {
    let bucket = MemoryBucket::new();
    MetaStore::create(bucket.clone(), sealer(), "laptop").unwrap();
    assert!(matches!(
        MetaStore::open(bucket, TestSealer::new([12; 32]), "thief"),
        Err(MetaError::Sealed { .. })
    ));
}

#[test]
fn a_poll_without_changes_is_one_conditional_read_answered_not_modified() {
    let bucket = MemoryBucket::new();
    let mut store = MetaStore::create(bucket.clone(), sealer(), "laptop").unwrap();
    set_ref(&mut store, MAIN, "c1");
    let before = bucket.counts();
    let report = store.sync().unwrap();
    let after = bucket.counts();
    assert!(!report.changed);
    assert!(report.entries.is_empty());
    assert_eq!(after.conditional_reads - before.conditional_reads, 1);
    assert_eq!(after.not_modified - before.not_modified, 1);
    assert_eq!(after.reads, before.reads);
    assert_eq!(after.range_reads, before.range_reads);
    assert_eq!(after.lists, 0);
    assert_eq!(after.writes, before.writes);
}

#[test]
fn a_publish_reaches_another_device_on_its_next_poll() {
    let bucket = MemoryBucket::new();
    let mut laptop = MetaStore::create(bucket.clone(), sealer(), "laptop").unwrap();
    let mut phone = MetaStore::open(bucket.clone(), sealer(), "phone").unwrap();
    let published = set_ref(&mut laptop, MAIN, "c1");
    assert_eq!(published.seq, 1);
    assert_eq!(published.attempts, 1);
    let report = phone.sync().unwrap();
    assert!(report.changed);
    assert_eq!(report.entries.len(), 1);
    assert_eq!(report.entries[0].writer, "laptop");
    assert_eq!(phone.state(), laptop.state());
    assert_eq!(phone.state().refs.get(MAIN).map(String::as_str), Some("c1"));
    // The pack is there for the phone to read.
    let mut objects = Objects::new();
    phone
        .fetch_pack(&phone.state().packs[0].clone(), &mut objects)
        .unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(bucket.counts().lists, 0);
}

#[test]
fn of_two_devices_publishing_at_once_exactly_one_wins_the_swap_and_the_other_merges_and_retries() {
    let bucket = MemoryBucket::new();
    let mut laptop = MetaStore::create(bucket.clone(), sealer(), "laptop").unwrap();
    let phone = MetaStore::open(bucket.clone(), sealer(), "phone").unwrap();

    // The phone publishes just before the laptop's swap.
    let (send_back, phone_back) = mpsc::channel();
    bucket.before_next_replace(keys::MANIFEST, move || {
        let mut phone = phone;
        let published = set_ref(&mut phone, MAIN, "phone-1");
        send_back.send((phone, published)).unwrap();
    });

    let mut seen = Vec::new();
    let published = laptop
        .publish(|state| {
            let base = state.refs.get(MAIN).cloned();
            seen.push(base.clone());
            // "Merging": the laptop's change on top of whatever the main ref is now.
            let new = match &base {
                None => "laptop-1".to_string(),
                Some(theirs) => format!("merge(laptop-1, {theirs})"),
            };
            Ok(Some(Publish {
                pack: Some(pack_of(&new)),
                updates: vec![RefUpdate {
                    name: MAIN.to_string(),
                    old: base,
                    new: Some(new),
                }],
                message: "laptop".to_string(),
            }))
        })
        .unwrap()
        .unwrap();
    let (mut phone, phone_published) = phone_back.recv().unwrap();

    assert_eq!(phone_published.seq, 1);
    assert_eq!(phone_published.attempts, 1);
    assert_eq!(published.seq, 2);
    assert_eq!(published.attempts, 2);
    assert_eq!(seen, vec![None, Some("phone-1".to_string())]);
    assert_eq!(bucket.counts().conflicts, 1);

    phone.sync().unwrap();
    assert_eq!(phone.state(), laptop.state());
    assert_eq!(
        laptop.state().refs.get(MAIN).map(String::as_str),
        Some("merge(laptop-1, phone-1)")
    );
    // The losing attempt's log entry is gone: one log object per entry.
    let logs = bucket
        .objects()
        .into_iter()
        .filter(|(key, _)| key.starts_with(keys::LOG_DIR))
        .count();
    assert_eq!(logs, 2);
}

/// Four devices on four threads, ten publishes each to their own ref: all land.
fn four_devices_publishing_at_once<B: Bucket + Clone + 'static>(bucket: B) {
    MetaStore::create(bucket.clone(), sealer(), "creator").unwrap();
    let threads: Vec<_> = (0..4)
        .map(|device| {
            let bucket = bucket.clone();
            std::thread::spawn(move || {
                let mut store = MetaStore::open(bucket, sealer(), &format!("device-{device}"))
                    .unwrap()
                    .with_attempts(200);
                let name = format!("refs/heads/device-{device}");
                for i in 1..=10 {
                    set_ref(&mut store, &name, &format!("{device}-{i}"));
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    let store = MetaStore::open(bucket, sealer(), "reader").unwrap();
    assert_eq!(store.state().head_seq, 40);
    assert_eq!(store.state().packs.len(), 40);
    for device in 0..4 {
        assert_eq!(
            store
                .state()
                .refs
                .get(&format!("refs/heads/device-{device}"))
                .cloned(),
            Some(format!("{device}-10"))
        );
    }
}

#[test]
fn four_devices_publishing_at_once_in_memory_all_land() {
    four_devices_publishing_at_once(MemoryBucket::new());
}

#[test]
fn four_devices_publishing_at_once_in_a_folder_all_land() {
    let dir = TempDir::new("meta-wal-race");
    four_devices_publishing_at_once(FolderBucket::new(dir.path()));
}

#[test]
fn a_checkpoint_plus_the_log_after_it_restores_the_state_without_the_older_log() {
    let bucket = MemoryBucket::new();
    let (_time, now) = clock(1_000);
    let mut laptop = MetaStore::create(bucket.clone(), sealer(), "laptop")
        .unwrap()
        .with_clock(now);
    let mut behind = MetaStore::open(bucket.clone(), sealer(), "old phone").unwrap();
    for i in 1..=2 {
        set_ref(&mut laptop, MAIN, &format!("c{i}"));
    }
    behind.sync().unwrap();
    assert_eq!(behind.state().head_seq, 2);
    for i in 3..=5 {
        set_ref(&mut laptop, MAIN, &format!("c{i}"));
    }
    set_ref(&mut laptop, "refs/heads/other", "x");
    let checkpoint = laptop.checkpoint().unwrap();
    assert_eq!(checkpoint.seq, 6);
    for i in 7..=9 {
        set_ref(&mut laptop, MAIN, &format!("c{i}"));
    }
    // The log up to the checkpoint is retired; collect it right away.
    let guard = laptop.acquire_lease(MAINTENANCE, 60).unwrap();
    assert_eq!(laptop.collect_garbage(&guard, 0).unwrap(), 6);
    laptop.release_lease(guard).unwrap();
    let logs: Vec<String> = bucket
        .objects()
        .into_iter()
        .map(|(key, _)| key)
        .filter(|key| key.starts_with(keys::LOG_DIR))
        .collect();
    assert_eq!(logs.len(), 3, "{logs:?}");

    // A new device: the checkpoint plus the three entries after it.
    let fresh = MetaStore::open(bucket.clone(), sealer(), "new phone").unwrap();
    assert_eq!(fresh.state(), laptop.state());
    // A device behind the checkpoint starts from it as well.
    let report = behind.sync().unwrap();
    assert!(report.from_checkpoint);
    assert_eq!(report.entries.len(), 3);
    assert_eq!(behind.state(), laptop.state());
    assert_eq!(behind.state().refs.get(MAIN).map(String::as_str), Some("c9"));
    assert_eq!(behind.state().refs.get("refs/heads/other").map(String::as_str), Some("x"));
}

#[test]
fn the_bucket_holds_no_name_no_path_and_no_git_id_in_plaintext() {
    let bucket = MemoryBucket::new();
    let mut laptop = MetaStore::create(bucket.clone(), sealer(), "Felix laptop").unwrap();
    let mut objects = Objects::new();
    let blob = objects.write_blob(b"object data/ab/0123456789abcdef\nsize 4711\n");
    let mut folder = Tree::new();
    folder
        .insert(TreeEntry {
            name: "beach.jpg".to_string(),
            mode: Mode::File,
            id: blob,
        })
        .unwrap();
    let folder_id = objects.write_tree(&folder);
    let mut root = Tree::new();
    root.insert(TreeEntry {
        name: "Holiday photos".to_string(),
        mode: Mode::Tree,
        id: folder_id,
    })
    .unwrap();
    let root_id = objects.write_tree(&root);
    let mut writer = PackWriter::new();
    for id in [blob, folder_id, root_id] {
        writer.add_from(&objects, &id).unwrap();
    }
    laptop
        .publish(|state| {
            Ok(Some(Publish {
                pack: Some(writer.clone()),
                updates: vec![RefUpdate {
                    name: MAIN.to_string(),
                    old: state.refs.get(MAIN).cloned(),
                    new: Some(root_id.to_hex()),
                }],
                message: "Added Holiday photos/beach.jpg".to_string(),
            }))
        })
        .unwrap()
        .unwrap();
    laptop.checkpoint().unwrap();
    let guard = laptop.acquire_lease(MAINTENANCE, 60).unwrap();

    let secrets: Vec<Vec<u8>> = vec![
        b"Holiday".to_vec(),
        b"beach".to_vec(),
        b"Felix".to_vec(),
        b"laptop".to_vec(),
        b"refs/heads".to_vec(),
        b"data/ab".to_vec(),
        b"size 4711".to_vec(),
        b"sha256".to_vec(),
        root_id.to_hex().into_bytes(),
        root_id.0.to_vec(),
        blob.to_hex()[..12].as_bytes().to_vec(),
        blob.0[..12].to_vec(),
    ];
    let all = bucket.objects();
    assert!(all.len() >= 6, "{} objects", all.len());
    for (key, bytes) in &all {
        assert!(key.starts_with(keys::ROOT), "{key}");
        for secret in &secrets {
            assert!(!contains(key.as_bytes(), secret), "{key} names {secret:?}");
            assert!(!contains(bytes, secret), "{key} holds {secret:?}");
        }
    }
    drop(guard);
}

#[test]
fn an_older_manifest_served_again_is_refused_as_a_rollback() {
    let bucket = MemoryBucket::new();
    let mut laptop = MetaStore::create(bucket.clone(), sealer(), "laptop").unwrap();
    let mut phone = MetaStore::open(bucket.clone(), sealer(), "phone").unwrap();
    set_ref(&mut laptop, MAIN, "c1");
    let old_manifest = bucket.read(keys::MANIFEST).unwrap().unwrap().0;
    set_ref(&mut laptop, MAIN, "c2");
    phone.sync().unwrap();
    bucket.overwrite(keys::MANIFEST, &old_manifest);
    assert!(matches!(
        phone.sync(),
        Err(MetaError::Rollback { seen: 3, served: 2 })
    ));
}

#[test]
fn a_log_entry_copied_over_another_does_not_open() {
    let bucket = MemoryBucket::new();
    let mut laptop = MetaStore::create(bucket.clone(), sealer(), "laptop").unwrap();
    let mut phone = MetaStore::open(bucket.clone(), sealer(), "phone").unwrap();
    set_ref(&mut laptop, MAIN, "c1");
    set_ref(&mut laptop, MAIN, "c2");
    let log = laptop.manifest().unwrap().log.clone();
    let first = bucket.read(&log[0].key).unwrap().unwrap().0;
    bucket.overwrite(&log[1].key, &first);
    assert!(matches!(phone.sync(), Err(MetaError::Sealed { .. })));
}

#[test]
fn a_publish_from_a_stale_ref_value_is_refused() {
    let bucket = MemoryBucket::new();
    let mut laptop = MetaStore::create(bucket, sealer(), "laptop").unwrap();
    set_ref(&mut laptop, MAIN, "c1");
    let result = laptop.publish(|_| {
        Ok(Some(Publish {
            pack: None,
            updates: vec![RefUpdate {
                name: MAIN.to_string(),
                old: None,
                new: Some("c2".to_string()),
            }],
            message: String::new(),
        }))
    });
    assert!(matches!(result, Err(MetaError::RefConflict { .. })));
    assert_eq!(laptop.state().head_seq, 1);
}

#[test]
fn a_publish_of_nothing_writes_nothing() {
    let bucket = MemoryBucket::new();
    let mut laptop = MetaStore::create(bucket.clone(), sealer(), "laptop").unwrap();
    let writes = bucket.counts().writes;
    assert_eq!(laptop.publish(|_| Ok(None)).unwrap(), None);
    assert_eq!(bucket.counts().writes, writes);
}

#[test]
fn a_lease_is_one_devices_until_it_expires_or_is_released() {
    let bucket = MemoryBucket::new();
    let (time, now) = clock(1_000);
    let read = Arc::clone(&time);
    let phone_now = move || read.load(Ordering::SeqCst);
    let mut laptop = MetaStore::create(bucket.clone(), sealer(), "laptop")
        .unwrap()
        .with_clock(now);
    let mut phone = MetaStore::open(bucket, sealer(), "phone")
        .unwrap()
        .with_clock(phone_now);

    let mut guard = laptop.acquire_lease(MAINTENANCE, 60).unwrap();
    assert_eq!(guard.expires_at, 1_060);
    match phone.acquire_lease(MAINTENANCE, 60) {
        Err(MetaError::LeaseHeld { holder, expires_at }) => {
            assert_eq!(holder, "laptop");
            assert_eq!(expires_at, 1_060);
        }
        other => panic!("{other:?}"),
    }
    // The holder may extend it while it holds it.
    time.store(1_050, Ordering::SeqCst);
    laptop.renew_lease(&mut guard, 60).unwrap();
    assert_eq!(guard.expires_at, 1_110);
    time.store(1_100, Ordering::SeqCst);
    assert!(phone.acquire_lease(MAINTENANCE, 60).is_err());

    // Expired: the phone takes it, and the laptop's renewal fails.
    time.store(1_111, Ordering::SeqCst);
    let phone_guard = phone.acquire_lease(MAINTENANCE, 60).unwrap();
    assert!(matches!(
        laptop.renew_lease(&mut guard, 60),
        Err(MetaError::LeaseHeld { .. })
    ));
    // Released: the laptop gets it at once.
    phone.release_lease(phone_guard).unwrap();
    assert!(laptop.acquire_lease(MAINTENANCE, 60).is_ok());
}

#[test]
fn compaction_folds_the_packs_into_one_and_garbage_collection_deletes_the_old_ones() {
    let bucket = MemoryBucket::new();
    let (time, now) = clock(1_000);
    let mut laptop = MetaStore::create(bucket.clone(), sealer(), "laptop")
        .unwrap()
        .with_clock(now);
    let mut phone = MetaStore::open(bucket.clone(), sealer(), "phone").unwrap();
    for i in 1..=4 {
        set_ref(&mut laptop, MAIN, &format!("c{i}"));
    }
    phone.sync().unwrap();
    let mut before = Objects::new();
    for pack in &phone.state().packs {
        phone.fetch_pack(pack, &mut before).unwrap();
    }
    assert_eq!(before.len(), 4);

    let guard = laptop.acquire_lease(MAINTENANCE, 600).unwrap();
    let compacted = laptop.compact(&guard).unwrap().unwrap();
    assert_eq!(compacted.replaced.len(), 4);
    assert_eq!(laptop.state().packs.len(), 1);
    assert_eq!(laptop.state().head_seq, 5);
    assert_eq!(laptop.state().refs.get(MAIN).map(String::as_str), Some("c4"));
    assert_eq!(laptop.compact(&guard).unwrap(), None);

    // Not yet: the old packs wait out the grace time.
    assert_eq!(laptop.collect_garbage(&guard, 300).unwrap(), 0);
    time.store(1_301, Ordering::SeqCst);
    assert_eq!(laptop.collect_garbage(&guard, 300).unwrap(), 8);
    let packs = bucket
        .objects()
        .into_iter()
        .filter(|(key, _)| key.starts_with(keys::WAL_DIR))
        .count();
    assert_eq!(packs, 2);

    // The phone reads the compacted pack: every object is still there.
    phone.sync().unwrap();
    assert_eq!(phone.state(), laptop.state());
    let mut after = Objects::new();
    phone
        .fetch_pack(&phone.state().packs[0].clone(), &mut after)
        .unwrap();
    assert_eq!(after.len(), 4);
    for id in before.ids() {
        assert!(after.contains(id));
    }
}

#[test]
fn maintenance_needs_a_lease_that_has_not_expired() {
    let bucket = MemoryBucket::new();
    let (time, now) = clock(1_000);
    let mut laptop = MetaStore::create(bucket, sealer(), "laptop")
        .unwrap()
        .with_clock(now);
    let guard = laptop.acquire_lease(MAINTENANCE, 10).unwrap();
    time.store(1_011, Ordering::SeqCst);
    assert!(matches!(
        laptop.compact(&guard),
        Err(MetaError::LeaseHeld { .. })
    ));
    assert!(matches!(
        laptop.collect_garbage(&guard, 0),
        Err(MetaError::LeaseHeld { .. })
    ));
}
