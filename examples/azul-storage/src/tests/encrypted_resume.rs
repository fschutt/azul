//! An encrypted file's upload that a dropped connection or a killed app cut off: its ciphertext
//! waits in the upload's resume folder, not in the system's temporary folder, and the next
//! upload of the same file sends only the parts the bucket lacks, under the upload id it kept.

use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use super::{
    fake_bucket::{FakeBucket, BUCKET},
    TempDir,
};
use crate::{
    crypto::{random_bytes, DriveKey},
    encrypted::{EncryptedDrive, IndexChange, IndexPage, MemoryIndex, NameIndex},
    Credentials, Drive, DriveError, ListRequest, S3Config, S3Drive,
};

/// 2026-10-10T00:00:00Z.
const NOW: u64 = 1_791_590_400;
/// The tests' part size (a real bucket wants 5 MiB and more).
const PART: usize = 1024;

fn drive_on(s3: &Arc<FakeBucket>, resume: &Path, now: u64) -> S3Drive {
    S3Drive::new(
        S3Config {
            endpoint: String::from("http://127.0.0.1:9000"),
            region: String::from("us-east-1"),
            bucket: BUCKET.to_string(),
            path_style: true,
        },
        Credentials::new("AKIDTEST", "test-secret"),
        s3.transport(),
    )
    .unwrap()
    .with_clock(move || now)
    .with_part_size(PART)
    .with_resume_dir(resume)
}

/// Bytes that do not compress: the object is as many parts as the file.
fn noise(len: usize) -> Vec<u8> {
    let mut bytes = vec![0u8; len];
    random_bytes(&mut bytes).unwrap();
    bytes
}

/// Every file under `dir`.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut folders = vec![dir.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                folders.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}

#[test]
fn a_cut_encrypted_upload_resumes_at_its_next_part() {
    let tmp = TempDir::new("encrypted-resume");
    let resume = tmp.path().join("resume");
    let path = tmp.path().join("film.bin");
    let plain = noise(10 * PART + 7);
    std::fs::write(&path, &plain).unwrap();
    let s3 = FakeBucket::new();
    let key = DriveKey::generate().unwrap();
    let index: Arc<dyn NameIndex> = Arc::new(MemoryIndex::new());

    s3.take_parts(Some(4));
    let first = EncryptedDrive::new(drive_on(&s3, &resume, NOW), key.clone(), Arc::clone(&index));
    assert!(
        first.put_file("film.bin", &path, &|_| {}).is_err(),
        "cut off after four parts"
    );
    let kept = files_under(&resume);
    assert!(
        kept.iter().any(|f| f.extension().is_none_or(|e| e != "json")),
        "the ciphertext waits beside the upload's state: {kept:?}"
    );

    // The app starts again: the same bucket, key and index, the same resume folder.
    s3.take_parts(None);
    s3.clear_log();
    let again = EncryptedDrive::new(drive_on(&s3, &resume, NOW + 60), key, index);
    let sent = again.put_file("film.bin", &path, &|_| {}).unwrap();
    assert_eq!(sent, plain.len() as u64);
    assert_eq!(again.get("film.bin").unwrap(), plain, "the file is whole");
    assert_eq!(s3.count("POST start"), 0, "the same upload: {:?}", s3.log());
    assert_eq!(s3.count("GET list-parts"), 1, "the bucket says which parts it has");
    let object_key = again
        .metadata("film.bin")
        .unwrap()
        .into_iter()
        .find(|(name, _)| name == crate::OBJECT_KEY_METADATA)
        .map(|(_, value)| value)
        .expect("the object's key");
    let stored = s3.object(&object_key).expect("the object").len();
    assert_eq!(
        s3.count("PUT part"),
        stored.div_ceil(PART) - 4,
        "only the parts that were missing: {:?}",
        s3.log()
    );
    assert_eq!(s3.count("POST complete"), 1);
    assert!(s3.uploads().is_empty());
    assert!(
        files_under(&resume).is_empty(),
        "a finished upload leaves no ciphertext and no state: {:?}",
        files_under(&resume)
    );
}

