//! X-DELTA-APPS4-2: the link between the Send/Receive of an Azlin drive and the token server it asks.
//! `azlin_sync`'s tests hand the sync an asker of their own, and `azlin_restore_now`'s are of the kit's
//! and mail-core's own calls: nothing entered the one function the app's sync thread calls
//! (`sync_drive`, past the part that opens the drive: `sync_opened`), so a later edit that made its
//! asker `&|| Ok(None)` - or that mapped a failed ask to "none" - kept every test green and the MAJOR
//! X-DELTA-APPS3-2 closed was open again: the snapshot of the last sign-in decided alone. These enter
//! there, over the drives AzMail syncs (a folder on disk, the encrypted drive over a bucket in memory)
//! and a transport that counts the token server's requests.

use std::sync::{Arc, Mutex};

use azul_storage::{
    keyring::{KeyringNamespace, KeyringStore, MemoryKeyring},
    Drive, HttpCall, HttpReply, LocalDrive, Method, Transport,
};

use crate::{
    account::{Account, AuthKind, AzlinLink, Secret, Security, Server},
    azlin::{self, AzlinSession},
    store::MailStore,
    sync::{SyncError, SyncReport},
    sync_opened,
    testutil::{MailFolder, TempDir},
    SyncInit, ThreadId,
};

/// 2026-09-30T08:42:00Z
const NOW: i64 = 1_790_757_720;
const DRIVE: &str = "d_42";
const TOKEN_SERVER: &str = "http://127.0.0.1:8081";

/// The token server: every request is recorded, the answers are the ones queued (an empty queue is a
/// connection that does not answer).
struct Tokens {
    calls: Mutex<Vec<HttpCall>>,
    answers: Mutex<Vec<HttpReply>>,
}

impl Tokens {
    fn answering(answers: Vec<HttpReply>) -> Tokens {
        Tokens {
            calls: Mutex::new(Vec::new()),
            answers: Mutex::new(answers),
        }
    }

