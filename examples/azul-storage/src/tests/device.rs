//! The keys on a device: a new drive's keys, unlocking from the keyring or the bucket, a second
//! device joining with an invite (once), recovery with the code, two devices setting one drive
//! up at once.

use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};

use super::{
    faulty::{Fail, Faulty},
    mem_bucket::MemBucket,
};
use crate::{
    crypto::{
        device::{
            add_recovery_code, adopt_invite, adopt_invite_reporting, drive_key_entry, enroll,
            invite_from_text, invite_text, is_encrypted, load_drive_key, load_key_at,
            load_member_secret, member_key_entry, plan_drive_key, planned_key_entry, recover,
            seal_invite, setup_new_drive, setup_new_drive_reporting, setup_pending, unlock,
            withdraw_invite, wraps_permit,
        },
        keys::{RecoveryCode, RecoveryKdf, RECOVERY_KEY_FILE},
        keystate::{Op, Refusal},
        to_hex, DriveKey,
    },
    keyring::{KeyringError, KeyringStore, MemoryKeyring},
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
    let opened = adopt_invite(&bucket, &second, DRIVE, &from_code, Some(&drive_key.id())).unwrap();
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
        adopt_invite(&bucket, &MemoryKeyring::new(), DRIVE, &from_code, None),
        Err(DriveError::Denied { .. })
    ));

    // A withdrawn invite opens nothing either - withdrawn for the permit of a member's removal, which only
    // the permit of that operation is.
    let unused = seal_invite(&bucket, DRIVE, &drive_key).unwrap();
    let wrong = wraps_permit(Op::NewCode, &first, DRIVE).unwrap();
    assert!(matches!(
        withdraw_invite(&wrong, &bucket, &unused),
        Err(DriveError::Denied { .. })
    ));
    let permit = wraps_permit(Op::RemoveMember, &first, DRIVE).unwrap();
    withdraw_invite(&permit, &bucket, &unused).unwrap();
    assert!(matches!(
        adopt_invite(&bucket, &MemoryKeyring::new(), DRIVE, &unused, None),
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
        adopt_invite(&bucket, &MemoryKeyring::new(), "d_other", &invite, None),
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

// ==== K3: a setup that stops halfway does not strand the drive ====

/// A bucket whose next puts of a member's key file fail (no network), and that notes whether the
/// device already kept the drive key when the recovery wrap went in.
struct Flaky<'a> {
    inner: &'a MemBucket,
    keyring: &'a MemoryKeyring,
    member_puts_failing: AtomicU32,
    /// Whether the keyring held the drive key (planned or active) when the recovery wrap was put.
    key_held_at_recovery_put: std::sync::Mutex<Option<bool>>,
}

impl<'a> Flaky<'a> {
    fn new(inner: &'a MemBucket, keyring: &'a MemoryKeyring, failing: u32) -> Flaky<'a> {
        Flaky {
            inner,
            keyring,
            member_puts_failing: AtomicU32::new(failing),
            key_held_at_recovery_put: std::sync::Mutex::new(None),
        }
    }

    fn note_recovery(&self, key: &str) {
        if key == RECOVERY_KEY_FILE {
            let held = load_key_at(self.keyring, &planned_key_entry(DRIVE))
                .unwrap()
                .is_some()
                || load_drive_key(self.keyring, DRIVE).unwrap().is_some();
            *self.key_held_at_recovery_put.lock().unwrap() = Some(held);
        }
    }
}

impl Drive for Flaky<'_> {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.inner.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.note_recovery(key);
        let is_member_file = key.starts_with(".azlin/keys/") && key != RECOVERY_KEY_FILE;
        if is_member_file && self.member_puts_failing.load(Ordering::SeqCst) > 0 {
            self.member_puts_failing.fetch_sub(1, Ordering::SeqCst);
            return Err(DriveError::Transport(String::from("the network went")));
        }
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.inner.head(key)
    }
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.note_recovery(key);
        self.inner.put_if(key, bytes, condition)
    }
}

