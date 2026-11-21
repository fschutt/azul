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
    AutoEncrypted, Drive, DriveError, HttpCall, HttpReply, ListRequest, Method, Precondition,
    Transport,
};

use crate::{
    encryption::Layers, jobs, renewals::notice_drive_in, unlock, AzlinWorld, KeyRecords,
    RotatedSessions, Slot,
};

const DRIVE: &str = "d_photos";

/// A fake S3 bucket: the S3 protocol in front of a [`MemoryBucket`], whose versions are the entity tags and whose
/// conditional writes are S3's `If-None-Match: *` and `If-Match` (412 when they do not hold): a listing as S3 answers it
/// (its delimiter and its pages), a GET - ranged too - and a HEAD of an object, a PUT - conditional too -, a DELETE
/// and the multi-object delete. Every request is recorded.
#[derive(Default)]
struct FakeS3 {
    bucket: MemoryBucket,
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
        self.bucket
            .put(key, bytes)
            .expect("a bucket in memory takes a put");
    }

    fn objects(&self) -> BTreeMap<String, Vec<u8>> {
        self.bucket.objects().into_iter().collect()
    }

    /// The bucket itself, for a test that sets a drive up in it before the opening.
    fn bucket(&self) -> &MemoryBucket {
        &self.bucket
    }

    /// Every request that reached the bucket, reads included.
    fn calls(&self) -> Vec<(Method, String)> {
        self.calls.lock().unwrap().clone()
    }

    /// What reached the bucket that is not a read.
    fn writes(&self) -> Vec<(Method, String)> {
        self.calls()
            .into_iter()
            .filter(|(method, _)| !matches!(method, Method::Get | Method::Head))
            .collect()
    }

    /// The entity tag of `key` as S3 sends it, quotes included.
    fn etag(&self, key: &str) -> String {
        let version = self
            .bucket
            .head(key)
            .ok()
            .and_then(|info| info.etag)
            .unwrap_or_default();
        format!("\"{version}\"")
    }

    fn answer(&self, call: &HttpCall) -> HttpReply {
        self.calls
            .lock()
            .unwrap()
            .push((call.method, call.url.clone()));
        let (path, query) = call.url.split_once('?').unwrap_or((call.url.as_str(), ""));
        let key = decoded(
            path.split_once("//")
                .map_or(path, |(_, rest)| rest)
                .split_once('/')
                .map_or("", |(_, rest)| rest)
                .strip_prefix("photos")
                .unwrap_or_default()
                .trim_start_matches('/'),
        );
        let header = |name: &str| {
            call.headers
                .iter()
                .find(|(have, _)| have.eq_ignore_ascii_case(name))
                .map(|(_, value)| value.as_str())
        };
        let param = |name: &str| {
            query
                .split('&')
                .find_map(|part| part.strip_prefix(&format!("{name}=")))
                .map(decoded)
        };
        let reply = |status: u16, headers: Vec<(&str, String)>, body: Vec<u8>| HttpReply {
            status,
            headers: headers
                .into_iter()
                .map(|(name, value)| (name.to_string(), value))
                .collect(),
            body,
        };
        let error = |status: u16, code: &str| {
            reply(
                status,
                vec![("content-type", String::from("application/xml"))],
                format!("<Error><Code>{code}</Code></Error>").into_bytes(),
            )
        };
        let described = |bytes: &[u8]| {
            vec![
                ("content-length", bytes.len().to_string()),
                ("etag", self.etag(&key)),
                (
                    "last-modified",
                    String::from("Thu, 01 Jan 2026 00:00:00 GMT"),
                ),
            ]
        };
        match call.method {
            Method::Get if query.contains("list-type=2") => {
                let request = ListRequest {
                    prefix: param("prefix").unwrap_or_default(),
                    delimiter: param("delimiter").filter(|d| !d.is_empty()),
                    continuation: param("continuation-token"),
                    max_keys: param("max-keys")
                        .and_then(|n| n.parse().ok())
                        .unwrap_or(1000),
                    refresh: false,
                };
                let Ok(page) = self.bucket.list(&request) else {
                    return error(500, "InternalError");
                };
                let mut body = String::from(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?><ListBucketResult \
                     xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><Name>photos</Name>",
                );
                body.push_str(&format!(
                    "<KeyCount>{}</KeyCount><MaxKeys>{}</MaxKeys><IsTruncated>{}</IsTruncated>",
                    page.objects.len() + page.folders.len(),
                    request.max_keys,
                    page.next.is_some()
                ));
                if let Some(next) = &page.next {
                    body.push_str(&format!(
                        "<NextContinuationToken>{}</NextContinuationToken>",
                        escaped(next)
                    ));
                }
                for object in &page.objects {
                    body.push_str(&format!(
                        "<Contents><Key>{}</Key><LastModified>2026-01-01T00:00:00.000Z</LastModified>\
                         <ETag>\"{}\"</ETag><Size>{}</Size></Contents>",
                        escaped(&object.key),
                        object.etag.clone().unwrap_or_default(),
                        object.size
                    ));
                }
                for folder in &page.folders {
                    body.push_str(&format!(
                        "<CommonPrefixes><Prefix>{}</Prefix></CommonPrefixes>",
                        escaped(folder)
                    ));
                }
                body.push_str("</ListBucketResult>");
                reply(
                    200,
                    vec![("content-type", String::from("application/xml"))],
                    body.into_bytes(),
                )
            }
            Method::Get | Method::Head => {
                let Ok(bytes) = self.bucket.get(&key) else {
                    // A HEAD's answer has no body.
                    return if call.method == Method::Head {
                        reply(404, Vec::new(), Vec::new())
                    } else {
                        error(404, "NoSuchKey")
                    };
                };
                let head = call.method == Method::Head;
                // `bytes=start-end` or `bytes=start-`.
                let range = header("range")
                    .and_then(|text| text.strip_prefix("bytes="))
                    .and_then(|text| text.split_once('-'))
                    .and_then(|(start, end)| {
                        Some((start.parse::<usize>().ok()?, end.parse::<usize>().ok()))
                    });
                match range {
                    Some((start, _)) if start >= bytes.len() => error(416, "InvalidRange"),
                    Some((start, end)) if !head => {
                        let end = end.map_or(bytes.len() - 1, |end| end.min(bytes.len() - 1));
                        let mut headers = described(&bytes[start..=end]);
                        headers.push((
                            "content-range",
                            format!("bytes {start}-{end}/{}", bytes.len()),
                        ));
                        reply(206, headers, bytes[start..=end].to_vec())
                    }
                    _ if head => reply(200, described(&bytes), Vec::new()),
                    _ => reply(200, described(&bytes), bytes),
                }
            }
            Method::Put if header("x-amz-copy-source").is_some() => error(501, "NotImplemented"),
            Method::Put => {
                let condition = match (header("if-none-match"), header("if-match")) {
                    (Some("*"), _) => Some(Precondition::Absent),
                    (_, Some(tag)) => {
                        Some(Precondition::Matches(tag.trim_matches('"').to_string()))
                    }
                    _ => None,
                };
                let written = match &condition {
                    Some(condition) => self.bucket.put_if(&key, &call.body, condition).map(|_| ()),
                    None => self.bucket.put(&key, &call.body),
                };
                match written {
                    Ok(()) => reply(200, vec![("etag", self.etag(&key))], Vec::new()),
                    Err(DriveError::Conflict { .. }) => error(412, "PreconditionFailed"),
                    Err(_) => error(500, "InternalError"),
                }
            }
            Method::Delete => {
                let _ = self.bucket.delete(&key);
                reply(204, Vec::new(), Vec::new())
            }
            // The multi-object delete: the keys of the body.
            Method::Post if query.contains("delete") => {
                let body = String::from_utf8_lossy(&call.body).into_owned();
                for part in body.split("<Key>").skip(1) {
                    if let Some((name, _)) = part.split_once("</Key>") {
                        let _ = self.bucket.delete(&unescaped(name));
                    }
                }
                reply(
                    200,
                    vec![("content-type", String::from("application/xml"))],
                    b"<DeleteResult></DeleteResult>".to_vec(),
                )
            }
            _ => error(501, "NotImplemented"),
        }
    }
}

