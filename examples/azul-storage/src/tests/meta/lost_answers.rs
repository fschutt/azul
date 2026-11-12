//! Writes that land while their answers are lost: a flapping network cuts the connection after
//! the bucket took a request, and the HTTP layer may send the same conditional request again,
//! which then meets its own new version. The repository must take its own write for its own,
//! and an encrypted file must never end up named without its object.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

use crate::meta::{
    keys, Bucket, Fetched, Kind, Listed, MemoryBucket, MetaError, MetaStore, PackWriter, Publish,
    RefUpdate, TestSealer, Version,
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
}

impl Answer {
    pub(crate) fn error(self) -> MetaError {
        match self {
            Answer::Transport => MetaError::Drive(DriveError::Transport("timed out".to_string())),
            Answer::Raced => MetaError::Raced {
                key: keys::MANIFEST.to_string(),
            },
            Answer::Denied => MetaError::Drive(DriveError::Denied {
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
}

impl LateSwap {
    pub(crate) fn new(inner: MemoryBucket) -> LateSwap {
        LateSwap {
            inner,
            armed: Mutex::new(None),
            pending: Mutex::new(None),
        }
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

    /// Whether the late swap is still on its way.
    pub(crate) fn on_its_way(&self) -> bool {
        self.pending.lock().unwrap().is_some()
    }
}

impl Bucket for LateSwap {
    fn read(&self, key: &str) -> Result<Option<(Vec<u8>, Version)>, MetaError> {
        let read = self.inner.read(key);
        self.land();
        read
    }
    fn read_if_changed(&self, key: &str, known: &str) -> Result<Fetched, MetaError> {
        let read = self.inner.read_if_changed(key, known);
        self.land();
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
fn publish_main(
    store: &mut MetaStore<&LostAnswers, TestSealer>,
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
    use std::sync::Arc;

    use super::{Lost, LostAnswers};
    use crate::{
        crypto::DriveKey,
        encrypted::{EncryptedDrive, NameIndex},
        meta::{keys, MemoryBucket, MetaIndex, MetaRepo, RepoOptions},
        Drive,
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