/// K3: the recovery wrap was published before this device held the key; a network error, a denied
/// keychain prompt or a kill after it left a bucket that counts as encrypted, with no wrap for the
/// device and no key in its keyring - the retry was refused ("encrypted already"), the recovery
/// code never shown: a paid drive nobody can open. Now the key is kept first and the same call
/// finishes the setup.
#[test]
fn a_setup_that_stopped_after_the_recovery_wrap_is_finished_by_the_same_call() {
    let bucket = MemBucket::new();
    let keyring = MemoryKeyring::new();
    let flaky = Flaky::new(&bucket, &keyring, 1);
    assert!(matches!(
        setup_new_drive(&flaky, &keyring, DRIVE, cheap()),
        Err(DriveError::Transport(_))
    ));
    let held = *flaky.key_held_at_recovery_put.lock().unwrap();
    assert_eq!(
        held,
        Some(true),
        "the recovery wrap went in once the device held the key"
    );
    assert!(
        bucket.object(RECOVERY_KEY_FILE).is_some(),
        "the earlier try's recovery wrap"
    );
    assert!(load_key_at(&keyring, &planned_key_entry(DRIVE))
        .unwrap()
        .is_some());
    assert_eq!(
        load_drive_key(&keyring, DRIVE).unwrap(),
        None,
        "not enrolled yet"
    );

    // Again: the same key, a new recovery code (the earlier one was never shown), this device
    // enrolled, the planned key gone.
    let before = bucket.object(RECOVERY_KEY_FILE).unwrap();
    let (key, code) = setup_new_drive(&flaky, &keyring, DRIVE, cheap()).unwrap();
    assert_ne!(
        bucket.object(RECOVERY_KEY_FILE).unwrap(),
        before,
        "replaced"
    );
    assert_eq!(load_drive_key(&keyring, DRIVE).unwrap(), Some(key.clone()));
    assert!(load_key_at(&keyring, &planned_key_entry(DRIVE))
        .unwrap()
        .is_none());
    assert_eq!(unlock(&bucket, &keyring, DRIVE).unwrap(), Some(key.clone()));
    assert_eq!(
        bucket.keys().len(),
        2,
        "this device's wrap and the recovery wrap"
    );
    assert_eq!(
        recover(&bucket, &MemoryKeyring::new(), DRIVE, &code).unwrap(),
        key,
        "the code returned is the one the wrap holds"
    );
    // And a finished setup is still refused.
    assert!(matches!(
        setup_new_drive(&bucket, &MemoryKeyring::new(), DRIVE, cheap()),
        Err(DriveError::InvalidConfig(_))
    ));
}

/// A keyring that takes no write: the user's "No", a locked keychain.
struct WritesRefused;

impl KeyringStore for WritesRefused {
    fn get(&self, _key: &str) -> Result<Option<String>, KeyringError> {
        Ok(None)
    }
    fn set(&self, _key: &str, _secret: &str) -> Result<(), KeyringError> {
        Err(KeyringError::Denied)
    }
    fn delete(&self, _key: &str) -> Result<(), KeyringError> {
        Ok(())
    }
}

/// K3: a device that cannot keep the key puts nothing into the bucket.
#[test]
fn a_keyring_that_takes_no_key_leaves_the_bucket_as_it_was() {
    let bucket = MemBucket::new();
    assert!(matches!(
        setup_new_drive(&bucket, &WritesRefused, DRIVE, cheap()),
        Err(DriveError::Denied { .. })
    ));
    assert!(bucket.keys().is_empty(), "{:?}", bucket.keys());
    assert!(!is_encrypted(&bucket).unwrap());
}

/// A key made for nothing - another device was first - does not stay behind as a planned key.
#[test]
fn the_loser_of_a_setup_race_keeps_no_planned_key() {
    let bucket = MemBucket::with_conditional_writes();
    setup_new_drive(&bucket, &MemoryKeyring::new(), DRIVE, cheap()).unwrap();
    let second = MemoryKeyring::new();
    assert!(matches!(
        setup_new_drive(&ListedTooEarly(&bucket), &second, DRIVE, cheap()),
        Err(DriveError::Conflict { .. })
    ));
    assert!(load_key_at(&second, &planned_key_entry(DRIVE))
        .unwrap()
        .is_none());
}

