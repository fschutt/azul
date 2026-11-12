//! The opening of an Azlin drive (`Slot::open_azlin_in`, the function `Slot::open` calls with this computer's
//! transport, keyring and folders) over a fake S3 bucket: the drive that comes out of it is the encryption's gate
//! and nothing else (X-DELTA-STRUCT3-10), a notice is written through that gate only (-13), the account's vault is
//! opened under its own drive id (-16) and a Lock lets go of every handle that holds the session (-15).
//!
//! The bucket is a fake [`Transport`] that answers a listing as S3 does and records every request: "the bucket is
//! unchanged" is that no write, delete or POST reached it and its objects are the ones it started with.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use azul_storage::{
    config::{DriveAuth, DriveEntry, DriveLocation},
    keyring::{KeyringNamespace, MemoryKeyring},
    meta::MemoryBucket,
    testing::TempDir,
    AutoEncrypted, Drive, DriveError, HttpCall, HttpReply, Method, Precondition, Transport,
};

use crate::{
    encryption::Layers, jobs, renewals::notice_drive_in, unlock, AzlinWorld, KeyRecords,
    RotatedSessions, Slot,
};

const DRIVE: &str = "d_photos";

/// A fake S3 bucket: a listing as S3 answers it (every object under the prefix, no delimiter), a GET and HEAD of an
/// object, a PUT and a DELETE that change what it holds. Every request is recorded.
#[derive(Default)]
struct FakeS3 {
    objects: Mutex<BTreeMap<String, Vec<u8>>>,
    calls: Mutex<Vec<(Method, String)>>,
}

impl FakeS3 {
    fn with(objects: &[(&str, &[u8])]) -> Arc<FakeS3> {
        let fake = FakeS3::default();
        for (key, bytes) in objects {
            fake.put(key, bytes);
        }
        Arc::new(fake)
    }

    fn put(&self, key: &str, bytes: &[u8]) {
        self.objects
            .lock()
            .unwrap()
            .insert(key.to_string(), bytes.to_vec());
    }

    fn objects(&self) -> BTreeMap<String, Vec<u8>> {
        self.objects.lock().unwrap().clone()
    }

    /// What reached the bucket that is not a read.
    fn writes(&self) -> Vec<(Method, String)> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(method, _)| !matches!(method, Method::Get | Method::Head))
            .cloned()
            .collect()
    }

    fn answer(&self, call: &HttpCall) -> HttpReply {
        self.calls
            .lock()
            .unwrap()
            .push((call.method, call.url.clone()));
        let (path, query) = call.url.split_once('?').unwrap_or((call.url.as_str(), ""));
        let key = path
            .split_once("//")
            .map_or(path, |(_, rest)| rest)
            .split_once('/')
            .map_or("", |(_, rest)| rest)
            .strip_prefix("photos")
            .unwrap_or_default()
            .trim_start_matches('/');
        let reply = |status: u16, body: Vec<u8>| HttpReply {
            status,
            headers: vec![
                ("content-type".to_string(), "application/xml".to_string()),
                ("etag".to_string(), "\"e1\"".to_string()),
            ],
            body,
        };
        match call.method {
            Method::Get if query.contains("list-type=2") => {
                let prefix = decoded(
                    query
                        .split('&')
                        .find_map(|part| part.strip_prefix("prefix="))
                        .unwrap_or_default(),
                );
                let mut body = String::from(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ListBucketResult \
                     xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><Name>photos</Name>",
                );
                let objects = self.objects.lock().unwrap();
                let listed: Vec<_> = objects
                    .iter()
                    .filter(|(name, _)| name.starts_with(&prefix))
                    .collect();
                body.push_str(&format!("<KeyCount>{}</KeyCount>", listed.len()));
                body.push_str("<MaxKeys>1000</MaxKeys><IsTruncated>false</IsTruncated>");
                for (name, bytes) in listed {
                    body.push_str(&format!(
                        "<Contents><Key>{name}</Key><LastModified>2026-01-01T00:00:00.000Z\
                         </LastModified><ETag>\"e1\"</ETag><Size>{}</Size></Contents>",
                        bytes.len()
                    ));
                }
                body.push_str("</ListBucketResult>");
                reply(200, body.into_bytes())
            }
            Method::Get | Method::Head => match self.objects.lock().unwrap().get(key) {
                Some(bytes) => reply(200, bytes.clone()),
                None => reply(404, b"<Error><Code>NoSuchKey</Code></Error>".to_vec()),
            },
            Method::Put => {
                self.put(key, &call.body);
                reply(200, Vec::new())
            }
            Method::Delete => {
                self.objects.lock().unwrap().remove(key);
                reply(204, Vec::new())
            }
            _ => reply(501, b"<Error><Code>NotImplemented</Code></Error>".to_vec()),
        }
    }
}

