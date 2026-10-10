//! Where the submission port's mail goes: AzMail's own sending path (azul-mail-core's
//! `send::send_prepared`) - the outbox, a DKIM signature per attempt, direct delivery to the
//! recipients' mail servers or the account's relay, the per-domain policy list - and the copy
//! that went out into the drive's Sent mailbox.
//!
//! The bridge keeps a spool of its own, laid out as an AzMail folder with one account (never
//! AzMail's own folder: two processes must not write one outbox or one index). A mail goes
//! out of the spool's outbox; when it went, AzMail's path files it in the spool's local Sent,
//! and [`AzMailSubmitter::drain`] moves every copy from there into the drive's Sent mailbox
//! (read, under its Azlin name) - right after the attempt, and after every retry of a queued
//! mail ([`AzMailSubmitter::retry`]). A copy filed in the drive is remembered by Message-ID,
//! so the mail program's own APPEND of it to Sent is not filed a second time.
//!
//! The sending settings are the account's `sending.json` (AzMail's format: the route, TLS,
//! DKIM); the DKIM private key and a submission password are read from the bridge's secret
//! store under AzMail's keyring names.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use azmail_core::{
    account::{self, Secret},
    folders::Role,
    send::{self, OutboxEntry, PreparedMail, RecipientProgress, SendRoute, SendSettings, SendStatus},
    store::{self as local, DriveFolder},
};
use azul_storage::time::parse_iso8601;

use crate::{
    bounce::{self, Failure},
    secrets::{self, KeyringStore},
    sent::{self, SentRegistry},
    smtp::{Submission, Submitter, Verdict},
    store::{MailStore, Marks},
};

/// The local Sent folder AzMail's path files a sent copy in (`mail/sent/...`).
fn local_sent() -> &'static str {
    Role::Sent.key().unwrap_or("sent")
}

/// Hands submitted mail to AzMail's sending path and files what went out in the drive.
pub struct AzMailSubmitter {
    spool: DriveFolder,
    account_id: String,
    settings_file: Option<PathBuf>,
    secrets: Arc<dyn KeyringStore>,
    store: Arc<dyn MailStore>,
    sent: Arc<SentRegistry>,
    drain_lock: Mutex<()>,
}

impl std::fmt::Debug for AzMailSubmitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AzMailSubmitter")
            .field("spool", &self.spool)
            .field("account_id", &self.account_id)
            .finish_non_exhaustive()
    }
}

impl AzMailSubmitter {
    /// A submitter whose spool is the folder `spool` (made when needed), sending as the
    /// account `account_id` (the name of its keyring entries), with the settings of
    /// `settings_file` (else the spool account's `sending.json`, else AzMail's defaults:
    /// direct delivery).
    #[must_use]
    pub fn new(
        spool: PathBuf,
        account_id: &str,
        settings_file: Option<PathBuf>,
        secrets: Arc<dyn KeyringStore>,
        store: Arc<dyn MailStore>,
        sent: Arc<SentRegistry>,
    ) -> AzMailSubmitter {
        AzMailSubmitter {
            spool: DriveFolder::outside(spool),
            account_id: account_id.to_string(),
            settings_file,
            secrets,
            store,
            sent,
            drain_lock: Mutex::new(()),
        }
    }

    /// The sending settings now, with the secrets they need in memory.
    #[must_use]
    pub fn settings(&self) -> SendSettings {
        let mut settings = match &self.settings_file {
            Some(path) => std::fs::read(path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<SendSettings>(&bytes).ok())
                .unwrap_or_default(),
            None => SendSettings::load(&self.spool, &self.account_id),
        };
        let wants_key = settings
            .dkim
            .as_ref()
            .is_some_and(|dkim| dkim.key_file.is_none());
        if wants_key {
            settings.dkim_key = self
                .secrets
                .get(&secrets::dkim_entry(&self.account_id))
                .ok()
                .flatten()
                .map(Secret::new);
        }
        if settings.route == SendRoute::Submission {
            settings.sign_in = self
                .secrets
                .get(&secrets::sign_in_entry(&self.account_id))
                .ok()
                .flatten()
                .map(Secret::new);
        }
        settings
    }

