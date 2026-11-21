//! AZD1, the drops of incoming mail: the shared test vector, what a drop opens for, the drop
//! key in the bucket, and ingestion.

use std::sync::Arc;

use super::mem_bucket::MemBucket;
use crate::{
    crypto::{
        device::{drop_key_permit, store_drive_key},
        drops::{
            disable_drop, enable_drop, ingest, is_drop_key, load_drop_key, new_drop_key,
            open_drop, replace_drop_key, seal_drop, seal_drop_with, DropFolder, DropKeyFile,
            DropPublic, DropSecret, Dropped, DROP_KEY_FILE, DROP_PREFIX, HEADER_LEN,
        },
        hex_array,
        keys::member_key_file,
        keystate::{Op, Permit},
        to_hex, CryptoError, DriveKey,
    },
    keyring::{KeyringStore, MemoryKeyring},
    Drive, DriveError,
};

/// The vector the Worker's tests and scripts/azlin_drop.py check too.
const VECTOR: &str = include_str!("../../../azlin-mail-worker/test/azd1-vector.json");

fn vector() -> serde_json::Value {
    serde_json::from_str(VECTOR).unwrap()
}

fn hex32(v: &serde_json::Value, field: &str) -> [u8; 32] {
    hex_array(v[field].as_str().unwrap()).unwrap()
}

const DRIVE: &str = "d_drop";

fn message(n: u32) -> Vec<u8> {
    format!("From: a@example.com\r\nSubject: message {n}\r\n\r\nbody {n}\r\n").into_bytes()
}

#[test]
fn the_azd1_vector_seals_and_opens_to_its_bytes() {
    let v = vector();
    let secret = DropSecret::from_bytes(hex32(&v, "drop_secret"));
    let public = secret.public();
    assert_eq!(public.to_hex(), v["drop_public"].as_str().unwrap());
    assert_eq!(to_hex(&public.key_id()), v["drop_key_id"].as_str().unwrap());
    let nonce: [u8; 12] = hex_array(v["nonce"].as_str().unwrap()).unwrap();
    let raw = v["raw"].as_str().unwrap().as_bytes();
    let drive = v["drive_id"].as_str().unwrap();
    let object_key = v["object_key"].as_str().unwrap();
    let received = v["received"].as_u64().unwrap();
    assert_eq!(v["folder"].as_str().unwrap(), "Inbox");
    let sealed = seal_drop_with(
        &hex32(&v, "ephemeral_secret"),
        &nonce,
        &public,
        drive,
        object_key,
        received,
        DropFolder::Inbox,
        raw,
    )
    .unwrap();
    assert_eq!(to_hex(&sealed), v["sealed"].as_str().unwrap());
    assert_eq!(
        to_hex(&sealed[21..53]),
        v["ephemeral_public"].as_str().unwrap()
    );
    let opened = open_drop(&sealed, &secret, drive, object_key).unwrap();
    assert_eq!(opened.received, received);
    assert_eq!(opened.folder, DropFolder::Inbox);
    assert_eq!(opened.raw.as_slice(), raw);
}

#[test]
fn a_drop_opens_only_for_its_drive_its_name_and_its_key() {
    let secret = DropSecret::generate().unwrap();
    let key = new_drop_key().unwrap();
    let raw = message(1);
    let sealed = seal_drop(&secret.public(), DRIVE, &key, 1_791_619_200, DropFolder::Spam, &raw)
        .unwrap();
    let opened = open_drop(&sealed, &secret, DRIVE, &key).unwrap();
    assert_eq!((opened.received, opened.folder), (1_791_619_200, DropFolder::Spam));
    assert_eq!(opened.raw.as_slice(), raw.as_slice());
    assert!(format!("{opened:?}").contains("bytes"), "the message is never printed");

    let damaged = |result: Result<Dropped, CryptoError>| matches!(result, Err(CryptoError::Damaged(_)));
    assert!(damaged(open_drop(&sealed, &secret, "d_other", &key)), "another drive");
    let other_key = new_drop_key().unwrap();
    assert!(damaged(open_drop(&sealed, &secret, DRIVE, &other_key)), "another name");
    assert!(matches!(
        open_drop(&sealed, &DropSecret::generate().unwrap(), DRIVE, &key),
        Err(CryptoError::WrongKey)
    ));
    for at in [21, HEADER_LEN - 1, HEADER_LEN, sealed.len() - 1] {
        let mut changed = sealed.clone();
        changed[at] ^= 1;
        assert!(damaged(open_drop(&changed, &secret, DRIVE, &key)), "byte {at}");
    }
    assert!(damaged(open_drop(&sealed[..sealed.len() - 1], &secret, DRIVE, &key)));
    assert!(damaged(open_drop(&sealed[..HEADER_LEN], &secret, DRIVE, &key)));
    let mut newer = sealed.clone();
    newer[4] = 2;
    assert!(matches!(
        open_drop(&newer, &secret, DRIVE, &key),
        Err(CryptoError::Unsupported(_))
    ));
    // A low-order ephemeral key (all zero) is refused, not used.
    let mut low = sealed;
    low[21..53].fill(0);
    assert!(damaged(open_drop(&low, &secret, DRIVE, &key)));
}

