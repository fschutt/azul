//! A key rotation's manifest swap whose answer is not the truth (X-DELTA-SYNC4-1). `reseal`
//! writes the repository again under the new key, swaps the manifest to the one that names it and
//! only then deletes what the old key sealed. The new checkpoint is the ONLY holder of the refs:
//! a swap that landed and answered with a conflict (its repeat met its own version) or with a
//! timeout, or one still on its way to the bucket, must leave the checkpoint where it is, and the
//! old objects must stay until the new set is proven whole.
//!
//! The rows of the table in the doc of `reseal`, one test each.

use super::lost_answers::{Answer, LateSwap, Lost, LostAnswers};
use crate::{
    meta::{
        keys,
        wal::{reseal, MAINTENANCE},
        Bucket, Kind, Manifest, MemoryBucket, MetaError, MetaStore, PackWriter, Publish, RefUpdate,
        Sealer, TestSealer,
    },
    DriveError,
};

const MAIN: &str = "refs/heads/main";
const NOW: u64 = 1_800_000_000;

fn old_key() -> TestSealer {
    TestSealer::new([11; 32])
}

fn new_key() -> TestSealer {
    TestSealer::new([22; 32])
}

/// Sets the main ref to `value`, with a pack of one blob.
fn publish_main(store: &mut MetaStore<MemoryBucket, TestSealer>, value: &str) {
    store
        .publish(|state| {
            let mut pack = PackWriter::new();
            pack.add(Kind::Blob, value.as_bytes().to_vec());
            Ok(Some(Publish {
                pack: Some(pack),
                updates: vec![RefUpdate {
                    name: MAIN.to_string(),
                    old: state.refs.get(MAIN).cloned(),
                    new: Some(value.to_string()),
                }],
                message: format!("set {value}"),
            }))
        })
        .expect("a commit lands");
}

/// A repository under the old key: three commits, so a log, packs and refs.
fn seeded() -> MemoryBucket {
    let bucket = MemoryBucket::new();
    let mut store = MetaStore::create(bucket.clone(), old_key(), "laptop").unwrap();
    for value in ["one", "two", "three"] {
        publish_main(&mut store, value);
    }
    bucket
}

/// The keys of the repository now.
fn keys_of(bucket: &MemoryBucket) -> Vec<String> {
    bucket.objects().into_iter().map(|(key, _)| key).collect()
}

/// The keys under the repository's checkpoint folder.
fn checkpoints(bucket: &MemoryBucket) -> Vec<String> {
    keys_of(bucket)
        .into_iter()
        .filter(|key| key.starts_with(keys::CHECKPOINT_DIR))
        .collect()
}

/// What a device that holds the new key reads from the bucket: the refs live in the checkpoint
/// the manifest names, so an open that works says the checkpoint is there.
fn opens_with_new(bucket: &MemoryBucket) -> Result<String, MetaError> {
    let store = MetaStore::open(bucket.clone(), new_key(), "phone")?;
    Ok(store
        .state()
        .refs
        .get(MAIN)
        .cloned()
        .expect("the main ref is in the re-sealed repository"))
}

/// What a device that holds the old key reads (the swap has not landed yet).
fn opens_with_old(bucket: &MemoryBucket) -> bool {
    MetaStore::open(bucket.clone(), old_key(), "phone").is_ok()
}

