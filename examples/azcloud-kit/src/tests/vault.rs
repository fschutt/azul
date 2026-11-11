//! A drive's period tokens are kept with the drive itself, encrypted with the drive key: a
//! second device of the drive redeems the months the first one kept, and the operator, who holds
//! the bucket, reads none of them.

use std::sync::Arc;

use azul_storage::{
    crypto::{
        azl1::{PublicHeader, HEADER_LEN},
        keys::{store_key_chain, KeyChain},
        DriveKey,
    },
    meta::MemoryBucket,
    testing::TempDir,
    ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition,
};

use super::{
    period::{keyring_with_session, redeeming_server, stored, DAY, ISSUE_KEY, NOW},
    Shared, TOKEN,
};
use crate::{
    period::{redeem_due, PeriodTokenStore, Redeemed},
    vault::{DriveVault, TokenVault, UnlockedDrive, VaultGrant, VaultState},
    TokenServer,
};

#[test]
fn a_second_device_redeems_the_months_the_first_one_kept_in_the_drive() {
    let bucket: Arc<dyn Drive> = Arc::new(MemoryBucket::new());
    let key = DriveKey::generate().unwrap();
    let drive = UnlockedDrive::new("d_1", bucket.clone(), key);
    let laptop = TempDir::new("azcloud-vault-laptop");
    let kept = PeriodTokenStore::new(laptop.path().join("period-tokens"))
        .for_drive(&drive)
        .unwrap();
    kept.add_tokens(&[stored("aa"), stored("bb")]).unwrap();
    kept.keep_grant(VaultGrant::new("ck_1", "100GB", 3, ISSUE_KEY)).unwrap();
    // The laptop is gone; the phone joined the drive (it holds the key) and looks.
    let phone = TempDir::new("azcloud-vault-phone");
    let tokens = PeriodTokenStore::new(phone.path().join("period-tokens"))
        .for_drive(&drive)
        .unwrap();
    let shared = keyring_with_session(&phone, "dt_f.3.newest");
    let fake = redeeming_server(shared.locks().clone(), "dt_f.3.newest", NOW + 2 * DAY, |_| None);
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match redeem_due(&server, &shared, &tokens, NOW) {
        Redeemed::Extended { count, .. } => assert_eq!(count, 1),
        other => panic!("the phone bought no month: {other:?}"),
    }
    // Every device sees the first token spent and the second one waiting.
    let left: Vec<String> = kept.unspent("100GB").unwrap().into_iter().map(|t| t.nonce).collect();
    assert_eq!(left, vec!["bb".repeat(32)]);
    assert!(tokens.state().unwrap().grant("ck_1").is_some(), "the grant is the drive's too");
    // The bucket holds the vault sealed: no token, no issue key, no checkout in the clear.
    let raw = String::from_utf8_lossy(&bucket.get(DriveVault::KEY).unwrap()).to_string();
    for secret in ["bb".repeat(32), ISSUE_KEY.to_string(), String::from("ck_1")] {
        assert!(!raw.contains(&secret), "the vault shows {secret}");
    }
    let other = UnlockedDrive::new("d_1", bucket, DriveKey::generate().unwrap());
    assert!(DriveVault::new(&other).read().is_err(), "another key opens nothing");
}

#[test]
fn tokens_this_computer_kept_alone_move_into_the_drive_at_its_first_look() {
    let bucket: Arc<dyn Drive> = Arc::new(MemoryBucket::new());
    let drive = UnlockedDrive::new("d_1", bucket, DriveKey::generate().unwrap());
    let laptop = TempDir::new("azcloud-vault-laptop");
    let store = PeriodTokenStore::new(laptop.path().join("period-tokens"));
    // what an older version wrote
    std::fs::create_dir_all(store.dir()).unwrap();
    let old = serde_json::json!({"format": 1, "tokens": [stored("aa")]});
    std::fs::write(store.path_of("d_1").unwrap(), old.to_string()).unwrap();
    assert_eq!(store.for_drive(&drive).unwrap().state().unwrap().tokens.len(), 1);
    let phone = TempDir::new("azcloud-vault-phone");
    let seen = PeriodTokenStore::new(phone.path().join("period-tokens"))
        .for_drive(&drive)
        .unwrap()
        .unspent("100GB")
        .unwrap();
    assert_eq!(seen, vec![stored("aa")], "in the drive now");
    assert_eq!(store.tokens("d_1").unwrap(), vec![stored("aa")], "and still here");
}