    fn asked(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

impl Transport for Tokens {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        self.calls.lock().unwrap().push(call.clone());
        let mut answers = self.answers.lock().unwrap();
        if answers.is_empty() {
            return Err(String::from("the token server did not answer"));
        }
        Ok(answers.remove(0))
    }
}

fn reply(status: u16, body: &str) -> HttpReply {
    HttpReply {
        status,
        headers: Vec::new(),
        body: body.as_bytes().to_vec(),
    }
}

fn mail(n: u32) -> Vec<u8> {
    format!(
        "Message-ID: <m{n}@example.org>\r\nDate: Wed, 30 Sep 2026 10:42:00 +0200\r\n\
         From: Ben Okafor <ben@example.org>\r\nTo: ada@example.org\r\nSubject: Mail {n}\r\n\
         \r\nBody {n}\r\n"
    )
    .into_bytes()
}

/// The job of the app's sync thread for an Azlin account whose token server is `TOKEN_SERVER`, the
/// keyring holding the newest drive token (the session this process holds is an older one), and this
/// computer's copy in `local`; the locks of the keyring are in `locks`, never in the user's folder.
fn job(local: &TempDir, locks: &TempDir) -> (SyncInit, AzlinSession) {
    let keyring = Arc::new(MemoryKeyring::new());
    let account = Account {
        id: String::from("ada@example.org"),
        email: String::from("ada@example.org"),
        name: String::from("Ada Lovelace"),
        username: String::from("ada@example.org"),
        imap: Server {
            host: String::new(),
            port: 0,
        },
        smtp: Server {
            host: String::new(),
            port: 0,
        },
        security: Security::Tls,
        auth: AuthKind::Password,
        folder: None,
        azlin: Some(AzlinLink {
            token_url: String::from(TOKEN_SERVER),
            drive_id: String::from(DRIVE),
        }),
    };
    keyring
        .set(
            &crate::account::azlin_keyring_key(&account.id),
            &AzlinSession::with_token(DRIVE, "dt_f.7.newest").to_secret(),
        )
        .unwrap();
    let job = SyncInit {
        thread: ThreadId::unique(),
        account,
        secret: Secret::new(String::new()),
        mail_root: local.folder(),
        azmail_root: local.folder(),
        extra_ca: None,
        dkim_key: None,
        endpoints: azlin::Endpoints::default(),
        keyring: azcloud_kit::SharedKeyring::with_lock_root(
            keyring,
            KeyringNamespace::azlin_shared(),
            &locks.0,
        ),
        store_first: false,
    };
    (job, AzlinSession::with_token(DRIVE, "dt_f.6.older"))
}

/// The drives AzMail syncs: a folder on disk, and the encrypted drive over a bucket in memory (what
/// `mail_drive::open` makes of an account's drive). What keeps them alive is the last of each.
fn drives() -> Vec<(&'static str, Arc<dyn Drive>, Box<dyn std::any::Any>)> {
    let bucket_dir = TempDir::new("sync-drive-bucket");
    let folder: Arc<dyn Drive> = Arc::new(LocalDrive::without_manifest(bucket_dir.0.clone()));
    #[allow(unused_mut)]
    let mut kinds: Vec<(&'static str, Arc<dyn Drive>, Box<dyn std::any::Any>)> =
        vec![("a folder", folder, Box::new(bucket_dir))];
    #[cfg(feature = "encryption")]
    {
        use azul_storage::{
            crypto::{device, keys::RecoveryKdf},
            meta::{MemoryBucket, RestoreSource},
        };
        let keys: Arc<dyn KeyringStore> = Arc::new(MemoryKeyring::new());
        let bucket = Arc::new(MemoryBucket::new());
        let cheap = RecoveryKdf::with_cost(64, 1, 1).unwrap();
        device::setup_new_drive(bucket.as_ref(), keys.as_ref(), DRIVE, cheap).unwrap();
        let encrypted: Arc<dyn Drive> = crate::mail_drive::wrap_auto(
            bucket,
            DRIVE,
            keys,
            RestoreSource::none("a test has no token server to ask"),
        );
        kinds.push(("the encrypted drive", encrypted, Box::new(())));
    }
    kinds
}

fn deliver(drive: &dyn Drive, n: u32) -> String {
    let key = azlin::message_key("Inbox", &azlin::object_name(&mail(n), NOW as u64 + u64::from(n)));
    drive.put(&key, &mail(n)).unwrap();
    key
}

fn sync(
    job: &SyncInit,
    session: &AzlinSession,
    drive: &dyn Drive,
    tokens: &Tokens,
    now: i64,
) -> Result<SyncReport, SyncError> {
    sync_opened(job, session, drive, tokens, now, &mut |_| true)
}

/// RED on the code before (the link was unpinned; the mutant `&|| Ok(None)` of the asker kept all 36
/// `azlin_sync` tests green): the sync of the app asks the token server - ONE `GET /v1/drives/{id}`
/// with the newest drive token - when a line of a folder is no longer in the drive, and no request at
/// all when nothing is gone; an answer of "no restore" lets the ordinary pull hold the mail, and a
/// failed ask is an unknown: the mail stays, nothing is held, and the report says why.
#[test]
fn x_delta_apps4_2_the_sync_of_an_open_drive_asks_the_token_server_once_when_mail_is_gone_and_never_otherwise(
) {
    for (kind, drive, _alive) in drives() {
        let local = TempDir::new("sync-drive-local");
        let locks = TempDir::new("sync-drive-locks");
        let (job, session) = job(&local, &locks);
        let one = deliver(&*drive, 1);
        let two = deliver(&*drive, 2);
        deliver(&*drive, 3);
        let tokens = Tokens::answering(vec![
            reply(200, &format!(r#"{{"id": "{DRIVE}", "restore": null}}"#)),
            // 403 is a refusal that asking again does not change (a 5xx would be asked again, after a pause).
            reply(403, "{}"),
        ]);

        // Nothing is gone: nothing is asked.
        let report = sync(&job, &session, &*drive, &tokens, NOW).unwrap();
        assert_eq!(report.fetched(), 3, "{kind}: {report:?}");
        assert_eq!(
            tokens.asked(),
            0,
            "{kind}: nothing is gone: the token server is not asked"
        );
        let again = sync(&job, &session, &*drive, &tokens, NOW + 10).unwrap();
        assert_eq!(
            tokens.asked(),
            0,
            "{kind}: a second Send/Receive with nothing gone asks nothing: {again:?}"
        );

        // Mail is gone, and the token server says the drive was not restored: ONE request, with the
        // newest token the keyring holds (not the older one this process holds); the pull holds the mail.
        drive.delete(&one).unwrap();
        let report = sync(&job, &session, &*drive, &tokens, NOW + 60).unwrap();
        assert_eq!(tokens.asked(), 1, "{kind}: one question");
        {
            let calls = tokens.calls.lock().unwrap();
            assert_eq!(calls[0].method, Method::Get, "{kind}");
            // Which token server it is depends on the build (a shipped build names the live one, whatever
            // the account says; a test build the account's): what is pinned is the question and its token.
            assert!(
                calls[0].url.ends_with(&format!("/v1/drives/{DRIVE}")),
                "{kind}: {}",
                calls[0].url
            );
            let auth = calls[0]
                .headers
                .iter()
                .find(|(name, _)| name == "authorization")
                .map(|(_, value)| value.as_str());
            assert_eq!(auth, Some("Bearer dt_f.7.newest"), "{kind}");
        }
        assert_eq!(
            report.removed(),
            1,
            "{kind}: an ordinary pull: the mail is held: {report:?}"
        );
        assert_eq!(report.kept_unasked(), 0, "{kind}");
        assert_eq!(report.restore_unknown, None, "{kind}");

        // Mail is gone and the token server refuses: an unknown - the mail stays, nothing is held,
        // and the report says why.
        drive.delete(&two).unwrap();
        let report = sync(&job, &session, &*drive, &tokens, NOW + 120).unwrap();
        assert_eq!(tokens.asked(), 2, "{kind}: asked again, once");
        assert_eq!(
            (report.removed(), report.kept_unasked()),
            (0, 1),
            "{kind}: {report:?}"
        );
        assert!(report.restore_unknown.is_some(), "{kind}: {report:?}");
        let store = MailStore::new(local.folder());
        let lines = crate::read_index(&store, "inbox");
        assert_eq!(
            lines
                .iter()
                .map(|line| line.subject.as_str())
                .collect::<Vec<_>>(),
            ["Mail 2", "Mail 3"],
            "{kind}: the mail the drive no longer lists stays: only the first went, held"
        );
    }
}

/// X-DELTA-APPS4-4 where the app's sync thread enters it, on the drives AzMail syncs: mail another device
/// MOVED to another folder is no mail "removed from the drive" - its line follows its object before any
/// folder is pulled, so nothing is held, nothing is fetched again and the token server is not asked (there is
/// nothing gone to ask about). RED on the code before: both archived messages were held under `removed/`
/// for 90 days and fetched again as new mail.
#[test]
fn x_delta_apps4_4_mail_another_device_moved_follows_its_object_on_the_drives_the_app_syncs() {
    for (kind, drive, _alive) in drives() {
        let local = TempDir::new("sync-drive-local");
        let locks = TempDir::new("sync-drive-locks");
        let (job, session) = job(&local, &locks);
        let keys: Vec<String> = (1..=3).map(|n| deliver(&*drive, n)).collect();
        let tokens = Tokens::answering(Vec::new());
        let report = sync(&job, &session, &*drive, &tokens, NOW).unwrap();
        assert_eq!(report.fetched(), 3, "{kind}: {report:?}");
        // Another device archives two of them: the same name in another folder, the old object deleted.
        for key in &keys[..2] {
            let name = key.rsplit('/').next().unwrap();
            drive
                .copy(key, &azlin::message_key("Archive", name))
                .unwrap();
            drive.delete(key).unwrap();
        }
        let report = sync(&job, &session, &*drive, &tokens, NOW + 60).unwrap();
        assert_eq!(
            (report.removed(), report.fetched()),
            (0, 0),
            "{kind}: moved is neither removed nor new: {report:?}"
        );
        assert_eq!(
            tokens.asked(),
            0,
            "{kind}: nothing is gone, so the token server is not asked"
        );
        let store = MailStore::new(local.folder());
        let subjects = |folder: &str| -> Vec<String> {
            crate::read_index(&store, folder)
                .iter()
                .map(|line| line.subject.clone())
                .collect()
        };
        assert_eq!(subjects("inbox"), ["Mail 3"], "{kind}");
        assert_eq!(subjects("archive"), ["Mail 1", "Mail 2"], "{kind}");
        assert!(
            store.keys(crate::store::REMOVED_PREFIX).is_empty(),
            "{kind}: nothing is held: {:?}",
            store.keys(crate::store::REMOVED_PREFIX)
        );
    }
}