/// Row "any answer, the read-back names this call's checkpoint" (412): the swap landed and its
/// repeat met its own version. The checkpoint stays, the old objects go, the repository opens with
/// the new key. RED on the old code: the 412 arm removed the checkpoint, the next run deleted the
/// old objects, and no device could open the repository any more.
#[test]
fn x_delta_sync4_1_a_swap_that_landed_and_answered_with_a_conflict_keeps_its_checkpoint() {
    let seeded = seeded();
    let old_objects = keys_of(&seeded);
    let bucket = LostAnswers::new(seeded.clone());
    bucket.lose_next(keys::MANIFEST, Lost::Resent, 0);
    let done = reseal(&bucket, &old_key(), &new_key(), "laptop", NOW)
        .expect("the swap landed: the call finishes it");
    assert_eq!(
        opens_with_new(&seeded).as_deref(),
        Ok("three"),
        "the checkpoint the manifest names is in the bucket: {done:?} {:?}",
        keys_of(&seeded)
    );
    assert!(done.switched, "{done:?}");
    assert_eq!(checkpoints(&seeded).len(), 1, "{:?}", checkpoints(&seeded));
    assert!(
        old_objects
            .iter()
            .filter(|key| key.starts_with(keys::LOG_DIR))
            .all(|key| !keys_of(&seeded).contains(key)),
        "the old log entries are gone"
    );
}

/// Row "412, the very manifest the swap carried" (X-DELTA-SYNC8-4): the swap landed, answered 412, and the read that
/// follows is served by a replica that lags - the OLD key's manifest as it was before. It is no word that nothing landed:
/// the new checkpoint stays, the old objects stay, and the call says the outcome is unknown. RED on the old code: the new
/// checkpoint was removed on that word, the manifest names it, and no device could open the repository.
#[test]
fn x_delta_sync8_4_a_412_read_back_from_a_lagging_replica_keeps_the_new_checkpoint() {
    let seeded = seeded();
    let old_objects = keys_of(&seeded);
    let bucket = LostAnswers::new(seeded.clone());
    bucket.lose_next(keys::MANIFEST, Lost::ResentLagging, 0);
    let done = reseal(&bucket, &old_key(), &new_key(), "laptop", NOW);
    assert!(
        matches!(done, Err(MetaError::OutcomeUnknown { .. })),
        "{done:?}"
    );
    assert_eq!(
        opens_with_new(&seeded).as_deref(),
        Ok("three"),
        "the checkpoint the manifest names is in the bucket: {:?}",
        keys_of(&seeded)
    );
    assert!(
        old_objects
            .iter()
            .filter(|key| key.starts_with(keys::LOG_DIR))
            .all(|key| keys_of(&seeded).contains(key)),
        "nothing of the old key was deleted on an unknown outcome"
    );
}

/// Row "any, cannot be read": the swap landed, answered 412 and the bucket cannot be read. The
/// outcome is unknown: both sets stay and the call says so. Run again when the bucket answers, it
/// finishes. RED on the old code: the checkpoint the manifest names was removed unread.
#[test]
fn x_delta_sync4_1_a_conflict_whose_read_back_fails_keeps_both_sets_and_is_an_unknown_outcome() {
    let seeded = seeded();
    let before = keys_of(&seeded);
    let bucket = LostAnswers::new(seeded.clone());
    bucket.lose_next(keys::MANIFEST, Lost::Resent, 1000);
    let first = reseal(&bucket, &old_key(), &new_key(), "laptop", NOW);
    let now = keys_of(&seeded);
    assert_eq!(
        checkpoints(&seeded).len(),
        1,
        "the new checkpoint the landed manifest names stays: {first:?} {now:?}"
    );
    assert!(
        matches!(first, Err(MetaError::OutcomeUnknown { .. })),
        "{first:?}"
    );
    assert!(
        before.iter().all(|key| now.contains(key)),
        "no object of the old key was deleted: {before:?} -> {now:?}"
    );

    bucket.reads_up();
    let second = reseal(&bucket, &old_key(), &new_key(), "laptop", NOW).expect("it finishes");
    assert!(
        !second.switched,
        "the swap landed in the first call: {second:?}"
    );
    assert_eq!(opens_with_new(&seeded).as_deref(), Ok("three"));
    assert_eq!(checkpoints(&seeded).len(), 1);
}

