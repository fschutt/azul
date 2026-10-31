//! A device keeps the id of the drive key it first held (its pin, beside its member key in the
//! keyring) and never takes another one from the bucket: whoever controls the storage can seal
//! a drive key of their own to a member's public key, and a device that unlocked from that wrap
//! would encrypt every new upload with it. Join and claim codes carry the drive key's id, so
//! even a device's first key is checked against something the server did not hand out.

use std::sync::Arc;

use crate::{
    crypto::{
        device::{
            adopt_invite, catch_up, commit_drive_key, committed_drive_key, drive_key_entry,
            load_drive_key, load_member_secret, member_key_entry, pinned_drive_key,
            plan_drive_key, planned_key_entry, recover, seal_invite, setup_new_drive, unlock,
        },
        keys::{store_member_wrap, MemberWrap, RecoveryKdf},
        to_hex, DriveKey, KeyId,
    },
    encrypted::open_encrypted,
    keyring::{KeyringStore, MemoryKeyring},
    meta::{keys, MemoryBucket, MetaIndexProvider},
    rotation::rotate,
    Drive, DriveError,
};

const DRIVE: &str = "d_key_pin";

fn cheap() -> RecoveryKdf {
    RecoveryKdf::with_cost(64, 1, 1).unwrap()
}

/// A drive set up on the laptop; nothing written yet, so the bucket holds no index.
fn empty_drive() -> (Arc<MemoryBucket>, MemoryKeyring, DriveKey, crate::crypto::keys::RecoveryCode) {
    let bucket = Arc::new(MemoryBucket::new());
    let laptop = MemoryKeyring::new();
    let (key, code) = setup_new_drive(bucket.as_ref(), &laptop, DRIVE, cheap()).unwrap();
    (bucket, laptop, key, code)
}

/// A drive set up on the laptop with one file: its index exists.
fn drive_with_files() -> (Arc<MemoryBucket>, MemoryKeyring, DriveKey) {
    let (bucket, laptop, key, _) = empty_drive();
    let plain: Arc<dyn Drive> = bucket.clone();
    let drive = open_encrypted(plain, &laptop, DRIVE, &MetaIndexProvider::new("Laptop")).unwrap();
    drive.put("notes/plan.txt", b"the plan").unwrap();
    (bucket, laptop, key)
}

/// What the storage does: a drive key of its own sealed to the member key of `keyring`, in
/// that member's key file. Returns the forged key.
fn substitute_wrap(bucket: &MemoryBucket, keyring: &MemoryKeyring) -> DriveKey {
    let public = load_member_secret(keyring, DRIVE).unwrap().unwrap().public();
    let forged = DriveKey::generate().unwrap();
    let wrap = MemberWrap::seal(&forged, DRIVE, &public.id(), &public).unwrap();
    store_member_wrap(bucket, &wrap).unwrap();
    forged
}

/// The keyring of a device that keeps only its member key for the drive (the drive key left
/// it): the member entry as the device keeps it, pin and all.
fn member_only(keyring: &MemoryKeyring) -> MemoryKeyring {
    let device = MemoryKeyring::new();
    let entry = member_key_entry(DRIVE);
    device.set(&entry, &keyring.get(&entry).unwrap().unwrap()).unwrap();
    device
}

/// The keyring of a device from before pins: its member key as it was kept then (no pin).
fn member_from_before(keyring: &MemoryKeyring) -> MemoryKeyring {
    let secret = load_member_secret(keyring, DRIVE).unwrap().unwrap();
    let device = MemoryKeyring::new();
    let text = format!(
        "{{\"kind\":\"member-key\",\"key\":\"{}\"}}",
        to_hex(&secret.to_bytes()[..])
    );
    device.set(&member_key_entry(DRIVE), &text).unwrap();
    device
}

fn is_refused<T>(result: &Result<T, DriveError>) -> bool {
    matches!(result, Err(DriveError::KeyRefused { .. }))
}

