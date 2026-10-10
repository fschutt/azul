//! The key flows over the drive index's policy: the drive's members and their wraps are
//! recorded in `.azlin/policy.toml` and `.azlin/keys/` of the metadata repository, and a
//! device that holds the drive key reads them from there. The bucket's key files are the copy
//! a device without the drive key opens. A drive whose index has no policy yet gets one at
//! its first key write; a bucket without an index keeps its key files as the record.

use std::sync::Arc;

use crate::{
    crypto::{
        device::{self, adopt_invite, drive_key_entry, load_member_secret, seal_invite, setup_new_drive, unlock},
        keys::{MemberSecret, MemberWrap, RecoveryKdf, KEYS_PREFIX},
        DriveKey,
    },
    encrypted::open_encrypted,
    keyring::{KeyringStore, MemoryKeyring},
    meta::{keys, MemoryBucket, MetaIndexProvider, MetaRepo},
    rotation::rotate,
    Drive,
};

const DRIVE: &str = "d_policy_keys";

fn cheap() -> RecoveryKdf {
    RecoveryKdf::with_cost(64, 1, 1).unwrap()
}

fn member_id(keyring: &MemoryKeyring) -> String {
    load_member_secret(keyring, DRIVE).unwrap().unwrap().public().id()
}

/// The member ids the index's policy names and the ids of its wraps; `None` without a policy.
fn recorded(bucket: &Arc<MemoryBucket>, key: &DriveKey) -> Option<(Vec<String>, Vec<String>)> {
    let repo = MetaRepo::open(Arc::clone(bucket), key.clone(), "checker", "Checker").unwrap();
    let policy = repo.policy().unwrap()?;
    let wraps = repo.member_wraps().unwrap();
    Some((
        policy.members.keys().cloned().collect(),
        wraps.keys().cloned().collect(),
    ))
}

fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids
}

/// A drive set up on the laptop, its index opened (it exists from then on) with one file.
fn drive_with_index() -> (Arc<MemoryBucket>, MemoryKeyring, DriveKey) {
    let bucket = Arc::new(MemoryBucket::new());
    let laptop = MemoryKeyring::new();
    let (key, _) = setup_new_drive(bucket.as_ref(), &laptop, DRIVE, cheap()).unwrap();
    let plain: Arc<dyn Drive> = bucket.clone();
    let drive = open_encrypted(plain, &laptop, DRIVE, &MetaIndexProvider::new("Laptop")).unwrap();
    drive.put("notes/plan.txt", b"the plan").unwrap();
    (bucket, laptop, key)
}

/// A second device joins with an invite.
fn join(bucket: &Arc<MemoryBucket>, key: &DriveKey) -> MemoryKeyring {
    let invite = seal_invite(bucket.as_ref(), DRIVE, key).unwrap();
    let phone = MemoryKeyring::new();
    adopt_invite(bucket.as_ref(), &phone, DRIVE, &invite).unwrap();
    phone
}

#[test]
fn the_first_key_write_moves_a_drives_members_into_its_policy() {
    let (bucket, laptop, key) = drive_with_index();
    assert_eq!(recorded(&bucket, &key), None, "no policy before the first key write");

    let phone = join(&bucket, &key);
    let both = sorted(vec![member_id(&laptop), member_id(&phone)]);
    assert_eq!(recorded(&bucket, &key), Some((both.clone(), both.clone())));
    assert_eq!(device::members(bucket.as_ref(), &key).unwrap(), both);

    // A key file planted in the bucket makes nobody a member: the policy is the record.
    let stranger = MemberSecret::generate().unwrap().public().id();
    let phone_file = format!("{KEYS_PREFIX}{}.key", member_id(&phone));
    let bytes = bucket.get(&phone_file).unwrap();
    bucket
        .put(&format!("{KEYS_PREFIX}{stranger}.key"), &bytes)
        .unwrap();
    assert_eq!(device::members(bucket.as_ref(), &key).unwrap(), both);

    // The drive's files are where they were.
    let plain: Arc<dyn Drive> = bucket.clone();
    let drive = open_encrypted(plain, &laptop, DRIVE, &MetaIndexProvider::new("Laptop")).unwrap();
    assert_eq!(drive.get("notes/plan.txt").unwrap(), b"the plan");
}