/// Row "a timeout, the read-back names this call's checkpoint": the swap landed, no answer. The
/// call finishes it (this row was right on the old code too).
#[test]
fn x_delta_sync4_1_a_swap_that_landed_and_timed_out_is_found_landed_and_finished() {
    let seeded = seeded();
    let bucket = LostAnswers::new(seeded.clone());
    bucket.lose_next(keys::MANIFEST, Lost::Transport, 0);
    let done = reseal(&bucket, &old_key(), &new_key(), "laptop", NOW).expect("found landed");
    assert!(done.switched, "{done:?}");
    assert_eq!(opens_with_new(&seeded).as_deref(), Ok("three"));
}

/// Row "a timeout, cannot be read": nothing is deleted (this row was right on the old code too).
#[test]
fn x_delta_sync4_1_a_timeout_whose_read_back_fails_keeps_both_sets() {
    let seeded = seeded();
    let before = keys_of(&seeded);
    let bucket = LostAnswers::new(seeded.clone());
    bucket.lose_next(keys::MANIFEST, Lost::Transport, 1000);
    assert!(reseal(&bucket, &old_key(), &new_key(), "laptop", NOW).is_err());
    let now = keys_of(&seeded);
    assert!(
        before.iter().all(|key| now.contains(key)),
        "{before:?} -> {now:?}"
    );
    bucket.reads_up();
    reseal(&bucket, &old_key(), &new_key(), "laptop", NOW).expect("it finishes");
    assert_eq!(opens_with_new(&seeded).as_deref(), Ok("three"));
}

/// Row "an error of unknown certainty (a timeout), another manifest": the swap did not land when
/// the read-back looked, and lands right after - a slow PUT. The checkpoint must stay. RED on the
/// old code: the generic arm took the read for the answer, removed the checkpoint, the PUT landed
/// and the manifest named a checkpoint that was gone.
#[test]
fn x_delta_sync4_1_a_slow_swap_that_lands_after_the_read_back_keeps_its_checkpoint() {
    let seeded = seeded();
    let bucket = LateSwap::new(seeded.clone());
    bucket.answer_next(keys::MANIFEST, Answer::Transport);
    let first = reseal(&bucket, &old_key(), &new_key(), "laptop", NOW);
    assert!(first.is_err(), "{first:?}");
    // The read-back landed it (the double applies the pending write right after a read).
    assert!(
        !bucket.on_its_way(),
        "the swap reached the bucket after the read-back"
    );
    assert!(
        !opens_with_old(&seeded),
        "the manifest is the new key's now"
    );
    assert_eq!(
        opens_with_new(&seeded).as_deref(),
        Ok("three"),
        "the checkpoint it names was not removed: {:?}",
        keys_of(&seeded)
    );
    // Run again: past the switch, it finishes.
    reseal(&bucket, &old_key(), &new_key(), "laptop", NOW).expect("it finishes");
    assert_eq!(opens_with_new(&seeded).as_deref(), Ok("three"));
    assert_eq!(checkpoints(&seeded).len(), 1);
}

/// Row "409 (under way), another manifest": the service answers that this very swap is still being
/// made; it lands a moment later. The checkpoint stays, the next attempt meets the landed swap and
/// finishes it. RED on the old code: the 409 arm removed the checkpoint at once.
#[test]
fn x_delta_sync4_1_a_swap_still_under_way_at_a_409_keeps_its_checkpoint() {
    let seeded = seeded();
    let bucket = LateSwap::new(seeded.clone());
    bucket.answer_next(keys::MANIFEST, Answer::Raced);
    reseal(&bucket, &old_key(), &new_key(), "laptop", NOW).expect("it finishes");
    assert!(!bucket.on_its_way());
    assert_eq!(
        opens_with_new(&seeded).as_deref(),
        Ok("three"),
        "the checkpoint the manifest names was not removed: {:?}",
        keys_of(&seeded)
    );
    assert_eq!(
        checkpoints(&seeded).len(),
        1,
        "the second attempt's checkpoint went (its swap lost), the first one stays: {:?}",
        checkpoints(&seeded)
    );
}

