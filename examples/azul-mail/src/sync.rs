//! Syncing an account's folders to files: which messages are new, fetching them in batches,
//! and writing them so that a second run fetches nothing twice and a crash loses nothing.
//!
//! For every selectable folder (see `folders::local_mailboxes`):
//!
//! 1. `SELECT` it. When its UIDVALIDITY is not the one in `state.json`, the server renumbered
//!    it: the folder's old files move to `stale/<folder>/<old uidvalidity>/` and it is synced
//!    from the start. When UIDNEXT says nothing arrived since the last UID, that is all.
//! 2. `UID SEARCH UID <last+1>:*` - and only UIDs above the last one count, because `n:*`
//!    always matches the newest message, even when `n` is above every UID.
//! 3. `UID FETCH <set> (UID FLAGS RFC822.SIZE INTERNALDATE)` for the new ones: the month a
//!    message arrived names its file, so the path is known before its body is fetched.
//! 4. In UID order, in batches (so many messages, so many bytes): a message whose file already
//!    has its size (written before a crash) is read back, not fetched; the rest come with one
//!    `UID FETCH <set> (UID BODY.PEEK[])` (PEEK: syncing marks nothing as read). Each body is
//!    written whole to `mail/<folder>/<yyyy>/<mm>/<uid>.eml`.
//! 5. Every so often, and at the end, `index.jsonl` and then `state.json` (with the last UID of
//!    the batches written) are written whole. The state never names a UID the index does not
//!    have, and the index keeps one line per UID, so a run that stops anywhere is picked up by
//!    the next one.

use crate::{
    folders::{self, LocalMailbox, ServerMailbox},
    store::{FolderState, IndexEntry, LocalFolder},
};

/// What `SELECT` reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selected {
    pub uidvalidity: u32,
    pub uid_next: Option<u32>,
    pub exists: u32,
}

/// A message's metadata, before its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageMeta {
    pub uid: u32,
    /// `RFC822.SIZE`
    pub size: Option<u64>,
    pub flags: Vec<String>,
    /// `INTERNALDATE`, seconds since 1970.
    pub internal_date: Option<i64>,
}

/// Why a sync ended early.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncError {
    /// No connection (DNS, TCP, TLS).
    Connect(String),
    /// The server refused the sign-in.
    Auth(String),
    /// The server answered something unexpected, or refused a command.
    Protocol(String),
    /// A file could not be written or read.
    Storage(String),
    /// The sync was asked to stop.
    Stopped,
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncError::Connect(e) => write!(f, "could not connect: {e}"),
            SyncError::Auth(e) => write!(f, "the server refused the sign-in: {e}"),
            SyncError::Protocol(e) => write!(f, "the server answered unexpectedly: {e}"),
            SyncError::Storage(e) => write!(f, "could not write the mail folder: {e}"),
            SyncError::Stopped => write!(f, "stopped"),
        }
    }
}

/// A mail server, as the sync sees it. `imap_client::ImapSource` is the real one.
pub trait MailSource {
    fn list(&mut self) -> Result<Vec<ServerMailbox>, SyncError>;
    fn select(&mut self, server_name: &str) -> Result<Selected, SyncError>;
    /// The UIDs `UID SEARCH UID <first>:*` answers (in any order).
    fn search_from(&mut self, first: u32) -> Result<Vec<u32>, SyncError>;
    fn fetch_meta(&mut self, uids: &[u32]) -> Result<Vec<MessageMeta>, SyncError>;
    /// The bodies of `uids` that still exist, with their UIDs.
    fn fetch_bodies(&mut self, uids: &[u32]) -> Result<Vec<(u32, Vec<u8>)>, SyncError>;
}

/// What the sync reports while it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// Folder `index` (from 0) of `count` is being synced.
    Folder {
        index: usize,
        count: usize,
        display: String,
    },
    /// `done` of the folder's `total` new messages are written.
    Messages {
        display: String,
        done: u64,
        total: u64,
    },
}

/// One folder's outcome.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderReport {
    pub key: String,
    pub display: String,
    /// Bodies fetched from the server.
    pub fetched: u64,
    /// Messages whose file was already written (before a crash), read back instead.
    pub reused: u64,
    /// Messages in the index now.
    pub messages: u64,
    /// The old UIDVALIDITY when the server had renumbered the folder.
    pub renumbered: Option<u32>,
}

/// The whole sync's outcome.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    pub folders: Vec<FolderReport>,
}

