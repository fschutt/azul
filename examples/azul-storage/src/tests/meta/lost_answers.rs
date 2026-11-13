//! Writes that land while their answers are lost: a flapping network cuts the connection after
//! the bucket took a request, and the HTTP layer may send the same conditional request again,
//! which then meets its own new version. The repository must take its own write for its own,
//! and an encrypted file must never end up named without its object.

use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Mutex,
};

use crate::meta::{
    keys, wal::reseal, Bucket, Fetched, Kind, Listed, MemoryBucket, MetaError, MetaStore,
    PackWriter, Publish, RefUpdate, TestSealer, Version,
};
use crate::{ByteRange, DriveError};

const MAIN: &str = "refs/heads/main";

/// How a lost answer looks to the caller.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Lost {
    /// The connection broke after the request was sent: no answer at all.
    Transport,
    /// The client sent the request again by itself, and the repeat met the version the first
    /// one made: a conflict.
    Resent,
}

/// A bucket whose next create or replace of a key containing a given fragment lands and loses
/// its answer; reads can stay down for a while after it (the network still flapping).
pub(crate) struct LostAnswers {
    pub inner: MemoryBucket,
    next: Mutex<Option<(String, Lost, usize)>>,
    reads_down: AtomicUsize,
}

impl LostAnswers {
    pub(crate) fn new(inner: MemoryBucket) -> LostAnswers {
        LostAnswers {
            inner,
            next: Mutex::new(None),
            reads_down: AtomicUsize::new(0),
        }
    }

    /// The next create or replace of a key containing `fragment` lands and answers as `how`;
    /// the `reads_down` reads after it fail as a lost connection does.
    pub(crate) fn lose_next(&self, fragment: &str, how: Lost, reads_down: usize) {
        *self.next.lock().unwrap() = Some((fragment.to_string(), how, reads_down));
    }

    /// Reads work again.
    pub(crate) fn reads_up(&self) {
        self.reads_down.store(0, Ordering::SeqCst);
    }

    fn lost(&self, key: &str) -> Option<Lost> {
        let mut next = self.next.lock().unwrap();
        let hit = next
            .as_ref()
            .is_some_and(|(fragment, _, _)| key.contains(fragment.as_str()));
        if !hit {
            return None;
        }
        let (_, how, reads_down) = next.take()?;
        self.reads_down.store(reads_down, Ordering::SeqCst);
        Some(how)
    }

    fn reading(&self) -> Result<(), MetaError> {
        let down = self.reads_down.load(Ordering::SeqCst);
        if down == 0 {
            return Ok(());
        }
        self.reads_down.store(down - 1, Ordering::SeqCst);
        Err(MetaError::Drive(DriveError::Transport(
            "connection reset by peer".to_string(),
        )))
    }

    fn answer(&self, key: &str, landed: Option<Version>) -> Result<Option<Version>, MetaError> {
        match self.lost(key) {
            None => Ok(landed),
            Some(Lost::Transport) => Err(MetaError::Drive(DriveError::Transport(
                "connection reset by peer".to_string(),
            ))),
            Some(Lost::Resent) => Err(MetaError::Conflict {
                key: key.to_string(),
            }),
        }
    }
}

impl Bucket for LostAnswers {
    fn read(&self, key: &str) -> Result<Option<(Vec<u8>, Version)>, MetaError> {
        self.reading()?;
        self.inner.read(key)
    }
    fn read_if_changed(&self, key: &str, known: &str) -> Result<Fetched, MetaError> {
        self.reading()?;
        self.inner.read_if_changed(key, known)
    }
    fn read_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, MetaError> {
        self.reading()?;
        Bucket::read_range(&self.inner, key, range)
    }
    fn create(&self, key: &str, bytes: &[u8]) -> Result<Option<Version>, MetaError> {
        let landed = self.inner.create(key, bytes)?;
        self.answer(key, landed)
    }
    fn replace(&self, key: &str, bytes: &[u8], known: &str) -> Result<Option<Version>, MetaError> {
        let landed = self.inner.replace(key, bytes, known)?;
        self.answer(key, landed)
    }
    fn remove(&self, key: &str) -> Result<(), MetaError> {
        self.inner.remove(key)
    }
    fn list_keys(&self, prefix: &str) -> Result<Vec<Listed>, MetaError> {
        self.inner.list_keys(prefix)
    }
}

/// How a swap that did not land at once answers.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Answer {
    /// A timeout: the request may be on its way to the bucket still.
    Transport,
    /// A 409: another write of the manifest - this very swap - is under way.
    Raced,
    /// A refusal for good (a frozen drive): nothing was written and nothing will be.
    Denied,
    /// A refusal for policy (an expired session, a drive that froze) of the request the transport
    /// sent AGAIN: the first request is on its way to the bucket still and lands right after the
    /// next read (X-DELTA-SYNC6-2). The caller sees the same error as for [`Answer::Denied`].
    DeniedFirstLands,
}

