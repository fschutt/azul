//! An encrypted Azlin drive across devices (feature `encryption`): the first device turns
//! encryption on, its join code carries the drive key sealed for the second device (once), both
//! read and write the same files, the recovery code opens the key on a third.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

use serde_json::Value;

use azul_storage::{
    crypto::{device, keys::RecoveryKdf, DriveKey},
    encrypted::{IndexProvider, MemoryIndex, NameIndex, OpenRefusal},
    keyring::{KeyringStore, MemoryKeyring},
    testing::TempDir,
    trust::{EncState, Via, Why},
    Drive, DriveError, HttpCall, HttpReply, Method, Transport,
};

use super::{bundle, fake_s3::FakeS3, header, json, Fake, Shared, TOKEN};
use crate::{
    account::{Account, AccountFile, JoinCode},
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
    Account::sign_up_at_token_server(&state, TOKEN, transports.clone(), "100GB", "Ann's drive")
        .unwrap()
}

/// Every drive is encrypted as it is made: the sign-up sets up its keys before anything else
/// can touch it, and hands back its recovery code once.
#[test]
fn a_signup_makes_the_drive_encrypted_before_anything_uses_it() {
    let s3 = FakeS3::new();
    let transports = cloud(&s3);
    let dir = TempDir::new("azcloud-enc-signup");
    let state = StateDir::open(dir.path()).unwrap();
    let keys = MemoryKeyring::new();
    let made = crate::encryption::NewKeys {
        keyring: &keys,
        s3_endpoint: None,
        kdf: cheap(),
    };
    let (account, code) =
        Account::signup(&state, TOKEN, transports, "100GB", "Ann's drive", made).unwrap();
    assert!(s3.keys().iter().any(|k| k == ".azlin/keys/recovery.key"));
    assert!(matches!(
        account.encryption_state(&keys).unwrap(),
        EncState::Encrypted {
            via: Via::KeyInKeyring,
            ..
        }
    ));
    assert!(!code.to_text().is_empty());
    // The account file wrote down that the drive was found encrypted, with its key's id.
    let file = AccountFile::load(&state.account_file()).unwrap();
    let record = file.get(None).unwrap();
    assert!(record.encrypted, "the drive is recorded as encrypted");
    let key = device::load_drive_key(&keys, "d_1").unwrap().unwrap();
    assert_eq!(record.key_id, Some(key.id().to_hex()));
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
        Err(CloudError::Drive(DriveError::Locked { .. }))
    ));
}

/// `cloud` with a bucket that refuses to delete an invite's wrap (403) and passes everything else
/// on.
fn cloud_keeping_invites(s3: &Arc<FakeS3>) -> TransportFactory {
    struct KeepsInvites(Box<dyn Transport>);

    impl Transport for KeepsInvites {
        fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
            if call.method == Method::Delete && call.url.contains("invite-") {
                return Ok(HttpReply {
                    status: 403,
                    headers: vec![(
                        String::from("content-type"),
                        String::from("application/xml"),
                    )],
                    body: b"<Error><Code>AccessDenied</Code><Message>no</Message></Error>".to_vec(),
                });
            }
            self.0.send(call)
        }
    }

    let inner = cloud(s3);
    Arc::new(move || Box::new(KeepsInvites(inner())) as Box<dyn Transport>)
}