/// `%XX` escapes of a query value read back.
fn decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' && at + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&text[at + 1..at + 3], 16) {
                out.push(byte);
                at += 3;
                continue;
            }
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

struct Handle(Arc<FakeS3>);

impl Transport for Handle {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        Ok(self.0.answer(call))
    }
}

fn azlin_entry(id: &str, drive_id: &str) -> DriveEntry {
    DriveEntry {
        id: id.to_string(),
        name: String::from("Photos"),
        location: DriveLocation::S3 {
            endpoint: String::from("https://s3.example.test"),
            region: String::from("us-east-1"),
            bucket: String::from("photos"),
            path_style: true,
            auth: DriveAuth::Azlin {
                drive_id: drive_id.to_string(),
                account_url: String::new(),
                encrypted: false,
                key_id: None,
            },
        },
    }
}

/// The text of the drive's keyring entry: credentials that never run out, so the opening and the calls after it
/// send nothing to the token server.
fn session_text() -> String {
    serde_json::json!({
        "drive_id": DRIVE,
        "drive_token": "dt_f.0.aaa",
        "access_key_id": "AKID",
        "secret_access_key": "secret",
    })
    .to_string()
}

struct Opened {
    slot: Slot,
    fake: Arc<FakeS3>,
    drive: Arc<dyn Drive>,
    _dir: TempDir,
}

/// A slot of the drive `DRIVE`, opened through the function the window calls, over `fake` and a keyring of the
/// test's own.
fn open(fake: &Arc<FakeS3>) -> Opened {
    let dir = TempDir::new("open-azlin");
    let keyring = azcloud_kit::SharedKeyring::with_lock_root(
        Arc::new(MemoryKeyring::new()),
        KeyringNamespace::azlin_shared(),
        dir.path(),
    );
    let mut slot = Slot::new(azlin_entry("slot-1", DRIVE));
    slot.secret = Some(azcloud_kit::Zeroizing::new(session_text()));
    let transports = {
        let fake = fake.clone();
        Arc::new(move || Box::new(Handle(fake.clone())) as Box<dyn Transport>)
    };
    let world = AzlinWorld {
        transports,
        nodes_file: dir.path().join("nodes.json"),
        iroh: None,
        layers: Layers {
            keyring: Arc::new(MemoryKeyring::new()),
            index_root: Some(dir.path().join("drive-index")),
        },
    };
    let drive = slot
        .open_azlin_in(
            &RotatedSessions::default(),
            &KeyRecords::default(),
            Some("https://token.example.test"),
            &keyring,
            world,
        )
        .expect("the slot opens");
    Opened {
        slot,
        fake: fake.clone(),
        drive,
        _dir: dir,
    }
}

/// Every way a file reaches a drive: none may reach the bucket.
fn assert_no_write_reaches_the_bucket(opened: &Opened, what: &str) {
    let drive = &opened.drive;
    let results = [
        ("put", drive.put("a.txt", b"x")),
        (
            "put_from",
            drive.put_from("b.txt", &mut &b"x"[..]).map(|_| ()),
        ),
        (
            "put_if",
            drive
                .put_if("c.txt", b"x", &Precondition::Absent)
                .map(|_| ()),
        ),
        ("create_folder", drive.create_folder("docs/")),
        ("delete", drive.delete("a.txt")),
    ];
    for (call, result) in results {
        assert!(
            result.is_err(),
            "{what}: a {call} was taken by the drive that opened ({result:?})"
        );
    }
    assert_eq!(
        opened.fake.writes(),
        Vec::new(),
        "{what}: the bucket was written to"
    );
}

/// X-DELTA-STRUCT3-10. A NEW drive - an empty bucket - taken by the opening takes no plaintext file: the drive that
/// comes out of `open_azlin_in` is the encryption's gate, a write is refused with the words of a new drive, and the
/// bucket is as it was. (The opener's gate was a closure the caller supplied, `Arc<dyn Drive>` in and out: one that
/// dropped the wrap sent every file to the bucket in the clear and passed every gate and test. It returns
/// `Arc<AutoEncrypted>` now - the compile-fail doctest of `open_azlin` - and this is the behaviour of what it
/// returns, entered through the function the window calls.)
#[test]
fn x_delta_struct3_10_a_new_drive_taken_by_the_opening_takes_no_plaintext_file() {
    let fake = FakeS3::with(&[]);
    let opened = open(&fake);
    let before = fake.objects();
    match opened.drive.put("a.txt", b"x") {
        Err(DriveError::Denied { message }) => {
            assert!(message.contains("not encrypted"), "{message}");
        }
        other => panic!("a plaintext put on a new drive: {other:?}"),
    }
    assert_no_write_reaches_the_bucket(&opened, "a new drive");
    assert_eq!(fake.objects(), before, "the bucket is unchanged");
    assert!(opened.slot.auto.is_some(), "the slot keeps the gate");
}