#[test]
fn a_device_the_policy_no_longer_names_does_not_unlock_with_the_key_file_left_in_the_bucket() {
    let (bucket, laptop, key) = drive_with_index();
    let phone = join(&bucket, &key);
    let phone_id = member_id(&phone);

    // Another device removed the phone from the policy; its key file stayed in the bucket.
    let mut repo = MetaRepo::open(Arc::clone(&bucket), key.clone(), "tablet", "Tablet").unwrap();
    let mut policy = repo.policy().unwrap().unwrap();
    policy.members.remove(&phone_id);
    repo.set_policy(&policy).unwrap();
    repo.set_member_wrap(&phone_id, None).unwrap();
    assert!(bucket
        .get(&format!("{KEYS_PREFIX}{phone_id}.key"))
        .is_ok());

    phone.delete(&drive_key_entry(DRIVE)).unwrap();
    assert_eq!(unlock(bucket.as_ref(), &phone, DRIVE).unwrap(), None);
    assert_eq!(phone.get(&drive_key_entry(DRIVE)).unwrap(), None, "no key kept");

    // A member the policy names unlocks from its key file as before.
    laptop.delete(&drive_key_entry(DRIVE)).unwrap();
    assert_eq!(unlock(bucket.as_ref(), &laptop, DRIVE).unwrap(), Some(key));
}

#[test]
fn a_drive_without_an_index_keeps_its_key_files_as_its_members() {
    let bucket = Arc::new(MemoryBucket::new());
    let laptop = MemoryKeyring::new();
    let (key, _) = setup_new_drive(bucket.as_ref(), &laptop, DRIVE, cheap()).unwrap();
    let phone = join(&bucket, &key);

    assert!(
        !bucket.objects().iter().any(|(name, _)| name.starts_with(keys::ROOT)),
        "the key flows create no index"
    );
    assert_eq!(
        device::members(bucket.as_ref(), &key).unwrap(),
        sorted(vec![member_id(&laptop), member_id(&phone)])
    );
}

#[test]
fn a_rotation_removes_every_member_the_policy_names_but_this_device() {
    let (bucket, laptop, key) = drive_with_index();
    let phone = join(&bucket, &key);
    // A tablet the policy names whose key file is not in the bucket (any more).
    let tablet = MemberSecret::generate().unwrap();
    let tablet_id = tablet.public().id();
    let wrap = MemberWrap::seal(&key, DRIVE, &tablet_id, &tablet.public()).unwrap();
    let mut repo = MetaRepo::open(Arc::clone(&bucket), key.clone(), "tablet", "Tablet").unwrap();
    let mut policy = repo.policy().unwrap().unwrap();
    let laptop_entry = policy.members[&member_id(&laptop)].clone();
    policy.members.insert(tablet_id.clone(), laptop_entry);
    repo.set_policy(&policy).unwrap();
    repo.set_member_wrap(&tablet_id, Some(&wrap.to_bytes())).unwrap();

    let plain: Arc<dyn Drive> = bucket.clone();
    let provider = MetaIndexProvider::new("Laptop");
    let rotated = rotate(plain, &laptop, DRIVE, &provider, cheap()).unwrap();
    assert_eq!(
        rotated.members_removed, 3,
        "the phone, the tablet and this device's old member key"
    );

    let new_key = device::load_drive_key(&laptop, DRIVE).unwrap().unwrap();
    let me = vec![member_id(&laptop)];
    assert_eq!(recorded(&bucket, &new_key), Some((me.clone(), me.clone())));
    assert_eq!(device::members(bucket.as_ref(), &new_key).unwrap(), me);
    assert!(bucket
        .get(&format!("{KEYS_PREFIX}{}.key", member_id(&phone)))
        .is_err());
}
