//! The keys on a device: a new drive's keys, unlocking from the keyring or the bucket, a second
//! device joining with an invite (once), recovery with the code, two devices setting one drive
//! up at once.

use super::mem_bucket::MemBucket;
use crate::{
    crypto::{
        device::{
            adopt_invite, drive_key_entry, enroll, forget_keys, invite_from_text, invite_text,
            is_encrypted, load_drive_key, load_member_secret, member_key_entry, recover,
            seal_invite, setup_new_drive, unlock, withdraw_invite,
        },
        keys::{RecoveryCode, RecoveryKdf, RECOVERY_KEY_FILE},
        to_hex,
    },
    keyring::{KeyringStore, MemoryKeyring},
    ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition,
};

const DRIVE: &str = "d_k3f9";

/// A tiny Argon2id cost: the tests run in debug builds too.
fn cheap() -> RecoveryKdf {
    RecoveryKdf::with_cost(64, 1, 1).unwrap()
}

#[test]
fn a_new_drive_gets_its_keys_its_recovery_wrap_and_this_devices_wrap() {
    let bucket = MemBucket::new();
    let keyring = MemoryKeyring::new();
    assert!(!is_encrypted(&bucket).unwrap());
    let (drive_key, code) = setup_new_drive(&bucket, &keyring, DRIVE, cheap()).unwrap();
    assert!(is_encrypted(&bucket).unwrap());

    assert_eq!(load_drive_key(&keyring, DRIVE).unwrap(), Some(drive_key.clone()));
    let member = load_member_secret(&keyring, DRIVE).unwrap().unwrap();
    let keys = bucket.keys();
    assert_eq!(
        keys,
        vec![
            format!(".azlin/keys/{}.key", member.public().id()),
            RECOVERY_KEY_FILE.to_string(),
        ]
    );
    // Neither the drive key nor the recovery code is in the bucket.
    let key_hex = to_hex(drive_key.as_bytes());
    for (key, bytes) in bucket.objects() {
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains(&key_hex), "{key}");
        assert!(!text.contains(code.to_text().as_str()), "{key}");
    }
    // A second set-up of the same bucket is refused.
    assert!(matches!(
        setup_new_drive(&bucket, &MemoryKeyring::new(), DRIVE, cheap()),
        Err(DriveError::InvalidConfig(_))
    ));
}

#[test]
fn a_device_unlocks_from_its_keyring_or_from_its_member_wrap() {
    let bucket = MemBucket::new();
    let keyring = MemoryKeyring::new();
    let (drive_key, _) = setup_new_drive(&bucket, &keyring, DRIVE, cheap()).unwrap();
    assert_eq!(unlock(&bucket, &keyring, DRIVE).unwrap(), Some(drive_key.clone()));

    // The keyring lost the drive key but kept the member secret: the wrap in the bucket opens.
    keyring.delete(&drive_key_entry(DRIVE)).unwrap();
    assert_eq!(unlock(&bucket, &keyring, DRIVE).unwrap(), Some(drive_key.clone()));
    assert_eq!(
        load_drive_key(&keyring, DRIVE).unwrap(),
        Some(drive_key),
        "kept again"
    );

    // A device that was never enrolled has nothing to unlock with.
    assert_eq!(unlock(&bucket, &MemoryKeyring::new(), DRIVE).unwrap(), None);

    forget_keys(&keyring, DRIVE).unwrap();
    assert!(keyring.get(&drive_key_entry(DRIVE)).unwrap().is_none());
    assert!(keyring.get(&member_key_entry(DRIVE)).unwrap().is_none());
}

#[test]
fn a_second_device_joins_with_an_invite_and_the_invite_opens_the_key_once() {
    let bucket = MemBucket::new();
    let first = MemoryKeyring::new();
    let (drive_key, _) = setup_new_drive(&bucket, &first, DRIVE, cheap()).unwrap();

    // The first device puts the invite's secret into the join code.
    let invite = seal_invite(&bucket, DRIVE, &drive_key).unwrap();
    let code_text = invite_text(&invite);
    assert_eq!(code_text.len(), 64);
    assert_eq!(bucket.keys().len(), 3, "this device, the recovery wrap, the invite");

    // The second device reads the code, takes the key and enrols itself.
    let second = MemoryKeyring::new();
    let from_code = invite_from_text(&code_text).unwrap();
    let opened = adopt_invite(&bucket, &second, DRIVE, &from_code).unwrap();
    assert_eq!(opened, drive_key);
    assert_eq!(load_drive_key(&second, DRIVE).unwrap(), Some(drive_key.clone()));
    let second_member = load_member_secret(&second, DRIVE).unwrap().unwrap();
    assert!(bucket
        .keys()
        .contains(&format!(".azlin/keys/{}.key", second_member.public().id())));
    assert!(
        !bucket.keys().iter().any(|k| k.contains("invite-")),
        "the invite wrap is gone"
    );
    // Its own wrap opens for it later.
    second.delete(&drive_key_entry(DRIVE)).unwrap();
    assert_eq!(unlock(&bucket, &second, DRIVE).unwrap(), Some(drive_key.clone()));

    // The same code a second time opens nothing.
    assert!(matches!(
        adopt_invite(&bucket, &MemoryKeyring::new(), DRIVE, &from_code),
        Err(DriveError::Denied { .. })
    ));

    // A withdrawn invite opens nothing either.
    let unused = seal_invite(&bucket, DRIVE, &drive_key).unwrap();
    withdraw_invite(&bucket, &unused).unwrap();
    assert!(matches!(
        adopt_invite(&bucket, &MemoryKeyring::new(), DRIVE, &unused),
        Err(DriveError::Denied { .. })
    ));
    assert!(invite_from_text("not hex").is_none());
}