/// X-DELTA-STRUCT3-10. A LOCKED drive - key files in its bucket and no key on this computer - takes no plaintext
/// file either, and its bucket is as it was.
#[test]
fn x_delta_struct3_10_a_locked_drive_taken_by_the_opening_takes_no_plaintext_file() {
    let fake = FakeS3::with(&[(".azlin/keys/member-1", b"not a key")]);
    let opened = open(&fake);
    let before = fake.objects();
    match opened.drive.put("a.txt", b"x") {
        Err(DriveError::Locked { .. }) => {}
        other => panic!("a plaintext put on a locked drive: {other:?}"),
    }
    assert_no_write_reaches_the_bucket(&opened, "a locked drive");
    assert_eq!(fake.objects(), before, "the bucket is unchanged");
}

/// X-DELTA-STRUCT3-10. A drive this computer CANNOT TELL - files and no key file, no key and no note that it was
/// encrypted - takes no plaintext file and its bucket is as it was.
#[test]
fn x_delta_struct3_10_an_undecidable_drive_taken_by_the_opening_takes_no_plaintext_file() {
    let fake = FakeS3::with(&[("photo.jpg", b"jpeg")]);
    let opened = open(&fake);
    let before = fake.objects();
    match opened.drive.put("a.txt", b"x") {
        Err(DriveError::Denied { message }) => {
            assert!(message.contains("cannot tell"), "{message}");
        }
        other => panic!("a plaintext put on a drive that cannot be told: {other:?}"),
    }
    assert_no_write_reaches_the_bucket(&opened, "an undecidable drive");
    assert_eq!(fake.objects(), before, "the bucket is unchanged");
}

/// X-DELTA-STRUCT3-10, the crash and race cell: the wrap is checked when the drive is opened, but the drive is
/// DECIDED at its first call, from the bucket and the keyring as they are then. A drive that was new when the slot
/// opened and was encrypted by another device before the first put (its key files are in the bucket now, this
/// computer has none) is locked for that put: the write is refused and nothing but the other device's files is in
/// the bucket.
#[test]
fn x_delta_struct3_10_a_drive_another_device_encrypts_between_the_opening_and_the_first_put_is_locked(
) {
    let fake = FakeS3::with(&[]);
    let opened = open(&fake);
    // Another device sets the drive up now.
    fake.put(".azlin/keys/member-2", b"not a key");
    let before = fake.objects();
    match opened.drive.put("a.txt", b"x") {
        Err(DriveError::Locked { .. }) => {}
        other => panic!("a plaintext put on a drive that became locked: {other:?}"),
    }
    assert_no_write_reaches_the_bucket(&opened, "a drive that became locked");
    assert_eq!(
        fake.objects(),
        before,
        "the bucket holds the other device's files only"
    );
}

/// X-DELTA-STRUCT3-15. A Lock lets go of every handle that holds the old session: the slot's account is held by the
/// encrypted local copies below the gate (ObjectCache -> RetryingDrive -> the bytes -> `Arc<AzlinAccount>`) as much as
/// by the gate, and `wipe` left `slot.objects` until the slot was dropped - the drive token and the bucket's
/// credentials stayed alive, able to refresh through the keyring's lock. Entered through the opening and `wipe`.
#[test]
fn x_delta_struct3_15_a_lock_drops_the_encrypted_local_copies_and_with_them_the_old_session() {
    let fake = FakeS3::with(&[]);
    let mut opened = open(&fake);
    let account = opened
        .slot
        .azlin
        .clone()
        .expect("the slot holds the account");
    assert!(
        opened.slot.objects.is_some(),
        "the opening kept the encrypted local copies (the test gave it a folder)"
    );
    // The handle the opening returned is the slot's own `drive`; the test lets go of its copy.
    drop(std::mem::replace(
        &mut opened.drive,
        Arc::new(MemoryBucket::new()),
    ));
    assert!(
        Arc::strong_count(&account) > 1,
        "the slot's handles hold the account"
    );
    unlock::wipe(&mut opened.slot);
    assert!(
        opened.slot.drive.is_none() && opened.slot.auto.is_none() && opened.slot.objects.is_none()
    );
    assert_eq!(
        Arc::strong_count(&account),
        1,
        "after the Lock nothing but this test holds the account: its session is gone"
    );
}