/// Row "412, another manifest" (a real race: the version moved, nothing landed): the checkpoint
/// of the lost attempt is nobody's and goes; the next attempt switches. Right on the old code
/// too - the new code still cleans up after a swap it KNOWS did not land.
#[test]
fn x_delta_sync4_1_a_swap_that_lost_a_real_race_removes_only_its_own_checkpoint() {
    let seeded = seeded();
    // Another writer of the old key moves the manifest between this call's read and its swap.
    let writer = seeded.clone();
    seeded.before_next_replace(keys::MANIFEST, move || {
        let mut store = MetaStore::open(writer, old_key(), "phone").unwrap();
        publish_main(&mut store, "four");
    });
    let done = reseal(&seeded, &old_key(), &new_key(), "laptop", NOW)
        .expect("the second attempt switches");
    assert!(done.switched, "{done:?}");
    assert_eq!(opens_with_new(&seeded).as_deref(), Ok("four"));
    assert_eq!(
        checkpoints(&seeded).len(),
        1,
        "the lost attempt's checkpoint is gone: {:?}",
        checkpoints(&seeded)
    );
}

/// Row "a refusal for policy, another manifest" (X-DELTA-SYNC6-2, which replaced the row "a
/// refusal for good: removed"): a frozen drive refuses the swap and nothing lands. The checkpoint
/// STAYS - the refusal is the answer of the last request the transport sent and says nothing of
/// the first - the old repository is untouched, and the orphan is only space: the next run
/// switches and the orphan sweep collects it after the grace (AZ-M6's leak stays bounded).
#[test]
fn x_delta_sync6_2_a_swap_the_storage_refused_leaves_its_checkpoint_as_space_the_sweep_collects() {
    let seeded = seeded();
    let before = keys_of(&seeded);
    let bucket = LateSwap::new(seeded.clone());
    bucket.answer_next(keys::MANIFEST, Answer::Denied);
    assert!(reseal(&bucket, &old_key(), &new_key(), "laptop", NOW).is_err());
    assert!(opens_with_old(&seeded));
    assert_eq!(
        checkpoints(&seeded).len(),
        1,
        "the refused attempt's checkpoint stays: {:?}",
        checkpoints(&seeded)
    );
    let now = keys_of(&seeded);
    assert!(
        before.iter().all(|key| now.contains(key)),
        "no object of the old key was deleted"
    );

    // Run again (the drive is open again): it switches, with a checkpoint of its own.
    let done = reseal(&bucket, &old_key(), &new_key(), "laptop", NOW).expect("it switches");
    assert!(done.switched, "{done:?}");
    assert_eq!(opens_with_new(&seeded).as_deref(), Ok("three"));
    assert_eq!(
        checkpoints(&seeded).len(),
        2,
        "the named one and the orphan: {:?}",
        checkpoints(&seeded)
    );
    // Maintenance of the new key's device: the orphan is retired, then collected.
    let mut janitor = MetaStore::open(seeded.clone(), new_key(), "phone").unwrap();
    let guard = janitor.acquire_lease(MAINTENANCE, 600).unwrap();
    assert_eq!(janitor.sweep_orphans(&guard, 0).unwrap(), 1);
    assert_eq!(janitor.collect_garbage(&guard, 0).unwrap(), 1);
    assert_eq!(checkpoints(&seeded).len(), 1, "{:?}", checkpoints(&seeded));
    assert_eq!(opens_with_new(&seeded).as_deref(), Ok("three"));
}