/// A planned key (a checkout's) meets a bucket another key set up: refused, the planned key kept.
#[test]
fn a_planned_key_meets_a_drive_that_another_key_set_up() {
    let bucket = MemBucket::new();
    let (other, _) = setup_new_drive(&bucket, &MemoryKeyring::new(), DRIVE, cheap()).unwrap();
    let keyring = MemoryKeyring::new();
    let planned = crate::crypto::DriveKey::generate().unwrap();
    assert_ne!(planned.id(), other.id());
    plan_drive_key(&keyring, DRIVE, &planned).unwrap();
    assert!(matches!(
        setup_new_drive(&bucket, &keyring, DRIVE, cheap()),
        Err(DriveError::KeyRefused { .. })
    ));
    assert_eq!(
        load_key_at(&keyring, &planned_key_entry(DRIVE)).unwrap(),
        Some(planned),
        "the planned key stays"
    );
    assert_eq!(load_drive_key(&keyring, DRIVE).unwrap(), None);
}

// ==== K19: the member key is made once ====

/// A keyring whose reads take a while - the window in which a second enrolment reads "no member
/// key" too - and that records what was held.
struct SlowReads {
    inner: MemoryKeyring,
    held: std::sync::Mutex<Vec<String>>,
}

impl SlowReads {
    fn new() -> SlowReads {
        SlowReads {
            inner: MemoryKeyring::new(),
            held: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn held(&self) -> Vec<String> {
        self.held.lock().unwrap().clone()
    }
}

impl KeyringStore for SlowReads {
    fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
        let answer = self.inner.get(key);
        std::thread::sleep(std::time::Duration::from_millis(80));
        answer
    }
    fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
        self.inner.set(key, secret)
    }
    fn delete(&self, key: &str) -> Result<(), KeyringError> {
        self.inner.delete(key)
    }
    /// The default hold of a store, recorded - and short, so a hold that nests fails the test
    /// instead of waiting out the default.
    fn hold(&self, key: &str) -> Result<crate::lock::HeldLock, KeyringError> {
        self.held.lock().unwrap().push(key.to_string());
        crate::lock::in_process(key, std::time::Duration::from_secs(3))
            .map_err(|e| KeyringError::Failed(e.to_string()))
    }
}

/// AZ-K19: two windows (or two apps) first-enrolling at once each read "no member key", each made
/// one, and the loser's wrap was sealed to a key no keyring held. The entry is held while the key is
/// made: the second one waits and finds the first's.
#[test]
fn az_k19_two_enrolments_at_once_share_one_member_secret() {
    use std::sync::{Arc, Barrier};

    let keyring = Arc::new(SlowReads::new());
    let barrier = Arc::new(Barrier::new(2));
    let enrol = |keyring: Arc<SlowReads>, barrier: Arc<Barrier>| {
        std::thread::spawn(move || {
            barrier.wait();
            crate::crypto::device::member_secret(keyring.as_ref(), DRIVE)
                .unwrap()
                .public()
                .id()
        })
    };
    let first = enrol(Arc::clone(&keyring), Arc::clone(&barrier));
    let second = enrol(Arc::clone(&keyring), Arc::clone(&barrier));
    let (first, second) = (first.join().unwrap(), second.join().unwrap());
    assert_eq!(first, second, "one member key for the drive, not one per window");
    assert_eq!(
        keyring.held(),
        vec![member_key_entry(DRIVE), member_key_entry(DRIVE)],
        "both took the member key's entry"
    );
}

/// AZ-K19: the pin and the claim code's commitment are read, decided and written back under the
/// same hold - once each, never inside another hold of the entry (that would wait for itself).
#[test]
fn az_k19_enrolling_and_committing_hold_the_member_key_once_each() {
    let keyring = SlowReads::new();
    let bucket = MemBucket::new();
    let key = crate::crypto::DriveKey::generate().unwrap();
    enroll(&bucket, &keyring, DRIVE, &key).unwrap();
    let held = keyring.held();
    assert!(held.len() >= 2, "the member key, then its pin: {held:?}");
    assert!(
        held.iter().all(|entry| *entry == member_key_entry(DRIVE)),
        "{held:?}"
    );

    let committed = SlowReads::new();
    let before = committed.held().len();
    crate::crypto::device::commit_drive_key(&committed, DRIVE, &key.id()).unwrap();
    assert_eq!(committed.held().len() - before, 1, "{:?}", committed.held());
}

// ==== X-AR5b-9: a joined device is not told it has not joined ====

/// A bucket that takes everything but a delete: that answers an error. With `lost` the delete went
/// through first and only its answer was lost.
struct DeleteRefused<'a> {
    bucket: &'a MemBucket,
    lost: bool,
}

impl<'a> DeleteRefused<'a> {
    fn new(bucket: &'a MemBucket) -> Self {
        DeleteRefused {
            bucket,
            lost: false,
        }
    }

