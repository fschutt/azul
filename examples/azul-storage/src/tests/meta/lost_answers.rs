//! Writes that land while their answers are lost: a flapping network cuts the connection after
//! the bucket took a request, and the HTTP layer may send the same conditional request again,
//! which then meets its own new version. The repository must take its own write for its own,
//! and an encrypted file must never end up named without its object.

use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    Arc, Mutex,
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

/// A bucket that dates what is written to it by a clock the test sets - the bucket's own time, which a
/// device's clock is not - over [`LateSwap`], whose swaps it can refuse and park. `dates: false` is a bucket that
/// tells no date at all.
struct Dated {
    inner: Arc<LateSwap>,
    clock: Arc<AtomicU64>,
    born: Mutex<std::collections::HashMap<String, u64>>,
    dates: bool,
}

impl Dated {
    fn new(inner: Arc<LateSwap>, clock: &Arc<AtomicU64>) -> Dated {
        Dated {
            inner,
            clock: Arc::clone(clock),
            born: Mutex::new(std::collections::HashMap::new()),
            dates: true,
        }
    }

    fn dating_nothing(mut self) -> Dated {
        self.dates = false;
        self
    }
}

impl Bucket for Dated {
    fn read(&self, key: &str) -> Result<Option<(Vec<u8>, Version)>, MetaError> {
        self.inner.read(key)
    }
    fn read_if_changed(&self, key: &str, known: &str) -> Result<Fetched, MetaError> {
        self.inner.read_if_changed(key, known)
    }
    fn read_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, MetaError> {
        Bucket::read_range(&*self.inner, key, range)
    }
    fn create(&self, key: &str, bytes: &[u8]) -> Result<Option<Version>, MetaError> {
        let made = self.inner.create(key, bytes)?;
        self.born
            .lock()
            .unwrap()
            .insert(key.to_string(), self.clock.load(Ordering::SeqCst));
        Ok(made)
    }
    fn replace(&self, key: &str, bytes: &[u8], known: &str) -> Result<Option<Version>, MetaError> {
        let made = self.inner.replace(key, bytes, known)?;
        // S3's Last-Modified moves with every PUT of the key, the manifest's with every swap.
        self.born
            .lock()
            .unwrap()
            .insert(key.to_string(), self.clock.load(Ordering::SeqCst));
        Ok(made)
    }
    fn remove(&self, key: &str) -> Result<(), MetaError> {
        self.inner.remove(key)?;
        self.born.lock().unwrap().remove(key);
        Ok(())
    }
    fn list_keys(&self, prefix: &str) -> Result<Vec<Listed>, MetaError> {
        self.inner.list_keys(prefix)
    }
    fn exists(&self, key: &str) -> Result<bool, MetaError> {
        self.inner.exists(key)
    }
    fn modified_of(&self, key: &str) -> Result<Option<u64>, MetaError> {
        if !self.dates {
            return Ok(None);
        }
        Ok(self.born.lock().unwrap().get(key).copied())
    }
}

const DAY: u64 = 86_400;

/// `secs` pass: the bucket's clock moves and so does the monotonic one - the two clocks a store's clear of leftovers
/// is judged by (never the device's: a test that moves only the device's clock `skew` changes nothing).
fn pass(time: &Arc<AtomicU64>, secs: u64) {
    time.fetch_add(secs, Ordering::SeqCst);
    crate::time::advance_monotonic(secs);
}

/// The keys of the repository's own objects under the folders the orphan sweep looks at.
fn repo_keys(bucket: &MemoryBucket) -> Vec<String> {
    let mut found = Vec::new();
    for dir in [keys::WAL_DIR, keys::LOG_DIR, keys::CHECKPOINT_DIR] {
        found.extend(
            bucket
                .list_keys(dir)
                .unwrap()
                .into_iter()
                .map(|listed| listed.key),
        );
    }
    found.sort();
    found
}

/// A store over a [`Dated`] bucket: the bucket's clock `time`, and its own clock `skew` ahead of it.
fn dated_store(
    late: &Arc<LateSwap>,
    dates: bool,
    time: &Arc<AtomicU64>,
    skew: &Arc<AtomicU64>,
) -> MetaStore<Arc<Dated>, TestSealer> {
    let dated = Dated::new(Arc::clone(late), time);
    let bucket = Arc::new(if dates { dated } else { dated.dating_nothing() });
    let device = {
        let (time, skew) = (Arc::clone(time), Arc::clone(skew));
        move || time.load(Ordering::SeqCst) + skew.load(Ordering::SeqCst)
    };
    MetaStore::create(bucket, sealer(), "laptop")
        .unwrap()
        .with_clock(device)
}

/// A bucket of the test: [`Dated`] over [`LateSwap`], shared by the runs of one test (a restart opens the same
/// bucket again).
fn dated_bucket(late: &Arc<LateSwap>, dates: bool, time: &Arc<AtomicU64>) -> Arc<Dated> {
    let dated = Dated::new(Arc::clone(late), time);
    Arc::new(if dates { dated } else { dated.dating_nothing() })
}

/// The device's clock: the bucket's time and `skew` seconds.
fn device_clock(
    time: &Arc<AtomicU64>,
    skew: &Arc<AtomicU64>,
) -> impl Fn() -> u64 + Send + Sync + 'static {
    let (time, skew) = (Arc::clone(time), Arc::clone(skew));
    move || time.load(Ordering::SeqCst) + skew.load(Ordering::SeqCst)
}

