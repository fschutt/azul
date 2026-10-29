//! An encrypted file's upload that a dropped connection or a killed app cut off: its ciphertext
//! waits in the upload's resume folder, not in the system's temporary folder, and the next
//! upload of the same file sends only the parts the bucket lacks, under the upload id it kept.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use super::{
    fake_bucket::{FakeBucket, BUCKET},
    TempDir,
};
use crate::{
    crypto::{random_bytes, DriveKey},
    encrypted::{EncryptedDrive, MemoryIndex, NameIndex},
    Credentials, Drive, S3Config, S3Drive,
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