#[test]
fn a_device_pins_the_drive_key_it_sets_up_joins_with_or_recovers() {
    let (bucket, laptop, key, code) = empty_drive();
    assert_eq!(pinned_drive_key(&laptop, DRIVE).unwrap(), Some(key.id()));

    let invite = seal_invite(bucket.as_ref(), DRIVE, &key).unwrap();
    let phone = MemoryKeyring::new();
    adopt_invite(bucket.as_ref(), &phone, DRIVE, &invite, Some(&key.id())).unwrap();
    assert_eq!(pinned_drive_key(&phone, DRIVE).unwrap(), Some(key.id()));

    let desktop = MemoryKeyring::new();
    recover(bucket.as_ref(), &desktop, DRIVE, &code).unwrap();
    assert_eq!(pinned_drive_key(&desktop, DRIVE).unwrap(), Some(key.id()));
}

#[test]
fn a_pinned_device_refuses_a_substituted_wrap_on_an_empty_drive() {
    let (bucket, laptop, key, _) = empty_drive();
    let device = member_only(&laptop);
    substitute_wrap(&bucket, &laptop);
    assert!(is_refused(&unlock(bucket.as_ref(), &device, DRIVE)));
    assert_eq!(load_drive_key(&device, DRIVE).unwrap(), None, "nothing kept");
    assert_eq!(pinned_drive_key(&device, DRIVE).unwrap(), Some(key.id()), "the pin stays");
}

#[test]
fn a_pinned_device_refuses_a_substituted_wrap_on_a_drive_with_files_too() {
    let (bucket, laptop, key) = drive_with_files();
    let device = member_only(&laptop);
    substitute_wrap(&bucket, &laptop);
    assert!(is_refused(&unlock(bucket.as_ref(), &device, DRIVE)));
    let plain: Arc<dyn Drive> = bucket.clone();
    let opened = open_encrypted(plain, &device, DRIVE, &MetaIndexProvider::new("Phone"));
    assert!(matches!(opened, Err(DriveError::KeyRefused { .. })));
    assert_eq!(pinned_drive_key(&device, DRIVE).unwrap(), Some(key.id()));
}

#[test]
fn a_pinned_device_still_unlocks_from_its_own_wrap() {
    let (bucket, laptop, key) = drive_with_files();
    let device = member_only(&laptop);
    assert_eq!(unlock(bucket.as_ref(), &device, DRIVE).unwrap(), Some(key.clone()));
    assert_eq!(load_drive_key(&device, DRIVE).unwrap(), Some(key));
}

#[test]
fn a_device_that_kept_the_drive_key_does_not_catch_up_to_a_substituted_wrap() {
    let (bucket, laptop, key) = drive_with_files();
    substitute_wrap(&bucket, &laptop);
    assert!(is_refused(&catch_up(bucket.as_ref(), &laptop, DRIVE)));
    assert_eq!(load_drive_key(&laptop, DRIVE).unwrap(), Some(key.clone()), "its key stays");

    // The index the server swapped no longer opens with the real key: the refusal is what the
    // drive says, not the index's error.
    Drive::put(bucket.as_ref(), keys::MANIFEST, b"not the drive's manifest").unwrap();
    let plain: Arc<dyn Drive> = bucket.clone();
    let opened = open_encrypted(plain, &laptop, DRIVE, &MetaIndexProvider::new("Laptop"));
    assert!(matches!(opened, Err(DriveError::KeyRefused { .. })));
    assert_eq!(load_drive_key(&laptop, DRIVE).unwrap(), Some(key));
}

#[test]
fn an_unpinned_device_on_an_empty_drive_asks_instead_of_trusting_the_wrap() {
    let (bucket, laptop, key, code) = empty_drive();
    let device = member_from_before(&laptop);
    // Not even the real wrap: an empty drive has nothing that says which key is the drive's.
    assert_eq!(unlock(bucket.as_ref(), &device, DRIVE).unwrap(), None);
    substitute_wrap(&bucket, &laptop);
    assert_eq!(unlock(bucket.as_ref(), &device, DRIVE).unwrap(), None);
    assert_eq!(load_drive_key(&device, DRIVE).unwrap(), None);
    assert_eq!(pinned_drive_key(&device, DRIVE).unwrap(), None);
    let plain: Arc<dyn Drive> = bucket.clone();
    let opened = open_encrypted(plain, &device, DRIVE, &MetaIndexProvider::new("Phone"));
    assert!(matches!(opened, Err(DriveError::Locked { .. })), "it asks");

    // The recovery code answers: the real key, pinned, its wrap sealed again.
    assert_eq!(recover(bucket.as_ref(), &device, DRIVE, &code).unwrap(), key);
    assert_eq!(pinned_drive_key(&device, DRIVE).unwrap(), Some(key.id()));
    device.delete(&drive_key_entry(DRIVE)).unwrap();
    assert_eq!(unlock(bucket.as_ref(), &device, DRIVE).unwrap(), Some(key));
}