impl Answer {
    pub(crate) fn error(self) -> MetaError {
        match self {
            Answer::Transport => MetaError::Drive(DriveError::Transport("timed out".to_string())),
            Answer::Raced => MetaError::Raced {
                key: keys::MANIFEST.to_string(),
            },
            Answer::Denied | Answer::DeniedFirstLands => MetaError::Drive(DriveError::Denied {
                message: "the drive is frozen".to_string(),
            }),
        }
    }
}

/// A bucket whose next replace of a key containing a fragment is not applied when it is asked: it
/// answers at once, and the write lands right after the first read that follows - a slow PUT, a
/// service that answers 409 for the swap it is still making. The read itself still sees the
/// object as it was.
pub(crate) struct LateSwap {
    inner: MemoryBucket,
    armed: Mutex<Option<(String, Answer)>>,
    pending: Mutex<Option<(String, Vec<u8>, String)>>,
    /// While set, the late swap lands only when a test says so ([`LateSwap::land_now`]) and no read
    /// brings it: a first request that waits in the node's queue for longer than the whole error path
    /// of the caller (the read-back, the compensation after it: X-DELTA-SYNC7-1).
    held: AtomicBool,
}

impl LateSwap {
    pub(crate) fn new(inner: MemoryBucket) -> LateSwap {
        LateSwap {
            inner,
            armed: Mutex::new(None),
            pending: Mutex::new(None),
            held: AtomicBool::new(false),
        }
    }

    /// The late swap does not land after the next read: it lands when [`LateSwap::land_now`] is called.
    #[must_use]
    pub(crate) fn held(self) -> LateSwap {
        self.held.store(true, Ordering::SeqCst);
        self
    }

    pub(crate) fn answer_next(&self, fragment: &str, answer: Answer) {
        *self.armed.lock().unwrap() = Some((fragment.to_string(), answer));
    }

    fn land(&self) {
        let pending = self.pending.lock().unwrap().take();
        if let Some((key, bytes, known)) = pending {
            self.inner
                .replace(&key, &bytes, &known)
                .expect("the late swap meets the version it was made from");
        }
    }

    /// The first request reaches the bucket now, whatever was read before it.
    pub(crate) fn land_now(&self) {
        self.land();
    }

    /// What a read brings: the late swap, unless a test holds it.
    fn land_after_read(&self) {
        if !self.held.load(Ordering::SeqCst) {
            self.land();
        }
    }

    /// Whether the late swap is still on its way.
    pub(crate) fn on_its_way(&self) -> bool {
        self.pending.lock().unwrap().is_some()
    }
}

impl Bucket for LateSwap {
    fn read(&self, key: &str) -> Result<Option<(Vec<u8>, Version)>, MetaError> {
        let read = self.inner.read(key);
        self.land_after_read();
        read
    }
    fn read_if_changed(&self, key: &str, known: &str) -> Result<Fetched, MetaError> {
        let read = self.inner.read_if_changed(key, known);
        self.land_after_read();
        read
    }
    fn read_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, MetaError> {
        Bucket::read_range(&self.inner, key, range)
    }
    fn create(&self, key: &str, bytes: &[u8]) -> Result<Option<Version>, MetaError> {
        self.inner.create(key, bytes)
    }
    fn replace(&self, key: &str, bytes: &[u8], known: &str) -> Result<Option<Version>, MetaError> {
        let hit = {
            let mut armed = self.armed.lock().unwrap();
            if armed
                .as_ref()
                .is_some_and(|(f, _)| key.contains(f.as_str()))
            {
                armed.take()
            } else {
                None
            }
        };
        let Some((_, answer)) = hit else {
            return self.inner.replace(key, bytes, known);
        };
        if !matches!(answer, Answer::Denied) {
            *self.pending.lock().unwrap() =
                Some((key.to_string(), bytes.to_vec(), known.to_string()));
        }
        Err(answer.error())
    }
    fn remove(&self, key: &str) -> Result<(), MetaError> {
        self.inner.remove(key)
    }
    fn list_keys(&self, prefix: &str) -> Result<Vec<Listed>, MetaError> {
        self.inner.list_keys(prefix)
    }
    fn exists(&self, key: &str) -> Result<bool, MetaError> {
        self.inner.exists(key)
    }
}

fn sealer() -> TestSealer {
    TestSealer::new([23; 32])
}

