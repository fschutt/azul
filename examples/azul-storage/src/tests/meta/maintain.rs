//! A maintenance round: under the lease, the orphan sweep, compaction, a checkpoint and the
//! garbage collection, each when it is due.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use crate::{
    meta::{
        keys, wal::MAINTENANCE, Bucket, Kind, Maintained, Maintenance, MemoryBucket, MetaError,
        MetaStore, Objects, PackWriter, Publish, RefUpdate, TestSealer, COMPACT_PACK_MAX,
    },
    DriveError,
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
        ..Maintenance::default()
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

// ==== AZ-M5: a round that holds on, keeps its lease, and folds only what a frozen drive takes ====

/// About `n` hex digits, different for every `i`, that a pack does not squeeze to nothing.
fn noise(i: usize, n: usize) -> String {
    let mut out = String::new();
    let mut k = 0;
    while out.len() < n {
        out.push_str(&blake3::hash(format!("{i}-{k}").as_bytes()).to_hex());
        k += 1;
    }
    out.truncate(n);
    out
}

/// The sealed objects of the repository under `prefix`.
fn objects_under(bucket: &MemoryBucket, prefix: &str) -> usize {
    bucket
        .objects()
        .into_iter()
        .filter(|(key, _)| key.starts_with(prefix))
        .count()
}

/// AZ-M5. A compaction that fails (here: the swap that would make it final is not answered) used to
/// end the round with its `?`, so the checkpoint and the collection never ran for a drive whose
/// compaction kept failing. They run; the round still says that the compaction failed.
#[test]
fn az_m5_a_compaction_that_fails_does_not_keep_the_checkpoint_and_the_collection_from_running() {
    let bucket = MemoryBucket::new();
    let mut laptop = MetaStore::create(bucket.clone(), sealer(), "laptop").unwrap();
    for i in 1..=5 {
        publish(&mut laptop, &format!("c{i}"));
    }
    bucket.fail_next_write(
        keys::MANIFEST,
        MetaError::Drive(DriveError::Transport("no answer".to_string())),
    );
    let error = laptop.maintain(&rules()).unwrap_err();
    assert!(matches!(error, MetaError::Drive(_)), "{error:?}");
    assert_eq!(
        objects_under(&bucket, keys::CHECKPOINT_DIR),
        1,
        "the checkpoint was written"
    );
    assert!(
        objects_under(&bucket, keys::LOG_DIR) <= 1,
        "and the log entries it covers were collected (the one the failed compaction left is an \
         orphan, retired by a later sweep)"
    );
    // The next round has nothing left that the failure kept back.
    let again = laptop.maintain(&rules()).unwrap().unwrap();
    assert!(again.compacted, "{again:?}");
}

/// AZ-M5. A round that takes longer than its lease extends the lease before the steps that hold
/// it: here the clock is twenty minutes on when the compaction's swap lands, and the collection
/// that follows used to meet an expired lease and fail the round.
#[test]
fn az_m5_a_round_that_outlasts_its_lease_extends_it_and_still_collects() {
    let bucket = MemoryBucket::new();
    let time = Arc::new(AtomicU64::new(1_000));
    let read = Arc::clone(&time);
    let mut laptop = MetaStore::create(bucket.clone(), sealer(), "laptop")
        .unwrap()
        .with_clock(move || read.load(Ordering::SeqCst));
    for i in 1..=5 {
        publish(&mut laptop, &format!("c{i}"));
    }
    let late = Arc::clone(&time);
    bucket.before_next_replace(keys::MANIFEST, move || {
        late.fetch_add(1_200, Ordering::SeqCst);
    });
    let done = laptop.maintain(&rules()).unwrap().unwrap();
    assert!(done.compacted, "{done:?}");
    assert!(done.collected > 0, "{done:?}");
    // The lease was given back: another device takes it at once.
    let mut phone = MetaStore::open(bucket, sealer(), "phone").unwrap();
    assert!(phone.acquire_lease(MAINTENANCE, 60).is_ok());
}

/// AZ-M5. A compaction folds the smallest packs whose bytes fit the size it is given and leaves a
/// pack that is bigger alone: the pack it makes is one a frozen drive's node still takes, and
/// every object is still in some pack.
#[test]
fn az_m5_a_compaction_folds_only_the_packs_that_fit_the_size_a_frozen_drive_takes() {
    let bucket = MemoryBucket::new();
    let mut laptop = MetaStore::create(bucket.clone(), sealer(), "laptop").unwrap();
    for i in 1..=5 {
        publish(&mut laptop, &noise(i, 20_000));
    }
    let biggest = laptop
        .state()
        .packs
        .iter()
        .map(|p| p.pack_size)
        .max()
        .unwrap();
    // Room for two of these packs, not for three.
    let cap = biggest * 2 + biggest / 2;
    let guard = laptop.acquire_lease(MAINTENANCE, 600).unwrap();

    let first = laptop
        .compact_within(&guard, cap)
        .unwrap()
        .expect("two fit");
    assert_eq!(first.replaced.len(), 2);
    assert!(
        first.pack.pack_size <= cap,
        "{} > {cap}",
        first.pack.pack_size
    );
    assert_eq!(laptop.state().packs.len(), 4);
    let second = laptop
        .compact_within(&guard, cap)
        .unwrap()
        .expect("two more fit");
    assert_eq!(second.replaced.len(), 2);
    assert_eq!(laptop.state().packs.len(), 3);
    // Two folded packs and a single one: the smallest two together are more than `cap`.
    assert_eq!(laptop.compact_within(&guard, cap).unwrap(), None);
    assert!(laptop.state().packs.iter().all(|p| p.pack_size <= cap));

    let phone = MetaStore::open(bucket, sealer(), "phone").unwrap();
    let mut all = Objects::new();
    for pack in &phone.state().packs.clone() {
        phone.fetch_pack(pack, &mut all).unwrap();
    }
    assert_eq!(all.len(), 5, "every object is in a pack");
}

/// AZ-M5. The round's rule says how big a pack its compaction may make, and the default is below
/// what the contract lets a frozen drive's node take in one PUT.
#[test]
fn az_m5_a_round_compacts_within_the_size_its_rule_names() {
    const _: () = assert!(COMPACT_PACK_MAX < azlin_contract::INDEX_OBJECT_MAX);
    assert_eq!(Maintenance::default().compact_max_bytes, COMPACT_PACK_MAX);
    let bucket = MemoryBucket::new();
    let mut laptop = MetaStore::create(bucket.clone(), sealer(), "laptop").unwrap();
    for i in 1..=5 {
        publish(&mut laptop, &noise(i, 20_000));
    }
    let biggest = laptop
        .state()
        .packs
        .iter()
        .map(|p| p.pack_size)
        .max()
        .unwrap();
    let rules = Maintenance {
        compact_max_bytes: biggest * 2 + biggest / 2,
        ..rules()
    };
    let done = laptop.maintain(&rules).unwrap().unwrap();
    assert!(done.compacted);
    assert_eq!(
        laptop.state().packs.len(),
        4,
        "two of the five packs were folded, the others stay: {done:?}"
    );
}