    fn answer_lost(bucket: &'a MemBucket) -> Self {
        DeleteRefused { bucket, lost: true }
    }
}

impl Drive for DeleteRefused<'_> {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.bucket.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.bucket.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.bucket.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.bucket.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        if self.lost {
            self.bucket.delete(key)?;
        }
        Err(DriveError::Transport(String::from(
            "the connection was cut",
        )))
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.bucket.head(key)
    }
    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.bucket.put_if(key, bytes, condition)
    }
}

/// X-AR5b-9: the invite's wrap is deleted after the device is enrolled. A delete that failed used
/// to come back as an error although the device held the key - and left the one-time code live.
/// Now the join is done, the key is the caller's, and the live invite is said (`invite_left`); the
/// same code is then withdrawn by deleting the wrap again, and a code that was used cannot be used
/// twice.
#[test]
fn x_ar5b_9_a_joined_device_is_told_it_joined_and_that_the_invite_is_still_live() {
    let bucket = MemBucket::new();
    let first = MemoryKeyring::new();
    let (drive_key, _) = setup_new_drive(&bucket, &first, DRIVE, cheap()).unwrap();
    let invite = seal_invite(&bucket, DRIVE, &drive_key).unwrap();
    let invite_wraps = bucket.keys().len();

    let second = MemoryKeyring::new();
    let adopted = adopt_invite_reporting(
        &DeleteRefused::new(&bucket),
        &second,
        DRIVE,
        &invite,
        Some(&drive_key.id()),
    )
    .expect("the device joined");
    assert_eq!(adopted.key, drive_key);
    assert!(
        matches!(adopted.invite_left, Some(DriveError::Transport(_))),
        "{:?}",
        adopted.invite_left
    );
    assert_eq!(
        load_drive_key(&second, DRIVE).unwrap(),
        Some(drive_key.clone()),
        "the key is kept on the joined device"
    );
    assert_eq!(
        bucket.keys().len(),
        invite_wraps + 1,
        "the invite's wrap is still there"
    );

    // The simple form joins the same way; the live invite is the caller's to withdraw.
    let third = MemoryKeyring::new();
    assert_eq!(
        adopt_invite(&DeleteRefused::new(&bucket), &third, DRIVE, &invite, None).unwrap(),
        drive_key
    );

    // A delete that works: nothing left. The code opens the key once.
    let fourth = MemoryKeyring::new();
    let adopted = adopt_invite_reporting(&bucket, &fourth, DRIVE, &invite, None).unwrap();
    assert!(adopted.invite_left.is_none());
    assert!(matches!(
        adopt_invite_reporting(&bucket, &MemoryKeyring::new(), DRIVE, &invite, None),
        Err(DriveError::Denied { .. })
    ));
}

/// X-AR5b-9: a delete that worked and whose answer was lost is not a live invite: the join looks
/// again before it says so, and finds the wrap gone.
#[test]
fn x_ar5b_9_a_delete_whose_answer_was_lost_leaves_no_live_invite_to_report() {
    let bucket = MemBucket::new();
    let (drive_key, _) = setup_new_drive(&bucket, &MemoryKeyring::new(), DRIVE, cheap()).unwrap();
    let invite = seal_invite(&bucket, DRIVE, &drive_key).unwrap();
    let invite_wraps = bucket.keys().len();

    let second = MemoryKeyring::new();
    let adopted = adopt_invite_reporting(
        &DeleteRefused::answer_lost(&bucket),
        &second,
        DRIVE,
        &invite,
        Some(&drive_key.id()),
    )
    .expect("the device joined");
    assert!(adopted.invite_left.is_none(), "{:?}", adopted.invite_left);
    assert_eq!(
        bucket.keys().len(),
        invite_wraps,
        "the invite is gone, this device's wrap is new"
    );
    assert!(matches!(
        adopt_invite_reporting(&bucket, &MemoryKeyring::new(), DRIVE, &invite, None),
        Err(DriveError::Denied { .. })
    ));
}

// ==== X-DELTA-AZDRIVE-KEYS-3: the answer of a PUT is not what the bucket holds ====

/// A bucket with conditional writes whose faults the test sets, over a fresh `MemBucket`.
fn faulty_bucket() -> (Arc<MemBucket>, Arc<Faulty>) {
    let mem = Arc::new(MemBucket::with_conditional_writes());
    let faulty = Arc::new(Faulty::new(mem.clone()));
    (mem, faulty)
}