/// `%XX` escapes of a query value or a path read back.
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

/// `text` as XML character data.
fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// XML character data read back.
fn unescaped(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
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

/// The text of the keyring entry of the drive `drive_id`: credentials that never run out, so the opening and the
/// calls after it send nothing to the token server.
fn session_text_of(drive_id: &str) -> String {
    serde_json::json!({
        "drive_id": drive_id,
        "drive_token": "dt_f.0.aaa",
        "access_key_id": "AKID",
        "secret_access_key": "secret",
    })
    .to_string()
}

/// The text of the keyring entry of the drive `DRIVE`.
fn session_text() -> String {
    session_text_of(DRIVE)
}

/// This computer's side of an opening: its folder, the keyring the Azlin sessions live in, the keyring the drives'
/// keys live in (the layers'), and the key ids the opening writes down for the UI thread.
struct Machine {
    dir: TempDir,
    sessions: Arc<MemoryKeyring>,
    keys: Arc<MemoryKeyring>,
    records: KeyRecords,
}

impl Machine {
    fn new() -> Machine {
        Machine {
            dir: TempDir::new("open-azlin"),
            sessions: Arc::new(MemoryKeyring::new()),
            keys: Arc::new(MemoryKeyring::new()),
            records: KeyRecords::default(),
        }
    }

    /// Opens `slot` over `fake` through the function the window calls.
    fn open(&self, slot: &mut Slot, fake: &Arc<FakeS3>) -> Result<Arc<dyn Drive>, DriveError> {
        let keyring = azcloud_kit::SharedKeyring::with_lock_root(
            self.sessions.clone(),
            KeyringNamespace::azlin_shared(),
            self.dir.path(),
        );
        let transports = {
            let fake = fake.clone();
            Arc::new(move || Box::new(Handle(fake.clone())) as Box<dyn Transport>)
        };
        let world = AzlinWorld {
            transports,
            nodes_file: self.dir.path().join("nodes.json"),
            iroh: None,
            layers: Layers {
                keyring: self.keys.clone(),
                index_root: Some(self.dir.path().join("drive-index")),
            },
        };
        slot.open_azlin_in(
            &RotatedSessions::default(),
            &self.records,
            Some("https://token.example.test"),
            &keyring,
            world,
        )
    }
}

struct Opened {
    slot: Slot,
    fake: Arc<FakeS3>,
    drive: Arc<dyn Drive>,
    /// The folder of the computer the drive was opened on: it lives as long as the opened drive.
    _machine: Machine,
}

/// A slot of the drive `DRIVE`, opened through the function the window calls, over `fake` and a keyring of the
/// test's own.
fn open(fake: &Arc<FakeS3>) -> Opened {
    let machine = Machine::new();
    let mut slot = Slot::new(azlin_entry("slot-1", DRIVE));
    slot.secret = Some(azcloud_kit::Zeroizing::new(session_text()));
    let drive = machine.open(&mut slot, fake).expect("the slot opens");
    Opened {
        slot,
        fake: fake.clone(),
        drive,
        _machine: machine,
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
/// dropped the wrap sent every file to the bucket in the clear and passed every gate and test. The opener makes the
/// gate itself now, from parts that carry no bytes and no id - the compile-fail doctests of `open_azlin` - and this is
/// the behaviour of what it makes, entered through the function the window calls.)
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
        config::{DriveId, EncryptionRecord, SlotId},
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
        DriveId::named(DRIVE),
        gate_bucket.clone(),
        EncryptionRecord::default(),
        SlotId::named("slot-1"),
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

/// X-DELTA-STRUCT4-2. The drives file is a file any process of the user can write, and the drive's id has two
/// sources in it: the entry's `drive_id` (what the keys are looked up under) and the session's (what the
/// credentials are spent under). An entry of drive A that carried the id of drive B - a drive this computer holds
/// a key for - was opened: the gate was built under B's id, and B's key was looked up over A's bucket. The opening
/// refuses a slot whose session is another drive's, with a sentence that names both ids; nothing is sent to the
/// bucket, nothing is written to either keyring, and the slot keeps no handle. Entered through the function the window
/// calls.
#[test]
fn x_delta_struct4_2_a_slot_whose_session_is_another_drives_than_its_entrys_is_refused_and_nothing_is_sent(
) {
    let fake = FakeS3::with(&[]);
    let machine = Machine::new();
    // The drives file says `d_photos`; the keyring's text for the slot is the session of `d_other`.
    let mut slot = Slot::new(azlin_entry("slot-1", DRIVE));
    slot.secret = Some(azcloud_kit::Zeroizing::new(session_text_of("d_other")));
    let refused = match machine.open(&mut slot, &fake) {
        Err(refused) => refused,
        Ok(_) => panic!("a slot whose entry and session name two drives was opened"),
    };
    match &refused {
        DriveError::InvalidConfig(why) => assert!(
            why.contains(DRIVE) && why.contains("d_other"),
            "the sentence names both ids: {why}"
        ),
        wrong => panic!("not refused as a configuration that disagrees with itself: {wrong:?}"),
    }
    assert_eq!(fake.calls(), Vec::new(), "nothing was sent to the bucket");
    assert!(fake.objects().is_empty(), "and nothing is in it");
    assert_eq!(
        machine.keys.names(),
        Vec::<String>::new(),
        "no key was made or looked up into"
    );
    assert_eq!(
        machine.sessions.names(),
        Vec::<String>::new(),
        "nothing was written to the sessions' keyring"
    );
    assert!(
        slot.drive.is_none()
            && slot.auto.is_none()
            && slot.azlin.is_none()
            && slot.objects.is_none(),
        "the slot keeps no handle of a drive that did not open"
    );
    // The control: the same slot with its own drive's session opens.
    let mut slot = Slot::new(azlin_entry("slot-1", DRIVE));
    slot.secret = Some(azcloud_kit::Zeroizing::new(session_text()));
    assert!(
        machine.open(&mut slot, &fake).is_ok(),
        "the entry's own session opens"
    );
}

// ==== The opening with a key, a wrong id and a record (X-DELTA-STRUCT4-3) ====
//
// The four tests above are refusals over an EMPTY keyring: a gate built under the wrong id, without its index provider
// or without the entry's record refuses them just the same, so swapping the two ids, dropping the provider and passing
// a default record all left them green. These open a drive that EXISTS - set up under its id, its key in this
// computer's keyring - and see what only the right wiring does: it opens, a file lands in the bucket as an AZL1 object
// and never in the clear, and the key id is told under the slot's name.

/// The file the tests of an open drive put: text that no sealed object may hold.
const PLAINTEXT: &[u8] = b"the plain text of a.txt: photos of the cat, summer 2026";

/// Whether `needle` is somewhere in `hay`.
fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|window| window == needle)
}

/// The slot `id` of the drive `drive_id` with the record the drives file holds of its encryption.
fn recorded_entry(id: &str, drive_id: &str, encrypted: bool, key_id: Option<&str>) -> DriveEntry {
    let mut entry = azlin_entry(id, drive_id);
    if let DriveLocation::S3 {
        auth:
            DriveAuth::Azlin {
                encrypted: recorded,
                key_id: recorded_key,
                ..
            },
        ..
    } = &mut entry.location
    {
        *recorded = encrypted;
        *recorded_key = key_id.map(str::to_string);
    }
    entry
}

/// A slot of `entry`, with the session of the drive `DRIVE`, opened over `fake` on `machine`.
fn open_entry(
    machine: &Machine,
    fake: &Arc<FakeS3>,
    entry: DriveEntry,
) -> (Slot, Result<Arc<dyn Drive>, DriveError>) {
    let mut slot = Slot::new(entry);
    slot.secret = Some(azcloud_kit::Zeroizing::new(session_text()));
    let opened = machine.open(&mut slot, fake);
    (slot, opened)
}

/// X-DELTA-STRUCT4-3. A drive SET UP under its id - its key files in the bucket, its key in this computer's keyring -
/// opens through `open_azlin_in`: a put is taken, it lands in the bucket as AZL1 objects and no object (a key, a name)
/// holds the text in the clear, the file reads back, and the key id the drive opened with is told under the SLOT's name
/// ("slot-1": what the UI thread writes it under), not under the drive's id. Fails when the gate is built under
/// another id than the account's (no key is found under the slot's name: the drive is locked), without its index
/// provider (a put is refused as unsupported) or with the key id told under the wrong name.
#[test]
fn x_delta_struct4_3_a_drive_set_up_under_its_id_opens_with_its_key_and_a_put_lands_as_azl1_never_in_the_clear(
) {
    use azul_storage::crypto::{device, keys::RecoveryKdf};

    let fake = FakeS3::with(&[]);
    let machine = Machine::new();
    let (key, _code) = device::setup_new_drive(
        fake.bucket(),
        machine.keys.as_ref(),
        DRIVE,
        RecoveryKdf::with_cost(64, 1, 1).unwrap(),
    )
    .expect("the drive is set up under its id");
    let (slot, opened) = open_entry(&machine, &fake, azlin_entry("slot-1", DRIVE));
    let drive = opened.expect("the slot opens");
    assert!(slot.auto.is_some(), "the slot keeps the gate");
    let before = fake.objects();
    drive
        .put("a.txt", PLAINTEXT)
        .expect("an encrypted drive that holds its key takes a file");
    assert_eq!(drive.get("a.txt").expect("the file reads back"), PLAINTEXT);
    let after = fake.objects();
    let new: Vec<(&String, &Vec<u8>)> = after
        .iter()
        .filter(|(name, _)| !before.contains_key(*name))
        .collect();
    assert!(
        new.iter().any(|(_, bytes)| bytes.starts_with(b"AZL1")),
        "the put lands as an AZL1 object: {:?}",
        new.iter().map(|(name, _)| name).collect::<Vec<_>>()
    );
    for (name, bytes) in &after {
        assert!(
            !contains(bytes, PLAINTEXT),
            "{name} holds the file's text in the clear"
        );
        assert!(!name.contains("a.txt"), "{name} names the file");
    }
    // The key it opened with is told for the UI thread under the SLOT's name.
    let told: Vec<(String, String)> = machine
        .records
        .lock()
        .unwrap()
        .iter()
        .map(|record| (record.drive_id.clone(), record.key_id.clone()))
        .collect();
    let expected = (String::from("slot-1"), key.id().to_hex());
    assert!(
        !told.is_empty() && told.iter().all(|record| *record == expected),
        "the key id is told under the slot's name: {told:?}"
    );
}

/// X-DELTA-STRUCT4-3. The SAME bucket - set up under `DRIVE` - with this computer's key under ANOTHER drive's id is a drive
/// whose key is missing: locked, a put is refused and the bucket is as it was. (A gate that found a key whatever it
/// was kept under would open it.)
#[test]
fn x_delta_struct4_3_the_same_bucket_with_the_key_under_another_id_is_locked() {
    use azul_storage::{
        crypto::{device, keys::RecoveryKdf, DriveKey},
        keyring::MemoryKeyring,
    };

    let fake = FakeS3::with(&[]);
    let machine = Machine::new();
    // Set up under its id, with the keys in a keyring of its own ...
    device::setup_new_drive(
        fake.bucket(),
        &MemoryKeyring::new(),
        DRIVE,
        RecoveryKdf::with_cost(64, 1, 1).unwrap(),
    )
    .expect("the drive is set up under its id");
    // ... and this computer holds a key under another drive's id only.
    device::store_drive_key(
        machine.keys.as_ref(),
        "d_other",
        &DriveKey::generate().unwrap(),
    )
    .unwrap();
    let before = fake.objects();
    let (_slot, opened) = open_entry(&machine, &fake, azlin_entry("slot-1", DRIVE));
    let drive = opened.expect("the slot opens: the drive is decided at its first call");
    match drive.put("a.txt", PLAINTEXT) {
        Err(DriveError::Locked { .. }) => {}
        other => panic!("a drive whose key is not this computer's took a file: {other:?}"),
    }
    assert_eq!(fake.objects(), before, "the bucket is as it was");
    assert!(
        machine.records.lock().unwrap().is_empty(),
        "no key id was told"
    );
}

/// X-DELTA-STRUCT4-3. A drive the drives file records as ENCRYPTED, whose key is gone from the keyring and whose bucket
/// hides its key files (it lists nothing), is locked - never read as a new drive. The record is the ENTRY's, read inside
/// the opener; one dropped on the way (`EncryptionRecord::default()`) read the same drive as new, and its first file went
/// in as a plain one's would. The control: the entry that records nothing is the new drive it looks like, refused with the
/// words of a new drive.
#[test]
fn x_delta_struct4_3_a_drive_recorded_as_encrypted_whose_key_is_gone_is_locked_not_new() {
    let fake = FakeS3::with(&[]);
    let machine = Machine::new();
    let recorded = recorded_entry(
        "slot-1",
        DRIVE,
        true,
        Some("00112233445566778899aabbccddeeff"),
    );
    let (_slot, opened) = open_entry(&machine, &fake, recorded);
    let drive = opened.expect("the slot opens");
    match drive.put("a.txt", PLAINTEXT) {
        Err(DriveError::Locked { .. }) => {}
        other => panic!("a drive recorded as encrypted, with no key, took a file: {other:?}"),
    }
    assert_eq!(fake.writes(), Vec::new(), "nothing was written");
    // The control: nothing recorded, nothing in the bucket: a new drive, refused as one.
    let (_slot, opened) = open_entry(&machine, &fake, azlin_entry("slot-2", DRIVE));
    let drive = opened.expect("the slot opens");
    match drive.put("a.txt", PLAINTEXT) {
        Err(DriveError::Denied { message }) => {
            assert!(message.contains("not encrypted"), "{message}");
        }
        other => panic!("a new drive took a plaintext file: {other:?}"),
    }
    assert_eq!(fake.writes(), Vec::new(), "nothing was written");
}