/// Row "a refusal for policy of the RE-SENT request, another manifest" (X-DELTA-SYNC6-2): the first
/// swap got no answer and is on its way to the bucket still; the transport sent it again and the
/// repeat was refused (the session expired); the read-back shows the old manifest; the first swap
/// lands right after it. The checkpoint must stay. RED on the old code: the generic arm took the
/// refusal for "nothing landed, nothing will" and removed the checkpoint, the first request then
/// landed, and the manifest - sealed with the new key - named a checkpoint that was gone: no key
/// opened the repository and every rerun of `reseal` ended in `Corrupt` (the old objects, the only
/// other copy, were kept for good).
#[test]
fn x_delta_sync6_2_a_policy_refusal_of_the_resent_swap_keeps_the_checkpoint_the_first_request_lands_on(
) {
    let seeded = seeded();
    let bucket = LateSwap::new(seeded.clone());
    bucket.answer_next(keys::MANIFEST, Answer::DeniedFirstLands);
    let first = reseal(&bucket, &old_key(), &new_key(), "laptop", NOW);
    assert!(
        matches!(first, Err(MetaError::Drive(DriveError::Denied { .. }))),
        "the caller is told the refusal: {first:?}"
    );
    assert!(
        !bucket.on_its_way(),
        "the first swap reached the bucket after the read-back"
    );
    assert!(
        !opens_with_old(&seeded),
        "the manifest is the new key's now"
    );
    assert_eq!(
        opens_with_new(&seeded).as_deref(),
        Ok("three"),
        "the checkpoint the manifest names was not removed: {:?}",
        keys_of(&seeded)
    );
    // Run again: past the switch, it deletes the old objects once the new set is proven whole.
    let again = reseal(&bucket, &old_key(), &new_key(), "laptop", NOW).expect("it finishes");
    assert!(!again.switched, "{again:?}");
    assert_eq!(opens_with_new(&seeded).as_deref(), Ok("three"));
    assert_eq!(checkpoints(&seeded).len(), 1, "{:?}", checkpoints(&seeded));
}

/// Step 3 deletes the only other copy, so it asks the new set first: a manifest of the new key
/// whose checkpoint is missing (what the old bug left behind, or a bucket that lost an object)
/// is no ground to delete the old objects. RED on the old code: run again, it deleted every one.
#[test]
fn x_delta_sync4_1_the_old_objects_are_not_deleted_while_the_new_checkpoint_is_missing() {
    let seeded = seeded();
    let old_pack = {
        let store = MetaStore::open(seeded.clone(), old_key(), "checker").unwrap();
        keys::pack(&store.state().packs[0].name)
    };
    // The switch lands; deleting an old pack does not.
    seeded.fail_next_write(
        &old_pack,
        MetaError::Drive(DriveError::Transport("the network went".to_string())),
    );
    assert!(reseal(&seeded, &old_key(), &new_key(), "laptop", NOW).is_err());
    assert!(opens_with_new(&seeded).is_ok(), "the switch landed");
    assert!(
        keys_of(&seeded).contains(&old_pack),
        "the old pack is still there"
    );

    // The new checkpoint, the one the manifest names, is lost from the bucket.
    let manifest = seeded
        .objects()
        .into_iter()
        .find(|(key, _)| key == keys::MANIFEST)
        .map(|(_, bytes)| bytes)
        .expect("a manifest");
    let plain = new_key()
        .open(keys::MANIFEST.as_bytes(), &manifest)
        .expect("the manifest opens with the new key");
    let manifest: Manifest = serde_json::from_slice(&plain).unwrap();
    let lost = manifest.checkpoint.expect("a checkpoint").key;
    seeded.remove(&lost).unwrap();
    let kept = keys_of(&seeded);
    assert!(kept.contains(&old_pack));

    let again = reseal(&seeded, &old_key(), &new_key(), "laptop", NOW);
    assert!(
        matches!(again, Err(MetaError::Corrupt { .. })),
        "a refusal that says the new checkpoint is missing: {again:?}"
    );
    assert_eq!(
        keys_of(&seeded),
        kept,
        "nothing was deleted: the old objects are the only copy"
    );
}