#[test]
fn an_unpinned_device_pins_the_key_its_drives_index_opens_with() {
    let (bucket, laptop, key) = drive_with_files();
    let device = member_from_before(&laptop);
    assert_eq!(unlock(bucket.as_ref(), &device, DRIVE).unwrap(), Some(key.clone()));
    assert_eq!(pinned_drive_key(&device, DRIVE).unwrap(), Some(key.id()));
}

#[test]
fn an_unpinned_device_refuses_a_wrap_its_drives_index_does_not_open_with() {
    let (bucket, laptop, _key) = drive_with_files();
    let device = member_from_before(&laptop);
    substitute_wrap(&bucket, &laptop);
    assert!(is_refused(&unlock(bucket.as_ref(), &device, DRIVE)));
    assert_eq!(load_drive_key(&device, DRIVE).unwrap(), None);
    assert_eq!(pinned_drive_key(&device, DRIVE).unwrap(), None);
}

#[test]
fn a_device_from_before_pins_pins_the_drive_key_it_keeps() {
    let (bucket, laptop, key, _) = empty_drive();
    let device = member_from_before(&laptop);
    let entry = drive_key_entry(DRIVE);
    device.set(&entry, &laptop.get(&entry).unwrap().unwrap()).unwrap();
    assert_eq!(unlock(bucket.as_ref(), &device, DRIVE).unwrap(), Some(key.clone()));
    assert_eq!(pinned_drive_key(&device, DRIVE).unwrap(), Some(key.id()));
    // From then on a substituted wrap is refused, the drive key gone or not.
    device.delete(&entry).unwrap();
    substitute_wrap(&bucket, &laptop);
    assert!(is_refused(&unlock(bucket.as_ref(), &device, DRIVE)));
}

#[test]
fn a_join_code_whose_key_id_is_not_the_sealed_keys_is_refused() {
    let (bucket, _laptop, key, _) = empty_drive();
    let invite = seal_invite(bucket.as_ref(), DRIVE, &key).unwrap();
    let phone = MemoryKeyring::new();
    let other = DriveKey::generate().unwrap().id();
    assert!(is_refused(&adopt_invite(bucket.as_ref(), &phone, DRIVE, &invite, Some(&other))));
    assert_eq!(load_drive_key(&phone, DRIVE).unwrap(), None, "nothing kept");
    assert_eq!(pinned_drive_key(&phone, DRIVE).unwrap(), None);
    // The code's own key id: it joins (the refusal did not spend the invite).
    let opened = adopt_invite(bucket.as_ref(), &phone, DRIVE, &invite, Some(&key.id())).unwrap();
    assert_eq!(opened, key);
}

#[test]
fn an_old_join_code_without_a_key_id_joins_and_the_pin_guards_the_device_from_then_on() {
    let (bucket, laptop, key, _) = empty_drive();
    let invite = seal_invite(bucket.as_ref(), DRIVE, &key).unwrap();
    let phone = MemoryKeyring::new();
    assert_eq!(adopt_invite(bucket.as_ref(), &phone, DRIVE, &invite, None).unwrap(), key);
    assert_eq!(pinned_drive_key(&phone, DRIVE).unwrap(), Some(key.id()));
    phone.delete(&drive_key_entry(DRIVE)).unwrap();
    assert_eq!(unlock(bucket.as_ref(), &phone, DRIVE).unwrap(), Some(key.clone()));
    phone.delete(&drive_key_entry(DRIVE)).unwrap();
    substitute_wrap(&bucket, &phone);
    assert!(is_refused(&unlock(bucket.as_ref(), &phone, DRIVE)));
    // The laptop that set the drive up is not touched by the phone's wrap.
    assert_eq!(unlock(bucket.as_ref(), &laptop, DRIVE).unwrap(), Some(key));
}