/// X-AR5b-9: a joining device that has the key is not told it has not joined because the one-time
/// wrap could not be deleted: `adopt_join_key_reporting` hands the key over and says the code is
/// live; the same call on a bucket that deletes says nothing.
#[test]
fn x_ar5b_9_a_join_whose_invite_could_not_be_deleted_is_a_join_that_says_so() {
    let s3 = FakeS3::new();
    let a_dir = TempDir::new("azcloud-enc-a");
    let a = signed_up(&cloud(&s3), &a_dir);
    let a_keys = MemoryKeyring::new();
    a.setup_encryption(&a_keys, cheap()).unwrap();
    let join = a.invite_with_key(None, &a_keys).unwrap();
    let decoded = JoinCode::decode(&join.encode()).unwrap();

    let b_dir = TempDir::new("azcloud-enc-b");
    let b_state = StateDir::open(b_dir.path()).unwrap();
    let keeping = cloud_keeping_invites(&s3);
    let b = Account::join(&b_state, TOKEN, keeping, &decoded).unwrap();
    let b_keys = MemoryKeyring::new();
    let adopted = b
        .adopt_join_key_reporting(&decoded, &b_keys)
        .expect("the join is not an error")
        .expect("the code carries the key");
    assert_eq!(
        Some(adopted.key.clone()),
        device::load_drive_key(&b_keys, "d_1").unwrap(),
        "the key is kept on the joined device"
    );
    assert!(adopted.invite_left.is_some(), "the code is still live");
    assert!(
        s3.keys().iter().any(|k| k.contains("invite-")),
        "the one-time wrap is still in the bucket: {:?}",
        s3.keys()
    );

    // The same code on a bucket that deletes: the device joins, and nothing is left of the invite.
    let c_dir = TempDir::new("azcloud-enc-c");
    let c_state = StateDir::open(c_dir.path()).unwrap();
    let c = Account::join(&c_state, TOKEN, cloud(&s3), &decoded).unwrap();
    let adopted = c
        .adopt_join_key_reporting(&decoded, &MemoryKeyring::new())
        .unwrap()
        .expect("the code carries the key");
    assert!(adopted.invite_left.is_none());
    assert!(
        s3.keys().iter().all(|k| !k.contains("invite-")),
        "{:?}",
        s3.keys()
    );
}

#[test]
fn a_join_code_names_the_key_it_seals_and_a_seal_of_another_key_is_refused() {
    let s3 = FakeS3::new();
    let transports = cloud(&s3);
    let a_dir = TempDir::new("azcloud-enc-a");
    let a = signed_up(&transports, &a_dir);
    let a_keys = MemoryKeyring::new();
    a.setup_encryption(&a_keys, cheap()).unwrap();
    let key = device::load_drive_key(&a_keys, "d_1").unwrap().unwrap();
    let join = a.invite_with_key(None, &a_keys).unwrap();
    assert_eq!(join.key_id, Some(key.id().to_hex()), "the code names the key it seals");
    let decoded = JoinCode::decode(&join.encode()).unwrap();
    assert_eq!(decoded.key_id, join.key_id);

    let b_dir = TempDir::new("azcloud-enc-b");
    let b_state = StateDir::open(b_dir.path()).unwrap();
    let b = Account::join(&b_state, TOKEN, transports.clone(), &decoded).unwrap();
    let b_keys = MemoryKeyring::new();
    // The code names another key than its seal holds (the seal was swapped in the bucket):
    // refused, and nothing of it is kept.
    let mut swapped = decoded.clone();
    swapped.key_id = Some(DriveKey::generate().unwrap().id().to_hex());
    assert!(matches!(
        b.adopt_join_key(&swapped, &b_keys),
        Err(CloudError::Drive(DriveError::KeyRefused { .. }))
    ));
    assert_eq!(device::load_drive_key(&b_keys, "d_1").unwrap(), None);
    // A damaged id is refused too, never read as a code that names none.
    let mut damaged = decoded.clone();
    damaged.key_id = Some(String::from("not hex"));
    assert!(matches!(
        b.adopt_join_key(&damaged, &b_keys),
        Err(CloudError::Failed(_))
    ));
    // The one-time wrap stayed: the code as it was made joins, and pins its key.
    assert_eq!(b.adopt_join_key(&decoded, &b_keys).unwrap(), Some(key.clone()));
    assert_eq!(device::pinned_drive_key(&b_keys, "d_1").unwrap(), Some(key.id()));
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
    assert_eq!(
        a.encryption_state(&keys).unwrap(),
        EncState::Unencrypted(Why::New)
    );
    a.setup_encryption(&keys, cheap()).unwrap();
    assert!(matches!(
        a.encryption_state(&keys).unwrap(),
        EncState::Encrypted {
            via: Via::KeyInKeyring,
            ..
        }
    ));

    // The keyring lost the drive key (not the member key): the account file recorded the drive
    // as encrypted, so it is locked here and not plain, and this device's wrap brings the key back.
    keys.delete(&device::drive_key_entry("d_1")).unwrap();
    assert!(matches!(
        a.encryption_state(&keys).unwrap(),
        EncState::Encrypted {
            via: Via::Record,
            ..
        }
    ));
    assert!(a.unlock_key(&keys).unwrap().is_some());
    assert!(device::load_drive_key(&keys, "d_1").unwrap().is_some());
    assert!(a.unlock_key(&MemoryKeyring::new()).unwrap().is_none());
}