#[test]
fn drop_names_are_random_under_their_folder_and_never_a_member_s_key_file() {
    let (a, b) = (new_drop_key().unwrap(), new_drop_key().unwrap());
    assert_ne!(a, b);
    assert!(a.starts_with(DROP_PREFIX) && is_drop_key(&a) && is_drop_key(&b));
    assert_eq!(a.len(), DROP_PREFIX.len() + 32);
    for not_a_drop in [
        "drop/000102030405060708090a0b0c0d0e0f",
        ".azlin/drop/000102030405060708090A0B0C0D0E0F",
        ".azlin/drop/0001",
        ".azlin/drop/readme.txt",
    ] {
        assert!(!is_drop_key(not_a_drop), "{not_a_drop}");
    }
    // `_drop` is no member id: the drop key's file never meets a member's.
    assert!(member_key_file("_drop").is_err());
    assert_eq!(DROP_KEY_FILE, ".azlin/keys/_drop.key");
}

/// The key permit's `DropKey` for `key` on a keyring that holds it: what an app asks before it turns
/// incoming mail on or off.
fn drop_permit(bucket: &dyn Drive, key: &DriveKey) -> Permit {
    let keyring = MemoryKeyring::new();
    store_drive_key(&keyring, DRIVE, key).unwrap();
    drop_key_permit(bucket, &keyring, DRIVE, key)
        .expect("the permit of a device that holds the key")
}

#[test]
fn the_drop_key_lives_in_the_bucket_sealed_with_the_drive_key() {
    let bucket = MemBucket::new();
    let k1 = DriveKey::generate().unwrap();
    let public = enable_drop(&drop_permit(&bucket, &k1), &bucket, &k1, DRIVE).unwrap();
    assert_eq!(
        enable_drop(&drop_permit(&bucket, &k1), &bucket, &k1, DRIVE).unwrap(),
        public,
        "the same key again"
    );
    let file_bytes = bucket.object(DROP_KEY_FILE).unwrap();
    let file = DropKeyFile::parse(&file_bytes).unwrap();
    assert_eq!(file.public, public);
    assert_eq!(file.drive_key_id, k1.id());
    let secret = load_drop_key(&bucket, &k1, DRIVE).unwrap().unwrap();
    assert_eq!(secret.public(), public);
    let text = String::from_utf8(file_bytes.clone()).unwrap();
    assert!(!text.contains(&to_hex(&secret.to_bytes()[..])), "no secret in the clear");
    assert!(matches!(
        file.open(&k1, "d_other"),
        Err(CryptoError::WrongKey)
    ));

    // X-DELTA-KEYS3-2: another drive key - K2 after a rotation made elsewhere, or K1 on a device whose key
    // was replaced - is NOT "no key here, make one": the file is not its to read, and a new key put over
    // it is the old secret and the mail sealed to it, gone. This test used to say "enabling again makes a
    // new one in its place": that was the defect.
    let k2 = DriveKey::generate().unwrap();
    let refused = load_drop_key(&bucket, &k2, DRIVE);
    assert!(
        matches!(refused, Err(DriveError::KeyRefused { .. })),
        "{refused:?}"
    );
    assert_eq!(
        bucket.object(DROP_KEY_FILE).unwrap(),
        file_bytes,
        "nothing was written"
    );
    assert_eq!(
        DropPublic::from_hex(&public.to_hex()),
        Some(public),
        "the public half's text"
    );
    // A rotation replaces it - through `rotate_drop_key`, which keeps the key before it.
    let rotated = crate::crypto::drops::rotate_drop_key(&bucket, &k1, &k2, DRIVE)
        .unwrap()
        .expect("incoming mail was on");
    assert_ne!(rotated, public);
    assert_eq!(
        load_drop_key(&bucket, &k2, DRIVE)
            .unwrap()
            .map(|k| k.public()),
        Some(rotated)
    );
    assert!(matches!(
        load_drop_key(&bucket, &k1, DRIVE),
        Err(DriveError::KeyRefused { .. })
    ));
    assert_ne!(replace_drop_key(&bucket, &k2, DRIVE).unwrap(), rotated);

    let permit = drop_permit(&bucket, &k2);
    disable_drop(&permit, &bucket, &k2, DRIVE).unwrap();
    assert!(load_drop_key(&bucket, &k2, DRIVE).unwrap().is_none());
}

