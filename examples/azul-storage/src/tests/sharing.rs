//! Shares of an encrypted drive: the manifest opens only with its share key and shows nothing
//! in the bucket, the shared files and nothing else read with it, presigned links, revocation.

use std::sync::{Arc, Mutex};

use super::mem_bucket::MemBucket;
use crate::{
    crypto::{
        share::{manifest_key, share_key_from_text, share_key_text, ShareManifest},
        CryptoError, DriveKey, ShareKey,
    },
    encrypted::{EncryptedDrive, MemoryIndex},
    sharing::{
        list_shares, open_share, open_shared_entry, parse_share_link, read_shared_entry,
        revoke_all_shares, revoke_share, share, share_link, Presigner, MAX_PRESIGNED_SECS,
    },
    time::now_unix,
    Drive, DriveError,
};

fn new_drive() -> EncryptedDrive<Arc<MemBucket>> {
    EncryptedDrive::new(
        Arc::new(MemBucket::new()),
        DriveKey::generate().unwrap(),
        Arc::new(MemoryIndex::new()),
    )
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

/// Records what it signs.
#[derive(Default)]
struct FakePresigner(Mutex<Vec<(String, u64)>>);

impl Presigner for FakePresigner {
    fn presigned_get(&self, key: &str, expires_secs: u64) -> Result<String, DriveError> {
        self.0.lock().unwrap().push((key.to_string(), expires_secs));
        Ok(format!("https://s3.example.test/bucket/{key}?X-Amz-Expires={expires_secs}&X-Amz-Signature=f00"))
    }
}

#[test]
fn a_share_opens_its_files_and_nothing_else() {
    let drive = new_drive();
    drive.put("Taxes 2025/return.pdf", b"the return").unwrap();
    drive.put("Taxes 2025/receipts/a.txt", b"receipt a").unwrap();
    drive.put("diary.txt", b"not shared").unwrap();
    drive.put("notes/plan.txt", b"the plan").unwrap();
    let made = share(
        &drive,
        &["Taxes 2025/", "notes/plan.txt"],
        Some("For the accountant"),
        None,
        None,
    )
    .unwrap();
    let mut names: Vec<&str> = made.manifest.entries.iter().map(|e| e.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["Taxes 2025/receipts/a.txt", "Taxes 2025/return.pdf", "plan.txt"]);
    assert_eq!(made.id, made.key.id());
    assert!(made.manifest_url.is_none());

    // Someone with the key only (the link): the manifest, then each file from the bucket.
    let key = share_key_from_text(&share_key_text(&made.key)).unwrap();
    let bucket = drive.inner().as_ref();
    let manifest = open_share(bucket, &key).unwrap();
    assert_eq!(manifest, made.manifest);
    assert_eq!(manifest.title.as_deref(), Some("For the accountant"));
    for entry in &manifest.entries {
        let path = if entry.name == "plan.txt" {
            String::from("notes/plan.txt")
        } else {
            entry.name.clone()
        };
        assert_eq!(read_shared_entry(bucket, entry, &key).unwrap(), drive.get(&path).unwrap());
    }

    // The share key opens no other file: the diary's key is wrapped by the drive key only.
    let diary = drive.entry("diary.txt").unwrap().object.unwrap();
    assert!(matches!(
        key.unwrap_file_key(&diary.wrapped_key, &diary.id),
        Err(CryptoError::WrongKey)
    ));
    // The bucket's manifest is ciphertext: no title, no name.
    let sealed = drive.inner().object(&manifest_key(&made.id)).unwrap();
    for plain in ["accountant", "Taxes", "plan.txt", "receipt"] {
        assert!(!contains(&sealed, plain.as_bytes()), "{plain} is in the bucket");
    }
}

#[test]
fn a_manifest_opens_only_with_its_key_and_unchanged() {
    let drive = new_drive();
    drive.put("a.txt", b"a").unwrap();
    let made = share(&drive, &["a.txt"], None, None, None).unwrap();
    let sealed = drive.inner().object(&manifest_key(&made.id)).unwrap();
    assert!(matches!(
        ShareManifest::open(&sealed, &ShareKey::generate().unwrap()),
        Err(CryptoError::WrongKey)
    ));
    let mut changed = sealed.clone();
    *changed.last_mut().unwrap() ^= 1;
    assert!(ShareManifest::open(&changed, &made.key).is_err());
    let mut newer = sealed;
    newer[4] = 2;
    assert!(matches!(
        ShareManifest::open(&newer, &made.key),
        Err(CryptoError::Unsupported(_))
    ));
    // An object swapped under the entry is caught.
    drive.put("b.txt", b"b").unwrap();
    let b = drive.entry("b.txt").unwrap().object.unwrap();
    let other_object = drive.inner().object(&b.id.bucket_key()).unwrap();
    let entry = &made.manifest.entries[0];
    assert!(open_shared_entry(&other_object, entry, &made.key).is_err());
}

#[test]
fn presigned_links_carry_the_manifest_and_every_object_for_at_most_seven_days() {
    let drive = new_drive();
    drive.put("photos/a.jpg", b"jpeg a").unwrap();
    drive.put("photos/b.jpg", b"jpeg b").unwrap();
    let presigner = FakePresigner::default();
    let expires = now_unix() + 3 * 24 * 3600;
    let made = share(&drive, &["photos/"], None, Some(expires), Some(&presigner)).unwrap();
    let signed = presigner.0.lock().unwrap().clone();
    assert_eq!(signed.len(), 3, "two objects and the manifest");
    assert!(signed.iter().all(|(_, secs)| *secs <= MAX_PRESIGNED_SECS && *secs > 0));
    assert_eq!(signed.last().unwrap().0, manifest_key(&made.id));
    assert!(made.manifest.entries.iter().all(|e| e.url.as_deref().is_some_and(|u| u.contains(&e.object.bucket_key()))));

    let manifest_url = made.manifest_url.clone().unwrap();
    let link = share_link("https://share.example.test/v", &manifest_url, &made.key);
    assert!(link.starts_with("https://share.example.test/v#m="));
    let (url, key) = parse_share_link(&link).unwrap();
    assert_eq!(url, manifest_url);
    assert_eq!(key, made.key);

    for too_long in [None, Some(now_unix() + MAX_PRESIGNED_SECS + 60), Some(now_unix() - 1)] {
        assert!(matches!(
            share(&drive, &["photos/"], None, too_long, Some(&presigner)),
            Err(DriveError::InvalidConfig(_))
        ));
    }
}

#[test]
fn two_files_of_one_name_and_paths_that_are_none_are_refused() {
    let drive = new_drive();
    drive.put("a/report.pdf", b"1").unwrap();
    drive.put("b/report.pdf", b"2").unwrap();
    assert!(matches!(
        share(&drive, &["a/report.pdf", "b/report.pdf"], None, None, None),
        Err(DriveError::InvalidKey { .. })
    ));
    assert!(matches!(
        share(&drive, &["missing.txt"], None, None, None),
        Err(DriveError::NotFound { .. })
    ));
}

#[test]
fn a_revoked_share_opens_no_more_and_a_rotation_revokes_them_all() {
    let drive = new_drive();
    drive.put("a.txt", b"a").unwrap();
    let first = share(&drive, &["a.txt"], None, None, None).unwrap();
    let second = share(&drive, &["a.txt"], None, None, None).unwrap();
    let bucket = drive.inner().as_ref();
    let mut ids = list_shares(bucket).unwrap();
    ids.sort();
    let mut expected = vec![first.id, second.id];
    expected.sort();
    assert_eq!(ids, expected);
    revoke_share(bucket, &first.id).unwrap();
    revoke_share(bucket, &first.id).unwrap();
    assert!(matches!(
        open_share(bucket, &first.key),
        Err(DriveError::NotFound { .. })
    ));
    assert!(open_share(bucket, &second.key).is_ok());
    assert_eq!(revoke_all_shares(bucket).unwrap(), 1);
    assert!(list_shares(bucket).unwrap().is_empty());
    assert_eq!(drive.get("a.txt").unwrap(), b"a", "the files stay");
}