#[test]
fn a_claim_codes_key_id_refuses_a_recovered_key_it_does_not_name() {
    let (bucket, _laptop, key, code) = empty_drive();
    let picker = MemoryKeyring::new();
    let other = DriveKey::generate().unwrap().id();
    commit_drive_key(&picker, DRIVE, &other).unwrap();
    assert_eq!(committed_drive_key(&picker, DRIVE).unwrap(), Some(other));
    assert!(is_refused(&recover(bucket.as_ref(), &picker, DRIVE, &code)));
    assert_eq!(load_drive_key(&picker, DRIVE).unwrap(), None);

    let picker = MemoryKeyring::new();
    commit_drive_key(&picker, DRIVE, &key.id()).unwrap();
    assert_eq!(recover(bucket.as_ref(), &picker, DRIVE, &code).unwrap(), key);
    assert_eq!(pinned_drive_key(&picker, DRIVE).unwrap(), Some(key.id()));
    assert_eq!(committed_drive_key(&picker, DRIVE).unwrap(), None, "held now: pinned");
}

#[test]
fn a_claim_codes_key_id_refuses_a_wrap_before_the_device_ever_held_the_key() {
    let (bucket, _laptop, key, _) = empty_drive();
    let picker = MemoryKeyring::new();
    commit_drive_key(&picker, DRIVE, &key.id()).unwrap();
    // The storage seals a key of its own to the picker's member key (made with the commitment).
    substitute_wrap(&bucket, &picker);
    assert!(is_refused(&unlock(bucket.as_ref(), &picker, DRIVE)));
    assert_eq!(load_drive_key(&picker, DRIVE).unwrap(), None);
}

#[test]
fn a_join_code_beats_a_claim_codes_key_id_it_contradicts() {
    // The claim code named the key the drive had then; a join code made on a device that holds
    // the drive key now is the newer word (the claim code came before a rotation).
    let (bucket, _laptop, key, _) = empty_drive();
    let picker = MemoryKeyring::new();
    let earlier: KeyId = DriveKey::generate().unwrap().id();
    commit_drive_key(&picker, DRIVE, &earlier).unwrap();
    let invite = seal_invite(bucket.as_ref(), DRIVE, &key).unwrap();
    assert_eq!(
        adopt_invite(bucket.as_ref(), &picker, DRIVE, &invite, Some(&key.id())).unwrap(),
        key
    );
    assert_eq!(pinned_drive_key(&picker, DRIVE).unwrap(), Some(key.id()));
}

#[test]
fn a_planned_drive_key_is_the_one_its_drive_is_set_up_with() {
    let bucket = MemoryBucket::new();
    let laptop = MemoryKeyring::new();
    let planned = DriveKey::generate().unwrap();
    plan_drive_key(&laptop, DRIVE, &planned).unwrap();
    let (key, _) = setup_new_drive(&bucket, &laptop, DRIVE, cheap()).unwrap();
    assert_eq!(key, planned);
    assert_eq!(pinned_drive_key(&laptop, DRIVE).unwrap(), Some(planned.id()));
    assert!(laptop.get(&planned_key_entry(DRIVE)).unwrap().is_none(), "used: gone");
}

#[test]
fn a_rotation_pins_the_new_key_on_the_rotating_device() {
    let (bucket, laptop, old) = drive_with_files();
    let plain: Arc<dyn Drive> = bucket.clone();
    let rotated = rotate(plain, &laptop, DRIVE, &MetaIndexProvider::new("Laptop"), cheap()).unwrap();
    assert_ne!(rotated.drive_key, old.id());
    assert_eq!(pinned_drive_key(&laptop, DRIVE).unwrap(), Some(rotated.drive_key));
    // Its own new wrap opens for it.
    let device = member_only(&laptop);
    let new = unlock(bucket.as_ref(), &device, DRIVE).unwrap().unwrap();
    assert_eq!(new.id(), rotated.drive_key);
}