/// X-DELTA-KEYS3-2 (the finding's own words): a device that still holds K1 after another device's rotation
/// turned incoming mail on again with `enable_drop`; `load_drop_key` read the K2 file as "no key" and a NEW
/// drop key was put over it - the secret the Worker's mail since the rotation was sealed to, gone. The
/// file is byte-identical after every call a K1 device makes, and each answers the typed refusal.
#[test]
fn x_delta_keys3_2_a_device_with_the_old_key_never_writes_over_the_drop_key_the_new_key_sealed() {
    let bucket = MemBucket::new();
    let (k1, k2) = (DriveKey::generate().unwrap(), DriveKey::generate().unwrap());
    // The drop key as a rotation left it: sealed under K2.
    enable_drop(&drop_permit(&bucket, &k1), &bucket, &k1, DRIVE).unwrap();
    crate::crypto::drops::rotate_drop_key(&bucket, &k1, &k2, DRIVE)
        .unwrap()
        .unwrap();
    let before = bucket.object(DROP_KEY_FILE).unwrap();
    let previous = bucket
        .object(crate::crypto::drops::PREVIOUS_DROP_KEY_FILE)
        .unwrap();

    // The key permit asks the file and refuses: nothing a K1 device does reaches the bucket.
    let keyring = MemoryKeyring::new();
    store_drive_key(&keyring, DRIVE, &k1).unwrap();
    let refused = drop_key_permit(&bucket, &keyring, DRIVE, &k1);
    assert!(
        matches!(refused, Err(DriveError::KeyRefused { .. })),
        "the cell refuses a drop key file of another key: {refused:?}"
    );

    // And the function itself refuses, for a permit asked before the file was another key's (a device
    // that turned mail off and on, a second process on the same keyring): the typed refusal, the file
    // as it was. A permit is no licence to overwrite.
    let empty = MemBucket::new();
    let early = drop_permit(&empty, &k1);
    let again = enable_drop(&early, &bucket, &k1, DRIVE);
    assert!(
        matches!(again, Err(DriveError::KeyRefused { .. })),
        "{again:?}"
    );
    let off = disable_drop(&early, &bucket, &k1, DRIVE);
    assert!(matches!(off, Err(DriveError::KeyRefused { .. })), "{off:?}");
    let read = load_drop_key(&bucket, &k1, DRIVE);
    assert!(
        matches!(read, Err(DriveError::KeyRefused { .. })),
        "{read:?}"
    );
    assert_eq!(
        bucket.object(DROP_KEY_FILE).unwrap(),
        before,
        "_drop.key is byte-identical"
    );
    assert_eq!(
        bucket
            .object(crate::crypto::drops::PREVIOUS_DROP_KEY_FILE)
            .unwrap(),
        previous,
        "so is the key before it"
    );
    // The sentence names the matter and what to do.
    if let Err(DriveError::KeyRefused { reason, .. }) = refused {
        assert!(reason.contains("another drive key"), "{reason}");
    }
    // The K2 device is not in the way: it reads the key it is entitled to.
    assert!(load_drop_key(&bucket, &k2, DRIVE).unwrap().is_some());
}