/// Where a test's store keeps the journal of its leftovers: the sealed bytes of the last write (a file of the
/// device, as far as the test goes).
#[derive(Clone, Default)]
struct Kept(Arc<Mutex<Option<Vec<u8>>>>);

impl Kept {
    fn sink(&self) -> impl Fn(&[u8]) + Send + Sync + 'static {
        let cell = Arc::clone(&self.0);
        move |sealed| *cell.lock().unwrap() = Some(sealed.to_vec())
    }

    fn bytes(&self) -> Option<Vec<u8>> {
        self.0.lock().unwrap().clone()
    }
}

/// What a bucket refuses (changing nothing) when a test says so: the create of a log entry, every write, one manifest
/// swap - and what a read does for a while after that swap. The refusals of a frozen drive between two PUTs.
struct Refusing {
    inner: MemoryBucket,
    /// Every create of a log entry answers `Denied`.
    entries: AtomicBool,
    /// Every create of a pack's index answers `Denied` (the pack, put first, went up).
    indexes: AtomicBool,
    /// Every create and replace answers `Denied` (the allowance is spent): deletes still go through.
    frozen: AtomicBool,
    /// The next replace of the manifest fails with this and does not land; every read fails `down` times after it.
    swap: Mutex<Option<(MetaError, usize)>>,
    reads_down: AtomicUsize,
}

impl Refusing {
    fn new(inner: MemoryBucket) -> Refusing {
        Refusing {
            inner,
            entries: AtomicBool::new(false),
            indexes: AtomicBool::new(false),
            frozen: AtomicBool::new(false),
            swap: Mutex::new(None),
            reads_down: AtomicUsize::new(0),
        }
    }

