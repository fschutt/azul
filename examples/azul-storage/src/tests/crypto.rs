//! The keys of an encrypted drive: file keys wrapped by the drive key or a share key, the
//! object ids that name the bucket's objects, ids of keys.

use std::collections::HashSet;

use crate::crypto::{
    CryptoError, DriveKey, FileKey, KeyId, ObjectId, ShareKey, WrappedKey, KEY_LEN,
};

#[test]
fn a_file_key_wrapped_by_the_drive_key_opens_for_its_object_only() {
    let drive = DriveKey::generate().unwrap();
    let file = FileKey::generate().unwrap();
    let object = ObjectId::generate().unwrap();
    let wrapped = drive.wrap_file_key(&file, &object).unwrap();
    assert_eq!(wrapped.key_id, drive.id());
    assert_eq!(drive.unwrap_file_key(&wrapped, &object).unwrap(), file);

    let another_object = ObjectId::generate().unwrap();
    assert!(matches!(
        drive.unwrap_file_key(&wrapped, &another_object),
        Err(CryptoError::Damaged(_))
    ));
    let another_drive = DriveKey::generate().unwrap();
    assert_eq!(
        another_drive.unwrap_file_key(&wrapped, &object),
        Err(CryptoError::WrongKey)
    );
}

#[test]
fn every_wrap_of_a_key_is_new_and_round_trips_through_its_bytes_and_hex() {
    let drive = DriveKey::generate().unwrap();
    let file = FileKey::generate().unwrap();
    let object = ObjectId::generate().unwrap();
    let a = drive.wrap_file_key(&file, &object).unwrap();
    let b = drive.wrap_file_key(&file, &object).unwrap();
    assert_ne!(a.nonce, b.nonce, "a fresh nonce per wrap");
    assert_ne!(a.sealed, b.sealed);

    let bytes = a.to_bytes();
    assert_eq!(bytes.len(), WrappedKey::LEN);
    assert_eq!(WrappedKey::from_bytes(&bytes), Some(a.clone()));
    assert_eq!(WrappedKey::from_hex(&a.to_hex()), Some(a.clone()));
    assert_eq!(WrappedKey::from_bytes(&bytes[1..]), None);

    let mut flipped = a.clone();
    flipped.sealed[0] ^= 1;
    assert!(matches!(
        drive.unwrap_file_key(&flipped, &object),
        Err(CryptoError::Damaged(_))
    ));
    let mut relabelled = a;
    relabelled.key_id = DriveKey::generate().unwrap().id();
    assert_eq!(
        drive.unwrap_file_key(&relabelled, &object),
        Err(CryptoError::WrongKey)
    );
}

#[test]
fn a_share_key_and_the_drive_key_cannot_open_each_others_wraps() {
    let bytes = [5u8; KEY_LEN];
    let drive = DriveKey::from_bytes(bytes);
    // The same bytes as a share key: another id, another wrapping key.
    let share = ShareKey::from_bytes(bytes);
    let file = FileKey::generate().unwrap();
    let object = ObjectId::generate().unwrap();

    let for_share = share.wrap_file_key(&file, &object).unwrap();
    assert_eq!(share.unwrap_file_key(&for_share, &object).unwrap(), file);
    assert_eq!(
        drive.unwrap_file_key(&for_share, &object),
        Err(CryptoError::WrongKey)
    );

    let for_drive = drive.wrap_file_key(&file, &object).unwrap();
    assert_eq!(
        share.unwrap_file_key(&for_drive, &object),
        Err(CryptoError::WrongKey)
    );
    // Even a wrap relabelled with the share's id does not open: the wrapping keys differ.
    let mut relabelled = for_drive;
    relabelled.key_id = share.id();
    assert!(matches!(
        share.unwrap_file_key(&relabelled, &object),
        Err(CryptoError::Damaged(_))
    ));
}

#[test]
fn an_object_id_names_a_data_object_under_a_two_digit_fan_out() {
    let fixed = ObjectId([0xab; 16]);
    let hex = "ab".repeat(16);
    assert_eq!(fixed.to_hex(), hex);
    assert_eq!(fixed.bucket_key(), format!("data/ab/{hex}"));
    assert_eq!(ObjectId::from_bucket_key(&fixed.bucket_key()), Some(fixed));
    assert_eq!(ObjectId::from_bucket_key(&format!("data/cd/{hex}")), None);
    assert_eq!(
        ObjectId::from_bucket_key(&format!("data/AB/{}", "AB".repeat(16))),
        None,
        "only the canonical lowercase form"
    );
    assert_eq!(ObjectId::from_bucket_key("data/ab/abab"), None);
    assert_eq!(ObjectId::from_bucket_key(&format!("mail/ab/{hex}")), None);

    let ids: HashSet<ObjectId> = (0..1000).map(|_| ObjectId::generate().unwrap()).collect();
    assert_eq!(ids.len(), 1000, "a thousand ids, a thousand values");
    for id in ids.iter().take(10) {
        let key = id.bucket_key();
        assert!(key.starts_with("data/"), "{key}");
        assert_eq!(key.len(), "data/".len() + 3 + 32, "{key}");
        assert_eq!(ObjectId::from_bucket_key(&key), Some(*id));
    }
}

#[test]
fn keys_never_print_and_compare_by_value() {
    let key = DriveKey::from_bytes([7u8; KEY_LEN]);
    assert_eq!(format!("{key:?}"), "DriveKey(***)");
    assert_eq!(
        format!("{:?}", FileKey::from_bytes([1u8; KEY_LEN])),
        "FileKey(***)"
    );
    assert_eq!(
        format!("{:?}", ShareKey::from_bytes([1u8; KEY_LEN])),
        "ShareKey(***)"
    );
    assert_eq!(key, DriveKey::from_bytes([7u8; KEY_LEN]));
    assert_ne!(key, DriveKey::from_bytes([8u8; KEY_LEN]));
    assert!(DriveKey::from_slice(&[7u8; 31]).is_none());
    assert_eq!(DriveKey::from_slice(&[7u8; 32]), Some(key.clone()));
    assert_ne!(
        DriveKey::generate().unwrap(),
        DriveKey::generate().unwrap(),
        "two random keys"
    );
    let wrapped = key
        .wrap_file_key(&FileKey::generate().unwrap(), &ObjectId([0u8; 16]))
        .unwrap();
    let shown = format!("{wrapped:?}");
    assert!(shown.contains(&key.id().to_hex()), "{shown}");
    assert!(!shown.contains("sealed"), "{shown}");
}

#[test]
fn a_key_id_is_stable_and_differs_between_keys_and_kinds_of_key() {
    let bytes = [9u8; KEY_LEN];
    let drive = DriveKey::from_bytes(bytes);
    assert_eq!(drive.id(), DriveKey::from_bytes(bytes).id());
    assert_ne!(drive.id(), DriveKey::from_bytes([10u8; KEY_LEN]).id());
    assert_ne!(drive.id(), ShareKey::from_bytes(bytes).id());
    assert_eq!(KeyId::from_hex(&drive.id().to_hex()), Some(drive.id()));
    assert_eq!(drive.id().to_hex().len(), 32);
    assert_ne!(&drive.id().0[..], &bytes[..16], "the id is not the key");
}

#[test]
fn a_crypto_error_comes_back_as_itself_through_an_io_error() {
    let io: std::io::Error = CryptoError::KeyMismatch.into();
    assert_eq!(io.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(CryptoError::from(io), CryptoError::KeyMismatch);
    let plain = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
    assert!(matches!(CryptoError::from(plain), CryptoError::Io(why) if why.contains("gone")));
}