/// X-DELTA-KEYS3-2: a drop key file that names this device's key and does not open with it is damaged, and
/// nobody can say what it held: it is an error, never "no key", and never written over. A file that is no
/// drop key file at all (junk, cut off, planted) is what it always was: no key, replaced.
#[test]
fn x_delta_keys3_2_a_damaged_drop_key_file_is_never_replaced_and_junk_is() {
    let bucket = MemBucket::new();
    let k1 = DriveKey::generate().unwrap();
    enable_drop(&drop_permit(&bucket, &k1), &bucket, &k1, DRIVE).unwrap();
    let mut file = DropKeyFile::parse(&bucket.object(DROP_KEY_FILE).unwrap()).unwrap();
    // The sealed secret changed: the file still parses and names K1.
    file.sealed[0] ^= 1;
    bucket.set(DROP_KEY_FILE, file.to_bytes());
    let damaged = bucket.object(DROP_KEY_FILE).unwrap();
    assert!(matches!(
        load_drop_key(&bucket, &k1, DRIVE),
        Err(DriveError::Corrupt { .. })
    ));
    assert!(matches!(
        drop_key_permit(&bucket, &keyring_of(&k1), DRIVE, &k1),
        Err(DriveError::Corrupt { .. })
    ));
    let early = drop_permit(&MemBucket::new(), &k1);
    assert!(matches!(
        enable_drop(&early, &bucket, &k1, DRIVE),
        Err(DriveError::Corrupt { .. })
    ));
    assert!(matches!(
        disable_drop(&early, &bucket, &k1, DRIVE),
        Err(DriveError::Corrupt { .. })
    ));
    assert_eq!(
        bucket.object(DROP_KEY_FILE).unwrap(),
        damaged,
        "left as it was"
    );

    // Junk is no drop key file.
    for junk in [
        &b"junk, not a drop key"[..],
        b"{\"format\":\"azlin-drive-key\"}",
        b"",
    ] {
        bucket.set(DROP_KEY_FILE, junk.to_vec());
        assert!(load_drop_key(&bucket, &k1, DRIVE).unwrap().is_none());
        let public = enable_drop(&drop_permit(&bucket, &k1), &bucket, &k1, DRIVE).unwrap();
        assert_eq!(
            load_drop_key(&bucket, &k1, DRIVE)
                .unwrap()
                .map(|k| k.public()),
            Some(public)
        );
    }
}

/// A keyring that holds `key` as the drive key.
fn keyring_of(key: &DriveKey) -> MemoryKeyring {
    let keyring = MemoryKeyring::new();
    store_drive_key(&keyring, DRIVE, key).unwrap();
    keyring
}

/// X-DELTA-KEYS3-2: two devices turn incoming mail on at once. The file is written only where nothing is,
/// conditionally: the second one's key does not replace the first's - it answers with the key that stands -
/// so the public half the Worker was given first is the one that opens the mail.
#[test]
fn x_delta_keys3_2_two_devices_turning_mail_on_at_once_leave_the_first_key() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    /// Looks empty to the first read of the drop key file, then the other device's key is there.
    struct LateKey {
        inner: MemBucket,
        first_read: AtomicUsize,
        theirs: Vec<u8>,
    }
    impl Drive for LateKey {
        fn list(&self, r: &crate::ListRequest) -> Result<crate::ListPage, DriveError> {
            self.inner.list(r)
        }
        fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
            if key == DROP_KEY_FILE && self.first_read.fetch_add(1, Ordering::SeqCst) == 0 {
                self.inner.set(DROP_KEY_FILE, self.theirs.clone());
                return Err(DriveError::NotFound {
                    key: key.to_string(),
                });
            }
            self.inner.get(key)
        }
        fn get_range(&self, key: &str, range: crate::ByteRange) -> Result<Vec<u8>, DriveError> {
            self.inner.get_range(key, range)
        }
        fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
            self.inner.put(key, bytes)
        }
        fn delete(&self, key: &str) -> Result<(), DriveError> {
            self.inner.delete(key)
        }
        fn head(&self, key: &str) -> Result<crate::ObjectInfo, DriveError> {
            self.inner.head(key)
        }
        fn put_if(
            &self,
            key: &str,
            bytes: &[u8],
            condition: &crate::Precondition,
        ) -> Result<Option<String>, DriveError> {
            self.inner.put_if(key, bytes, condition)
        }
    }
    let k1 = DriveKey::generate().unwrap();
    let theirs = {
        let other = MemBucket::new();
        enable_drop(&drop_permit(&other, &k1), &other, &k1, DRIVE).unwrap();
        other.object(DROP_KEY_FILE).unwrap()
    };
    let their_public = DropKeyFile::parse(&theirs).unwrap().public;
    let bucket = LateKey {
        inner: MemBucket::with_conditional_writes(),
        first_read: AtomicUsize::new(0),
        theirs: theirs.clone(),
    };
    let permit = drop_permit(&MemBucket::new(), &k1);
    let ours = enable_drop(&permit, &bucket, &k1, DRIVE).unwrap();
    assert_eq!(ours, their_public, "the key that was there first stands");
    assert_eq!(
        bucket.inner.object(DROP_KEY_FILE).unwrap(),
        theirs,
        "and the file is theirs"
    );
}