/// X-AR5b-1: the command line meets a drive whose bucket lists no key file and holds a file - a
/// drive made before encryption, or one whose storage hides its key files, which cannot be told
/// apart - on a device that holds no key and recorded nothing: `azcloud up`, `down`, `ls`, `rm`
/// and the rest open nothing and write nothing. It used to take it for a plaintext drive "as its
/// bucket is" and put the file in the clear.
#[test]
fn x_ar5b_1_a_keyless_device_opens_no_drive_whose_bucket_lists_no_key_files_and_writes_nothing() {
    let s3 = FakeS3::new();
    let transports = cloud(&s3);
    let dir = TempDir::new("azcloud-enc-hostile");
    let a = signed_up(&transports, &dir);
    s3.write("decoy.txt", b"plain".to_vec());
    let before = s3.keys();
    let provider = SharedIndex(Arc::new(MemoryIndex::new()));

    let keyless = MemoryKeyring::new();
    let refused = a.open_encrypted_only(&keyless, &provider);
    assert!(
        matches!(refused, Err(OpenRefusal::NotEncrypted(Why::HiddenOrAbsent))),
        "{refused:?}"
    );
    assert_eq!(
        a.encryption_state(&keyless).unwrap(),
        EncState::Unencrypted(Why::HiddenOrAbsent)
    );
    assert_eq!(s3.keys(), before, "nothing was written");
    assert!(
        !AccountFile::load(&a.state().account_file())
            .unwrap()
            .get(None)
            .unwrap()
            .encrypted
    );

    // A keyring that cannot be read decides nothing either.
    struct Declined;
    impl KeyringStore for Declined {
        fn get(&self, _: &str) -> Result<Option<String>, azul_storage::keyring::KeyringError> {
            Err(azul_storage::keyring::KeyringError::Denied)
        }
        fn set(&self, _: &str, _: &str) -> Result<(), azul_storage::keyring::KeyringError> {
            Err(azul_storage::keyring::KeyringError::Denied)
        }
        fn delete(&self, _: &str) -> Result<(), azul_storage::keyring::KeyringError> {
            Err(azul_storage::keyring::KeyringError::Denied)
        }
    }
    assert!(matches!(
        a.open_encrypted_only(&Declined, &provider),
        Err(OpenRefusal::Keyring(DriveError::Denied { .. }))
    ));
    assert_eq!(s3.keys(), before);
}

