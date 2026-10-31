//! A drive's period tokens are kept with the drive itself, encrypted with the drive key: a
//! second device of the drive redeems the months the first one kept, and the operator, who holds
//! the bucket, reads none of them.

use std::sync::Arc;

use azul_storage::{crypto::DriveKey, meta::MemoryBucket, testing::TempDir, Drive};

use super::{
    period::{keyring_with_session, redeeming_server, stored, DAY, ISSUE_KEY, NOW},
    Shared, TOKEN,
};
use crate::{
    period::{redeem_due, PeriodTokenStore, Redeemed},
    vault::{DriveVault, TokenVault, UnlockedDrive, VaultGrant},
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
