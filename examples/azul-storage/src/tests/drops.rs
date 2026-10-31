//! AZD1, the drops of incoming mail: the shared test vector, what a drop opens for, the drop
//! key in the bucket, and ingestion.

use std::sync::Arc;

use super::mem_bucket::MemBucket;
use crate::{
    crypto::{
        drops::{
            disable_drop, enable_drop, ingest, is_drop_key, load_drop_key, new_drop_key,
            open_drop, replace_drop_key, seal_drop, seal_drop_with, DropFolder, DropKeyFile,
            DropPublic, DropSecret, Dropped, DROP_KEY_FILE, DROP_PREFIX, HEADER_LEN,
        },
        hex_array,
        keys::member_key_file,
        to_hex, CryptoError, DriveKey,
    },
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

#[test]
fn the_drop_key_lives_in_the_bucket_sealed_with_the_drive_key() {
    let bucket = MemBucket::new();
    let k1 = DriveKey::generate().unwrap();
    let public = enable_drop(&bucket, &k1, DRIVE).unwrap();
    assert_eq!(enable_drop(&bucket, &k1, DRIVE).unwrap(), public, "the same key again");
    let file_bytes = bucket.object(DROP_KEY_FILE).unwrap();
    let file = DropKeyFile::parse(&file_bytes).unwrap();
    assert_eq!(file.public, public);
    assert_eq!(file.drive_key_id, k1.id());
    let secret = load_drop_key(&bucket, &k1, DRIVE).unwrap().unwrap();
    assert_eq!(secret.public(), public);
    let text = String::from_utf8(file_bytes).unwrap();
    assert!(!text.contains(&to_hex(&secret.to_bytes()[..])), "no secret in the clear");
    assert!(matches!(
        file.open(&k1, "d_other"),
        Err(CryptoError::WrongKey)
    ));

    // A rotated drive key does not open it: enabling again makes a new one in its place.
    let k2 = DriveKey::generate().unwrap();
    assert!(load_drop_key(&bucket, &k2, DRIVE).unwrap().is_none());
    let new_public = enable_drop(&bucket, &k2, DRIVE).unwrap();
    assert_ne!(new_public, public);
    assert!(load_drop_key(&bucket, &k1, DRIVE).unwrap().is_none());
    assert_ne!(replace_drop_key(&bucket, &k2, DRIVE).unwrap(), new_public);
    assert_eq!(DropPublic::from_hex(&new_public.to_hex()), Some(new_public));

    disable_drop(&bucket).unwrap();
    assert!(load_drop_key(&bucket, &k2, DRIVE).unwrap().is_none());
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