/// X-AR5b-1: a drive that was set up, then opened, is written down as encrypted; the device that
/// lost its key (a reinstall: the same state folder, a new keyring) meets a bucket that now hides
/// its key files and holds a decoy, and is told the drive is LOCKED - never plain.
#[test]
fn x_ar5b_1_a_drive_found_encrypted_stays_locked_without_its_key_whatever_the_bucket_lists() {
    let s3 = FakeS3::new();
    let transports = cloud(&s3);
    let dir = TempDir::new("azcloud-enc-recorded");
    let a = signed_up(&transports, &dir);
    let keys = MemoryKeyring::new();
    a.setup_encryption(&keys, cheap()).unwrap();
    let provider = SharedIndex(Arc::new(MemoryIndex::new()));
    let opened = a.open_encrypted_only(&keys, &provider).unwrap();
    opened.put("notes/plan.txt", b"the plan").unwrap();

    // Every key file leaves the bucket's listing (here: they are gone), a decoy stands in.
    for key in s3.keys().into_iter().filter(|k| k.starts_with(".azlin/")) {
        s3.remove(&key);
    }
    s3.write("decoy.txt", b"plain".to_vec());
    let before = s3.keys();
    let reinstalled = MemoryKeyring::new();
    let locked = a.open_encrypted_only(&reinstalled, &provider);
    assert!(
        matches!(&locked, Err(OpenRefusal::Locked { recorded: true, .. })),
        "{locked:?}"
    );
    assert_eq!(s3.keys(), before, "nothing was written");
}

/// The token server and the bucket with the network's DNS down: the sign-up lists node `n1` at
/// 127.0.0.1, and a host name only resolves once the transport was told its addresses.
struct WithoutDns {
    s3: Arc<FakeS3>,
    /// The hosts the transport was told addresses for, in order.
    told: Mutex<Vec<String>>,
}

impl Transport for WithoutDns {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        if call.url.starts_with(TOKEN) {
            let nodes = r#""nodes": [{"name": "n1", "url": "http://n1.nodes.test:19001",
                                      "ready": true, "addresses": ["127.0.0.1"]}]"#;
            return Ok(json(
                201,
                &bundle("AKID1", "2099-01-01T00:00:00Z", "dt_f.0.a").replace(r#""nodes": []"#, nodes),
            ));
        }
        let rest = call.url.split_once("://").map_or("", |(_, r)| r);
        let authority = rest.split('/').next().unwrap_or("");
        let host = authority.rsplit_once(':').map_or(authority, |(h, _)| h);
        let known = host.parse::<std::net::IpAddr>().is_ok()
            || self.told.lock().unwrap().iter().any(|h| h == host);
        if !known {
            return Err(format!("DNS resolution failed for {}", call.url));
        }
        Ok(self.s3.answer(call))
    }

    fn fallback_addresses(&self, host: &str, _addresses: &[String]) -> bool {
        self.told.lock().unwrap().push(host.to_string());
        true
    }
}

struct SharedWithoutDns(Arc<WithoutDns>);

impl Transport for SharedWithoutDns {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        self.0.send(call)
    }

    fn fallback_addresses(&self, host: &str, addresses: &[String]) -> bool {
        self.0.fallback_addresses(host, addresses)
    }
}