fn plain_azlin_slot(drive: Option<Arc<dyn Drive>>, auto: Option<Arc<AutoEncrypted>>) -> Slot {
    let mut slot = Slot::new(azlin_entry("slot-1", DRIVE));
    slot.drive = drive;
    slot.auto = auto;
    slot
}

/// X-DELTA-STRUCT3-13. The drive a notice is written to is chosen by a function of the slots (it was a function of
/// the window's state that no test could call): it is the slot's encryption gate and never the slot's other drives.
/// A slot whose `drive` is a PLAIN drive and whose gate is another drive answers the gate, written to as a document
/// of the encrypted drive (nothing of it in the clear in either bucket); a slot that holds a plain `drive` and no
/// gate (not opened through the stack) has none - the old `notice_drive` answered that plain drive, and the notice
/// went into the bucket in the clear.
#[test]
fn x_delta_struct3_13_a_notice_goes_through_the_slots_gate_and_never_to_a_plain_drive() {
    use azul_storage::{
        config::EncryptionRecord,
        crypto::{device, keys::RecoveryKdf},
    };

    let key = "Azlin notices/2026-12-01 price change 100GB.txt";
    let text = "The price of 100GB changes on 2026-12-01.";
    // The slot's gate over an encrypted drive's bucket ...
    let gate_bucket = Arc::new(MemoryBucket::new());
    let keyring = Arc::new(MemoryKeyring::new());
    device::setup_new_drive(
        gate_bucket.as_ref(),
        keyring.as_ref(),
        DRIVE,
        RecoveryKdf::with_cost(64, 1, 1).unwrap(),
    )
    .unwrap();
    let auto = crate::encryption::wrap_with(
        keyring,
        Some(Arc::new(crate::encryption::meta_provider(None))),
        DRIVE,
        gate_bucket.clone(),
        EncryptionRecord::default(),
        "slot-1",
        &KeyRecords::default(),
    );
    // ... and a plain drive of its own as the slot's `drive`: the two differ.
    let plain = Arc::new(MemoryBucket::new());
    let slots = [plain_azlin_slot(
        Some(plain.clone() as Arc<dyn Drive>),
        Some(auto.clone()),
    )];

    let notice = notice_drive_in(&slots, DRIVE).expect("the slot's gate");
    assert!(Arc::ptr_eq(&notice, &auto), "the gate, not another drive");
    assert!(
        notice_drive_in(&slots, "d_other").is_none(),
        "another drive's id finds no slot"
    );
    jobs::save_notice(&notice, key, text).expect("the encrypted drive takes it");
    assert_eq!(
        auto.get(key).unwrap(),
        text.as_bytes(),
        "a document of the drive"
    );
    assert!(
        plain.objects().is_empty(),
        "nothing in the slot's plain drive"
    );
    for (name, bytes) in gate_bucket.objects() {
        assert!(
            !String::from_utf8_lossy(&bytes).contains("price of 100GB"),
            "{name} shows the notice in the clear"
        );
    }

    // A slot that holds a plain drive and no gate has no notice drive: nothing is written, nothing marked.
    let slots = [plain_azlin_slot(
        Some(plain.clone() as Arc<dyn Drive>),
        None,
    )];
    assert!(notice_drive_in(&slots, DRIVE).is_none());
    assert!(plain.objects().is_empty());
}

/// X-DELTA-STRUCT3-16. The vault of the account of drive A is never asked for under drive B's id: `drive_tokens`,
/// given an account of another drive than the id it is asked for, refuses and reads and writes nothing (the vault is
/// opened under the account's own id: the kit's `unlocked` takes none).
#[test]
fn x_delta_struct3_16_an_account_is_never_asked_for_another_drives_vault() {
    let fake = FakeS3::with(&[]);
    let opened = open(&fake);
    let account = opened
        .slot
        .azlin
        .clone()
        .expect("the slot holds the account");
    let dir = TempDir::new("struct3-16-store");
    let store = azcloud_kit::PeriodTokenStore::new(dir.path().join("tokens"));
    let asked_for_b = jobs::drive_tokens(&store, "d_other", Some(&account));
    match asked_for_b {
        Err(why) => assert!(why.contains("d_other") && why.contains(DRIVE), "{why}"),
        Ok(_) => panic!("drive B's tokens were answered from drive A's account"),
    }
    assert!(
        !dir.path().join("tokens").exists(),
        "nothing was written for either drive"
    );
    assert_eq!(fake.writes(), Vec::new(), "and nothing reached A's bucket");
    // The control: its own id is answered (this computer's vault: the key is not here).
    assert!(jobs::drive_tokens(&store, DRIVE, Some(&account)).is_ok());
}
