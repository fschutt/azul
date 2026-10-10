//! An encrypted Azlin drive across devices (feature `encryption`): the first device turns
//! encryption on, its join code carries the drive key sealed for the second device (once), both
//! read and write the same files, the recovery code opens the key on a third.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use serde_json::Value;

use azul_storage::{
    crypto::{device, keys::RecoveryKdf, DriveKey},
    encrypted::{IndexProvider, MemoryIndex, NameIndex},
    keyring::{KeyringStore, MemoryKeyring},
    testing::TempDir,
    Drive, DriveError, Method, Transport,
};

use super::{bundle, fake_s3::FakeS3, header, json, Fake, Shared, TOKEN};
use crate::{
    account::{Account, JoinCode},
    cloudflare::{Cloudflare, DEFAULT_WORKER, DROP_KEY_VARIABLE},
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
        if url.ends_with("/lockdown") {
            LOCKDOWNS.fetch_add(1, Ordering::SeqCst);
            return Ok(json(
                200,
                &bundle("AKID9", "2099-01-01T00:00:00Z", "dt_f.9.locked"),
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

/// The lockdowns the fake token server answered.
static LOCKDOWNS: AtomicUsize = AtomicUsize::new(0);

/// One index in memory both devices share: the stand-in for the drive's encrypted metadata
/// repository (nothing of it is sealed: its rekey has nothing to do).
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

    fn rekey(
        &self,
        _drive: &str,
        _bucket: Arc<dyn Drive>,
        _old: &DriveKey,
        _new: &DriveKey,
    ) -> Result<(), DriveError> {
        Ok(())
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

#[test]
fn incoming_mail_gets_a_drop_key_whose_public_half_goes_to_the_customers_worker() {
    let s3 = FakeS3::new();
    let transports = cloud(&s3);
    let dir = TempDir::new("azcloud-enc-drop");
    let a = signed_up(&transports, &dir);
    let keys = MemoryKeyring::new();
    assert!(
        matches!(a.enable_mail_drop(&keys), Err(CloudError::Failed(_))),
        "a plain drive gets its mail as it is"
    );
    assert_eq!(a.mail_drop_key(&keys).unwrap(), None);
    a.setup_encryption(&keys, cheap()).unwrap();
    let public = a.enable_mail_drop(&keys).unwrap();
    assert_eq!(a.enable_mail_drop(&keys).unwrap(), public, "the same key again");
    assert_eq!(a.mail_drop_key(&keys).unwrap(), Some(public));
    assert!(s3.keys().iter().any(|k| k == ".azlin/keys/_drop.key"));

    // The Worker's variable, through the customer's own token, straight to Cloudflare.
    let fake = Fake::new(|_, _| Ok(json(200, r#"{"success": true, "errors": [], "result": {}}"#)));
    let cloudflare = Cloudflare::new(
        Box::new(Shared(fake.clone())),
        "0123456789ABCDEF0123456789abcdef",
        " cf-token ",
    )
    .unwrap()
    .with_base("https://cf.test/client/v4/");
    assert!(!format!("{cloudflare:?}").contains("cf-token"));
    cloudflare
        .set_worker_secret(DEFAULT_WORKER, DROP_KEY_VARIABLE, &public.to_hex())
        .unwrap();
    let calls = fake.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 1);
    let call = &calls[0];
    assert_eq!(call.method, Method::Put);
    assert_eq!(
        call.url,
        "https://cf.test/client/v4/accounts/0123456789abcdef0123456789abcdef/workers/scripts/\
         azlin-mail-worker/secrets"
    );
    assert_eq!(header(call, "authorization"), Some("Bearer cf-token"));
    let body: Value = serde_json::from_slice(&call.body).unwrap();
    assert_eq!(body["name"], DROP_KEY_VARIABLE);
    assert_eq!(body["text"], public.to_hex());
    assert_eq!(body["type"], "secret_text");

    // Cloudflare's refusal says why; names that are not names never reach it.
    let refusing = Fake::new(|_, _| {
        Ok(json(
            403,
            r#"{"success": false, "errors": [{"code": 10000, "message": "Authentication error"}]}"#,
        ))
    });
    let cloudflare =
        Cloudflare::new(Box::new(Shared(refusing.clone())), &"a".repeat(32), "t").unwrap();
    let refused = cloudflare.set_worker_secret(DEFAULT_WORKER, DROP_KEY_VARIABLE, "x");
    assert!(
        matches!(&refused, Err(CloudError::Failed(why)) if why.contains("Authentication error")),
        "{refused:?}"
    );
    assert!(cloudflare.set_worker_secret("../x", DROP_KEY_VARIABLE, "x").is_err());
    assert_eq!(refusing.calls.lock().unwrap().len(), 1);
    assert!(Cloudflare::new(Box::new(Shared(refusing)), "not-an-id", "t").is_err());
}

#[test]
fn i_was_hacked_locks_the_drive_down_then_rotates_its_key() {
    let s3 = FakeS3::new();
    let transports = cloud(&s3);
    let dir = TempDir::new("azcloud-enc-rotate");
    let mut a = signed_up(&transports, &dir);
    let keys = MemoryKeyring::new();
    let old_code = a.setup_encryption(&keys, cheap()).unwrap();
    let provider = SharedIndex(Arc::new(MemoryIndex::new()));
    a.open_encrypted(&keys, &provider)
        .unwrap()
        .put("notes/plan.txt", b"the plan")
        .unwrap();
    let old = device::load_drive_key(&keys, "d_1").unwrap().unwrap();
    let lockdowns = LOCKDOWNS.load(Ordering::SeqCst);

    let rotated = a.rotate_drive_key(&keys, &provider, cheap()).unwrap();
    assert!(LOCKDOWNS.load(Ordering::SeqCst) > lockdowns, "the lockdown went first");
    assert_ne!(rotated.drive_key, old.id());
    assert_eq!(rotated.rewrapped, 1);
    assert_eq!(
        device::load_drive_key(&keys, "d_1").unwrap().map(|k| k.id()),
        Some(rotated.drive_key)
    );
    assert_eq!(
        a.open_encrypted(&keys, &provider)
            .unwrap()
            .get("notes/plan.txt")
            .unwrap(),
        b"the plan"
    );
    assert!(a.recover_key(&old_code, &MemoryKeyring::new()).is_err());
    assert!(a.recover_key(&rotated.recovery_code, &MemoryKeyring::new()).is_ok());

    // Re-encrypt everything: the one file into a new object.
    let before = a
        .open_encrypted(&keys, &provider)
        .unwrap()
        .entry("notes/plan.txt")
        .unwrap()
        .object_id();
    let mut state = azul_storage::rotation::ReencryptState::new(u64::MAX);
    assert!(a
        .reencrypt(&keys, &provider, &mut state, &mut |_| Ok(()), &|| false)
        .unwrap());
    assert_eq!(state.done, 1);
    let drive = a.open_encrypted(&keys, &provider).unwrap();
    assert_ne!(drive.entry("notes/plan.txt").unwrap().object_id(), before);
    assert_eq!(drive.get("notes/plan.txt").unwrap(), b"the plan");
}

/// `git clone azlin::drive://d_1` (azul-storage's `git-remote-azlin` hands the URL to
/// `azcloud git-remote`): plain git reads the drive index in the account's bucket, over the
/// account's signed S3 requests, with the drive key this device keeps.
#[test]
fn git_reads_the_drive_index_in_the_accounts_bucket_through_the_remote_helper() {
    let s3 = FakeS3::new();
    let transports = cloud(&s3);
    let dir = TempDir::new("azcloud-enc-git");
    let a = signed_up(&transports, &dir);
    let keys = MemoryKeyring::new();
    a.setup_encryption(&keys, cheap()).unwrap();
    let provider = azul_storage::meta::MetaIndexProvider::new("Laptop");
    let drive = a.open_encrypted(&keys, &provider).unwrap();
    drive.put("notes/plan.txt", b"the plan").unwrap();

    let mut output = Vec::new();
    let mut packs = 0;
    a.serve_git_remote(
        &keys,
        std::io::Cursor::new("option object-format\nlist\n\n"),
        &mut output,
        &mut |_: &[u8]| {
            packs += 1;
            Ok(())
        },
    )
    .unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(text.starts_with("ok\n:object-format sha256\n"), "{text}");
    assert!(
        text.contains(" refs/heads/main\n@refs/heads/main HEAD\n"),
        "{text}"
    );
    assert_eq!(packs, 0, "a list fetches nothing");

    // A device without the drive key reads nothing.
    assert!(a
        .serve_git_remote(
            &MemoryKeyring::new(),
            std::io::Cursor::new("list\n\n"),
            &mut Vec::new(),
            &mut |_: &[u8]| Ok(()),
        )
        .is_err());
}