#[test]
fn with_dns_down_the_key_listing_reaches_the_bucket_at_the_nodes_addresses() {
    let without_dns = Arc::new(WithoutDns {
        s3: FakeS3::new(),
        told: Mutex::new(Vec::new()),
    });
    let shared = without_dns.clone();
    let transports: TransportFactory =
        Arc::new(move || Box::new(SharedWithoutDns(shared.clone())) as Box<dyn Transport>);
    let dir = TempDir::new("azcloud-enc-no-dns");
    let a = signed_up(&transports, &dir)
        .with_s3_endpoint(Some("http://blk.dns-test.invalid:19000"));
    let keys = MemoryKeyring::new();
    assert_eq!(
        a.encryption_state(&keys).unwrap(),
        EncState::Unencrypted(Why::New),
        "the listing of the key files goes to the block host at the nodes' addresses"
    );
    assert!(
        without_dns
            .told
            .lock()
            .unwrap()
            .iter()
            .any(|h| h == "blk.dns-test.invalid"),
        "the block host was given the nodes' addresses before it was asked"
    );
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

/// K2: whoever can write to the bucket can plant a rotation journal. It must not make "I was
/// hacked" skip the account's lockdown (the attacker's tokens would stay valid) or fail the
/// rotation on a key nobody has: the journal of a rotation that stopped on this device is the
/// one that verifies under this device's new key.
#[test]
fn a_planted_rotation_journal_does_not_skip_the_lockdown() {
    let s3 = FakeS3::new();
    let transports = cloud(&s3);
    let dir = TempDir::new("azcloud-enc-planted-journal");
    let mut a = signed_up(&transports, &dir);
    let keys = MemoryKeyring::new();
    a.setup_encryption(&keys, cheap()).unwrap();
    let provider = SharedIndex(Arc::new(MemoryIndex::new()));
    a.open_encrypted(&keys, &provider)
        .unwrap()
        .put("notes/plan.txt", b"the plan")
        .unwrap();
    let old = device::load_drive_key(&keys, "d_1").unwrap().unwrap();
    let planted = format!(
        r#"{{"format":"azlin-drive-key","version":1,"kind":"rotation","old":"{}","new":"{}","phase":"started"}}"#,
        old.id().to_hex(),
        "cd".repeat(16)
    );
    s3.write(".azlin/keys/_rotation.key", planted.into_bytes());
    let lockdowns = LOCKDOWNS.load(Ordering::SeqCst);

    let rotated = a.rotate_drive_key(&keys, &provider, cheap()).unwrap();
    assert!(
        LOCKDOWNS.load(Ordering::SeqCst) > lockdowns,
        "the lockdown went first, whatever the bucket held"
    );
    assert_ne!(rotated.drive_key, old.id());
    assert_eq!(
        a.open_encrypted(&keys, &provider)
            .unwrap()
            .get("notes/plan.txt")
            .unwrap(),
        b"the plan"
    );
    assert!(
        s3.read(".azlin/keys/_rotation.key").is_none(),
        "the rotation that ran replaced the planted journal and removed its own"
    );
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
    let provider = azul_storage::meta::MetaIndexProvider::new(
        azul_storage::meta::IndexProfile::in_memory("Laptop"),
    );
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

// ==== X-DELTA-AZDRIVE-KEYS-25: a keyring that keeps no key is asked before the server is ====

/// A keyring that keeps no drive key: the plain secrets file of a command line whose user did not
/// say `--allow-plain-secrets` (AZ-K7).
fn plain_secrets(dir: &TempDir) -> (StateDir, crate::secrets::FileSecrets) {
    let state = StateDir::open(dir.path()).unwrap();
    let secrets = state.secrets();
    (state, secrets)
}

/// X-DELTA-AZDRIVE-KEYS-25: the sign-up made the drive at the token server first and was refused at
/// the first key it wrote, which left a drive that takes no file. Refused before any server call.
#[test]
fn x_delta_azdrive_keys_25_a_signup_into_a_keyring_that_keeps_no_key_asks_the_server_for_nothing() {
    let fake = Fake::new(|_, _| Ok(json(500, r#"{"error": "never reached"}"#)));
    let transports: TransportFactory = {
        let fake = fake.clone();
        Arc::new(move || Box::new(Shared(fake.clone())) as Box<dyn Transport>)
    };
    let dir = TempDir::new("azcloud-signup-plain");
    let (state, secrets) = plain_secrets(&dir);
    let keys = crate::encryption::NewKeys {
        keyring: &secrets,
        s3_endpoint: None,
        kdf: cheap(),
    };
    let refused = Account::signup(&state, TOKEN, transports, "100GB", "Ann's drive", keys)
        .err()
        .expect("a plain secrets file keeps no key")
        .to_string();
    assert!(refused.contains("plain file"), "{refused}");
    assert!(
        fake.calls().is_empty(),
        "the token server was not asked: {} call(s)",
        fake.calls().len()
    );
}

/// X-DELTA-AZDRIVE-KEYS-25: a join spends the code's one-time token; a code that carries a key is
/// refused by a keyring that keeps none BEFORE that, and one without a key asks nothing of it.
#[test]
fn x_delta_azdrive_keys_25_a_join_code_with_a_key_is_refused_before_it_is_spent_by_a_keyring_that_keeps_none(
) {
    use azul_storage::footprint::Allow;
    let dir = TempDir::new("azcloud-join-plain");
    let (state, secrets) = plain_secrets(&dir);
    let carries = JoinCode {
        drive_id: String::from("d_1"),
        drive_token: String::from("dt_m.0.joins"),
        member: String::from("m_laptop"),
        name: String::from("Ann's drive"),
        token_url: TOKEN.to_string(),
        key_seal: Some("ab".repeat(32)),
        key_id: None,
    };
    let refused = Account::check_join_key_can_be_kept(&carries, &secrets)
        .unwrap_err()
        .to_string();
    assert!(refused.contains("plain file"), "{refused}");
    assert!(
        refused.contains("--allow-plain-secrets"),
        "it says what the user can do: {refused}"
    );

    // a code without a key asks nothing of the keyring
    let without = JoinCode {
        key_seal: None,
        ..carries.clone()
    };
    Account::check_join_key_can_be_kept(&without, &secrets).unwrap();
    // and with the user's word the key is kept
    let allowed = state.allowing_plain_secrets(Allow::PlainSecrets).secrets();
    Account::check_join_key_can_be_kept(&carries, &allowed).unwrap();
}

/// X-DELTA-AZDRIVE-KEYS-25: "I was hacked" locked the drive down and was refused at its first new
/// key, so the other devices were locked out and the key was not replaced. Refused before the
/// lockdown: the error is the keyring's, not the token server's.
#[test]
fn x_delta_azdrive_keys_25_a_rotation_with_a_keyring_that_keeps_no_key_is_refused_before_the_lockdown(
) {
    let s3 = FakeS3::new();
    let transports = cloud(&s3);
    let dir = TempDir::new("azcloud-enc-rotate-plain");
    let mut account = signed_up(&transports, &dir);
    let keys = MemoryKeyring::new();
    account.setup_encryption(&keys, cheap()).unwrap();
    let provider = SharedIndex(Arc::new(MemoryIndex::new()));

    let plain_dir = TempDir::new("azcloud-enc-rotate-plain-secrets");
    let (_, plain) = plain_secrets(&plain_dir);
    let refused = account
        .rotate_drive_key(&plain, &provider, cheap())
        .err()
        .expect("a plain secrets file keeps no key")
        .to_string();
    assert!(refused.contains("plain file"), "{refused}");
}

/// X-DELTA-AZDRIVE-KEYS-21: the account file's record is read the way AzDrive reads the drives
/// file's: a key id that is no key id is a damaged record, and the drive does not open on it - it
/// was "no id, so nothing to compare" (`and_then(KeyId::from_hex)`).
#[test]
fn x_delta_azdrive_keys_21_an_unreadable_key_id_in_the_account_file_is_a_damaged_record() {
    let s3 = FakeS3::new();
    let transports = cloud(&s3);
    let dir = TempDir::new("azcloud-enc-damaged-record");
    let state = StateDir::open(dir.path()).unwrap();
    let keys = MemoryKeyring::new();
    let made = crate::encryption::NewKeys {
        keyring: &keys,
        s3_endpoint: None,
        kdf: cheap(),
    };
    let (account, _code) =
        Account::signup(&state, TOKEN, transports, "100GB", "Ann's drive", made).unwrap();
    let good = account.recorded().unwrap();
    assert!(
        good.encrypted && good.key_id.is_some() && !good.damaged,
        "{good:?}"
    );

    let mut file = AccountFile::load(&state.account_file()).unwrap();
    file.drives[0].key_id = Some(String::from("not a key id"));
    file.save(&state.account_file()).unwrap();
    let damaged = account.recorded().unwrap();
    assert!(damaged.damaged && damaged.encrypted, "{damaged:?}");
    let provider = SharedIndex(Arc::new(MemoryIndex::new()));
    let refused = account.open_encrypted_only(&keys, &provider);
    assert!(
        matches!(
            &refused,
            Err(OpenRefusal::Refused(DriveError::KeyRefused { .. }))
        ),
        "{:?}",
        refused.as_ref().err()
    );
}