/// X-DELTA-AZDRIVE-KEYS-3: the first PUT of `recovery.key` lands and its answer is lost. The
/// setup deleted the only copy of the key it had made on ANY error of that PUT - while the wrap
/// sealed to it was in the bucket: a bucket that counts as encrypted, a recovery code never shown,
/// and no key to open it. Now the bucket is looked at: our wrap is there, the setup goes on.
#[test]
fn x_delta_azdrive_keys_3_a_wrap_that_landed_with_its_answer_lost_ends_the_setup() {
    let (mem, faulty) = faulty_bucket();
    faulty.fail_write(RECOVERY_KEY_FILE, Fail::AnswerLost, 1);
    let keyring = MemoryKeyring::new();
    let (key, code) = setup_new_drive(faulty.as_ref(), &keyring, DRIVE, cheap())
        .expect("the wrap is in the bucket: the setup is finished");
    assert_eq!(
        recover(mem.as_ref(), &MemoryKeyring::new(), DRIVE, &code).unwrap(),
        key,
        "the code returned is the one the wrap holds"
    );
    assert_eq!(unlock(mem.as_ref(), &keyring, DRIVE).unwrap(), Some(key));
    assert!(load_key_at(&keyring, &planned_key_entry(DRIVE))
        .unwrap()
        .is_none());
}

/// X-DELTA-AZDRIVE-KEYS-3: the same, but the resend of the conditional PUT is refused as "the
/// object is there" (412, `Conflict`): the answer a landed write gets the second time.
#[test]
fn x_delta_azdrive_keys_3_a_wrap_whose_resend_is_refused_as_there_already_ends_the_setup() {
    let (mem, faulty) = faulty_bucket();
    faulty.fail_write(RECOVERY_KEY_FILE, Fail::AnswerConflict, 1);
    let keyring = MemoryKeyring::new();
    let (key, code) = setup_new_drive(faulty.as_ref(), &keyring, DRIVE, cheap())
        .expect("the 412 is our own write: the setup is finished");
    assert_eq!(
        recover(mem.as_ref(), &MemoryKeyring::new(), DRIVE, &code).unwrap(),
        key
    );
    assert_eq!(load_drive_key(&keyring, DRIVE).unwrap(), Some(key));
}

/// X-DELTA-AZDRIVE-KEYS-3: a PUT that did not land (nothing in the bucket) keeps the key it made:
/// the next call finds it as the planned key and finishes with it.
#[test]
fn x_delta_azdrive_keys_3_a_wrap_that_never_landed_keeps_the_planned_key() {
    let (mem, faulty) = faulty_bucket();
    faulty.fail_write(RECOVERY_KEY_FILE, Fail::Refused, 1);
    let keyring = MemoryKeyring::new();
    assert!(matches!(
        setup_new_drive(faulty.as_ref(), &keyring, DRIVE, cheap()),
        Err(DriveError::Transport(_))
    ));
    let planned = load_key_at(&keyring, &planned_key_entry(DRIVE))
        .unwrap()
        .expect("the key made now stays: the bucket holds no wrap that says it is for nothing");
    let (key, code) = setup_new_drive(faulty.as_ref(), &keyring, DRIVE, cheap()).unwrap();
    assert_eq!(key, planned, "the same key");
    assert_eq!(
        recover(mem.as_ref(), &MemoryKeyring::new(), DRIVE, &code).unwrap(),
        key
    );
}

/// X-DELTA-AZDRIVE-KEYS-3: a bucket that cannot be read back after a failed PUT does not say
/// whether the key seals anything: it stays.
#[test]
fn x_delta_azdrive_keys_3_a_bucket_that_cannot_be_read_back_keeps_the_planned_key() {
    let (_mem, faulty) = faulty_bucket();
    faulty.fail_write(RECOVERY_KEY_FILE, Fail::AnswerLost, 1);
    faulty.fail_read(RECOVERY_KEY_FILE, 1);
    let keyring = MemoryKeyring::new();
    assert!(setup_new_drive(faulty.as_ref(), &keyring, DRIVE, cheap()).is_err());
    assert!(
        load_key_at(&keyring, &planned_key_entry(DRIVE))
            .unwrap()
            .is_some(),
        "the wrap may be in the bucket: the key is the only way into what it seals"
    );
    // The next call: the wrap of the earlier try is ours (the key id), it is replaced, done.
    let (key, code) = setup_new_drive(faulty.as_ref(), &keyring, DRIVE, cheap()).unwrap();
    assert_eq!(
        recover(faulty.as_ref(), &MemoryKeyring::new(), DRIVE, &code).unwrap(),
        key
    );
}

