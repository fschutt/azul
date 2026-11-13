//! X-DELTA-AZDRIVE-KEYS-43 (AZ-C3 scenario B, AZ-M5): an object whose size the index names is
//! asked for WITHIN that size - an encrypted file, a pack (whole, hot, or for a repack) - so a
//! storage that streams on, or answers with a body swapped for a bigger one, is refused on its
//! way in and not after the memory is full. These reads were plain `Drive::get`s, which take
//! whatever comes and compare the length afterwards.

use std::sync::{Arc, Mutex, PoisonError};

use super::mem_bucket::MemBucket;
use crate::{
    crypto::{random_bytes, DriveKey, ObjectId},
    encrypted::{EncryptedDrive, MemoryIndex},
    limits::Limit,
    packing::{PackOptions, HOT_READS, PACK_MAX},
    BatchPut, ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition,
};

/// A bucket that writes down how each object is read - the limit of each `get_limited`, each plain
/// `get` - and that, like a transport with a limit, refuses an object over the limit before it
/// hands out a byte of it.
struct Watched {
    inner: MemBucket,
    limited: Mutex<Vec<(String, u64)>>,
    plain: Mutex<Vec<String>>,
    /// Whether `head` fails (a drive that cannot say a size).
    head_fails: Mutex<bool>,
}

impl Watched {
    fn new() -> Watched {
        Watched {
            inner: MemBucket::new(),
            limited: Mutex::new(Vec::new()),
            plain: Mutex::new(Vec::new()),
            head_fails: Mutex::new(false),
        }
    }

    fn limits_of(&self, key: &str) -> Vec<u64> {
        self.limited
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(k, _)| k == key)
            .map(|(_, max)| *max)
            .collect()
    }

    fn plain_gets_of(&self, key: &str) -> usize {
        self.plain
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|k| *k == key)
            .count()
    }

    fn forget(&self) {
        self.limited
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        self.plain
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    /// The bucket keys of the data objects (packs and files with objects of their own).
    fn data_objects(&self) -> Vec<String> {
        self.inner
            .keys()
            .into_iter()
            .filter(|key| ObjectId::from_bucket_key(key).is_some())
            .collect()
    }
}

impl Drive for Watched {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.plain
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(key.to_string());
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.inner.get_range(key, range)
    }
    fn get_limited(&self, key: &str, limit: Limit) -> Result<Vec<u8>, DriveError> {
        self.limited
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((key.to_string(), limit.max));
        let info = self.inner.head(key)?;
        limit.check_object(key, info.size)?;
        self.inner.get(key)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        if *self
            .head_fails
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
        {
            return Err(DriveError::Unsupported(String::from("no sizes here")));
        }
        self.inner.head(key)
    }
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.inner.put_if(key, bytes, condition)
    }
    fn conditional_writes(&self) -> bool {
        self.inner.conditional_writes()
    }
}

fn noise(len: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; len];
    random_bytes(&mut bytes).unwrap();
    bytes
}

fn drive_over(watched: &Arc<Watched>) -> EncryptedDrive<Arc<Watched>> {
    EncryptedDrive::new(
        Arc::clone(watched),
        DriveKey::generate().unwrap(),
        Arc::new(MemoryIndex::new()),
    )
}

#[test]
fn x_delta_azdrive_keys_43_an_encrypted_file_is_asked_for_within_the_size_its_index_names() {
    let watched = Arc::new(Watched::new());
    let drive = drive_over(&watched);
    let plain = noise(20_000);
    drive.put("a.bin", &plain).unwrap();
    let object = drive.entry("a.bin").unwrap().object.unwrap();
    let key = object.id.bucket_key();
    watched.forget();

    assert_eq!(drive.get("a.bin").unwrap(), plain);
    assert_eq!(
        watched.limits_of(&key),
        vec![object.stored_size],
        "asked within the index's size"
    );
    assert_eq!(
        watched.plain_gets_of(&key),
        0,
        "never by a plain GET that takes whatever comes"
    );
}

#[test]
fn x_delta_azdrive_keys_43_an_object_bigger_than_the_index_names_is_refused_on_its_way_in() {
    let watched = Arc::new(Watched::new());
    let drive = drive_over(&watched);
    drive.put("a.bin", &noise(20_000)).unwrap();
    let object = drive.entry("a.bin").unwrap().object.unwrap();
    let key = object.id.bucket_key();

    // The storage answers with the object and 4 MiB more.
    let mut bigger = watched.inner.object(&key).unwrap();
    bigger.extend(std::iter::repeat(0u8).take(4 << 20));
    watched.inner.set(&key, bigger);
    watched.forget();

    let refused = drive.get("a.bin").unwrap_err();
    match &refused {
        DriveError::Corrupt { reason, .. } => assert!(
            reason.contains("may be at most"),
            "the limit names itself (the old code found the length out after the read): {reason}"
        ),
        other => panic!("{other:?}"),
    }
    assert_eq!(watched.plain_gets_of(&key), 0);
}