/// AZ-C15: the drive's vault is one object id for every version, so the storage can put an older
/// valid version back. The laptop that wrote the newer ones still holds them: its next look joins
/// the two and writes the union back, so the replay is overwritten, the spent token stays spent
/// and nothing the newer versions held is lost; a device that comes after the heal reads the
/// newer state.
#[test]
fn az_c15_an_older_vault_the_storage_puts_back_is_healed_from_this_computers_copy() {
    let bucket: Arc<dyn Drive> = Arc::new(MemoryBucket::new());
    let drive = UnlockedDrive::new("d_1", bucket.clone(), DriveKey::generate().unwrap());
    let laptop = TempDir::new("azcloud-vault-laptop");
    let kept = PeriodTokenStore::new(laptop.path().join("period-tokens"))
        .for_drive(&drive)
        .unwrap();
    kept.add_tokens(&[stored("aa")]).unwrap();
    let first = bucket.get(DriveVault::KEY).unwrap();
    kept.add_tokens(&[stored("bb")]).unwrap();
    kept.spend(&"aa".repeat(32)).unwrap();
    kept.keep_grant(VaultGrant::new("ck_1", "100GB", 3, ISSUE_KEY))
        .unwrap();
    assert_ne!(
        bucket.get(DriveVault::KEY).unwrap(),
        first,
        "newer versions were written"
    );
    // The storage puts the first version back: valid (the same key, the same id), so it opens.
    bucket.put(DriveVault::KEY, &first).unwrap();
    let replayed = DriveVault::new(&drive).read().unwrap();
    assert_eq!(replayed.tokens, vec![stored("aa")], "the old version opens");
    assert!(replayed.grant("ck_1").is_none());
    // The laptop looks: the spent token stays spent, the grant is still there ...
    let state = kept.state().unwrap();
    assert_eq!(state.tokens, vec![stored("bb")], "aa stays spent");
    assert!(state.grant("ck_1").is_some());
    // ... and the drive's vault holds the union again.
    let healed = DriveVault::new(&drive).read().unwrap();
    assert_eq!(healed.tokens, vec![stored("bb")]);
    assert!(healed.spent.contains(&"aa".repeat(32)));
    assert!(healed.grant("ck_1").is_some());
    let phone = TempDir::new("azcloud-vault-phone");
    let seen = PeriodTokenStore::new(phone.path().join("period-tokens"))
        .for_drive(&drive)
        .unwrap();
    assert_eq!(seen.unspent("100GB").unwrap(), vec![stored("bb")]);
}

// ==== X-DELTA-AZDRIVE-KEYS-5: the vault and a rotation of the drive's key ====

/// X-DELTA-AZDRIVE-KEYS-5: a rotation re-wraps the index and the member files; the period vault
/// stayed under the key it was written with and `DriveVault::open` used the current key only, so
/// after K1 -> K2 it answered "does not open" for ever (and `join` fetched first: it could not
/// even overwrite it). The key chain - K1 sealed under K2 - holds the key that wrapped it.
#[test]
fn x_delta_azdrive_keys_5_a_vault_sealed_before_a_rotation_opens_through_the_key_chain() {
    let bucket: Arc<dyn Drive> = Arc::new(MemoryBucket::new());
    let k1 = DriveKey::generate().unwrap();
    let before = UnlockedDrive::new("d_1", bucket.clone(), k1.clone());
    let laptop = TempDir::new("azcloud-vault-laptop");
    let kept = PeriodTokenStore::new(laptop.path().join("period-tokens"))
        .for_drive(&before)
        .unwrap();
    kept.add_tokens(&[stored("aa")]).unwrap();
    kept.keep_grant(VaultGrant::new("ck_1", "100GB", 3, ISSUE_KEY))
        .unwrap();

    // The drive's key is rotated: K2, and K1 sealed under it in the key chain.
    let k2 = DriveKey::generate().unwrap();
    let mut chain = KeyChain::new();
    chain.add("d_1", &k1, &k2).unwrap();
    store_key_chain(bucket.as_ref(), &chain).unwrap();
    let after = UnlockedDrive::new("d_1", bucket.clone(), k2.clone());
    let key_id_of_the_object = || {
        let bytes = bucket.get(DriveVault::KEY).unwrap();
        PublicHeader::parse(&bytes[..HEADER_LEN])
            .unwrap()
            .wrapped_key
            .key_id
    };
    assert_eq!(key_id_of_the_object(), k1.id());

    // A device that joined again holds K2 only: it reads the vault, every part of it.
    let phone = TempDir::new("azcloud-vault-phone");
    let seen = PeriodTokenStore::new(phone.path().join("period-tokens"))
        .for_drive(&after)
        .unwrap();
    let state = seen
        .state_of_all()
        .expect("the vault opens through the key chain, in every place");
    assert_eq!(state.tokens, vec![stored("aa")]);
    assert!(
        state.grant("ck_1").is_some(),
        "the grant that only the drive held"
    );

    // The next write puts it under the current key, even when it has nothing new to add.
    DriveVault::new(&after)
        .join(&VaultState::default())
        .unwrap();
    assert_eq!(
        key_id_of_the_object(),
        k2.id(),
        "written again under the current key"
    );
    assert_eq!(
        DriveVault::new(&after).read().unwrap().tokens,
        vec![stored("aa")]
    );

    // A key the chain does not hold opens nothing.
    let stranger = UnlockedDrive::new("d_1", bucket, DriveKey::generate().unwrap());
    assert!(DriveVault::new(&stranger).read().is_err());
}

