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