// ==== X-DELTA-AZDRIVE-KEYS-41: a key file over the limit, on a bucket that refuses it on its way ====

/// What an S3 bucket does with `get_limited`: an object over the limit is refused as a transport
/// error (its text names the limit, it is not for sorting), where `MemBucket` answers `Protocol`.
struct Strict<'a>(&'a MemBucket);

impl Drive for Strict<'_> {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.0.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.0.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.0.get_range(key, range)
    }
    fn get_limited(&self, key: &str, limit: crate::limits::Limit) -> Result<Vec<u8>, DriveError> {
        if self.0.head(key)?.size > limit.max {
            return Err(DriveError::Transport(String::from(
                "ResponseTooLarge { limit: .. }",
            )));
        }
        self.0.get(key)
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
}

/// X-DELTA-AZDRIVE-KEYS-41: a writer puts 65 KiB at the first code's file and among the further
/// codes' files. On S3 the recovery by a second kit stopped at the first one (a transport error is
/// not "no wrap"); now an over-limit file is no wrap on any kind of drive, and the kit opens.
#[test]
fn x_delta_azdrive_keys_41_an_oversize_key_file_on_a_refusing_bucket_stops_no_recovery() {
    let mem = MemBucket::new();
    let keyring = MemoryKeyring::new();
    let (key, _first_code) = setup_new_drive(&mem, &keyring, DRIVE, cheap()).unwrap();
    let (_file, second_code) = add_recovery_code(&mem, &keyring, DRIVE, cheap()).unwrap();
    mem.set(RECOVERY_KEY_FILE, vec![b' '; 65 * 1024]);
    mem.set(
        ".azlin/keys/recovery-0000000000000001.key",
        vec![b' '; 65 * 1024],
    );
    let recovered = recover(&Strict(&mem), &MemoryKeyring::new(), DRIVE, &second_code)
        .expect("the second kit opens the drive: the junk files are no wraps");
    assert_eq!(recovered, key);
}

/// X-DELTA-AZDRIVE-KEYS-41: a further code's wrap of a cost this build does not derive with is
/// left out, and said when no wrap opens: the code may be right.
#[test]
fn x_delta_azdrive_keys_41_a_wrap_of_a_cost_this_build_cannot_try_is_said_not_called_a_wrong_code()
{
    let mem = MemBucket::new();
    let keyring = MemoryKeyring::new();
    setup_new_drive(&mem, &keyring, DRIVE, cheap()).unwrap();
    let (file, code) = add_recovery_code(&mem, &keyring, DRIVE, cheap()).unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&mem.object(&file).unwrap()).unwrap();
    json["kdf"]["memory_kib"] = serde_json::json!(4_000_000_000u64);
    mem.set(&file, serde_json::to_vec(&json).unwrap());
    // The first code's wrap is there and is another code's: this code opens nothing, and one of
    // the wraps it could have opened is one this build cannot derive.
    let err = recover(&mem, &MemoryKeyring::new(), DRIVE, &code).unwrap_err();
    assert!(
        matches!(err, DriveError::Unsupported(_)),
        "an unsupported cost is said, the code is not called wrong: {err:?}"
    );
}

/// X-DELTA-AZDRIVE-KEYS-22: the member files of a drive are listed within the page cap - a storage
/// that lists member keys for ever used to keep `load_member_wraps` (and the opening that asked for
/// it) reading and growing for ever; it is refused as an unending listing now.
#[test]
fn x_delta_azdrive_keys_22_the_member_files_of_a_listing_that_never_ends_are_refused() {
    let endless =
        super::ops::Endless::new(".azlin/keys/", |n| format!(".azlin/keys/m{n}.key"), false);
    let refused = crate::crypto::keys::load_member_wraps(&endless).unwrap_err();
    assert!(
        matches!(&refused, DriveError::Protocol(why) if why.contains("does not end")),
        "{refused:?}"
    );
    assert_eq!(
        endless.asked() as usize,
        crate::limits::named::LISTING_PAGES
    );
}