/// X-DELTA-KEYS3-2: a permit of another operation, or asked while the keyring held another drive key, is no
/// permit of this write.
#[test]
fn x_delta_keys3_2_a_permit_of_another_operation_or_another_key_writes_nothing() {
    use crate::crypto::device::wraps_permit;
    let bucket = MemBucket::new();
    let (k1, k2) = (DriveKey::generate().unwrap(), DriveKey::generate().unwrap());
    let keyring = keyring_of(&k1);
    let other_op = wraps_permit(Op::NewCode, &keyring, DRIVE).unwrap();
    let denied = enable_drop(&other_op, &bucket, &k1, DRIVE);
    assert!(
        matches!(denied, Err(DriveError::Denied { .. })),
        "{denied:?}"
    );
    let for_k1 = drop_permit(&bucket, &k1);
    let denied = enable_drop(&for_k1, &bucket, &k2, DRIVE);
    assert!(
        matches!(denied, Err(DriveError::Denied { .. })),
        "{denied:?}"
    );
    let denied = disable_drop(&other_op, &bucket, &k1, DRIVE);
    assert!(
        matches!(denied, Err(DriveError::Denied { .. })),
        "{denied:?}"
    );
    assert!(
        bucket.keys().is_empty(),
        "nothing was written: {:?}",
        bucket.keys()
    );
    assert!(KeyringStore::get(&keyring, "unused").unwrap().is_none());
}

#[test]
fn ingest_files_every_drop_once_and_leaves_what_it_cannot_open() {
    let bucket = Arc::new(MemBucket::new());
    let secret = DropSecret::generate().unwrap();
    let mut good = Vec::new();
    for n in 0..2 {
        let key = new_drop_key().unwrap();
        let sealed =
            seal_drop(&secret.public(), DRIVE, &key, 100 + u64::from(n), DropFolder::Inbox, &message(n))
                .unwrap();
        bucket.put(&key, &sealed).unwrap();
        good.push(key);
    }
    let foreign = new_drop_key().unwrap();
    let to_another_key = seal_drop(
        &DropSecret::generate().unwrap().public(),
        DRIVE,
        &foreign,
        5,
        DropFolder::Inbox,
        &message(9),
    )
    .unwrap();
    bucket.put(&foreign, &to_another_key).unwrap();
    let garbage = new_drop_key().unwrap();
    bucket.put(&garbage, b"not a drop").unwrap();
    bucket.put(".azlin/drop/notes.txt", b"not a drop name").unwrap();

    let mut delivered = Vec::new();
    let report = ingest(bucket.as_ref(), &secret, DRIVE, &mut |dropped| {
        delivered.push((dropped.received, dropped.raw.to_vec()));
        Ok(())
    })
    .unwrap();
    assert_eq!(report.delivered, 2);
    delivered.sort();
    assert_eq!(delivered, vec![(100, message(0)), (101, message(1))]);
    let mut left: Vec<String> = report.left.iter().map(|(key, _)| key.clone()).collect();
    left.sort();
    let mut expected_left = vec![foreign.clone(), garbage.clone()];
    expected_left.sort();
    assert_eq!(left, expected_left);
    for key in &good {
        assert!(bucket.object(key).is_none(), "an ingested drop is deleted");
    }
    assert!(bucket.object(&foreign).is_some() && bucket.object(&garbage).is_some());

    // A delivery that fails stops the run and keeps the drop for the next one.
    let key = new_drop_key().unwrap();
    let sealed = seal_drop(&secret.public(), DRIVE, &key, 7, DropFolder::Inbox, &message(7)).unwrap();
    bucket.put(&key, &sealed).unwrap();
    let failed = ingest(bucket.as_ref(), &secret, DRIVE, &mut |_| {
        Err(DriveError::Io(String::from("the drive is offline")))
    });
    assert!(matches!(failed, Err(DriveError::Io(_))));
    assert!(bucket.object(&key).is_some());
}