    fn denied() -> MetaError {
        MetaError::Drive(DriveError::Denied {
            message: "the drive is frozen".to_string(),
        })
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
}

impl Bucket for Refusing {
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
        if self.frozen.load(Ordering::SeqCst)
            || (self.entries.load(Ordering::SeqCst) && key.starts_with(keys::LOG_DIR))
            || (self.indexes.load(Ordering::SeqCst)
                && key.starts_with(keys::WAL_DIR)
                && key.ends_with(".idx"))
        {
            return Err(Refusing::denied());
        }
        self.inner.create(key, bytes)
    }
    fn replace(&self, key: &str, bytes: &[u8], known: &str) -> Result<Option<Version>, MetaError> {
        if self.frozen.load(Ordering::SeqCst) {
            return Err(Refusing::denied());
        }
        if key == keys::MANIFEST {
            if let Some((error, down)) = self.swap.lock().unwrap().take() {
                self.reads_down.store(down, Ordering::SeqCst);
                return Err(error);
            }
        }
        self.inner.replace(key, bytes, known)
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

/// Counts the looks a store takes at single objects (a HEAD each): `exists` and `modified_of`.
struct Counting<B> {
    inner: B,
    looks: AtomicUsize,
}

impl<B: Bucket> Bucket for Counting<B> {
    fn read(&self, key: &str) -> Result<Option<(Vec<u8>, Version)>, MetaError> {
        self.inner.read(key)
    }
    fn read_if_changed(&self, key: &str, known: &str) -> Result<Fetched, MetaError> {
        self.inner.read_if_changed(key, known)
    }
    fn read_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, MetaError> {
        self.inner.read_range(key, range)
    }
    fn create(&self, key: &str, bytes: &[u8]) -> Result<Option<Version>, MetaError> {
        self.inner.create(key, bytes)
    }
    fn replace(&self, key: &str, bytes: &[u8], known: &str) -> Result<Option<Version>, MetaError> {
        self.inner.replace(key, bytes, known)
    }
    fn remove(&self, key: &str) -> Result<(), MetaError> {
        self.inner.remove(key)
    }
    fn list_keys(&self, prefix: &str) -> Result<Vec<Listed>, MetaError> {
        self.inner.list_keys(prefix)
    }
    fn exists(&self, key: &str) -> Result<bool, MetaError> {
        self.looks.fetch_add(1, Ordering::SeqCst);
        self.inner.exists(key)
    }
    fn modified_of(&self, key: &str) -> Result<Option<u64>, MetaError> {
        self.looks.fetch_add(1, Ordering::SeqCst);
        self.inner.modified_of(key)
    }
}

/// The keys the store's manifest names (the manifest itself, the log, the packs and their indexes).
fn named_by(store: &MetaStore<impl Bucket, TestSealer>) -> std::collections::HashSet<String> {
    let manifest = store.manifest().expect("a manifest");
    let mut named: std::collections::HashSet<String> =
        manifest.log.iter().map(|entry| entry.key.clone()).collect();
    for pack in &manifest.packs {
        named.insert(keys::pack(&pack.name));
        named.insert(keys::idx(&pack.name));
    }
    named
}

/// The repository's own objects in `bucket` that no manifest of `store` names, sorted.
fn unnamed_in(bucket: &MemoryBucket, store: &MetaStore<impl Bucket, TestSealer>) -> Vec<String> {
    let named = named_by(store);
    repo_keys(bucket)
        .into_iter()
        .filter(|key| !named.contains(key))
        .collect()
}

fn sorted(mut keys: Vec<String>) -> Vec<String> {
    keys.sort();
    keys
}

/// Sets the main ref to a value that carries the head it was built on: a retry after a lost race builds another
/// pack than the attempt that lost.
fn publish_at_head(
    store: &mut MetaStore<impl Bucket, TestSealer>,
    value: &str,
) -> Result<Option<crate::meta::Published>, MetaError> {
    store.publish(|state| {
        let mut pack = PackWriter::new();
        pack.add(
            Kind::Blob,
            format!("{value} at {}", state.head_seq).into_bytes(),
        );
        Ok(Some(Publish {
            pack: Some(pack),
            updates: vec![RefUpdate {
                name: MAIN.to_string(),
                old: state.refs.get(MAIN).cloned(),
                new: Some(format!("{value} at {}", state.head_seq)),
            }],
            message: format!("set {value}"),
        }))
    })
}

/// X-DELTA-SYNC7-4. A drive that refuses every manifest swap (frozen, over its quota) is tried again by every
/// pass, and each try leaves its log entry, its pack and the pack's index: the sweep that would free them is a
/// swap itself, which such a drive refuses too, and what they weigh is the allowance the drive keeps for its
/// owner's next commit. The store that left them writes them down and removes them itself once they are two
/// days old by the bucket's dates (the age of an orphan before the sweep deletes it) and no manifest names
/// them - a delete is what a frozen drive still takes. A device whose clock runs far ahead cannot make a fresh
/// object old, and a bucket that dates nothing clears nothing. RED on the old code: the bucket holds the
/// leftovers of every try (3 more per try).
#[test]
fn x_delta_sync7_4_what_refused_publishes_leave_is_removed_by_the_store_once_it_is_old_and_named_by_nothing() {
    let time = Arc::new(AtomicU64::new(1_000_000));
    let skew = Arc::new(AtomicU64::new(0));
    let data = MemoryBucket::new();
    let late = Arc::new(LateSwap::new(data.clone()));
    let mut store = dated_store(&late, true, &time, &skew);
    publish_main(&mut store, "one").unwrap();
    let named = repo_keys(&data);

    let mut counts = Vec::new();
    for value in ["two", "three", "four", "five", "six"] {
        pass(&time, 2 * DAY);
        late.answer_next(keys::MANIFEST, Answer::Denied);
        let refused = publish_main(&mut store, value);
        assert!(
            matches!(refused, Err(MetaError::Drive(DriveError::Denied { .. }))),
            "{refused:?}"
        );
        counts.push(repo_keys(&data).len());
    }
    assert_eq!(
        counts[0],
        named.len() + 3,
        "the first refused try leaves its entry, its pack and its index"
    );
    assert!(
        counts.iter().all(|n| *n == counts[0]),
        "every later try removed the leftovers of the one before it (two days old, named by nothing): {counts:?}"
    );

    // A try at once after the last one removes nothing: what is not old yet may still be named. A device whose
    // clock is a year ahead changes nothing of that - the bucket's own dates decide.
    skew.store(400 * DAY, Ordering::SeqCst);
    let before = repo_keys(&data);
    late.answer_next(keys::MANIFEST, Answer::Denied);
    assert!(publish_main(&mut store, "seven").is_err());
    assert!(
        before.iter().all(|key| repo_keys(&data).contains(key)),
        "a leftover younger than two days by the bucket's dates is kept, whatever the device's clock says"
    );
    skew.store(0, Ordering::SeqCst);

    // The drive thaws: the next publish lands, and the repository is what the swaps said.
    pass(&time, 2 * DAY);
    publish_main(&mut store, "eight").unwrap();
    assert_eq!(
        main_ref_as_another_device_sees_it(&data).as_deref(),
        Some("eight")
    );
    assert_eq!(
        repo_keys(&data).len(),
        named.len() + 3,
        "nothing of the refused tries is left: the entry, the pack and the index of the publish that landed"
    );
}

/// X-DELTA-SYNC7-4, X-DELTA-SYNC9-2: a bucket that tells no date at all does not matter to what THIS run left - the
/// monotonic clock ages it - and clears nothing an earlier run left, which only the bucket's dates could age: what it
/// cannot date is not old (the control of the old test, split in two since the age of a key of this run no longer
/// comes from the bucket).
#[test]
fn x_delta_sync7_4_a_bucket_that_dates_nothing_ages_what_this_run_left_by_the_monotonic_clock_and_nothing_else(
) {
    let time = Arc::new(AtomicU64::new(1_000_000));
    let skew = Arc::new(AtomicU64::new(0));
    let data = MemoryBucket::new();
    let late = Arc::new(LateSwap::new(data.clone()));
    let kept = Kept::default();
    let bucket = dated_bucket(&late, false, &time);
    let mut store = MetaStore::create(Arc::clone(&bucket), sealer(), "laptop")
        .unwrap()
        .with_clock(device_clock(&time, &skew))
        .keep_leftovers(None, kept.sink());
    publish_main(&mut store, "one").unwrap();
    let named = repo_keys(&data).len();
    // This run: removed after two days of its own time, whatever the bucket says (or does not say).
    for (n, value) in ["two", "three", "four"].into_iter().enumerate() {
        pass(&time, 3 * DAY);
        late.answer_next(keys::MANIFEST, Answer::Denied);
        assert!(publish_main(&mut store, value).is_err());
        assert_eq!(
            repo_keys(&data).len(),
            named + 3,
            "try {n}: the leftovers of the one before it went, by the monotonic clock"
        );
    }
    // An earlier run's: the bucket dates nothing, so nothing can be said to be old, however long ago.
    let saved = kept.bytes().expect("the journal");
    drop(store);
    let before = repo_keys(&data);
    pass(&time, 30 * DAY);
    let mut again = MetaStore::open(Arc::clone(&bucket), sealer(), "laptop")
        .unwrap()
        .with_clock(device_clock(&time, &skew))
        .keep_leftovers(Some(&saved), kept.sink());
    assert_eq!(
        again.leftover_keys().len(),
        3,
        "the journal brought the keys"
    );
    publish_main(&mut again, "five").unwrap();
    assert!(
        before.iter().all(|key| repo_keys(&data).contains(key)),
        "a bucket that dates nothing clears nothing of an earlier run's: what it cannot date is not old"
    );
    assert_eq!(
        again.leftover_keys().len(),
        3,
        "they stay on the list for a later look"
    );
}

/// X-DELTA-SYNC7-4, the cell that must not move: the swap the transport sent again was refused for policy, the
/// FIRST request lands at the node a day later, and the manifest it makes names the log entry and the pack the
/// refused try left. The store finds them named and lets them go off its list unharmed: removing them would
/// break the repository for every device (X-DELTA-SYNC6-2).
#[test]
fn x_delta_sync7_4_a_leftover_a_manifest_names_after_all_is_never_removed() {
    let time = Arc::new(AtomicU64::new(1_000_000));
    let skew = Arc::new(AtomicU64::new(0));
    let data = MemoryBucket::new();
    let late = Arc::new(LateSwap::new(data.clone()).held());
    let mut store = dated_store(&late, true, &time, &skew);
    late.answer_next(keys::MANIFEST, Answer::DeniedFirstLands);
    assert!(publish_main(&mut store, "one").is_err());
    assert!(late.on_its_way());
    // Three days later the first request lands: the manifest names what the refused try left.
    pass(&time, 3 * DAY);
    late.land_now();
    // The next publish polls, finds the leftovers named, and removes nothing.
    publish_main(&mut store, "two").unwrap();
    assert_eq!(
        main_ref_as_another_device_sees_it(&data).as_deref(),
        Some("two"),
        "every entry the manifest names is still there"
    );
}

/// X-DELTA-SYNC9-2 (a): the pack of an attempt whose ENTRY the bucket refuses is written down. The pack and its index
/// went up, the entry's create is refused (the allowance ended between two objects, a freeze began) and the swap is
/// never tried: nothing named the pack, and the leftover of the old code was only noted after the swap's refusal. Two
/// days later the store removes both. RED on the old code: they stay in the bucket for good (only the orphan sweep,
/// which is a swap itself, could find them).
#[test]
fn x_delta_sync9_2_the_pack_of_an_entry_the_bucket_refuses_is_written_down_and_removed_once_it_is_old(
) {
    let data = MemoryBucket::new();
    let bucket = Arc::new(Refusing::new(data.clone()));
    let mut store = MetaStore::create(Arc::clone(&bucket), sealer(), "laptop").unwrap();
    publish_main(&mut store, "one").unwrap();
    let named = repo_keys(&data);

    bucket.entries.store(true, Ordering::SeqCst);
    let refused = publish_main(&mut store, "two");
    assert!(
        matches!(refused, Err(MetaError::Drive(DriveError::Denied { .. }))),
        "{refused:?}"
    );
    let left: Vec<String> = repo_keys(&data)
        .into_iter()
        .filter(|key| !named.contains(key))
        .collect();
    assert_eq!(
        left.len(),
        2,
        "the pack and its index went up, the entry did not: {left:?}"
    );
    assert!(
        left.iter().all(|key| store.leftover_keys().contains(key)),
        "the pack of the refused entry is on the list: {:?}",
        store.leftover_keys()
    );

    // The drive takes entries again; two days later a publish removes them before it uploads anything.
    bucket.entries.store(false, Ordering::SeqCst);
    crate::time::advance_monotonic(2 * DAY);
    publish_main(&mut store, "three").unwrap();
    assert!(
        left.iter().all(|key| !repo_keys(&data).contains(key)),
        "two days old and named by nothing: gone"
    );
    assert_eq!(
        main_ref_as_another_device_sees_it(&data).as_deref(),
        Some("three")
    );
}

/// X-DELTA-SYNC9-2 (a), the other half of the same cell: the PACK went up and its INDEX is refused. The pack is written
/// down before the error is handed on - `put_pack` returns, the publish ends, and the entry is never tried - and goes
/// two days later. RED when `put_pack` does not write it down: it stays in the bucket for good.
#[test]
fn x_delta_sync9_2_a_pack_whose_index_the_bucket_refuses_is_written_down_and_removed_once_it_is_old(
) {
    let data = MemoryBucket::new();
    let bucket = Arc::new(Refusing::new(data.clone()));
    let mut store = MetaStore::create(Arc::clone(&bucket), sealer(), "laptop").unwrap();
    publish_main(&mut store, "one").unwrap();
    let named = repo_keys(&data);

    bucket.indexes.store(true, Ordering::SeqCst);
    let refused = publish_main(&mut store, "two");
    assert!(
        matches!(refused, Err(MetaError::Drive(DriveError::Denied { .. }))),
        "{refused:?}"
    );
    let left: Vec<String> = repo_keys(&data)
        .into_iter()
        .filter(|key| !named.contains(key))
        .collect();
    assert_eq!(left.len(), 1, "only the pack went up: {left:?}");
    assert!(
        left[0].ends_with(".pack") && store.leftover_keys().contains(&left[0]),
        "the pack is on the list: {:?}",
        store.leftover_keys()
    );

    bucket.indexes.store(false, Ordering::SeqCst);
    crate::time::advance_monotonic(2 * DAY);
    publish_main(&mut store, "three").unwrap();
    assert!(
        !repo_keys(&data).contains(&left[0]),
        "two days old and named by nothing: gone"
    );
}

/// X-DELTA-SYNC9-2 (c): a drive whose allowance the leftovers have spent refuses the pack's PUT first, and the clear
/// that would make room must not wait for a write: it runs at the top of the publish, before the first byte. RED on
/// the old code: the clear sat behind the entry's create, which the refused pack never reached, so the leftovers
/// stayed for good and the owner's next commit stayed refused.
#[test]
fn x_delta_sync9_2_a_publish_the_bucket_refuses_at_its_first_put_still_clears_what_is_old() {
    let data = MemoryBucket::new();
    let bucket = Arc::new(Refusing::new(data.clone()));
    let mut store = MetaStore::create(Arc::clone(&bucket), sealer(), "laptop").unwrap();
    publish_main(&mut store, "one").unwrap();
    let named = repo_keys(&data);
    *bucket.swap.lock().unwrap() = Some((Refusing::denied(), 0));
    assert!(publish_main(&mut store, "two").is_err());
    let left: Vec<String> = repo_keys(&data)
        .into_iter()
        .filter(|key| !named.contains(key))
        .collect();
    assert_eq!(left.len(), 3);

    // Now nothing is taken: the allowance is spent. Days pass; the next publish is refused at its pack ...
    bucket.frozen.store(true, Ordering::SeqCst);
    crate::time::advance_monotonic(3 * DAY);
    let refused = publish_main(&mut store, "three");
    assert!(
        matches!(refused, Err(MetaError::Drive(DriveError::Denied { .. }))),
        "{refused:?}"
    );
    // ... and has removed the leftovers first: deletes are what a frozen drive still takes.
    assert!(
        left.iter().all(|key| !repo_keys(&data).contains(key)),
        "the leftovers are gone, so the room is back: {:?}",
        repo_keys(&data)
    );
}

/// X-DELTA-SYNC9-2 (b): the journal is the device's state - written before the refused publish returns, read at the
/// start of the next run - so a kill between the refusal and the clear loses nothing. An earlier run's keys have no
/// age of this process: the bucket's own dates judge them, against the newest object the store knows of (the manifest,
/// the last key), a lower bound of the bucket's time, so a run that finds them not old yet looks again at the next
/// start. RED on a store that does not read its journal: the keys stay in the bucket for good.
#[test]
fn x_delta_sync9_2_the_journal_survives_a_kill_and_a_later_start_removes_what_the_bucket_dates_old()
{
    let time = Arc::new(AtomicU64::new(1_000_000));
    let skew = Arc::new(AtomicU64::new(0));
    let data = MemoryBucket::new();
    let late = Arc::new(LateSwap::new(data.clone()));
    let bucket = dated_bucket(&late, true, &time);
    let kept = Kept::default();
    let mut store = MetaStore::create(Arc::clone(&bucket), sealer(), "laptop")
        .unwrap()
        .with_clock(device_clock(&time, &skew))
        .keep_leftovers(None, kept.sink());
    publish_main(&mut store, "one").unwrap();
    let named = repo_keys(&data);
    pass(&time, 2 * DAY);
    late.answer_next(keys::MANIFEST, Answer::Denied);
    assert!(publish_main(&mut store, "two").is_err());
    let left = unnamed_in(&data, &store);
    assert_eq!(left.len(), 3, "{left:?} of {named:?}");
    assert_eq!(
        sorted(store.leftover_keys()),
        left,
        "the journal names what the try left"
    );
    let saved = kept
        .bytes()
        .expect("the journal was written before the refused publish returned");
    // The kill.
    drop(store);

    // A new process, two days later by the bucket's clock. Its first publish finds the keys not old against what it
    // knows of the bucket's time (the manifest was last swapped before them): they stay, on the list.
    pass(&time, 2 * DAY);
    let mut again = MetaStore::open(Arc::clone(&bucket), sealer(), "laptop")
        .unwrap()
        .with_clock(device_clock(&time, &skew))
        .keep_leftovers(Some(&saved), kept.sink());
    assert_eq!(
        sorted(again.leftover_keys()),
        left,
        "the new process has the journal"
    );
    publish_main(&mut again, "three").unwrap();
    assert!(
        left.iter().all(|key| repo_keys(&data).contains(key)),
        "not old against the manifest of the last swap: kept for now"
    );
    drop(again);

    // The next start: the manifest was swapped two days after them.
    pass(&time, DAY);
    let saved = kept.bytes().expect("the journal");
    let mut third = MetaStore::open(Arc::clone(&bucket), sealer(), "laptop")
        .unwrap()
        .with_clock(device_clock(&time, &skew))
        .keep_leftovers(Some(&saved), kept.sink());
    publish_main(&mut third, "four").unwrap();
    assert!(
        left.iter().all(|key| !repo_keys(&data).contains(key)),
        "two days older than the last swap by the bucket's own dates, named by nothing: gone"
    );
    assert!(
        third.leftover_keys().is_empty(),
        "{:?}",
        third.leftover_keys()
    );
    assert_eq!(
        main_ref_as_another_device_sees_it(&data).as_deref(),
        Some("four")
    );
}

/// X-DELTA-SYNC9-2: the journal is bounded to the newest keys, and a restart reads exactly what was written. (The bound is
/// narrowed to 30 keys for this thread: fifteen refused tries leave 45.)
#[test]
fn x_delta_sync9_2_the_journal_is_bounded_and_a_restart_reads_what_was_written() {
    let _narrow = crate::meta::wal::narrow_leftovers_kept(30);
    let data = MemoryBucket::new();
    let late = Arc::new(LateSwap::new(data.clone()));
    let kept = Kept::default();
    let mut store = MetaStore::create(Arc::clone(&late), sealer(), "laptop")
        .unwrap()
        .keep_leftovers(None, kept.sink());
    publish_main(&mut store, "one").unwrap();
    for i in 0..15 {
        late.answer_next(keys::MANIFEST, Answer::Denied);
        assert!(publish_main(&mut store, &format!("v{i}")).is_err());
    }
    let list = store.leftover_keys();
    assert_eq!(
        list.len(),
        30,
        "bounded: 45 keys were left, the newest 30 are kept"
    );
    let saved = kept.bytes().expect("the journal");
    let again = MetaStore::open(Arc::clone(&late), sealer(), "laptop")
        .unwrap()
        .keep_leftovers(Some(&saved), kept.sink());
    assert_eq!(
        again.leftover_keys(),
        list,
        "a restart reads exactly the keys that were written, in order"
    );
}

/// X-DELTA-SYNC9-2, the file of a device: a repository over a cache folder keeps the journal there, next to its
/// state, written before the refused commit returns, and the next run of the same folder has it.
#[test]
fn x_delta_sync9_2_a_device_with_a_cache_folder_keeps_the_journal_in_a_file_the_next_run_reads() {
    use crate::meta::{merge::keep_both, Change, MetaRepo, RepoOptions};

    let dir = super::TempDir::new("leftovers-journal");
    let options = RepoOptions {
        cache_dir: Some(dir.path().to_path_buf()),
        ..RepoOptions::default()
    };
    let data = MemoryBucket::new();
    let late = Arc::new(LateSwap::new(data.clone()));
    let mut repo = MetaRepo::create_with(
        Arc::clone(&late),
        sealer(),
        "dev-laptop",
        "Laptop",
        &options,
    )
    .unwrap();
    let id = repo.write_blob(b"the first file");
    let first = [Change::Put {
        path: "a.txt".to_string(),
        id,
    }];
    repo.commit(&first, "first", &mut keep_both).unwrap();
    let named = repo_keys(&data);
    late.answer_next(keys::MANIFEST, Answer::Denied);
    let id = repo.write_blob(b"the second file");
    let second = [Change::Put {
        path: "b.txt".to_string(),
        id,
    }];
    assert!(repo.commit(&second, "second", &mut keep_both).is_err());
    let left: Vec<String> = repo_keys(&data)
        .into_iter()
        .filter(|key| !named.contains(key))
        .collect();
    assert_eq!(
        left.len(),
        3,
        "the refused commit left its entry, its pack and its index"
    );
    assert!(
        dir.path().join("leftovers").exists(),
        "the journal is a file in the cache folder"
    );
    drop(repo);

    let again = MetaRepo::open_with(
        Arc::clone(&late),
        sealer(),
        "dev-laptop",
        "Laptop",
        &options,
    )
    .unwrap();
    assert_eq!(
        sorted(again.store().leftover_keys()),
        left,
        "the next run of the same folder has what the refused commit left"
    );
}

/// X-DELTA-SYNC9-3: the leftovers are looked at once per start and once per two days, not at every attempt of every
/// publish. 67 refused tries leave 201 keys; a new run brings them and publishes five times: the keys are not old
/// yet, so the first look ends at the first of them (the two dates of what the store knows of the bucket's time and
/// that one: three HEADs) and the other publishes look at nothing. Two days later it looks again. RED on the old
/// loop: every attempt asked 1 + 2 x 201 HEADs before its swap.
#[test]
fn x_delta_sync9_3_a_long_list_of_young_leftovers_costs_a_look_at_the_first_and_nothing_per_publish(
) {
    let time = Arc::new(AtomicU64::new(1_000_000));
    let skew = Arc::new(AtomicU64::new(0));
    let data = MemoryBucket::new();
    let late = Arc::new(LateSwap::new(data.clone()));
    let bucket = dated_bucket(&late, true, &time);
    let kept = Kept::default();
    let mut store = MetaStore::create(Arc::clone(&bucket), sealer(), "laptop")
        .unwrap()
        .with_clock(device_clock(&time, &skew))
        .keep_leftovers(None, kept.sink());
    publish_main(&mut store, "one").unwrap();
    for i in 0..67 {
        late.answer_next(keys::MANIFEST, Answer::Denied);
        assert!(publish_main(&mut store, &format!("v{i}")).is_err());
    }
    assert_eq!(store.leftover_keys().len(), 201);
    let saved = kept.bytes().expect("the journal");
    drop(store);

    let counting = Arc::new(Counting {
        inner: Arc::clone(&bucket),
        looks: AtomicUsize::new(0),
    });
    let mut again = MetaStore::open(Arc::clone(&counting), sealer(), "laptop")
        .unwrap()
        .with_clock(device_clock(&time, &skew))
        .keep_leftovers(Some(&saved), kept.sink());
    let looks = || counting.looks.load(Ordering::SeqCst);
    publish_main(&mut again, "five-a").unwrap();
    let first = looks();
    assert!(
        first <= 4,
        "the first publish of a start looks at the first key only: {first} looks"
    );
    for value in ["five-b", "five-c", "five-d", "five-e"] {
        publish_main(&mut again, value).unwrap();
    }
    assert_eq!(
        looks(),
        first,
        "no later publish of the two days looks at anything"
    );
    assert_eq!(again.leftover_keys().len(), 201, "young: all kept");

    pass(&time, 2 * DAY);
    publish_main(&mut again, "later").unwrap();
    assert!(looks() > first, "two days later the store looks again");
}

/// X-DELTA-SYNC9-4: the premise of the removal stated where the deleter is. A signed request lands within
/// [`REQUEST_WINDOW_SECS`] of its signing or never - the node's own rule, which `crate::s3::MAX_CLOCK_SKEW_SECS` is
/// the client's copy of - and a leftover must have outlived a generous number of such windows (192: two days, what an
/// orphan waits before the sweep deletes it). The test ties the constants to each other and to the window.
#[test]
fn x_delta_sync9_4_the_age_a_leftover_must_reach_is_many_windows_of_the_nodes_request_window() {
    use crate::meta::{wal, Maintenance};

    assert_eq!(
        wal::REQUEST_WINDOW_SECS,
        crate::s3::MAX_CLOCK_SKEW_SECS,
        "the window is the node's"
    );
    assert_eq!(wal::REQUEST_WINDOW_SECS, 15 * 60, "SigV4's fifteen minutes");
    assert_eq!(
        wal::LEFTOVER_MIN_AGE_SECS,
        wal::LEFTOVER_WINDOWS * wal::REQUEST_WINDOW_SECS,
        "the age is a number of windows"
    );
    assert!(
        wal::LEFTOVER_MIN_AGE_SECS >= 96 * wal::REQUEST_WINDOW_SECS,
        "at least ninety-six windows: a day by the node's own rule"
    );
    assert_eq!(wal::LEFTOVER_MIN_AGE_SECS, 2 * DAY);
    let orphans = Maintenance::default();
    assert!(
        wal::LEFTOVER_MIN_AGE_SECS >= orphans.orphan_age,
        "never less than an orphan waits before the sweep retires it"
    );
    assert!(
        wal::LEFTOVER_EVERY_SECS >= wal::LEFTOVER_MIN_AGE_SECS,
        "a look is not due before a key can be old"
    );
}

/// X-DELTA-SYNC9-4, the cell the age exists for, pinned: a request that lands AFTER the age - the node did not keep
/// its window - makes a manifest that names the entry and the pack the clear has removed, and every device then reads
/// "the manifest names it, but it is missing". The age is the only guard, which is why it is a number of windows and
/// not the poll: against a request that lands after the poll the poll is no guard.
#[test]
fn x_delta_sync9_4_a_request_that_lands_after_the_clear_breaks_the_manifest_the_age_is_the_only_guard(
) {
    let data = MemoryBucket::new();
    let late = Arc::new(LateSwap::new(data.clone()).held());
    let mut store = MetaStore::create(Arc::clone(&late), sealer(), "laptop").unwrap();
    late.answer_next(keys::MANIFEST, Answer::DeniedFirstLands);
    assert!(publish_main(&mut store, "one").is_err());
    assert!(late.on_its_way());
    // Three days pass (the premise says the request is gone by now); the next publish clears what the refused try left
    // - and the first request lands right before that publish's own swap.
    crate::time::advance_monotonic(3 * DAY);
    let parked = Arc::clone(&late);
    data.before_next_replace(keys::MANIFEST, move || parked.land_now());
    let broken = publish_main(&mut store, "two");
    assert!(
        matches!(&broken, Err(MetaError::Corrupt { reason, .. }) if reason.contains("is missing")),
        "a manifest that names a removed entry: {broken:?}"
    );
    let other = MetaStore::open(data.clone(), sealer(), "phone");
    assert!(
        matches!(&other, Err(MetaError::Corrupt { reason, .. }) if reason.contains("is missing")),
        "every other device reads the same: {:?}",
        other.map(|_| ())
    );
}

/// X-DELTA-SYNC9-5: a pack of identical objects meets an old leftover's name. "six" is refused (its swap), two days
/// pass, "six" is published again: the same objects, the same pack name. The old clear ran after the new entry was
/// written and before its swap, found `pack/H` old and named by nothing - the swap that names it was not made yet - and
/// removed the pack the swap then named. The clear now runs before the attempt's first byte, so the pack the swap names
/// is made after it. RED on the old code: the repository's pack is missing.
#[test]
fn x_delta_sync9_5_a_pack_of_identical_objects_meets_an_old_leftover_and_the_pack_the_swap_names_is_there(
) {
    let time = Arc::new(AtomicU64::new(1_000_000));
    let skew = Arc::new(AtomicU64::new(0));
    let data = MemoryBucket::new();
    let late = Arc::new(LateSwap::new(data.clone()));
    let mut store = dated_store(&late, true, &time, &skew);
    publish_main(&mut store, "one").unwrap();
    late.answer_next(keys::MANIFEST, Answer::Denied);
    assert!(publish_main(&mut store, "six").is_err());
    pass(&time, 2 * DAY);
    publish_main(&mut store, "six").unwrap();

    let other = MetaStore::open(data.clone(), sealer(), "phone").unwrap();
    assert_eq!(
        other.state().refs.get(MAIN).map(String::as_str),
        Some("six")
    );
    for pack in &other.state().packs {
        let mut objects = crate::meta::Objects::new();
        other
            .fetch_pack(pack, &mut objects)
            .unwrap_or_else(|e| panic!("the pack {} the manifest names is there: {e}", pack.name));
    }
}

/// X-DELTA-SYNC9-6 (the 409 arm): a swap that answers 409 may be the swap itself, still under way: its entry stays
/// and is written down, with the pack. Here the first swap never lands and the retry does; the entry the retry
/// replaced is unnamed, and the pack - the same objects - is named. Two days later the entry goes and the pack stays.
/// RED when the arm does not write the entry down: it stays in the bucket for good.
#[test]
fn x_delta_sync9_6_the_409_arm_writes_the_entry_down_and_the_clear_leaves_what_a_manifest_names() {
    let data = MemoryBucket::new();
    let late = Arc::new(LateSwap::new(data.clone()).held());
    let mut store = MetaStore::create(Arc::clone(&late), sealer(), "laptop").unwrap();
    publish_main(&mut store, "one").unwrap();
    late.answer_next(keys::MANIFEST, Answer::Raced);
    let published = publish_main(&mut store, "two")
        .unwrap()
        .expect("the retry lands");
    assert_eq!(published.attempts, 2, "the first swap answered 409");
    let unnamed = unnamed_in(&data, &store);
    assert_eq!(
        unnamed.len(),
        1,
        "the first attempt's entry stays: {unnamed:?}"
    );
    assert!(unnamed[0].starts_with(keys::LOG_DIR));
    assert!(
        store.leftover_keys().contains(&unnamed[0]),
        "the 409 arm wrote the entry down: {:?}",
        store.leftover_keys()
    );
    let pack_keys: Vec<String> = store
        .leftover_keys()
        .into_iter()
        .filter(|key| key.starts_with(keys::WAL_DIR))
        .collect();
    assert_eq!(pack_keys.len(), 2, "and the pack with its index");

    crate::time::advance_monotonic(2 * DAY);
    publish_main(&mut store, "three").unwrap();
    assert!(
        !repo_keys(&data).contains(&unnamed[0]),
        "two days old and named by nothing: the entry is gone"
    );
    assert!(
        pack_keys.iter().all(|key| repo_keys(&data).contains(key)),
        "the pack the manifest names is not removed"
    );
}

/// X-DELTA-SYNC9-6 (the unknown arm): a swap whose conflict cannot be read back is an unknown outcome - the entry, the
/// pack and its index stay, written down. Here the swap did not land. Two days later all three are gone. RED when the
/// arm does not write them down.
#[test]
fn x_delta_sync9_6_the_unknown_arm_writes_the_entry_and_the_pack_down() {
    let data = MemoryBucket::new();
    let bucket = Arc::new(Refusing::new(data.clone()));
    let mut store = MetaStore::create(Arc::clone(&bucket), sealer(), "laptop").unwrap();
    publish_main(&mut store, "one").unwrap();
    let named = repo_keys(&data);
    *bucket.swap.lock().unwrap() = Some((
        MetaError::Conflict {
            key: keys::MANIFEST.to_string(),
        },
        1_000,
    ));
    let unknown = publish_main(&mut store, "two");
    assert!(
        matches!(unknown, Err(MetaError::OutcomeUnknown { .. })),
        "{unknown:?}"
    );
    bucket.reads_down.store(0, Ordering::SeqCst);
    let left: Vec<String> = repo_keys(&data)
        .into_iter()
        .filter(|key| !named.contains(key))
        .collect();
    assert_eq!(left.len(), 3, "{left:?}");
    assert_eq!(
        sorted(store.leftover_keys()),
        left,
        "the unknown arm wrote all three down"
    );

    crate::time::advance_monotonic(2 * DAY);
    publish_main(&mut store, "three").unwrap();
    assert!(
        left.iter().all(|key| !repo_keys(&data).contains(key)),
        "two days old and named by nothing: gone"
    );
}

/// X-DELTA-SYNC9-6 (the lost race): the attempt that loses (412, another device's manifest) removes its entry at once
/// and leaves its pack and index, which the retry does not need - it builds another - and writes them down. RED when
/// the arm does not write them down: they stay in the bucket for good.
#[test]
fn x_delta_sync9_6_the_lost_race_writes_the_pack_down_and_the_entry_goes_at_once() {
    let data = MemoryBucket::new();
    let mut store = MetaStore::create(data.clone(), sealer(), "laptop").unwrap();
    publish_main(&mut store, "one").unwrap();
    let mut phone = MetaStore::open(data.clone(), sealer(), "phone").unwrap();
    // Another device's publish lands right before the laptop's swap.
    data.before_next_replace(keys::MANIFEST, move || {
        publish_main(&mut phone, "phone").unwrap();
    });
    let landed = publish_at_head(&mut store, "two")
        .unwrap()
        .expect("the retry lands");
    assert_eq!(landed.attempts, 2, "the first attempt lost the race");
    let unnamed = unnamed_in(&data, &store);
    assert_eq!(
        unnamed.len(),
        2,
        "the lost attempt's pack and index, not its entry: {unnamed:?}"
    );
    assert!(unnamed.iter().all(|key| key.starts_with(keys::WAL_DIR)));
    assert_eq!(sorted(store.leftover_keys()), unnamed, "written down");

    crate::time::advance_monotonic(2 * DAY);
    publish_main(&mut store, "three").unwrap();
    assert!(
        unnamed.iter().all(|key| !repo_keys(&data).contains(key)),
        "two days old and named by nothing: gone"
    );
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
