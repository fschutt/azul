//! An encrypted Azlin drive across devices (feature `encryption`): the first device turns
//! encryption on, its join code carries the drive key sealed for the second device (once), both
//! read and write the same files, the recovery code opens the key on a third.

use std::sync::Arc;

use azul_storage::{
    crypto::{device, keys::RecoveryKdf, DriveKey},
    encrypted::{IndexProvider, MemoryIndex, NameIndex},
    keyring::{KeyringStore, MemoryKeyring},
    testing::TempDir,
    Drive, DriveError, Transport,
};

use super::{bundle, fake_s3::FakeS3, header, json, Fake, Shared, TOKEN};
use crate::{
    account::{Account, JoinCode},
    drive::TransportFactory,
    error::CloudError,
    state::StateDir,
};

/// A tiny Argon2id cost: the tests run in debug builds too.
fn cheap() -> RecoveryKdf {
    RecoveryKdf::with_cost(64, 1, 1).unwrap()
}

/// The token server and the bucket on one transport: a sign-up answers drive `d_1` (bucket
/// `d-1` at the tests' S3 endpoint), a member family's token is exchanged once, every other
/// call goes to `s3`.
fn cloud(s3: &Arc<FakeS3>) -> TransportFactory {
    let s3 = s3.clone();
    let fake = Fake::new(move |call, _| {
        let url = call.url.as_str();
        if !url.starts_with(TOKEN) {
            return Ok(s3.answer(call));
        }
        if url.ends_with("/v1/drives") {
            return Ok(json(
                201,
                &bundle("AKID1", "2099-01-01T00:00:00Z", "dt_f.0.a"),
            ));
        }
        if url.ends_with("/members") {
            return Ok(json(
                201,
                r#"{"member": "m_laptop", "drive_token": "dt_m.0.joins"}"#,
            ));
        }
        if url.ends_with("/credentials") && header(call, "authorization") == Some("Bearer dt_m.0.joins")
        {
            return Ok(json(
                200,
                &bundle("AKID2", "2099-01-01T00:00:00Z", "dt_m.1.x"),
            ));
        }
        Ok(json(401, r#"{"error": "token_reuse", "message": "reused"}"#))
    });
    Arc::new(move || Box::new(Shared(fake.clone())) as Box<dyn Transport>)
}

/// One index in memory both devices share: the stand-in for the drive's encrypted metadata
/// repository.
struct SharedIndex(Arc<MemoryIndex>);

impl IndexProvider for SharedIndex {
    fn open_index(
        &self,
        _drive: &str,
        _bucket: Arc<dyn Drive>,
        _drive_key: &DriveKey,
    ) -> Result<Arc<dyn NameIndex>, DriveError> {
        Ok(self.0.clone())
    }
}

fn signed_up(transports: &TransportFactory, dir: &TempDir) -> Account {
    let state = StateDir::open(dir.path()).unwrap();
    Account::signup(&state, TOKEN, transports.clone(), "100GB", "Ann's drive").unwrap()
}

#[test]
fn a_second_device_gets_the_drive_key_through_its_join_code_once() {
    let s3 = FakeS3::new();
    let transports = cloud(&s3);
    let a_dir = TempDir::new("azcloud-enc-a");
    let a = signed_up(&transports, &a_dir);
    let a_keys = MemoryKeyring::new();
    let recovery = a.setup_encryption(&a_keys, cheap()).unwrap();
    assert!(s3.keys().iter().any(|k| k == ".azlin/keys/recovery.key"));

    let join = a.invite_with_key(None, &a_keys).unwrap();
    assert_eq!(join.key_seal.as_deref().map(str::len), Some(64));
    let text = join.encode();

    // The second device: the code's token joins, the code's seal opens the key.
    let b_dir = TempDir::new("azcloud-enc-b");
    let b_state = StateDir::open(b_dir.path()).unwrap();
    let decoded = JoinCode::decode(&text).unwrap();
    let b = Account::join(&b_state, TOKEN, transports.clone(), &decoded).unwrap();
    let b_keys = MemoryKeyring::new();
    let key = b
        .adopt_join_key(&decoded, &b_keys)
        .unwrap()
        .expect("the code carries the key");
    assert_eq!(Some(key.clone()), device::load_drive_key(&a_keys, "d_1").unwrap());
    assert!(
        !s3.keys().iter().any(|k| k.contains("invite-")),
        "the one-time wrap is gone: {:?}",
        s3.keys()
    );
    assert!(matches!(
        a.adopt_join_key(&decoded, &MemoryKeyring::new()),
        Err(CloudError::Drive(DriveError::Denied { .. }))
    ));

    // Both devices read and write the same files.
    let provider = SharedIndex(Arc::new(MemoryIndex::new()));
    let on_a = a.open_encrypted(&a_keys, &provider).unwrap();
    on_a.put("notes/plan.txt", b"the plan").unwrap();
    let on_b = b.open_encrypted(&b_keys, &provider).unwrap();
    assert_eq!(on_b.get("notes/plan.txt").unwrap(), b"the plan");
    assert!(s3.keys().iter().all(|k| !k.contains("plan")), "{:?}", s3.keys());

    // The recovery code opens the key on a third device; a device with nothing is refused.
    let c_keys = MemoryKeyring::new();
    assert_eq!(b.recover_key(&recovery, &c_keys).unwrap(), key);
    assert!(matches!(
        b.open_encrypted(&MemoryKeyring::new(), &provider),
        Err(CloudError::Drive(DriveError::Denied { .. }))
    ));
}

#[test]
fn a_join_code_of_a_drive_without_encryption_carries_no_key() {
    let s3 = FakeS3::new();
    let transports = cloud(&s3);
    let dir = TempDir::new("azcloud-enc-plain");
    let a = signed_up(&transports, &dir);
    let join = a.invite_with_key(None, &MemoryKeyring::new()).unwrap();
    assert_eq!(join.key_seal, None);
    assert!(a
        .adopt_join_key(&join, &MemoryKeyring::new())
        .unwrap()
        .is_none());
    let mut damaged = join;
    damaged.key_seal = Some(String::from("not hex"));
    assert!(matches!(
        a.adopt_join_key(&damaged, &MemoryKeyring::new()),
        Err(CloudError::Failed(_))
    ));
    assert!(s3.keys().is_empty(), "nothing was written: {:?}", s3.keys());
}

#[test]
fn encryption_is_turned_on_once() {
    let s3 = FakeS3::new();
    let transports = cloud(&s3);
    let dir = TempDir::new("azcloud-enc-twice");
    let a = signed_up(&transports, &dir);
    a.setup_encryption(&MemoryKeyring::new(), cheap()).unwrap();
    assert!(matches!(
        a.setup_encryption(&MemoryKeyring::new(), cheap()),
        Err(CloudError::Drive(DriveError::InvalidConfig(_)))
    ));
}

#[test]
fn a_device_tells_its_encryption_and_unlocks_from_its_own_wrap() {
    let s3 = FakeS3::new();
    let transports = cloud(&s3);
    let dir = TempDir::new("azcloud-enc-status");
    let a = signed_up(&transports, &dir);
    let keys = MemoryKeyring::new();
    assert_eq!(a.encryption_status(&keys).unwrap(), (false, false));
    a.setup_encryption(&keys, cheap()).unwrap();
    assert_eq!(a.encryption_status(&keys).unwrap(), (true, true));
    assert!(a.holds_drive_key(&keys).unwrap());

    // The keyring lost the drive key (not the member key): this device's wrap brings it back.
    keys.delete(&device::drive_key_entry("d_1")).unwrap();
    assert_eq!(a.encryption_status(&keys).unwrap(), (true, false));
    assert!(a.unlock_key(&keys).unwrap().is_some());
    assert!(a.holds_drive_key(&keys).unwrap());
    assert!(a.unlock_key(&MemoryKeyring::new()).unwrap().is_none());
}

#[test]
fn a_configured_s3_endpoint_is_the_one_the_keys_go_to() {
    let s3 = FakeS3::new();
    let transports = cloud(&s3);
    let dir = TempDir::new("azcloud-enc-endpoint");
    let a = signed_up(&transports, &dir);
    assert_eq!(a.s3_endpoint(), super::S3, "the drive's own by default");
    let a = a.with_s3_endpoint(Some(" http://127.0.0.1:19999/ "));
    assert_eq!(a.s3_endpoint(), "http://127.0.0.1:19999");
    assert_eq!(a.bucket_drive().unwrap().config().endpoint, "http://127.0.0.1:19999");
    let a = a.with_s3_endpoint(None);
    assert_eq!(a.s3_endpoint(), super::S3);
}