// ==== X-DELTA-AZDRIVE-KEYS-8: this computer's vault may go with the drive only when the drive holds it ====

/// A drive that does not take a write of one key (no answer, a token that ran out).
struct RefusesPuts {
    inner: Arc<dyn Drive>,
    key: &'static str,
}

impl Drive for RefusesPuts {
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
        if key == self.key {
            return Err(DriveError::Transport(String::from("no answer")));
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
        if key == self.key {
            return Err(DriveError::Transport(String::from("no answer")));
        }
        self.inner.put_if(key, bytes, condition)
    }
}

/// X-DELTA-AZDRIVE-KEYS-8: "Remove drive" erased the local vault on the strength of a recovery
/// code's check; a prepaid month whose write into the drive had failed was only here. The vault
/// is redundant when the drive's own vault holds all of it (it is written once more, then read),
/// and not when the drive has none, or does not take the write.
#[test]
fn x_delta_azdrive_keys_8_this_computers_vault_is_redundant_only_when_the_drive_holds_it() {
    let mem: Arc<dyn Drive> = Arc::new(MemoryBucket::new());
    let key = DriveKey::generate().unwrap();
    let laptop = TempDir::new("azcloud-vault-laptop");
    let store = PeriodTokenStore::new(laptop.path().join("period-tokens"));

    // This computer alone (the drive's key is not here): nothing to lose while it holds nothing.
    let alone = store.on_this_device("d_1").unwrap();
    assert!(
        alone.local_is_redundant().unwrap(),
        "nothing kept: nothing to lose"
    );
    alone.add_tokens(&[stored("aa")]).unwrap();
    assert!(
        !alone.local_is_redundant().unwrap(),
        "the months are only on this computer"
    );

    // The drive has a vault and takes the write: it holds them now.
    let unlocked = UnlockedDrive::new("d_1", mem.clone(), key.clone());
    let with_drive = store.for_drive(&unlocked).unwrap();
    assert!(with_drive.local_is_redundant().unwrap());
    assert_eq!(
        DriveVault::new(&unlocked).read().unwrap().tokens,
        vec![stored("aa")]
    );

    // The drive's vault does not take the write: the months are only here. (A bucket of its own:
    // a drive's vault object is bound to its drive id, so the vault of "d_1" in the bucket above
    // does not open for "d_2" - see the end of this test.)
    let other = TempDir::new("azcloud-vault-other");
    let store = PeriodTokenStore::new(other.path().join("period-tokens"));
    store
        .on_this_device("d_2")
        .unwrap()
        .add_tokens(&[stored("bb")])
        .unwrap();
    let refusing: Arc<dyn Drive> = Arc::new(RefusesPuts {
        inner: Arc::new(MemoryBucket::new()),
        key: DriveVault::KEY,
    });
    let unlocked = UnlockedDrive::new("d_2", refusing, key.clone());
    let tokens = store.for_drive(&unlocked).unwrap();
    assert!(
        !tokens.local_is_redundant().unwrap(),
        "the drive's vault lacks them"
    );

    // A vault that does not open for this drive - another drive's, found in its bucket - is not
    // a vault that lacks the months: whether they are kept elsewhere is not known, and not
    // knowing is no answer (`local_is_redundant`), so a removal keeps the keys.
    let unlocked = UnlockedDrive::new("d_2", mem, key);
    let tokens = store.for_drive(&unlocked).unwrap();
    assert!(
        tokens.local_is_redundant().is_err(),
        "d_1's vault in d_2's bucket is refused, not read as empty"
    );
}