    /// Moves every copy AzMail's path filed in the spool's Sent into the drive's Sent mailbox
    /// (read, oldest first) and forgets it in the spool; how many moved. A copy the drive
    /// refuses stays for the next drain.
    pub fn drain(&self) -> usize {
        let _one_at_a_time = self
            .drain_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let folder = account::account_dir(&self.spool, &self.account_id);
        let spool = local::MailStore::new(folder);
        let index_key = local::index_key(local_sent());
        let Ok(text) = spool.get(&index_key) else {
            return 0;
        };
        let entries = local::index_from_jsonl(&String::from_utf8_lossy(&text));
        if entries.is_empty() {
            return 0;
        }
        let Some(sent_box) = self
            .store
            .mailboxes()
            .ok()
            .and_then(|boxes| boxes.into_iter().find(|b| b.role == Role::Sent))
        else {
            return 0;
        };
        let mut kept = Vec::new();
        let mut moved = 0;
        for entry in entries {
            let Ok(bytes) = spool.get(&entry.path) else {
                // The file is gone: so is the entry.
                continue;
            };
            let arrived = parse_iso8601(&entry.date).unwrap_or_else(crate::store::now);
            let marks = Marks {
                seen: true,
                ..Marks::default()
            };
            match self.store.append(&sent_box.path, &bytes, arrived, marks) {
                Ok(stored) => {
                    if let Some(id) = sent::message_id_of(&bytes) {
                        self.sent.remember(&id, &sent_box.path, &stored.name);
                    }
                    let _ = spool.delete(&entry.path);
                    moved += 1;
                }
                Err(_) => kept.push(entry),
            }
        }
        let _ = spool.put(&index_key, local::index_to_jsonl(&kept).as_bytes());
        moved
    }

    /// Tries the queued mail whose time has come again, then files what went out.
    /// Tries the queued mail whose time has come again, files what went out, and puts a
    /// report into the Inbox for every recipient that will never get a mail now (the mail
    /// program had its 250 long ago).
    pub fn retry(&self) {
        let settings = self.settings();
        let spool = local::MailStore::new(account::account_dir(&self.spool, &self.account_id));
        let before: HashMap<String, (OutboxEntry, Vec<u8>)> =
            send::outbox_entries(&self.spool, &self.account_id)
                .into_iter()
                .map(|entry| {
                    let eml = spool
                        .get(&format!("{}/{}.eml", send::OUTBOX_DIR, entry.id))
                        .unwrap_or_default();
                    (entry.id.clone(), (entry, eml))
                })
                .collect();
        let results = send::retry_outbox(&self.spool, &self.account_id, &settings, false);
        let after: HashMap<String, OutboxEntry> = send::outbox_entries(&self.spool, &self.account_id)
            .into_iter()
            .map(|entry| (entry.id.clone(), entry))
            .collect();
        self.drain();
        for (id, status) in results {
            let SendStatus::Failed { reason } = status else {
                continue;
            };
            let Some((old, eml)) = before.get(&id) else {
                continue;
            };
            let failed_before: HashSet<&str> = old
                .recipients
                .iter()
                .filter(|r| r.state == RecipientProgress::Failed)
                .map(|r| r.address.as_str())
                .collect();
            let failures: Vec<Failure> = match after.get(&id) {
                // Still in the outbox: its recipients say who failed this time.
                Some(entry) => entry
                    .recipients
                    .iter()
                    .filter(|r| {
                        r.state == RecipientProgress::Failed
                            && !failed_before.contains(r.address.as_str())
                    })
                    .map(|r| Failure {
                        address: r.address.clone(),
                        code: r.code,
                        reason: r.reason.clone(),
                    })
                    .collect(),
                // Gone into Sent (some got it): the status names the ones who did not.
                None => old
                    .recipients
                    .iter()
                    .filter(|r| r.state == RecipientProgress::Pending && reason.contains(&r.address))
                    .map(|r| Failure {
                        address: r.address.clone(),
                        code: None,
                        reason: reason.clone(),
                    })
                    .collect(),
            };
            if failures.is_empty() {
                continue;
            }
            let others = after.get(&id).map_or(true, |entry| {
                entry
                    .recipients
                    .iter()
                    .any(|r| r.state == RecipientProgress::Sent)
            });
            self.file_report(&old.from, eml, &failures, others);
        }
    }

    /// Puts a delivery report into the drive's Inbox (unread) for the mail `original` of
    /// `sender`; never sends it anywhere.
    fn file_report(&self, sender: &str, original: &[u8], failures: &[Failure], others_got_it: bool) {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let now = crate::store::now();
        let token = format!("{now:x}{:x}", COUNTER.fetch_add(1, Ordering::SeqCst));
        let bytes = bounce::report(
            sender,
            original,
            failures,
            others_got_it,
            i64::try_from(now).unwrap_or(0),
            &token,
        );
        let inbox = self
            .store
            .mailboxes()
            .ok()
            .and_then(|boxes| boxes.into_iter().find(|b| b.role == Role::Inbox))
            .map_or_else(|| String::from("Inbox"), |b| b.path);
        let _ = self.store.append(&inbox, &bytes, now, Marks::default());
    }
}

