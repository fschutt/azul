//! A maintenance round: under the lease, the orphan sweep, compaction, a checkpoint and the
//! garbage collection, each when it is due.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use crate::meta::{
    keys, wal::MAINTENANCE, Bucket, Kind, Maintained, Maintenance, MemoryBucket, MetaStore,
    PackWriter, Publish, RefUpdate, TestSealer,
};

const MAIN: &str = "refs/heads/main";

fn sealer() -> TestSealer {
    TestSealer::new([91; 32])
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

fn rules() -> Maintenance {
    Maintenance {
        lease_secs: 600,
        compact_at_packs: 4,
        checkpoint_at_entries: 3,
        orphan_age: 0,
        grace: 0,
    }
}

#[test]
fn a_maintenance_round_sweeps_compacts_checkpoints_and_collects_what_is_due() {
    let bucket = MemoryBucket::new();
    let time = Arc::new(AtomicU64::new(1_000));
    let read = Arc::clone(&time);
    let mut laptop = MetaStore::create(bucket.clone(), sealer(), "laptop")
        .unwrap()
        .with_clock(move || read.load(Ordering::SeqCst));
    for i in 1..=5 {
        publish(&mut laptop, &format!("c{i}"));
    }
    // A crashed writer's pack.
    let sealed = pack_of("never published").seal(&sealer()).unwrap();
    bucket.create(&keys::pack(&sealed.name), &sealed.pack).unwrap();
    bucket.create(&keys::idx(&sealed.name), &sealed.idx).unwrap();

    let done = laptop.maintain(&rules()).unwrap().unwrap();
    assert_eq!(done.swept, 2);
    assert!(done.compacted);
    assert!(done.checkpointed);
    // The orphan's 2, the five old packs' 10, the six log entries (five pushes, the compaction).
    assert_eq!(done.collected, 18, "{done:?}");
    let left: Vec<String> = bucket
        .objects()
        .into_iter()
        .map(|(key, _)| key)
        .filter(|key| key.starts_with(keys::WAL_DIR) || key.starts_with(keys::LOG_DIR))
        .collect();
    assert_eq!(left.len(), 2, "the compacted pack and its index: {left:?}");
    let phone = MetaStore::open(bucket.clone(), sealer(), "phone").unwrap();
    assert_eq!(phone.state().refs.get(MAIN).map(String::as_str), Some("c5"));
    assert_eq!(phone.state().packs.len(), 1);

    // Nothing is due any more.
    assert_eq!(laptop.maintain(&rules()).unwrap(), Some(Maintained::default()));
}

#[test]
fn a_maintenance_round_leaves_the_work_to_the_device_that_holds_the_lease() {
    let bucket = MemoryBucket::new();
    let mut laptop = MetaStore::create(bucket.clone(), sealer(), "laptop").unwrap();
    for i in 1..=5 {
        publish(&mut laptop, &format!("c{i}"));
    }
    let mut phone = MetaStore::open(bucket, sealer(), "phone").unwrap();
    let guard = phone.acquire_lease(MAINTENANCE, 600).unwrap();
    assert_eq!(laptop.maintain(&rules()).unwrap(), None);
    assert_eq!(laptop.state().packs.len(), 5);
    phone.release_lease(guard).unwrap();
    assert!(laptop.maintain(&rules()).unwrap().is_some());
}