/// X-DELTA-AZDRIVE-KEYS-22: the count of a drive's other devices lists `.azlin/keys/` within the
/// page cap, as the member files do - an unending listing is an error, not a count that never ends.
#[test]
fn x_delta_azdrive_keys_22_the_other_devices_of_a_listing_that_never_ends_are_not_counted() {
    let endless =
        super::ops::Endless::new(".azlin/keys/", |n| format!(".azlin/keys/m{n}.key"), false);
    let refused =
        crate::crypto::device::other_devices(&endless, &MemoryKeyring::new(), DRIVE).unwrap_err();
    assert!(
        matches!(&refused, DriveError::Protocol(why) if why.contains("does not end")),
        "{refused:?}"
    );
    assert_eq!(
        endless.asked() as usize,
        crate::limits::named::LISTING_PAGES
    );
}

// ==== X-DELTA-KEYS2-3: nothing after the enrolment of a new drive is an error ====

/// A keyring that refuses to delete one entry (a prompt answered No).
struct RefusesToDelete {
    inner: MemoryKeyring,
    entry: String,
}

impl KeyringStore for RefusesToDelete {
    fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
        self.inner.get(key)
    }
    fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
        self.inner.set(key, secret)
    }
    fn delete(&self, key: &str) -> Result<(), KeyringError> {
        if key == self.entry {
            return Err(KeyringError::Denied);
        }
        self.inner.delete(key)
    }
}

/// X-DELTA-KEYS2-3: `setup_new_drive` ended with an error when the keyring refused to delete the
/// planted key - AFTER the recovery wrap was in the bucket and the drive key in the keyring - and
/// the code was dropped: the drive was encrypted, its only recovery wrap opened with a code nobody
/// saw. Now the code comes back and the entry is named in `left_over`.
#[test]
fn x_delta_keys2_3_a_keyring_that_refuses_the_planted_keys_delete_still_returns_the_code() {
    let bucket = MemBucket::with_conditional_writes();
    let planned = planned_key_entry(DRIVE);
    let keyring = RefusesToDelete {
        inner: MemoryKeyring::new(),
        entry: planned.clone(),
    };
    let made = setup_new_drive_reporting(&bucket, &keyring, DRIVE, cheap())
        .expect("the drive is set up and its code is its recovery code: it comes back");
    assert!(
        made.left_over.iter().any(|left| left.contains(&planned)),
        "the entry that stays is named: {:?}",
        made.left_over
    );
    assert!(
        !format!("{made:?}").contains(made.code.to_text().as_str()),
        "the code is not in Debug"
    );

    // The state the old code ended its error in is a drive that works: the drive key is the
    // keyring's, the code opens the wrap, and the planted copy of the key is what is left.
    assert_eq!(
        load_drive_key(&keyring, DRIVE).unwrap(),
        Some(made.drive_key.clone())
    );
    let other = MemoryKeyring::new();
    let recovered = recover(&bucket, &other, DRIVE, &made.code).expect("the code opens the drive");
    assert_eq!(recovered.id(), made.drive_key.id());
    assert_eq!(
        load_key_at(&keyring, &planned).unwrap().map(|k| k.id()),
        Some(made.drive_key.id()),
        "the planted copy stays: it is a copy of the drive key"
    );
    assert!(
        setup_pending(&keyring, DRIVE).unwrap(),
        "and says a setup left it"
    );
}

/// X-DELTA-KEYS2-3: the same through the call every other app makes.
#[test]
fn x_delta_keys2_3_setup_new_drive_is_not_an_error_after_the_enrolment() {
    let bucket = MemBucket::with_conditional_writes();
    let keyring = RefusesToDelete {
        inner: MemoryKeyring::new(),
        entry: planned_key_entry(DRIVE),
    };
    let (drive_key, code) = setup_new_drive(&bucket, &keyring, DRIVE, cheap())
        .expect("an error after the enrolment would drop the code");
    assert_eq!(
        recover(&bucket, &MemoryKeyring::new(), DRIVE, &code)
            .unwrap()
            .id(),
        drive_key.id()
    );
}

/// X-DELTA-KEYS2-3: a keyring that takes everything leaves nothing.
#[test]
fn x_delta_keys2_3_a_setup_that_ends_cleanly_reports_nothing_left() {
    let bucket = MemBucket::with_conditional_writes();
    let keyring = MemoryKeyring::new();
    let made = setup_new_drive_reporting(&bucket, &keyring, DRIVE, cheap()).unwrap();
    assert!(made.left_over.is_empty(), "{:?}", made.left_over);
    assert!(!setup_pending(&keyring, DRIVE).unwrap());
}