/// Sets the main ref to `value`, with a pack of one blob.
fn publish_main<B: Bucket>(
    store: &mut MetaStore<B, TestSealer>,
    value: &str,
) -> Result<Option<crate::meta::Published>, MetaError> {
    store.publish(|state| {
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
}

/// Another device opens the repository and reads the main ref: every log entry the manifest
/// names must be there.
fn main_ref_as_another_device_sees_it(bucket: &MemoryBucket) -> Option<String> {
    let other = MetaStore::open(bucket.clone(), sealer(), "phone").expect(
        "another device opens the repository: every log entry its manifest names is in the bucket",
    );
    other.state().refs.get(MAIN).cloned()
}

#[test]
fn a_manifest_swap_that_landed_and_whose_repeat_met_its_own_version_lands_and_keeps_its_log_entry()
{
    let bucket = LostAnswers::new(MemoryBucket::new());
    let mut store = MetaStore::create(&bucket, sealer(), "laptop").unwrap();
    bucket.lose_next(keys::MANIFEST, Lost::Resent, 0);
    let published = publish_main(&mut store, "one");
    assert!(matches!(published, Ok(Some(_))), "{published:?}");
    assert_eq!(
        main_ref_as_another_device_sees_it(&bucket.inner).as_deref(),
        Some("one")
    );
    // The device goes on publishing on top of it.
    assert!(matches!(publish_main(&mut store, "two"), Ok(Some(_))));
    assert_eq!(
        main_ref_as_another_device_sees_it(&bucket.inner).as_deref(),
        Some("two")
    );
}

#[test]
fn a_log_entry_whose_create_landed_and_whose_repeat_met_it_is_this_device_s_own_and_the_publish_lands(
) {
    let bucket = LostAnswers::new(MemoryBucket::new());
    let mut store = MetaStore::create(&bucket, sealer(), "laptop").unwrap();
    bucket.lose_next(keys::LOG_DIR, Lost::Resent, 0);
    let published = publish_main(&mut store, "one");
    assert!(matches!(published, Ok(Some(_))), "{published:?}");
    assert_eq!(
        main_ref_as_another_device_sees_it(&bucket.inner).as_deref(),
        Some("one")
    );
}

#[test]
fn a_manifest_swap_whose_answer_was_lost_while_the_bucket_stays_away_is_found_landed_later() {
    let bucket = LostAnswers::new(MemoryBucket::new());
    let mut store = MetaStore::create(&bucket, sealer(), "laptop").unwrap();
    bucket.lose_next(keys::MANIFEST, Lost::Transport, 1000);
    // It cannot know yet: an error, never a removed entry the manifest names.
    let _ = publish_main(&mut store, "one");
    bucket.reads_up();
    assert_eq!(
        main_ref_as_another_device_sees_it(&bucket.inner).as_deref(),
        Some("one")
    );
    // Back online, the device finds its change landed and goes on from it.
    assert!(matches!(publish_main(&mut store, "two"), Ok(Some(_))));
    assert_eq!(
        main_ref_as_another_device_sees_it(&bucket.inner).as_deref(),
        Some("two")
    );
}

/// X-DELTA-SYNC6-2 at the store. The swap that names the new log entry got no answer, the
/// transport sent it again and the repeat was refused for policy (the session expired, the drive
/// froze); the read-back shows the old manifest; the FIRST request then lands. The entry must
/// still be there. RED on the old code: the refusal arm of `commit_entry` removed the entry on
/// the word of the last request, the first request landed and the manifest named a log entry that
/// was gone - no device could open the repository any more.
#[test]
fn x_delta_sync6_2_a_policy_refusal_of_the_resent_commit_keeps_the_log_entry_the_first_request_lands_on(
) {
    let bucket = LateSwap::new(MemoryBucket::new());
    let mut store = MetaStore::create(&bucket, sealer(), "laptop").unwrap();
    bucket.answer_next(keys::MANIFEST, Answer::DeniedFirstLands);
    let published = publish_main(&mut store, "one");
    assert!(
        matches!(published, Err(MetaError::Drive(DriveError::Denied { .. }))),
        "the caller is told the refusal: {published:?}"
    );
    assert!(
        !bucket.on_its_way(),
        "the first swap reached the bucket after the read-back"
    );
    // Another device opens the repository: every log entry the manifest names must be there.
    assert_eq!(
        main_ref_as_another_device_sees_it(&bucket.inner).as_deref(),
        Some("one"),
        "the entry the landed manifest names was not removed"
    );
}

/// A bucket whose next manifest swap is answered "precondition failed" (it did not land) and whose manifest
/// then reads "no such object": a lagging replica, a cache, a bucket that lost it. Nothing else changes.
struct ForgetsTheManifest {
    inner: MemoryBucket,
    /// The next swap of the manifest is refused with a 412, and every read of the manifest after it says
    /// "no such object".
    armed: Mutex<bool>,
    forgetting: AtomicBool,
}

impl ForgetsTheManifest {
    fn new(inner: MemoryBucket) -> ForgetsTheManifest {
        ForgetsTheManifest {
            inner,
            armed: Mutex::new(false),
            forgetting: AtomicBool::new(false),
        }
    }

    fn lose_the_next_swap(&self) {
        *self.armed.lock().unwrap() = true;
    }

    fn the_keys_under(&self, prefix: &str) -> Vec<String> {
        self.inner
            .list_keys(prefix)
            .unwrap()
            .into_iter()
            .map(|listed| listed.key)
            .collect()
    }
}

impl Bucket for ForgetsTheManifest {
    fn read(&self, key: &str) -> Result<Option<(Vec<u8>, Version)>, MetaError> {
        if key == keys::MANIFEST && self.forgetting.load(Ordering::SeqCst) {
            return Ok(None);
        }
        self.inner.read(key)
    }
    fn read_if_changed(&self, key: &str, known: &str) -> Result<Fetched, MetaError> {
        self.inner.read_if_changed(key, known)
    }
    fn read_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, MetaError> {
        Bucket::read_range(&self.inner, key, range)
    }
    fn create(&self, key: &str, bytes: &[u8]) -> Result<Option<Version>, MetaError> {
        self.inner.create(key, bytes)
    }
    fn replace(&self, key: &str, bytes: &[u8], known: &str) -> Result<Option<Version>, MetaError> {
        if key == keys::MANIFEST && std::mem::take(&mut *self.armed.lock().unwrap()) {
            self.forgetting.store(true, Ordering::SeqCst);
            return Err(MetaError::Conflict {
                key: key.to_string(),
            });
        }
        self.inner.replace(key, bytes, known)
    }
    fn remove(&self, key: &str) -> Result<(), MetaError> {
        self.inner.remove(key)
    }
    fn list_keys(&self, prefix: &str) -> Result<Vec<Listed>, MetaError> {
        self.inner.list_keys(prefix)
    }
}

/// X-DELTA-SYNC7-7. Two read-backs of the manifest answered a missing manifest differently: `swap_outcome`
/// (the rotation) said "unknown - nobody removes the manifest", `swap_landed` (a commit, a checkpoint) said
/// "no manifest of ours" and the 412 arm REMOVED the log entry or the checkpoint - which the manifest may
/// name, if the bucket that said "no such object" is a lagging replica. Now the two ask one function
/// (`read_back_manifest`) and are unknown: nothing is removed and the caller is told the outcome is not
/// known; the rotation is the third arm of the same bucket. RED on the old code: the log entry (and the
/// checkpoint) are gone from the bucket.
#[test]
fn x_delta_sync7_7_a_manifest_that_reads_missing_after_a_412_removes_neither_entry_nor_checkpoint()
{
    // A commit.
    let bucket = ForgetsTheManifest::new(MemoryBucket::new());
    let mut store = MetaStore::create(&bucket, sealer(), "laptop").unwrap();
    assert!(matches!(publish_main(&mut store, "one"), Ok(Some(_))));
    let before = bucket.the_keys_under(keys::LOG_DIR);
    bucket.lose_the_next_swap();
    let published = publish_main(&mut store, "two");
    assert!(
        matches!(published, Err(MetaError::OutcomeUnknown { .. })),
        "a missing manifest is no answer: {published:?}"
    );
    let after = bucket.the_keys_under(keys::LOG_DIR);
    assert!(
        after.len() > before.len(),
        "the log entry of the swap that may have landed was removed: {before:?} -> {after:?}"
    );

    // A checkpoint.
    let bucket = ForgetsTheManifest::new(MemoryBucket::new());
    let mut store = MetaStore::create(&bucket, sealer(), "laptop").unwrap();
    assert!(matches!(publish_main(&mut store, "one"), Ok(Some(_))));
    assert!(matches!(publish_main(&mut store, "two"), Ok(Some(_))));
    let before = bucket.the_keys_under(keys::CHECKPOINT_DIR);
    bucket.lose_the_next_swap();
    let checkpointed = store.checkpoint();
    assert!(
        matches!(checkpointed, Err(MetaError::OutcomeUnknown { .. })),
        "a missing manifest is no answer: {checkpointed:?}"
    );
    let after = bucket.the_keys_under(keys::CHECKPOINT_DIR);
    assert!(
        after.len() > before.len(),
        "the checkpoint the manifest may name was removed: {before:?} -> {after:?}"
    );

    // A rotation (`reseal`): the same answer to the read-back of ITS swap. The new checkpoint, which the
    // landed manifest may name, stays, and so does every object of the old key.
    let seeded = MemoryBucket::new();
    {
        let mut store = MetaStore::create(seeded.clone(), sealer(), "laptop").unwrap();
        assert!(matches!(publish_main(&mut store, "one"), Ok(Some(_))));
        assert!(matches!(publish_main(&mut store, "two"), Ok(Some(_))));
    }
    let bucket = ForgetsTheManifest::new(seeded);
    let old_log = bucket.the_keys_under(keys::LOG_DIR);
    let old_checkpoints = bucket.the_keys_under(keys::CHECKPOINT_DIR);
    bucket.lose_the_next_swap();
    let resealed = reseal(
        &bucket,
        &sealer(),
        &TestSealer::new([24; 32]),
        "laptop",
        1_800_000_000,
    );
    assert!(
        matches!(resealed, Err(MetaError::OutcomeUnknown { .. })),
        "a missing manifest is no answer: {resealed:?}"
    );
    let log = bucket.the_keys_under(keys::LOG_DIR);
    let checkpoints = bucket.the_keys_under(keys::CHECKPOINT_DIR);
    assert!(
        old_log.iter().all(|key| log.contains(key))
            && old_checkpoints.iter().all(|key| checkpoints.contains(key)),
        "an object of the old key was removed on a missing manifest: {log:?} {checkpoints:?}"
    );
    assert!(
        checkpoints.len() > old_checkpoints.len(),
        "the new checkpoint the manifest may name was removed: {old_checkpoints:?} -> {checkpoints:?}"
    );
}

/// AZ-M1 at the store: a swap that answered with a conflict and cannot be read back is an
/// unknown outcome, never the conflict itself (a caller reads that as "refused for good").
#[test]
fn az_m1_a_conflict_answer_whose_re_read_fails_is_an_unknown_outcome_and_not_a_conflict() {
    let bucket = LostAnswers::new(MemoryBucket::new());
    let mut store = MetaStore::create(&bucket, sealer(), "laptop").unwrap();
    bucket.lose_next(keys::MANIFEST, Lost::Resent, 1000);
    let published = publish_main(&mut store, "one");
    assert!(
        matches!(published, Err(MetaError::OutcomeUnknown { .. })),
        "{published:?}"
    );
    bucket.reads_up();
    // The swap had landed: another device reads it, and this one goes on from it.
    assert_eq!(
        main_ref_as_another_device_sees_it(&bucket.inner).as_deref(),
        Some("one")
    );
    assert!(matches!(publish_main(&mut store, "two"), Ok(Some(_))));
}

#[cfg(feature = "encryption")]
mod encrypted {
    use std::{
        io::Cursor,
        sync::{Arc, Mutex},
    };

    use super::{Answer, LateSwap, Lost, LostAnswers};
    use crate::{
        crypto::DriveKey,
        encrypted::{EncryptedDrive, Expect, NameIndex},
        meta::{keys, MemoryBucket, MetaIndex, MetaRepo, RepoOptions},
        packing::{PackOptions, JOURNAL_STALE_SECS},
        time::now_unix,
        BatchStream, ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition,
    };

    /// A device's encrypted drive: its data in `data`, its index through `meta`.
    fn laptop(
        data: &MemoryBucket,
        meta: &Arc<LostAnswers>,
        key: &DriveKey,
    ) -> EncryptedDrive<MemoryBucket> {
        let options = RepoOptions {
            cache_dir: None,
            lazy: false,
            ..RepoOptions::default()
        };
        let repo =
            MetaRepo::create_with(Arc::clone(meta), key.clone(), "laptop", "Laptop", &options)
                .unwrap();
        let index: Arc<dyn NameIndex> = Arc::new(MetaIndex::new(repo).with_poll_every(0));
        EncryptedDrive::new(data.clone(), key.clone(), index)
    }

    /// Another device, reading the same bucket without faults.
    fn phone(data: &MemoryBucket, key: &DriveKey) -> EncryptedDrive<MemoryBucket> {
        let repo = MetaRepo::open(data.clone(), key.clone(), "phone", "Phone").unwrap();
        let index: Arc<dyn NameIndex> = Arc::new(MetaIndex::new(repo).with_poll_every(0));
        EncryptedDrive::new(data.clone(), key.clone(), index)
    }

    #[test]
    fn a_file_whose_index_change_landed_while_its_answer_was_lost_still_reads() {
        let key = DriveKey::generate().unwrap();
        let data = MemoryBucket::new();
        let meta = Arc::new(LostAnswers::new(data.clone()));
        let drive = laptop(&data, &meta, &key);
        // The swap that names the file lands; its answer is lost and the bucket cannot be read
        // for a while, so the write cannot tell whether the file is named.
        meta.lose_next(keys::MANIFEST, Lost::Transport, 1000);
        let _ = drive.put("report.txt", b"the only copy of the report");
        meta.reads_up();
        let other = phone(&data, &key);
        if other.head("report.txt").is_ok() {
            assert_eq!(
                other.get("report.txt").unwrap(),
                b"the only copy of the report",
                "the index names the file: its object must still be in the bucket"
            );
        }
        // And the laptop, back online, reads it too.
        if drive.head("report.txt").is_ok() {
            assert_eq!(
                drive.get("report.txt").unwrap(),
                b"the only copy of the report"
            );
        }
    }

    /// AZ-M1. The swap that names the file landed, its repeat met the version it made (the
    /// bucket's answer is a conflict), and the read that would tell fails too: the write cannot
    /// know whether the file is named. That is an unknown outcome - an error of the "no answer"
    /// kind - and never "someone else wrote first", which would take the object out of the
    /// bucket while the index names it.
    #[test]
    fn az_m1_a_conflict_whose_re_read_fails_is_an_unknown_outcome_and_the_object_stays() {
        let key = DriveKey::generate().unwrap();
        let data = MemoryBucket::new();
        let meta = Arc::new(LostAnswers::new(data.clone()));
        let drive = laptop(&data, &meta, &key);
        meta.lose_next(keys::MANIFEST, Lost::Resent, 1000);
        let put = drive.put("report.txt", b"the only copy of the report");
        match put {
            Err(crate::DriveError::Transport(_)) => {}
            other => panic!(
                "a swap whose outcome is unknown answers as a lost connection does, not as a \
                 refusal for good: {other:?}"
            ),
        }
        meta.reads_up();
        // The swap had landed: the index names the file, and the object must still be there.
        let other = phone(&data, &key);
        assert_eq!(
            other
                .get("report.txt")
                .expect("the index names the file: its object must still be in the bucket"),
            b"the only copy of the report"
        );
    }

    /// X-DELTA-SYNC4-2 through the real index, the interleaving of the finding: a pack of new files
    /// is put, the swap that names it answers 409 ("another write of the manifest is in progress")
    /// and lands a moment later. The publish tries again, meets its OWN commit, and the rebuilt
    /// change expects every path absent: `Conflict` at each, until every target is dead.
    /// `commit_pack` read that as "refused for good" and deleted the pack the manifest names.
    /// RED on the old code: every write answers a conflict, the pack is gone from the bucket and
    /// the index names 30 files whose bytes are nowhere.
    #[test]
    fn x_delta_sync4_2_a_pack_whose_manifest_swap_answered_409_and_landed_is_kept() {
        use super::{Answer, LateSwap};
        use crate::{packing::PackOptions, BatchPut, Precondition};

        let key = DriveKey::generate().unwrap();
        let data = MemoryBucket::new();
        let meta = Arc::new(LateSwap::new(data.clone()));
        let options = RepoOptions {
            cache_dir: None,
            lazy: false,
            ..RepoOptions::default()
        };
        let repo =
            MetaRepo::create_with(Arc::clone(&meta), key.clone(), "laptop", "Laptop", &options)
                .unwrap();
        let index: Arc<dyn NameIndex> = Arc::new(MetaIndex::new(repo).with_poll_every(0));
        let drive = EncryptedDrive::new(data.clone(), key.clone(), index)
            .with_packing(PackOptions::default());
        let files: Vec<(String, Vec<u8>)> = (0..30)
            .map(|i| {
                (
                    format!("notes/{i:03}.txt"),
                    format!("the note {i}, written once").into_bytes(),
                )
            })
            .collect();
        let absent = Precondition::Absent;
        let items: Vec<BatchPut<'_>> = files
            .iter()
            .map(|(path, bytes)| BatchPut::new(path, bytes).when(&absent))
            .collect();
        meta.answer_next(keys::MANIFEST, Answer::Raced);
        let results = drive.put_many(&items);
        assert!(
            !meta.on_its_way(),
            "the swap landed after the read that looked for it"
        );
        assert!(
            results.iter().all(Result::is_ok),
            "the files are named by the manifest that landed: {:?}",
            results.iter().filter_map(|r| r.as_ref().err()).next()
        );
        let other = phone(&data, &key);
        for (path, bytes) in &files {
            assert_eq!(
                &other.get(path).unwrap_or_else(|e| panic!(
                    "{path}: the index names it, its pack must be there: {e}"
                )),
                bytes,
                "{path}"
            );
        }
    }

    // ---- X-DELTA-SYNC7-1: the compensation of a new object after a refusal of the repeat ----

    const REPORT: &[u8] = b"the only copy of the report, and what the drive must not lose";

    /// The data bucket of the drive in the tests below: the plain bucket, and every delete of it is
    /// followed by the first request of the parked manifest swap reaching the bucket - the compensation
    /// is quicker than a request that waits in the node's queue. The keys it deleted are told.
    struct QueuedBehind {
        inner: MemoryBucket,
        meta: Arc<LateSwap>,
        deleted: Mutex<Vec<String>>,
    }

    impl QueuedBehind {
        fn deleted(&self) -> Vec<String> {
            self.deleted.lock().unwrap().clone()
        }
    }

    impl Drive for QueuedBehind {
        fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
            self.inner.list(request)
        }
        fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
            self.inner.get(key)
        }
        fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
            self.inner.get_range(key, range)
        }
        fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
            self.inner.put(key, bytes)
        }
        fn put_if(
            &self,
            key: &str,
            bytes: &[u8],
            condition: &Precondition,
        ) -> Result<Option<String>, DriveError> {
            self.inner.put_if(key, bytes, condition)
        }
        fn delete(&self, key: &str) -> Result<(), DriveError> {
            self.inner.delete(key)?;
            self.deleted.lock().unwrap().push(key.to_string());
            self.meta.land_now();
            Ok(())
        }
        fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
            self.inner.head(key)
        }
    }

    /// A laptop whose manifest swaps go through `LateSwap` held until a test lands them, over the data
    /// bucket `QueuedBehind`; `data` is the bucket both share, as another device sees it.
    fn queued_laptop(
        data: &MemoryBucket,
        key: &DriveKey,
    ) -> (
        Arc<LateSwap>,
        Arc<QueuedBehind>,
        EncryptedDrive<Arc<QueuedBehind>>,
    ) {
        let meta = Arc::new(LateSwap::new(data.clone()).held());
        let options = RepoOptions {
            cache_dir: None,
            lazy: false,
            ..RepoOptions::default()
        };
        let repo =
            MetaRepo::create_with(Arc::clone(&meta), key.clone(), "laptop", "Laptop", &options)
                .unwrap();
        let index: Arc<dyn NameIndex> = Arc::new(MetaIndex::new(repo).with_poll_every(0));
        let bucket = Arc::new(QueuedBehind {
            inner: data.clone(),
            meta: Arc::clone(&meta),
            deleted: Mutex::new(Vec::new()),
        });
        let drive = EncryptedDrive::new(Arc::clone(&bucket), key.clone(), index);
        (meta, bucket, drive)
    }

    /// The first request of a write whose answer was lost reached the node, the transport sent it again
    /// and the repeat was refused for policy (the drive began to move, the quota ended with the first
    /// request); the manifest read back is the old one; the first request is queued and lands only after
    /// the caller's whole error path. The data object the manifest will name must still be in the bucket
    /// when it lands: `Refusal::of_object` took every refusal of the node for proof that nothing landed
    /// and `Staged::failed` deleted the object of the file (X-DELTA-SYNC7-1, door `bind`). RED on the old
    /// code: the object is deleted and every read of the file says "its object is missing".
    #[test]
    fn x_delta_sync7_1_put_keeps_the_object_when_the_resent_swap_is_refused_and_the_first_request_lands_late(
    ) {
        let key = DriveKey::generate().unwrap();
        let data = MemoryBucket::new();
        let (meta, bucket, drive) = queued_laptop(&data, &key);
        meta.answer_next(keys::MANIFEST, Answer::DeniedFirstLands);
        let put = drive.put("report.txt", REPORT);
        assert!(
            matches!(put, Err(DriveError::Denied { .. })),
            "the caller is told the refusal: {put:?}"
        );
        assert_eq!(
            bucket.deleted(),
            Vec::<String>::new(),
            "the object of a change whose first request may still land was deleted on the word of the \
             repeat's refusal"
        );
        assert!(
            meta.on_its_way(),
            "the first swap is still queued at the node"
        );
        meta.land_now();
        // The manifest that landed names the file: every device reads it, and its object is there.
        assert_eq!(
            phone(&data, &key)
                .get("report.txt")
                .expect("the index names the file: its object must still be in the bucket"),
            REPORT
        );
        assert_eq!(drive.get("report.txt").unwrap(), REPORT, "the laptop too");
        // The upkeep a stale time later finds the journaled object named and clears the entry only.
        let later = now_unix() + JOURNAL_STALE_SECS + 1;
        assert_eq!(
            drive.recover_packs(later).unwrap(),
            1,
            "the object was written in the journal of what is in flight"
        );
        assert_eq!(
            bucket.deleted(),
            Vec::<String>::new(),
            "a named object is no garbage"
        );
        assert_eq!(drive.get("report.txt").unwrap(), REPORT);
    }

    /// The same at `rewrite` (door 2 of the compensation): the re-encryption of a file whose manifest
    /// swap is refused for policy by the repeat and lands late must keep the new object the head then
    /// names.
    #[test]
    fn x_delta_sync7_1_rewrite_keeps_the_object_when_the_resent_swap_is_refused_and_the_first_request_lands_late(
    ) {
        let key = DriveKey::generate().unwrap();
        let data = MemoryBucket::new();
        let (meta, bucket, drive) = queued_laptop(&data, &key);
        drive.put("a.txt", REPORT).unwrap();
        let entry = drive.entry("a.txt").unwrap();
        meta.answer_next(keys::MANIFEST, Answer::DeniedFirstLands);
        let mut body = Cursor::new(REPORT.to_vec());
        let rewritten = drive.rewrite(
            "a.txt",
            &entry,
            &mut body,
            drive.options(),
            &|_| true,
            &|| {},
        );
        assert!(
            matches!(rewritten, Err(DriveError::Denied { .. })),
            "the refusal is handed on"
        );
        assert_eq!(
            bucket.deleted(),
            Vec::<String>::new(),
            "the new object of the rewrite was deleted on the word of the repeat's refusal"
        );
        meta.land_now();
        assert_eq!(
            phone(&data, &key)
                .get("a.txt")
                .expect("the head names the new object: it must be in the bucket"),
            REPORT
        );
    }

    /// The same at `promote` (door 3): a packed file moving into an object of its own.
    #[test]
    fn x_delta_sync7_1_promote_keeps_the_object_when_the_resent_swap_is_refused_and_the_first_request_lands_late(
    ) {
        let key = DriveKey::generate().unwrap();
        let data = MemoryBucket::new();
        let (meta, bucket, drive) = queued_laptop(&data, &key);
        let drive = drive.with_packing(PackOptions::default());
        drive.write("small.txt", b"small", Expect::Any).unwrap();
        assert!(drive
            .entry("small.txt")
            .unwrap()
            .object
            .unwrap()
            .packed
            .is_some());
        meta.answer_next(keys::MANIFEST, Answer::DeniedFirstLands);
        let promoted = drive.promote("small.txt");
        assert!(
            matches!(promoted, Err(DriveError::Denied { .. })),
            "the refusal is handed on: {promoted:?}"
        );
        assert_eq!(
            bucket.deleted(),
            Vec::<String>::new(),
            "the file's own object was deleted on the word of the repeat's refusal"
        );
        meta.land_now();
        assert_eq!(
            phone(&data, &key)
                .get("small.txt")
                .expect("the head names the file's own object: it must be in the bucket"),
            b"small"
        );
    }

    /// The same at the batch of large files (door 4): its error arm took no part in the compensation
    /// at all - the objects stayed without a word in the journal, which only the journal's upkeep
    /// finds again. RED on the old code: the upkeep finds nothing in flight (0), where the object the
    /// late manifest names is waiting for it to be cleared as named (1).
    #[test]
    fn x_delta_sync7_1_a_batch_of_large_files_journals_its_objects_when_the_resent_swap_is_refused()
    {
        let key = DriveKey::generate().unwrap();
        let data = MemoryBucket::new();
        let (meta, bucket, drive) = queued_laptop(&data, &key);
        let mut body = Cursor::new(REPORT.to_vec());
        let mut items = vec![BatchStream::new(
            "big.bin",
            &mut body as &mut (dyn std::io::Read + Send),
        )];
        meta.answer_next(keys::MANIFEST, Answer::DeniedFirstLands);
        let results = drive.put_from_many(&mut items);
        assert!(
            matches!(results[0], Err(DriveError::Denied { .. })),
            "the refusal is handed on: {:?}",
            results[0]
        );
        assert_eq!(bucket.deleted(), Vec::<String>::new());
        meta.land_now();
        assert_eq!(drive.get("big.bin").unwrap(), REPORT);
        let later = now_unix() + JOURNAL_STALE_SECS + 1;
        assert_eq!(
            drive.recover_packs(later).unwrap(),
            1,
            "the batch's object was written in the journal of what is in flight, as every other new \
             object whose change may have landed is"
        );
        assert_eq!(
            drive.get("big.bin").unwrap(),
            REPORT,
            "a named object is no garbage"
        );
    }

    /// The batch of large files (door 4), the answer that was lost: the swap landed, the read-back
    /// failed, the index can be asked afterwards and names the objects: the files are written. RED on
    /// the old code: the error arm told every file "failed" though the head names it.
    #[test]
    fn x_delta_sync7_1_a_batch_of_large_files_whose_swap_landed_and_whose_answer_was_lost_is_written(
    ) {
        let key = DriveKey::generate().unwrap();
        let data = MemoryBucket::new();
        let meta = Arc::new(LostAnswers::new(data.clone()));
        let drive = laptop(&data, &meta, &key);
        let mut first = Cursor::new(b"the first large file".to_vec());
        let mut second = Cursor::new(b"the second large file".to_vec());
        let mut items = vec![
            BatchStream::new("one.bin", &mut first as &mut (dyn std::io::Read + Send)),
            BatchStream::new("two.bin", &mut second as &mut (dyn std::io::Read + Send)),
        ];
        // The swap lands and answers as a lost connection; the one read that would tell fails too.
        meta.lose_next(keys::MANIFEST, Lost::Transport, 1);
        let results = drive.put_from_many(&mut items);
        assert!(
            results.iter().all(Result::is_ok),
            "the index names both files: {:?}",
            results.iter().filter_map(|r| r.as_ref().err()).next()
        );
        let other = phone(&data, &key);
        assert_eq!(other.get("one.bin").unwrap(), b"the first large file");
        assert_eq!(other.get("two.bin").unwrap(), b"the second large file");
    }

    #[test]
    fn a_file_whose_index_change_was_sent_again_and_met_itself_is_written() {
        let key = DriveKey::generate().unwrap();
        let data = MemoryBucket::new();
        let meta = Arc::new(LostAnswers::new(data.clone()));
        let drive = laptop(&data, &meta, &key);
        meta.lose_next(keys::MANIFEST, Lost::Resent, 0);
        drive
            .put("notes.txt", b"written once, its answer lost")
            .expect("the repeat met the write's own version: the write landed");
        assert_eq!(
            phone(&data, &key).get("notes.txt").unwrap(),
            b"written once, its answer lost"
        );
    }
}
