//! The drive index under a new drive key (feature `encryption`): every object of the
//! repository sealed again, the old ones gone, and a stop at any point leaves a repository
//! that opens with the old key or with the new one.

use std::sync::Arc;

use crate::{
    crypto::DriveKey,
    encrypted::{EncryptedDrive, IndexProvider},
    meta::{keys, MemoryBucket, MetaError, MetaIndexProvider, MetaRepo},
    Drive, DriveError,
};

const DRIVE: &str = "d_rekey";

/// The key id an object of the repository is sealed with (the drive key's sealer writes
/// `"AZM1" | key id | ...`; a pack starts with its first sealed chunk).
fn sealed_by(bytes: &[u8]) -> Option<[u8; 16]> {
    if bytes.len() < 20 || &bytes[..4] != b"AZM1" {
        return None;
    }
    let mut id = [0u8; 16];
    id.copy_from_slice(&bytes[4..20]);
    Some(id)
}

/// Every object of the repository with the key id that sealed it.
fn repository(bucket: &MemoryBucket) -> Vec<(String, Option<[u8; 16]>)> {
    bucket
        .objects()
        .into_iter()
        .filter(|(key, _)| key.starts_with(keys::ROOT))
        .map(|(key, bytes)| (key, sealed_by(&bytes)))
        .collect()
}

fn all_sealed_by(bucket: &MemoryBucket, key: &DriveKey) -> bool {
    let objects = repository(bucket);
    !objects.is_empty() && objects.iter().all(|(_, id)| *id == Some(key.id().0))
}

/// A drive with an index of three commits (a file, another, a delete).
fn drive_with_files(bucket: &Arc<MemoryBucket>, provider: &MetaIndexProvider, key: &DriveKey) {
    let index = provider.open_index(DRIVE, bucket.clone(), key).unwrap();
    let drive = EncryptedDrive::new(bucket.clone() as Arc<dyn Drive>, key.clone(), index);
    drive.put("notes/plan.txt", b"the plan").unwrap();
    drive.put("a.txt", b"a").unwrap();
    drive.delete("a.txt").unwrap();
}

fn opens_with(bucket: &Arc<MemoryBucket>, key: &DriveKey) -> bool {
    MetaRepo::open(bucket.clone(), key.clone(), "checker", "Checker").is_ok()
}

#[test]
fn a_rekeyed_index_is_sealed_with_the_new_key_only_and_keeps_its_entries_and_history() {
    let (old, new) = (DriveKey::generate().unwrap(), DriveKey::generate().unwrap());
    let bucket = Arc::new(MemoryBucket::new());
    let provider = MetaIndexProvider::new("Laptop");
    drive_with_files(&bucket, &provider, &old);
    assert!(all_sealed_by(&bucket, &old));

    provider.rekey(DRIVE, bucket.clone(), &old, &new).unwrap();
    assert!(all_sealed_by(&bucket, &new), "{:?}", repository(&bucket));
    assert!(!opens_with(&bucket, &old));
    assert!(matches!(
        MetaRepo::open(bucket.clone(), old.clone(), "checker", "Checker"),
        Err(MetaError::Sealed { .. })
    ));
    let index = provider.open_index(DRIVE, bucket.clone(), &new).unwrap();
    assert!(index.get("notes/plan.txt").unwrap().is_some());
    assert_eq!(index.get("a.txt").unwrap(), None);
    // The history came along: the delete and both writes.
    let repo = MetaRepo::open(bucket.clone(), new.clone(), "checker", "Checker").unwrap();
    assert_eq!(repo.history().unwrap().len(), 3);
    // Run again, nothing changes.
    provider.rekey(DRIVE, bucket.clone(), &old, &new).unwrap();
    assert!(all_sealed_by(&bucket, &new));
}

#[test]
fn a_rekey_stopped_before_its_switch_leaves_the_index_on_the_old_key_and_finishes_when_run_again() {
    let (old, new) = (DriveKey::generate().unwrap(), DriveKey::generate().unwrap());
    let bucket = Arc::new(MemoryBucket::new());
    let provider = MetaIndexProvider::new("Laptop");
    drive_with_files(&bucket, &provider, &old);

    bucket.fail_next_write(
        keys::MANIFEST,
        MetaError::Drive(DriveError::Transport("the network went".to_string())),
    );
    assert!(provider.rekey(DRIVE, bucket.clone(), &old, &new).is_err());
    assert!(opens_with(&bucket, &old));
    assert!(!opens_with(&bucket, &new));
    let index = provider.open_index(DRIVE, bucket.clone(), &old).unwrap();
    assert!(index.get("notes/plan.txt").unwrap().is_some());

    provider.rekey(DRIVE, bucket.clone(), &old, &new).unwrap();
    assert!(opens_with(&bucket, &new));
    assert!(all_sealed_by(&bucket, &new), "{:?}", repository(&bucket));
}

#[test]
fn a_rekey_stopped_after_its_switch_opens_with_the_new_key_and_finishes_when_run_again() {
    let (old, new) = (DriveKey::generate().unwrap(), DriveKey::generate().unwrap());
    let bucket = Arc::new(MemoryBucket::new());
    let provider = MetaIndexProvider::new("Laptop");
    drive_with_files(&bucket, &provider, &old);
    let old_pack = {
        let repo = MetaRepo::open(bucket.clone(), old.clone(), "checker", "Checker").unwrap();
        keys::pack(&repo.store().state().packs[0].name)
    };

    // The switch lands; deleting an old pack does not.
    bucket.fail_next_write(
        &old_pack,
        MetaError::Drive(DriveError::Transport("the network went".to_string())),
    );
    assert!(provider.rekey(DRIVE, bucket.clone(), &old, &new).is_err());
    assert!(opens_with(&bucket, &new));
    assert!(!opens_with(&bucket, &old));
    assert!(bucket.objects().iter().any(|(key, _)| *key == old_pack));

    provider.rekey(DRIVE, bucket.clone(), &old, &new).unwrap();
    assert!(bucket.objects().iter().all(|(key, _)| *key != old_pack));
    assert!(all_sealed_by(&bucket, &new), "{:?}", repository(&bucket));
}

#[test]
fn a_drive_without_an_index_yet_rekeys_as_nothing_to_do() {
    let (old, new) = (DriveKey::generate().unwrap(), DriveKey::generate().unwrap());
    let bucket = Arc::new(MemoryBucket::new());
    MetaIndexProvider::new("Laptop")
        .rekey(DRIVE, bucket.clone(), &old, &new)
        .unwrap();
    assert!(repository(&bucket).is_empty());
}