// ==== X-KEYSTATE-1: the setup reads the drive key ====

/// The drive key of a device, and a bucket that lists no key file of it any more (a bucket writer
/// deleted them, or the drive was reset): a setup run here made a SECOND key, published its recovery
/// wrap and let `enroll` keep it in the first one's place - everything sealed under the first (the
/// index, the files) opened with nothing, and the key files that could have said so were the ones
/// deleted. `azcloud encrypt --yes` reaches this flow through the kit's `setup_encryption`. The setup
/// reads the drive key now, asks the key permit's Setup cell, and stops.
#[test]
fn x_keystate_1_a_setup_on_a_device_that_holds_the_drive_key_makes_no_second_key() {
    let bucket = MemBucket::new();
    let keyring = MemoryKeyring::new();
    let (first, _code) = setup_new_drive(&bucket, &keyring, DRIVE, cheap()).unwrap();
    for key in bucket.keys() {
        bucket.delete(&key).unwrap();
    }
    assert!(!is_encrypted(&bucket).unwrap(), "no key file is left");

    let refused = setup_new_drive(&bucket, &keyring, DRIVE, cheap());
    match &refused {
        Err(DriveError::InvalidConfig(said)) => {
            assert_eq!(said, &Refusal::EncryptedAlready.sentence(DRIVE));
        }
        other => panic!("the setup ran on a drive whose key this device holds: {other:?}"),
    }
    assert_eq!(
        load_drive_key(&keyring, DRIVE).unwrap(),
        Some(first),
        "the first key is still the drive key"
    );
    assert!(
        bucket.keys().is_empty(),
        "no recovery key, no member key was published: {:?}",
        bucket.keys()
    );
    assert!(
        load_key_at(&keyring, &planned_key_entry(DRIVE))
            .unwrap()
            .is_none(),
        "no second key was kept for the setup"
    );
}

/// X-DELTA-STRUCT-KEYSTATE-5 at the flow: a planned key of ANOTHER id beside the drive key is no copy
/// of it. The setup that finishes "the same call" would have replaced the drive key with it.
#[test]
fn x_delta_struct_keystate_5_a_setup_does_not_finish_with_a_planned_key_that_is_not_the_drive_key()
{
    let bucket = MemBucket::new();
    let keyring = MemoryKeyring::new();
    let (first, _code) = setup_new_drive(&bucket, &keyring, DRIVE, cheap()).unwrap();
    for key in bucket.keys() {
        bucket.delete(&key).unwrap();
    }
    let planned = DriveKey::generate().unwrap();
    plan_drive_key(&keyring, DRIVE, &planned).unwrap();

    assert!(matches!(
        setup_new_drive(&bucket, &keyring, DRIVE, cheap()),
        Err(DriveError::InvalidConfig(_))
    ));
    assert_eq!(load_drive_key(&keyring, DRIVE).unwrap(), Some(first));
    assert_eq!(
        load_key_at(&keyring, &planned_key_entry(DRIVE)).unwrap(),
        Some(planned),
        "the planned key stays as it was"
    );
    assert!(bucket.keys().is_empty(), "{:?}", bucket.keys());
}

/// A drive key entry the setup could not read (it holds no key) is not "no key": the setup stops with
/// the keyring's own error. It used to make a key and overwrite the entry.
#[test]
fn x_keystate_1_a_drive_key_entry_that_cannot_be_read_is_not_replaced_by_a_setup() {
    let bucket = MemBucket::new();
    let keyring = MemoryKeyring::new();
    keyring.set(&drive_key_entry(DRIVE), "not a key").unwrap();

    assert!(matches!(
        setup_new_drive(&bucket, &keyring, DRIVE, cheap()),
        Err(DriveError::InvalidConfig(_))
    ));
    assert_eq!(
        keyring.get(&drive_key_entry(DRIVE)).unwrap().as_deref(),
        Some("not a key"),
        "the entry was left as it was"
    );
    assert!(bucket.keys().is_empty(), "{:?}", bucket.keys());
    assert!(load_key_at(&keyring, &planned_key_entry(DRIVE))
        .unwrap()
        .is_none());
}