impl SyncReport {
    pub fn fetched(&self) -> u64 {
        self.folders.iter().map(|f| f.fetched).sum()
    }

    pub fn reused(&self) -> u64 {
        self.folders.iter().map(|f| f.reused).sum()
    }
}

/// How the sync batches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncOptions {
    /// Messages per body fetch at most.
    pub batch_messages: usize,
    /// Bytes per body fetch at most (a bigger message goes alone).
    pub batch_bytes: u64,
    /// UIDs per metadata fetch at most.
    pub meta_chunk: usize,
    /// Now, seconds since 1970, for `synced_at`.
    pub now: i64,
}

impl Default for SyncOptions {
    fn default() -> SyncOptions {
        SyncOptions {
            batch_messages: 25,
            batch_bytes: 8 * 1024 * 1024,
            meta_chunk: 500,
            now: 0,
        }
    }
}

/// What to do with a folder, from its state file and `SELECT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FolderPlan {
    /// The server renumbered the folder: move its files from this UIDVALIDITY aside.
    pub renumbered: Option<u32>,
    /// Messages up to this UID are synced.
    pub last_uid: u32,
    /// Nothing can be new: no search needed.
    pub nothing_new: bool,
}

/// See [`FolderPlan`].
pub fn plan_folder(state: Option<&FolderState>, selected: &Selected) -> FolderPlan {
    todo!()
}

/// The UIDs of `found` above `last`, ascending, once each.
pub fn new_uids(found: &[u32], last: u32) -> Vec<u32> {
    todo!()
}

/// A UID set for a command: ascending runs as `a:b`, joined with commas (`1:3,7,9:10`).
pub fn uid_set(uids: &[u32]) -> String {
    todo!()
}

/// Consecutive batches of `metas` (in their order), each with at most `max_count` messages and,
/// counting only messages to fetch (`to_fetch[i]`), at most `max_bytes` bytes - except that a
/// message bigger than `max_bytes` is a batch of its own.
pub fn batches(
    metas: &[MessageMeta],
    to_fetch: &[bool],
    max_count: usize,
    max_bytes: u64,
) -> Vec<std::ops::Range<usize>> {
    todo!()
}

/// Whether the index and state are due to be written, `since` messages after the last time,
/// for an index of `index_len` lines: every 200 messages, or every tenth of the index when that
/// is more, so a big folder's index is not rewritten over and over.
pub fn checkpoint_due(since: u64, index_len: usize) -> bool {
    todo!()
}