#[test]
fn an_invite_is_bound_to_its_drive() {
    let bucket = MemBucket::new();
    let (drive_key, _) = setup_new_drive(&bucket, &MemoryKeyring::new(), DRIVE, cheap()).unwrap();
    let invite = seal_invite(&bucket, DRIVE, &drive_key).unwrap();
    assert!(matches!(
        adopt_invite(&bucket, &MemoryKeyring::new(), "d_other", &invite),
        Err(DriveError::Denied { .. })
    ));
}

#[test]
fn the_recovery_code_enrols_a_device_that_lost_everything() {
    let bucket = MemBucket::new();
    let (drive_key, code) =
        setup_new_drive(&bucket, &MemoryKeyring::new(), DRIVE, cheap()).unwrap();
    let new_device = MemoryKeyring::new();
    assert_eq!(unlock(&bucket, &new_device, DRIVE).unwrap(), None);

    let typed = RecoveryCode::parse(&code.to_text().to_lowercase()).unwrap();
    assert_eq!(recover(&bucket, &new_device, DRIVE, &typed).unwrap(), drive_key);
    assert_eq!(
        load_drive_key(&new_device, DRIVE).unwrap(),
        Some(drive_key.clone())
    );
    new_device.delete(&drive_key_entry(DRIVE)).unwrap();
    assert_eq!(
        unlock(&bucket, &new_device, DRIVE).unwrap(),
        Some(drive_key),
        "enrolled: its own wrap opens"
    );

    assert!(matches!(
        recover(
            &bucket,
            &MemoryKeyring::new(),
            DRIVE,
            &RecoveryCode::generate().unwrap()
        ),
        Err(DriveError::Denied { .. })
    ));
}

/// The bucket as a second device saw it a moment too early: its listing shows no key files
/// yet; every other call is the bucket's.
struct ListedTooEarly<'a>(&'a MemBucket);

impl Drive for ListedTooEarly<'_> {
    fn list(&self, _request: &ListRequest) -> Result<ListPage, DriveError> {
        Ok(ListPage::default())
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.0.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.0.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.0.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.0.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.0.head(key)
    }
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.0.put_if(key, bytes, condition)
    }
}

#[test]
fn two_devices_setting_one_drive_up_at_once_cannot_both_win() {
    let bucket = MemBucket::with_conditional_writes();
    let (first_key, first_code) =
        setup_new_drive(&bucket, &MemoryKeyring::new(), DRIVE, cheap()).unwrap();
    let before = bucket.object(RECOVERY_KEY_FILE).unwrap();
    // The second device's check came before the first device's files: its conditional PUT of
    // the recovery wrap loses, and nothing of it lands.
    let second = MemoryKeyring::new();
    assert!(matches!(
        setup_new_drive(&ListedTooEarly(&bucket), &second, DRIVE, cheap()),
        Err(DriveError::Conflict { .. })
    ));
    assert_eq!(bucket.object(RECOVERY_KEY_FILE).unwrap(), before);
    assert!(load_drive_key(&second, DRIVE).unwrap().is_none());
    assert_eq!(bucket.keys().len(), 2, "the first device's wrap and its recovery wrap");
    // The first device's code still opens the drive.
    assert_eq!(
        recover(&bucket, &MemoryKeyring::new(), DRIVE, &first_code).unwrap(),
        first_key
    );
}

#[test]
fn enrolling_seals_the_key_to_this_devices_own_member_key() {
    let bucket = MemBucket::new();
    let keyring = MemoryKeyring::new();
    let drive_key = crate::crypto::DriveKey::generate().unwrap();
    let member = enroll(&bucket, &keyring, DRIVE, &drive_key).unwrap();
    assert_eq!(
        member,
        load_member_secret(&keyring, DRIVE)
            .unwrap()
            .unwrap()
            .public()
            .id()
    );
    // Enrolling again keeps the same member key (and rewrites its wrap).
    assert_eq!(enroll(&bucket, &keyring, DRIVE, &drive_key).unwrap(), member);
    assert_eq!(bucket.keys(), vec![format!(".azlin/keys/{member}.key")]);
}

#[test]
fn a_keyring_entry_that_holds_no_key_is_refused_not_guessed() {
    let keyring = MemoryKeyring::new();
    keyring.set(&drive_key_entry(DRIVE), "{\"kind\": \"member-key\", \"key\": \"00\"}").unwrap();
    assert!(matches!(
        load_drive_key(&keyring, DRIVE),
        Err(DriveError::InvalidConfig(_))
    ));
    keyring.set(&drive_key_entry(DRIVE), "not json").unwrap();
    assert!(matches!(
        load_drive_key(&keyring, DRIVE),
        Err(DriveError::InvalidConfig(why)) if !why.contains("not json")
    ));
}