/// Three small files share a pack; the third read of the pack takes it whole (it is hot): within
/// one chunk, the most a pack can be.
#[test]
fn x_delta_azdrive_keys_43_a_pack_that_is_read_whole_is_asked_for_within_one_chunk() {
    let watched = Arc::new(Watched::new());
    let drive = EncryptedDrive::new(
        Arc::clone(&watched),
        DriveKey::generate().unwrap(),
        Arc::new(MemoryIndex::new()),
    )
    .with_packing(PackOptions::default());
    let files: Vec<(String, Vec<u8>)> = (0..3)
        .map(|i| (format!("notes/{i}.txt"), noise(300 + i)))
        .collect();
    let items: Vec<BatchPut<'_>> = files.iter().map(|(p, b)| BatchPut::new(p, b)).collect();
    for result in drive.put_many(&items) {
        result.unwrap();
    }
    let packs = watched.data_objects();
    assert_eq!(packs.len(), 1, "the three files share one pack: {packs:?}");
    watched.forget();

    for _ in 0..=HOT_READS {
        assert_eq!(drive.get("notes/0.txt").unwrap(), files[0].1);
    }
    assert_eq!(
        watched.limits_of(&packs[0]),
        vec![PACK_MAX],
        "the hot pack is taken whole once, within one chunk"
    );
    assert_eq!(
        watched.plain_gets_of(&packs[0]),
        0,
        "never by a plain GET that takes whatever comes"
    );
}

/// A re-encryption reads each pack whole to write it anew: within one chunk as well.
#[test]
fn x_delta_azdrive_keys_43_a_pack_that_is_repacked_is_asked_for_within_one_chunk() {
    let watched = Arc::new(Watched::new());
    let drive = EncryptedDrive::new(
        Arc::clone(&watched),
        DriveKey::generate().unwrap(),
        Arc::new(MemoryIndex::new()),
    )
    .with_packing(PackOptions::default());
    let files: Vec<(String, Vec<u8>)> = (0..3)
        .map(|i| (format!("notes/{i}.txt"), noise(300 + i)))
        .collect();
    let items: Vec<BatchPut<'_>> = files.iter().map(|(p, b)| BatchPut::new(p, b)).collect();
    for result in drive.put_many(&items) {
        result.unwrap();
    }
    let packs = watched.data_objects();
    assert_eq!(packs.len(), 1, "{packs:?}");
    watched.forget();

    let report = drive
        .reencrypt_packs(u64::MAX, crate::time::now_unix())
        .unwrap();
    assert_eq!(report.packs_read, 1, "{report:?}");
    assert_eq!(watched.limits_of(&packs[0]), vec![PACK_MAX]);
    assert_eq!(watched.plain_gets_of(&packs[0]), 0);
}

/// `get_after_looking`: the size first, the read only within the limit, and a drive that cannot say
/// a size leaves the read to decide.
#[test]
fn x_delta_azdrive_keys_43_a_service_without_a_limit_is_asked_the_size_before_the_read() {
    let watched = Watched::new();
    watched.inner.set("big", vec![1u8; 1000]);
    watched.inner.set("small", vec![2u8; 10]);
    let limit = Limit::new("a test object", 100);

    // over the limit: refused by the size, no byte read
    let before = watched.inner.gets();
    let err = limit.get_after_looking(&watched, "big").unwrap_err();
    assert!(matches!(err, DriveError::Protocol(_)), "{err:?}");
    assert_eq!(watched.inner.gets(), before, "the object is not started");

    // within it: read
    assert_eq!(
        limit.get_after_looking(&watched, "small").unwrap(),
        vec![2u8; 10]
    );
    // no object: the NotFound of the size, nothing read
    assert!(matches!(
        limit.get_after_looking(&watched, "none"),
        Err(DriveError::NotFound { .. })
    ));

    // a drive that cannot say a size: the read decides, and the length is checked after it
    *watched.head_fails.lock().unwrap() = true;
    assert_eq!(
        limit.get_after_looking(&watched, "small").unwrap(),
        vec![2u8; 10]
    );
    let err = limit.get_after_looking(&watched, "big").unwrap_err();
    assert!(matches!(err, DriveError::Protocol(_)), "{err:?}");
}