/// Syncs every folder of `source` into `store`. `progress` is told what is happening; when it
/// answers `false`, the sync writes what it has and stops with [`SyncError::Stopped`].
pub fn sync_account(
    source: &mut dyn MailSource,
    store: &LocalFolder,
    options: &SyncOptions,
    progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<SyncReport, SyncError> {
    todo!()
}

/// Syncs one folder (see the module documentation).
pub fn sync_folder(
    source: &mut dyn MailSource,
    store: &LocalFolder,
    mailbox: &LocalMailbox,
    options: &SyncOptions,
    progress: &mut dyn FnMut(Progress) -> bool,
) -> Result<FolderReport, SyncError> {
    todo!()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{
        store::{index_from_jsonl, index_key, message_key, state_key},
        testutil::TempDir,
    };

    /// 2026-09-30T08:42:00Z
    const SEP_30: i64 = 1_790_757_720;

    fn message(n: u32) -> Vec<u8> {
        format!(
            "Message-ID: <m{n}@example.org>\r\nDate: Wed, 30 Sep 2026 10:42:00 +0200\r\n\
             From: Ada <ada@example.org>\r\nTo: ben@example.org\r\nSubject: Message {n}\r\n\r\n\
             Body {n}\r\n"
        )
        .into_bytes()
    }

    #[derive(Default)]
    struct FakeFolder {
        attributes: Vec<String>,
        uidvalidity: u32,
        messages: BTreeMap<u32, Vec<u8>>,
    }

    /// An in-memory server that counts what it is asked for.
    #[derive(Default)]
    struct FakeServer {
        folders: BTreeMap<String, FakeFolder>,
        selected: Option<String>,
        /// UIDs whose bodies were fetched, per folder, in order.
        fetched: Vec<(String, u32)>,
        body_fetches: usize,
        searches: usize,
        /// Fail the n-th body fetch (from 1), once.
        fail_body_fetch: Option<usize>,
        /// UIDs to leave out of body answers (expunged meanwhile).
        vanished: Vec<u32>,
        /// Answer SELECT without UIDNEXT (it is optional in IMAP4rev1).
        hide_uid_next: bool,
    }

    impl FakeServer {
        fn with(folders: &[(&str, &[&str], &[u32])]) -> FakeServer {
            let mut server = FakeServer::default();
            for (name, attributes, uids) in folders {
                server.folders.insert(
                    name.to_string(),
                    FakeFolder {
                        attributes: attributes.iter().map(|a| a.to_string()).collect(),
                        uidvalidity: 1,
                        messages: uids.iter().map(|&u| (u, message(u))).collect(),
                    },
                );
            }
            server
        }

        fn folder(&self) -> &FakeFolder {
            &self.folders[self.selected.as_ref().unwrap()]
        }

        fn fetched_uids(&self, folder: &str) -> Vec<u32> {
            self.fetched
                .iter()
                .filter(|(f, _)| f == folder)
                .map(|(_, u)| *u)
                .collect()
        }
    }

    impl MailSource for FakeServer {
        fn list(&mut self) -> Result<Vec<ServerMailbox>, SyncError> {
            Ok(self
                .folders
                .iter()
                .map(|(name, f)| ServerMailbox {
                    name: name.clone(),
                    delimiter: Some(String::from("/")),
                    attributes: f.attributes.clone(),
                })
                .collect())
        }

        fn select(&mut self, name: &str) -> Result<Selected, SyncError> {
            let f = self
                .folders
                .get(name)
                .ok_or_else(|| SyncError::Protocol(format!("no folder {name}")))?;
            let selected = Selected {
                uidvalidity: f.uidvalidity,
                uid_next: if self.hide_uid_next {
                    None
                } else {
                    Some(f.messages.keys().last().map_or(1, |u| u + 1))
                },
                exists: f.messages.len() as u32,
            };
            self.selected = Some(name.to_string());
            Ok(selected)
        }

        fn search_from(&mut self, first: u32) -> Result<Vec<u32>, SyncError> {
            self.searches += 1;
            let uids: Vec<u32> = self.folder().messages.keys().copied().collect();
            let mut found: Vec<u32> = uids.iter().copied().filter(|&u| u >= first).collect();
            // `n:*` matches the newest message even when n is above it (RFC 3501).
            if let Some(&newest) = uids.last() {
                if !found.contains(&newest) {
                    found.push(newest);
                }
            }
            found.reverse();
            Ok(found)
        }

        fn fetch_meta(&mut self, uids: &[u32]) -> Result<Vec<MessageMeta>, SyncError> {
            let f = self.folder();
            Ok(uids
                .iter()
                .filter_map(|u| {
                    f.messages.get(u).map(|m| MessageMeta {
                        uid: *u,
                        size: Some(m.len() as u64),
                        flags: vec![String::from("\\Seen")],
                        internal_date: Some(SEP_30),
                    })
                })
                .collect())
        }

        fn fetch_bodies(&mut self, uids: &[u32]) -> Result<Vec<(u32, Vec<u8>)>, SyncError> {
            self.body_fetches += 1;
            if self.fail_body_fetch == Some(self.body_fetches) {
                self.fail_body_fetch = None;
                return Err(SyncError::Connect(String::from("connection reset")));
            }
            let name = self.selected.clone().unwrap();
            let mut out = Vec::new();
            for u in uids {
                if self.vanished.contains(u) {
                    continue;
                }
                if let Some(m) = self.folders[&name].messages.get(u) {
                    out.push((*u, m.clone()));
                    self.fetched.push((name.clone(), *u));
                }
            }
            Ok(out)
        }
    }

    fn options() -> SyncOptions {
        SyncOptions {
            batch_messages: 2,
            batch_bytes: 1 << 20,
            meta_chunk: 3,
            now: SEP_30,
        }
    }

    fn run(server: &mut FakeServer, store: &LocalFolder) -> Result<SyncReport, SyncError> {
        sync_account(server, store, &options(), &mut |_| true)
    }

    fn index(store: &LocalFolder, folder: &str) -> Vec<IndexEntry> {
        index_from_jsonl(&String::from_utf8(store.get(&index_key(folder)).unwrap()).unwrap())
    }

    fn state(store: &LocalFolder, folder: &str) -> FolderState {
        FolderState::from_json(&String::from_utf8(store.get(&state_key(folder)).unwrap()).unwrap())
            .unwrap()
    }

    #[test]
    fn the_first_sync_writes_every_message_its_index_and_its_state() {
        let dir = TempDir::new("sync");
        let store = LocalFolder::new(dir.0.clone());
        let mut server = FakeServer::with(&[
            ("INBOX", &[], &[1, 2, 3, 5, 8]),
            ("Junk", &["\\Junk"], &[4]),
        ]);
        let report = run(&mut server, &store).unwrap();
        assert_eq!(report.fetched(), 6);
        assert_eq!(report.reused(), 0);
        for uid in [1, 2, 3, 5, 8] {
            assert_eq!(
                store.get(&message_key("inbox", 2026, 9, uid)).unwrap(),
                message(uid)
            );
        }
        assert_eq!(
            store.get(&message_key("spam", 2026, 9, 4)).unwrap(),
            message(4)
        );
        let inbox = index(&store, "inbox");
        assert_eq!(
            inbox.iter().map(|e| e.uid).collect::<Vec<_>>(),
            vec![1, 2, 3, 5, 8]
        );
        assert_eq!(inbox[0].subject, "Message 1");
        assert_eq!(inbox[0].path, "mail/inbox/2026/09/1.eml");
        assert_eq!(inbox[0].flags, vec![String::from("\\Seen")]);
        let s = state(&store, "inbox");
        assert_eq!(
            (s.server_name.as_str(), s.uidvalidity, s.last_uid),
            ("INBOX", 1, 8)
        );
        assert_eq!(s.messages, 5);
        assert_eq!(s.synced_at, "2026-09-30T08:42:00Z");
        assert_eq!(state(&store, "spam").server_name, "Junk");
        assert_eq!(
            store.folders(),
            vec![String::from("inbox"), String::from("spam")]
        );
    }

    #[test]
    fn a_second_sync_fetches_nothing_twice() {
        let dir = TempDir::new("sync");
        let store = LocalFolder::new(dir.0.clone());
        let mut server = FakeServer::with(&[("INBOX", &[], &[1, 2, 3]), ("Spam", &[], &[7])]);
        run(&mut server, &store).unwrap();
        let fetches = server.body_fetches;
        let again = run(&mut server, &store).unwrap();
        assert_eq!(again.fetched(), 0);
        assert_eq!(server.body_fetches, fetches, "no body fetch at all");
        assert_eq!(server.fetched_uids("INBOX"), vec![1, 2, 3]);
        assert_eq!(index(&store, "inbox").len(), 3);
        assert_eq!(index(&store, "spam").len(), 1);
    }

    #[test]
    fn new_mail_is_fetched_alone_even_when_the_server_has_no_uidnext() {
        let dir = TempDir::new("sync");
        let store = LocalFolder::new(dir.0.clone());
        let mut server = FakeServer::with(&[("INBOX", &[], &[1, 2])]);
        server.hide_uid_next = true;
        run(&mut server, &store).unwrap();
        // Without UIDNEXT the folder is searched, and `3:*` answers UID 2: nothing is new.
        assert_eq!(run(&mut server, &store).unwrap().fetched(), 0);
        assert_eq!(server.fetched_uids("INBOX"), vec![1, 2]);
        server
            .folders
            .get_mut("INBOX")
            .unwrap()
            .messages
            .insert(9, message(9));
        let report = run(&mut server, &store).unwrap();
        assert_eq!(report.fetched(), 1);
        assert_eq!(server.fetched_uids("INBOX"), vec![1, 2, 9]);
        assert_eq!(
            index(&store, "inbox")
                .iter()
                .map(|e| e.uid)
                .collect::<Vec<_>>(),
            vec![1, 2, 9]
        );
        assert_eq!(state(&store, "inbox").last_uid, 9);
    }

    #[test]
    fn a_renumbered_folder_moves_aside_and_syncs_from_the_start() {
        let dir = TempDir::new("sync");
        let store = LocalFolder::new(dir.0.clone());
        let mut server = FakeServer::with(&[("INBOX", &[], &[1, 2])]);
        run(&mut server, &store).unwrap();
        {
            let inbox = server.folders.get_mut("INBOX").unwrap();
            inbox.uidvalidity = 2;
            inbox.messages = [(1, message(21))].into_iter().collect();
        }
        let report = run(&mut server, &store).unwrap();
        assert_eq!(report.folders[0].renumbered, Some(1));
        assert_eq!(
            store.get(&message_key("inbox", 2026, 9, 1)).unwrap(),
            message(21)
        );
        assert_eq!(
            store.get("stale/inbox/1/2026/09/1.eml").unwrap(),
            message(1)
        );
        assert_eq!(
            store.get("stale/inbox/1/2026/09/2.eml").unwrap(),
            message(2)
        );
        let inbox = index(&store, "inbox");
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].subject, "Message 21");
        assert_eq!(state(&store, "inbox").uidvalidity, 2);
    }

    #[test]
    fn a_sync_that_broke_off_is_picked_up_without_fetching_twice() {
        let dir = TempDir::new("sync");
        let store = LocalFolder::new(dir.0.clone());
        let mut server = FakeServer::with(&[("INBOX", &[], &[1, 2, 3, 4, 5])]);
        // Batches of 2: the second body fetch (UIDs 3 and 4) fails.
        server.fail_body_fetch = Some(2);
        assert!(run(&mut server, &store).is_err());
        assert_eq!(server.fetched_uids("INBOX"), vec![1, 2]);
        let report = run(&mut server, &store).unwrap();
        assert_eq!(report.fetched(), 3);
        assert_eq!(server.fetched_uids("INBOX"), vec![1, 2, 3, 4, 5]);
        assert_eq!(
            index(&store, "inbox")
                .iter()
                .map(|e| e.uid)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 5]
        );
    }

    #[test]
    fn a_message_written_before_a_crash_is_read_back_not_fetched() {
        let dir = TempDir::new("sync");
        let store = LocalFolder::new(dir.0.clone());
        // UIDs 1 and 2 were written, but the index and state never were.
        store
            .put(&message_key("inbox", 2026, 9, 1), &message(1), false)
            .unwrap();
        store
            .put(&message_key("inbox", 2026, 9, 2), &message(2), false)
            .unwrap();
        // A file cut short is fetched again: its size is not the server's.
        store
            .put(&message_key("inbox", 2026, 9, 3), b"From: a", false)
            .unwrap();
        let mut server = FakeServer::with(&[("INBOX", &[], &[1, 2, 3])]);
        let report = run(&mut server, &store).unwrap();
        assert_eq!(report.reused(), 2);
        assert_eq!(server.fetched_uids("INBOX"), vec![3]);
        assert_eq!(
            store.get(&message_key("inbox", 2026, 9, 3)).unwrap(),
            message(3)
        );
        let inbox = index(&store, "inbox");
        assert_eq!(inbox.len(), 3);
        assert_eq!(inbox[1].subject, "Message 2");
    }

    #[test]
    fn a_message_gone_before_its_body_came_is_skipped() {
        let dir = TempDir::new("sync");
        let store = LocalFolder::new(dir.0.clone());
        let mut server = FakeServer::with(&[("INBOX", &[], &[1, 2, 3])]);
        server.vanished = vec![2];
        run(&mut server, &store).unwrap();
        assert_eq!(
            index(&store, "inbox")
                .iter()
                .map(|e| e.uid)
                .collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert_eq!(state(&store, "inbox").last_uid, 3);
    }

    #[test]
    fn a_stopped_sync_keeps_what_it_wrote() {
        let dir = TempDir::new("sync");
        let store = LocalFolder::new(dir.0.clone());
        let mut server = FakeServer::with(&[("INBOX", &[], &[1, 2, 3, 4, 5])]);
        let mut seen = Vec::new();
        let stopped = sync_account(&mut server, &store, &options(), &mut |p| {
            let go_on = !matches!(p, Progress::Messages { done: 2, .. });
            seen.push(p);
            go_on
        });
        assert_eq!(stopped, Err(SyncError::Stopped));
        assert!(seen.contains(&Progress::Folder {
            index: 0,
            count: 1,
            display: String::from("Inbox")
        }));
        assert_eq!(index(&store, "inbox").len(), 2);
        assert_eq!(state(&store, "inbox").last_uid, 2);
        let report = run(&mut server, &store).unwrap();
        assert_eq!(report.fetched(), 3);
    }

    #[test]
    fn progress_counts_the_folders_new_messages() {
        let dir = TempDir::new("sync");
        let store = LocalFolder::new(dir.0.clone());
        let mut server = FakeServer::with(&[("INBOX", &[], &[1, 2, 3])]);
        let mut seen = Vec::new();
        sync_account(&mut server, &store, &options(), &mut |p| {
            seen.push(p);
            true
        })
        .unwrap();
        let counts: Vec<(u64, u64)> = seen
            .iter()
            .filter_map(|p| match p {
                Progress::Messages { done, total, .. } => Some((*done, *total)),
                _ => None,
            })
            .collect();
        assert_eq!(counts.last(), Some(&(3, 3)));
        assert!(counts.windows(2).all(|w| w[0].0 <= w[1].0), "{counts:?}");
    }

    #[test]
    fn an_empty_folder_is_still_a_synced_folder() {
        let dir = TempDir::new("sync");
        let store = LocalFolder::new(dir.0.clone());
        let mut server = FakeServer::with(&[("INBOX", &[], &[]), ("Drafts", &["\\Drafts"], &[])]);
        run(&mut server, &store).unwrap();
        assert_eq!(server.searches, 0);
        assert_eq!(
            store.folders(),
            vec![String::from("drafts"), String::from("inbox")]
        );
        assert_eq!(state(&store, "inbox").last_uid, 0);
        assert_eq!(index(&store, "inbox").len(), 0);
    }

    #[test]
    fn the_plan_follows_the_state_and_select() {
        let sel = |uidvalidity, uid_next, exists| Selected {
            uidvalidity,
            uid_next,
            exists,
        };
        let mut s = FolderState::create("INBOX", "Inbox", 5);
        s.last_uid = 10;
        assert_eq!(
            plan_folder(None, &sel(5, Some(3), 2)),
            FolderPlan {
                renumbered: None,
                last_uid: 0,
                nothing_new: false
            }
        );
        assert_eq!(
            plan_folder(Some(&s), &sel(5, Some(11), 7)),
            FolderPlan {
                renumbered: None,
                last_uid: 10,
                nothing_new: true
            }
        );
        assert_eq!(
            plan_folder(Some(&s), &sel(5, Some(12), 8)),
            FolderPlan {
                renumbered: None,
                last_uid: 10,
                nothing_new: false
            }
        );
        assert_eq!(
            plan_folder(Some(&s), &sel(5, None, 8)),
            FolderPlan {
                renumbered: None,
                last_uid: 10,
                nothing_new: false
            }
        );
        assert_eq!(
            plan_folder(Some(&s), &sel(6, Some(4), 3)),
            FolderPlan {
                renumbered: Some(5),
                last_uid: 0,
                nothing_new: false
            }
        );
        assert_eq!(
            plan_folder(Some(&s), &sel(5, Some(40), 0)),
            FolderPlan {
                renumbered: None,
                last_uid: 10,
                nothing_new: true
            }
        );
    }

    #[test]
    fn only_uids_above_the_last_are_new() {
        assert_eq!(new_uids(&[9, 3, 12, 11, 12], 10), vec![11, 12]);
        // `11:*` on a folder whose newest UID is 10 answers 10.
        assert_eq!(new_uids(&[10], 10), Vec::<u32>::new());
        assert_eq!(new_uids(&[], 0), Vec::<u32>::new());
    }

    #[test]
    fn uid_sets_are_compressed_runs() {
        assert_eq!(uid_set(&[1, 2, 3, 7, 9, 10]), "1:3,7,9:10");
        assert_eq!(uid_set(&[5]), "5");
        assert_eq!(uid_set(&[]), "");
    }

    #[test]
    fn batches_are_bounded_by_count_and_bytes() {
        let meta = |uid, size| MessageMeta {
            uid,
            size: Some(size),
            flags: Vec::new(),
            internal_date: None,
        };
        let metas = [
            meta(1, 10),
            meta(2, 10),
            meta(3, 10),
            meta(4, 100),
            meta(5, 10),
        ];
        let all = [true; 5];
        assert_eq!(batches(&metas, &all, 2, 1000), vec![0..2, 2..4, 4..5]);
        assert_eq!(batches(&metas, &all, 10, 25), vec![0..2, 2..3, 3..4, 4..5]);
        // Messages read back from disk do not count against the bytes.
        let reused = [true, false, false, true, true];
        assert_eq!(batches(&metas, &reused, 10, 120), vec![0..5]);
        assert_eq!(
            batches(&[], &[], 2, 10),
            Vec::<std::ops::Range<usize>>::new()
        );
    }

    #[test]
    fn checkpoints_come_every_200_messages_or_every_tenth_of_the_index() {
        assert!(!checkpoint_due(199, 0));
        assert!(checkpoint_due(200, 0));
        assert!(checkpoint_due(200, 2000));
        assert!(!checkpoint_due(200, 50_000));
        assert!(checkpoint_due(5000, 50_000));
    }
}
