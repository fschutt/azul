//! S3's 409 on a raced conditional write: tried again, never taken for "there".

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use crate::{
    meta::{
        keys, Bucket, DriveBucket, Kind, MemoryBucket, MetaError, MetaStore, Objects, PackWriter,
        Publish, RefUpdate, TestSealer,
    },
    ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition, ServiceError,
};

const MAIN: &str = "refs/heads/main";

fn sealer() -> TestSealer {
    TestSealer::new([41; 32])
}

fn pack_of(content: &str) -> PackWriter {
    let mut writer = PackWriter::new();
    writer.add(Kind::Blob, content.as_bytes().to_vec());
    writer
}

fn publish<B: Bucket>(store: &mut MetaStore<B, TestSealer>, value: &str) -> u32 {
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
        .unwrap()
        .attempts
}

fn raced(key: &str) -> MetaError {
    MetaError::Raced {
        key: key.to_string(),
    }
}

#[test]
fn a_409_on_the_manifest_swap_is_tried_again() {
    let bucket = MemoryBucket::new();
    let mut store = MetaStore::create(bucket.clone(), sealer(), "laptop").unwrap();
    bucket.fail_next_write(keys::MANIFEST, raced(keys::MANIFEST));
    assert_eq!(publish(&mut store, "c1"), 2);
    let logs = bucket
        .objects()
        .into_iter()
        .filter(|(key, _)| key.starts_with(keys::LOG_DIR))
        .count();
    assert_eq!(logs, 1);
    let phone = MetaStore::open(bucket, sealer(), "phone").unwrap();
    assert_eq!(phone.state().refs.get(MAIN).map(String::as_str), Some("c1"));
}

#[test]
fn a_409_on_a_pack_is_never_taken_for_a_pack_that_is_there() {
    let bucket = MemoryBucket::new();
    let mut store = MetaStore::create(bucket.clone(), sealer(), "laptop").unwrap();
    let name = pack_of("c1").name(&sealer());
    bucket.fail_next_write(&keys::pack(&name), raced(&keys::pack(&name)));
    assert_eq!(publish(&mut store, "c1"), 2);
    // The manifest names the pack, and the pack is there whole.
    let phone = MetaStore::open(bucket.clone(), sealer(), "phone").unwrap();
    let mut objects = Objects::new();
    phone
        .fetch_pack(&phone.state().packs[0].clone(), &mut objects)
        .unwrap();
    assert_eq!(objects.len(), 1);
}

/// A drive whose conditional writes answer 409 while `busy` is above 0.
struct Busy {
    inner: MemoryBucket,
    busy: Arc<AtomicU32>,
}

impl Drive for Busy {
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
        Drive::put(&self.inner, key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.inner.head(key)
    }
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        let busy = self
            .busy
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1));
        if busy.is_ok() {
            return Err(DriveError::Service(ServiceError {
                status: 409,
                code: "ConditionalRequestConflict".to_string(),
                message: "A conflicting conditional operation is in progress".to_string(),
                ..ServiceError::default()
            }));
        }
        self.inner.put_if(key, bytes, condition)
    }
}

#[test]
fn a_drive_answering_409_is_a_race_for_the_bucket_and_the_publish_lands() {
    let busy = Arc::new(AtomicU32::new(1));
    let drive = DriveBucket::new(Busy {
        inner: MemoryBucket::new(),
        busy: Arc::clone(&busy),
    });
    assert!(matches!(
        drive.create("x", b"x"),
        Err(MetaError::Raced { .. })
    ));
    assert!(drive.create("x", b"x").is_ok());
    // 412 stays a conflict.
    assert!(matches!(
        drive.create("x", b"y"),
        Err(MetaError::Conflict { .. })
    ));

    let mut store = MetaStore::create(drive, sealer(), "laptop").unwrap();
    busy.store(2, Ordering::SeqCst);
    assert_eq!(publish(&mut store, "c1"), 3);
}