impl Submitter for AzMailSubmitter {
    fn submit(&self, submission: &Submission) -> Verdict {
        let settings = self.settings();
        let mail = PreparedMail {
            from: submission.from.clone(),
            recipients: submission.recipients.clone(),
            bytes: submission.message.clone(),
        };
        let outcome = send::send_prepared(&self.spool, &self.account_id, &settings, &mail);
        self.drain();
        // Taken with failures (some got it, or some still wait): the sender learns who will
        // never get it from a report in the Inbox, as from any mail server.
        let taken = outcome.delivered > 0 || outcome.pending > 0;
        if taken && !outcome.failures.is_empty() {
            let failures: Vec<Failure> = outcome
                .failures
                .iter()
                .map(|r| Failure {
                    address: r.address.clone(),
                    code: r.code,
                    reason: r.reason.clone(),
                })
                .collect();
            self.file_report(&submission.from, &submission.message, &failures, outcome.delivered > 0);
        }
        match outcome.status {
            SendStatus::Sent { .. } => Verdict::Accepted(String::from("Sent")),
            SendStatus::Queued { reason } => {
                Verdict::Accepted(format!("Queued, tried again later: {reason}"))
            }
            // Some recipients got it, or it waits for some: the mail is not refused (the
            // program must not send it again to the ones who have it).
            SendStatus::Failed { reason } if outcome.delivered > 0 || outcome.pending > 0 => {
                Verdict::Accepted(format!("Partly delivered: {reason}"))
            }
            SendStatus::Failed { reason } => Verdict::Refused(reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use azmail_core::{
        send::TlsPolicy,
        testutil::{spawn_smtp_sink, SinkScript},
    };
    use azul_storage::{keyring::MemoryKeyring, testing::TempDir, Drive};

    use super::*;
    use crate::{memory::MemoryDrive, store::DriveMailStore};

    const ACCOUNT: &str = "ada@example.org";
    const MESSAGE: &str = "From: Ada <ada@example.org>\r\nTo: ben@example.net\r\n\
        Subject: From the bridge\r\nMessage-ID: <b1@example.org>\r\n\
        Date: Thu, 01 Oct 2026 08:30:00 +0000\r\n\r\nHello Ben.\r\n";

    /// A submitter whose route is the sink on `port` (no TLS), over a drive in memory.
    fn submitter(dir: &TempDir, port: u16) -> (AzMailSubmitter, Arc<MemoryDrive>, Arc<SentRegistry>) {
        let settings = SendSettings {
            route: SendRoute::Smtp {
                host: String::from("127.0.0.1"),
                port,
            },
            tls: TlsPolicy::Off,
            ..SendSettings::default()
        };
        let file = dir.0.join("sending.json");
        std::fs::write(&file, serde_json::to_vec(&settings).unwrap()).unwrap();
        let drive = Arc::new(MemoryDrive::new());
        let registry = Arc::new(SentRegistry::new());
        let submitter = AzMailSubmitter::new(
            dir.0.join("spool"),
            ACCOUNT,
            Some(file),
            Arc::new(MemoryKeyring::new()),
            Arc::new(DriveMailStore::new(drive.clone())),
            registry.clone(),
        );
        (submitter, drive, registry)
    }

    fn submission() -> Submission {
        Submission {
            from: String::from(ACCOUNT),
            recipients: vec![String::from("ben@example.net"), String::from("bcc@example.com")],
            message: MESSAGE.as_bytes().to_vec(),
        }
    }

    #[test]
    fn a_submitted_mail_goes_out_through_azmails_path_and_its_copy_into_the_drives_sent() {
        let dir = TempDir::new("bridge-send");
        let (port, sessions) = spawn_smtp_sink(SinkScript::default());
        let (submitter, drive, registry) = submitter(&dir, port);
        let verdict = submitter.submit(&submission());
        assert_eq!(verdict, Verdict::Accepted(String::from("Sent")));
        let session = sessions
            .recv_timeout(Duration::from_secs(30))
            .expect("the sink saw a session");
        assert!(session.commands.iter().any(|c| c == "RCPT TO:<bcc@example.com>"));
        assert_eq!(session.message, MESSAGE);
        let sent: Vec<String> = drive
            .keys()
            .into_iter()
            .filter(|k| k.starts_with("mail/Sent/"))
            .collect();
        assert_eq!(sent.len(), 1, "{:?}", drive.keys());
        assert_eq!(drive.get(&sent[0]).unwrap(), MESSAGE.as_bytes());
        let id = azmail_core::azlin::message_id(&sent[0]).unwrap();
        assert!(drive.keys().contains(&format!("mail/.state/{id}/seen")));
        assert!(registry.find("b1@example.org").is_some());
        // Nothing stays in the spool: the outbox and its Sent are empty.
        assert_eq!(submitter.drain(), 0);
        let outbox = dir.0.join("spool").join(ACCOUNT).join("outbox");
        let left = std::fs::read_dir(outbox).map(|d| d.count()).unwrap_or(0);
        assert_eq!(left, 0);
    }

    #[test]
    fn a_mail_some_recipients_never_get_leaves_a_report_in_the_inbox() {
        let dir = TempDir::new("bridge-send");
        let script = SinkScript {
            rcpt: vec![(String::from("bcc@example.com"), String::from("550 5.1.1 no such user"))],
            ..SinkScript::default()
        };
        let (port, _sessions) = spawn_smtp_sink(script);
        let (submitter, drive, _) = submitter(&dir, port);
        let verdict = submitter.submit(&submission());
        assert!(matches!(&verdict, Verdict::Accepted(text) if text.starts_with("Partly")), "{verdict:?}");
        assert_eq!(drive.keys().iter().filter(|k| k.starts_with("mail/Sent/")).count(), 1);
        let inbox: Vec<String> = drive
            .keys()
            .into_iter()
            .filter(|k| k.starts_with("mail/Inbox/"))
            .collect();
        assert_eq!(inbox.len(), 1, "{:?}", drive.keys());
        let report = String::from_utf8(drive.get(&inbox[0]).unwrap()).unwrap();
        assert!(report.contains("Subject: Undelivered mail: From the bridge\r\n"), "{report}");
        assert!(report.contains("Final-Recipient: rfc822; bcc@example.com\r\n"), "{report}");
        assert!(report.contains("Status: 5.1.1\r\n"), "{report}");
        assert!(report.contains("The other recipients got it."), "{report}");
        assert!(!report.contains("Final-Recipient: rfc822; ben@example.net"), "{report}");
        // Unread: no seen marker for it.
        let id = azmail_core::azlin::message_id(&inbox[0]).unwrap();
        assert!(!drive.keys().contains(&format!("mail/.state/{id}/seen")));
    }

    #[test]
    fn a_mail_nobody_takes_is_refused_and_not_filed() {
        let dir = TempDir::new("bridge-send");
        let script = SinkScript {
            rcpt: vec![
                (String::from("ben@example.net"), String::from("550 5.1.1 no such user")),
                (String::from("bcc@example.com"), String::from("550 5.1.1 no such user")),
            ],
            ..SinkScript::default()
        };
        let (port, _sessions) = spawn_smtp_sink(script);
        let (submitter, drive, _) = submitter(&dir, port);
        let verdict = submitter.submit(&submission());
        assert!(matches!(verdict, Verdict::Refused(_)), "{verdict:?}");
        assert!(!drive.keys().iter().any(|k| k.starts_with("mail/Sent/")));
    }

    #[test]
    fn the_dkim_key_comes_from_the_secret_store_under_azmails_name() {
        let dir = TempDir::new("bridge-send");
        let settings = SendSettings {
            dkim: Some(send::DkimSettings {
                domain: String::from("example.org"),
                selector: String::from("s1"),
                key_file: None,
                public_key: String::new(),
            }),
            ..SendSettings::default()
        };
        let file = dir.0.join("sending.json");
        std::fs::write(&file, serde_json::to_vec(&settings).unwrap()).unwrap();
        let store = Arc::new(MemoryKeyring::new());
        store
            .set(&secrets::dkim_entry(ACCOUNT), "-----BEGIN PRIVATE KEY-----")
            .unwrap();
        let submitter = AzMailSubmitter::new(
            dir.0.join("spool"),
            ACCOUNT,
            Some(file),
            store,
            Arc::new(DriveMailStore::new(Arc::new(MemoryDrive::new()))),
            Arc::new(SentRegistry::new()),
        );
        let loaded = submitter.settings();
        assert_eq!(
            loaded.dkim_key.as_ref().map(Secret::expose),
            Some("-----BEGIN PRIVATE KEY-----")
        );
        assert!(loaded.sign_in.is_none());
    }
}