/// An index whose next `failing` changes get no answer (a commit over a dropped connection, a
/// service over its quota), then works.
struct FlakyIndex {
    inner: MemoryIndex,
    failing: AtomicUsize,
}

impl NameIndex for FlakyIndex {
    fn get(&self, path: &str) -> Result<Option<crate::encrypted::IndexEntry>, DriveError> {
        self.inner.get(path)
    }

    fn list(&self, request: &ListRequest) -> Result<IndexPage, DriveError> {
        self.inner.list(request)
    }

    fn apply(&self, changes: Vec<IndexChange>) -> Result<crate::meta::Released, DriveError> {
        if self
            .failing
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok()
        {
            return Err(DriveError::Transport(String::from(
                "the index's commit got no answer",
            )));
        }
        self.inner.apply(changes)
    }
}

/// AZ-M3. A finished upload whose index commit fails keeps its encrypted copy and its record:
/// the next try finds the object up already and only names it, under the same object id - it
/// does not encrypt the file anew (a new object id, an orphan nothing names) and does not send
/// the 20 GB again.
#[test]
fn az_m3_a_failed_index_commit_keeps_the_copy_and_the_retry_only_names_the_object() {
    let tmp = TempDir::new("encrypted-resume-bind");
    let resume = tmp.path().join("resume");
    let path = tmp.path().join("film.bin");
    let plain = noise(10 * PART + 7);
    std::fs::write(&path, &plain).unwrap();
    let s3 = FakeBucket::new();
    let key = DriveKey::generate().unwrap();
    let index: Arc<dyn NameIndex> = Arc::new(FlakyIndex {
        inner: MemoryIndex::new(),
        failing: AtomicUsize::new(1),
    });

    let first = EncryptedDrive::new(drive_on(&s3, &resume, NOW), key.clone(), Arc::clone(&index));
    assert!(
        first.put_file("film.bin", &path, &|_| {}).is_err(),
        "the object went up whole, then the index's commit failed"
    );
    assert_eq!(
        s3.count("POST complete"),
        1,
        "the object is whole: {:?}",
        s3.log()
    );
    let kept = files_under(&resume);
    let has = |ext: &str| kept.iter().any(|f| f.extension().is_some_and(|e| e == ext));
    assert!(
        has("azl1") && has("json"),
        "the ciphertext and its record stay until the index names the object: {kept:?}"
    );

    // The next try (the sync's next pass): the commit works now.
    s3.clear_log();
    let again = EncryptedDrive::new(drive_on(&s3, &resume, NOW + 60), key, index);
    let sent = again.put_file("film.bin", &path, &|_| {}).unwrap();
    assert_eq!(sent, plain.len() as u64);
    assert_eq!(
        again.get("film.bin").unwrap(),
        plain,
        "the file is named and whole"
    );
    assert_eq!(
        s3.count("POST start") + s3.count("PUT part"),
        0,
        "the object is up already: nothing is sent again: {:?}",
        s3.log()
    );
    assert!(
        files_under(&resume).is_empty(),
        "the index names the object now: the copy and its record go: {:?}",
        files_under(&resume)
    );
}

/// AZ-C12. A kept copy is reused only for the file that is, for certain, still the file it was
/// made of. An edit that keeps the size and puts the modification time back (`touch -r`) is not
/// the file it was: the status-change time moved. The old copy served the old bytes as the new
/// version of the file.
#[test]
fn az_c12_an_edit_that_keeps_size_and_time_is_not_served_from_the_old_copy() {
    let tmp = TempDir::new("encrypted-resume-c12");
    let resume = tmp.path().join("resume");
    let path = tmp.path().join("film.bin");
    let old = noise(10 * PART + 7);
    std::fs::write(&path, &old).unwrap();
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();
    let s3 = FakeBucket::new();
    let key = DriveKey::generate().unwrap();
    let index: Arc<dyn NameIndex> = Arc::new(MemoryIndex::new());

    s3.take_parts(Some(4));
    let first = EncryptedDrive::new(drive_on(&s3, &resume, NOW), key.clone(), Arc::clone(&index));
    assert!(
        first.put_file("film.bin", &path, &|_| {}).is_err(),
        "cut off"
    );
    assert!(
        files_under(&resume)
            .iter()
            .any(|f| f.extension().is_some_and(|e| e == "azl1")),
        "the copy waits for the next try"
    );

    // Edited: other bytes of the same length, the same modification time. The status-change time
    // is the clock's and no tool sets it back.
    std::thread::sleep(std::time::Duration::from_millis(30));
    let edited = noise(old.len());
    std::fs::write(&path, &edited).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(before)
        .unwrap();

    s3.take_parts(None);
    s3.clear_log();
    let again = EncryptedDrive::new(drive_on(&s3, &resume, NOW + 60), key, index);
    again.put_file("film.bin", &path, &|_| {}).unwrap();
    assert_eq!(
        again.get("film.bin").unwrap(),
        edited,
        "the new bytes, not the old copy's"
    );
    assert!(
        files_under(&resume).is_empty(),
        "{:?}",
        files_under(&resume)
    );
}

