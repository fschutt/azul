//! The sample account (`--sample`): the test server's sample mail filed as if it had been
//! synced, so AzMail can be tried (and screenshotted) without a mail account or a network.
//!
//! `ada@example.org` ("Ada Lovelace"), IMAP `localhost:1143` without TLS (Send / Receive
//! reaches a server only when one runs there: `scripts/imap_server.py`), sending through an SMTP
//! server on `localhost:2525` without STARTTLS (`scripts/azmail_smtp_sink.py`). The mail is
//! `scripts/sample_mail/` (built into the binary): a reply with three quote levels, an HTML
//! newsletter with pictures, an attachment, a Latin-1 message, Sent, a phishing mail in Junk,
//! a draft and a nested folder. Nothing is written when the account is there already.

use std::path::{Path, PathBuf};

use crate::{
    account::{self, Account, AuthKind, Security, Server},
    message,
    send::{SendRoute, SendSettings, TlsPolicy},
    store::{self, FolderState, IndexEntry, LocalFolder},
};

/// The sample account's address.
pub const SAMPLE_EMAIL: &str = "ada@example.org";

/// `(folder key, server name, display, the messages)`.
const MAIL: &[(&str, &str, &str, &[&[u8]])] = &[
    (
        "inbox",
        "INBOX",
        "Inbox",
        &[
            include_bytes!("../scripts/sample_mail/INBOX/0001-plain-reply.eml"),
            include_bytes!("../scripts/sample_mail/INBOX/0002-newsletter.eml"),
            include_bytes!("../scripts/sample_mail/INBOX/0003-attachment.eml"),
            include_bytes!("../scripts/sample_mail/INBOX/0004-latin1.eml"),
        ],
    ),
    (
        "sent",
        "Sent",
        "Sent",
        &[include_bytes!("../scripts/sample_mail/Sent/0001-garden-plan.eml")],
    ),
    (
        "spam",
        "Spam",
        "Spam",
        &[include_bytes!("../scripts/sample_mail/Spam/0001-phishing.eml")],
    ),
    (
        "drafts",
        "Drafts",
        "Drafts",
        &[include_bytes!("../scripts/sample_mail/Entwürfe/0001-draft.eml")],
    ),
    ("Work", "Work", "Work", &[]),
    (
        "Work.Projects",
        "Work/Projects",
        "Work/Projects",
        &[include_bytes!("../scripts/sample_mail/Work/Projects/0001-kickoff.eml")],
    ),
];

/// The sample account.
pub fn sample_account() -> Account {
    Account {
        id: String::from(SAMPLE_EMAIL),
        email: String::from(SAMPLE_EMAIL),
        name: String::from("Ada Lovelace"),
        username: String::from(SAMPLE_EMAIL),
        imap: Server {
            host: String::from("localhost"),
            port: 1143,
        },
        smtp: Server {
            host: String::from("localhost"),
            port: 2525,
        },
        security: Security::Plain,
        auth: AuthKind::Password,
        folder: None,
    }
}

/// Writes the sample account and its mail under the AzMail folder `root` (nothing when the
/// account is there already). Returns the account's folder.
pub fn install(root: &Path) -> std::io::Result<PathBuf> {
    let account = sample_account();
    let dir = account::account_dir(root, &account.id);
    if dir.join(account::ACCOUNT_FILE).is_file() {
        return Ok(dir);
    }
    let store = LocalFolder::new(account::mail_root(root, &account));
    for (key, server_name, display, messages) in MAIL {
        file_folder(&store, key, server_name, display, messages)?;
    }
    account::save(root, &account)?;
    let settings = SendSettings {
        route: SendRoute::Smtp {
            host: String::from("localhost"),
            port: 2525,
        },
        tls: TlsPolicy::Off,
        ..SendSettings::default()
    };
    settings.save(root, &account.id)?;
    Ok(dir)
}

/// One folder: every message under UIDs 1.., its index and its state.
fn file_folder(
    store: &LocalFolder,
    key: &str,
    server_name: &str,
    display: &str,
    messages: &[&[u8]],
) -> std::io::Result<()> {
    let mut index: Vec<IndexEntry> = Vec::with_capacity(messages.len());
    let flags = if key == "inbox" {
        Vec::new()
    } else {
        vec![String::from("\\Seen")]
    };
    for (i, bytes) in messages.iter().enumerate() {
        let uid = i as u32 + 1;
        // Filed under the month of its Date header (the server's arrival time would be used
        // by a sync).
        let date = message::parse_view(bytes)
            .map(|v| v.date)
            .unwrap_or_default();
        let secs = chrono::DateTime::parse_from_rfc3339(&date)
            .map(|d| d.timestamp())
            .unwrap_or(0);
        let (year, month) = message::year_month(secs);
        let path = store::message_key(key, year, month, uid);
        store.put(&path, bytes, true)?;
        index.push(message::index_entry(uid, bytes, &flags, Some(secs), &path));
    }
    store.put(
        &store::index_key(key),
        store::index_to_jsonl(&index).as_bytes(),
        true,
    )?;
    let mut state = FolderState::create(server_name, display, 1);
    state.last_uid = messages.len() as u32;
    state.messages = messages.len() as u64;
    store.put(&store::state_key(key), state.to_json().as_bytes(), true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn the_sample_account_is_filed_like_a_synced_one_once() {
        let dir = TempDir::new("sample");
        let at = install(&dir.0).unwrap();
        assert_eq!(at, dir.0.join(SAMPLE_EMAIL));
        let (accounts, skipped) = account::load_all(&dir.0);
        assert!(skipped.is_empty());
        assert_eq!(accounts, vec![sample_account()]);
        let store = LocalFolder::new(at.clone());
        assert_eq!(
            store.folders(),
            vec!["Work", "Work.Projects", "drafts", "inbox", "sent", "spam"]
        );
        let inbox = store::index_from_jsonl(
            &String::from_utf8(store.get(&store::index_key("inbox")).unwrap()).unwrap(),
        );
        assert_eq!(inbox.len(), 4);
        assert!(inbox.iter().any(|e| e.subject == "Garden Weekly: bulbs, frost and a sale"));
        assert!(inbox.iter().all(|e| e.flags.is_empty()), "the inbox is unread");
        let settings = SendSettings::load(&dir.0, SAMPLE_EMAIL);
        assert_eq!(settings.tls, TlsPolicy::Off);
        // A second run leaves it alone.
        std::fs::write(at.join("marker"), b"x").unwrap();
        install(&dir.0).unwrap();
        assert!(at.join("marker").is_file());
    }
}