/// AZ-C12. A record names the copy it describes (the object's id is in the copy's name), so two
/// processes that encrypt one file never leave a record beside the other one's copy; a new
/// encryption replaces the copy and the record together.
#[test]
fn az_c12_a_record_names_its_copy_and_a_new_encryption_replaces_both() {
    let tmp = TempDir::new("encrypted-resume-c12-names");
    let resume = tmp.path().join("resume");
    let path = tmp.path().join("film.bin");
    let plain = noise(10 * PART + 7);
    std::fs::write(&path, &plain).unwrap();
    let s3 = FakeBucket::new();
    let key = DriveKey::generate().unwrap();
    let index: Arc<dyn NameIndex> = Arc::new(MemoryIndex::new());

    s3.take_parts(Some(4));
    let first = EncryptedDrive::new(drive_on(&s3, &resume, NOW), key.clone(), Arc::clone(&index));
    assert!(first.put_file("film.bin", &path, &|_| {}).is_err());
    let record_and_copy = |resume: &Path| {
        let files = files_under(resume);
        let in_encrypted = |f: &&PathBuf| {
            f.parent()
                .and_then(Path::file_name)
                .is_some_and(|name| name == "encrypted")
        };
        let record = files
            .iter()
            .filter(in_encrypted)
            .find(|f| f.extension().is_some_and(|e| e == "json"))
            .cloned()
            .expect("the record");
        let copy = files
            .iter()
            .find(|f| f.extension().is_some_and(|e| e == "azl1"))
            .cloned()
            .expect("the copy");
        (record, copy)
    };
    let (record, copy) = record_and_copy(&resume);
    let text = std::fs::read(&record).unwrap();
    let object_id = serde_json::from_slice::<serde_json::Value>(&text).unwrap()["object_id"]
        .as_str()
        .unwrap()
        .to_string();
    let copy_name = copy.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        copy_name.contains(&object_id),
        "the copy {copy_name} carries the object id {object_id} of its record"
    );
    assert_eq!(
        std::fs::metadata(&copy).unwrap().len(),
        serde_json::from_slice::<serde_json::Value>(&text).unwrap()["object_len"]
            .as_u64()
            .unwrap()
    );

    // The file changes: the next try encrypts it anew, and the old copy goes with its record.
    std::thread::sleep(std::time::Duration::from_millis(30));
    let edited = noise(plain.len() + 100);
    std::fs::write(&path, &edited).unwrap();
    s3.take_parts(Some(2));
    let second = EncryptedDrive::new(drive_on(&s3, &resume, NOW + 60), key, index);
    assert!(second.put_file("film.bin", &path, &|_| {}).is_err());
    let (record2, copy2) = record_and_copy(&resume);
    assert_eq!(record2, record, "one record per file");
    assert_ne!(copy2, copy, "the new encryption has a copy of its own");
    assert!(!copy.exists(), "the old copy is gone");
    let azl1: Vec<_> = files_under(&resume)
        .into_iter()
        .filter(|f| f.extension().is_some_and(|e| e == "azl1"))
        .collect();
    assert_eq!(azl1.len(), 1, "{azl1:?}");
}
