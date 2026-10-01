//! The synced mail as files, laid out like an object store's keys.
//!
//! Under the account's mail folder (see `account::mail_root`):
//!
//! ```text
//! mail/<folder>/<yyyy>/<mm>/<uid>.eml   one message, the exact bytes the server sent
//! mail/<folder>/index.jsonl             one line per message (IndexEntry)
//! mail/<folder>/state.json              UIDVALIDITY and the last synced UID (FolderState)
//! stale/<folder>/<uidvalidity>/...      a folder's files from before the server renumbered it
//! ```
//!
//! `<yyyy>/<mm>` is the month the server received the message (its INTERNALDATE, in UTC), so a
//! message's path is known before its body is fetched. Every key is a `/`-separated relative path,
//! the same string an S3 bucket would use under the account's prefix.
//!
//! [`LocalFolder`] is the one writer. It only puts, gets and sizes whole files (writing a
//! temporary file and renaming it, so a crash never leaves half a file) and moves a folder aside:
//! the operations the shared `Drive` (DRIVE1's `examples/azul-storage`) offers, which is where it
//! will be swapped in.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The prefix of every synced folder.
pub const MAIL_PREFIX: &str = "mail";
/// Where a renumbered folder's old files go.
pub const STALE_PREFIX: &str = "stale";
pub const INDEX_FILE: &str = "index.jsonl";
pub const STATE_FILE: &str = "state.json";
/// The `format` of a folder's state file.
pub const STATE_FORMAT: &str = "azmail.folder";
/// The state file version this AzMail writes, and the newest it reads.
pub const STATE_VERSION: u64 = 1;

/// Whether `key` is a relative `/`-separated path with no empty, `.` or `..` segment, no
/// backslash and no NUL: the keys AzMail writes, and nothing that could leave the folder.
pub fn is_valid_key(key: &str) -> bool {
    !key.is_empty()
        && !key.contains('\\')
        && !key.contains('\0')
        && key
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

/// `mail/<folder>/<yyyy>/<mm>/<uid>.eml`. A year or month of 0 (no date) is `0000/00`.
pub fn message_key(folder: &str, year: i32, month: u32, uid: u32) -> String {
    format!("{MAIL_PREFIX}/{folder}/{year:04}/{month:02}/{uid}.eml")
}

/// `mail/<folder>`
pub fn folder_prefix(folder: &str) -> String {
    format!("{MAIL_PREFIX}/{folder}")
}

/// `mail/<folder>/index.jsonl`
pub fn index_key(folder: &str) -> String {
    format!("{MAIL_PREFIX}/{folder}/{INDEX_FILE}")
}

/// `mail/<folder>/state.json`
pub fn state_key(folder: &str) -> String {
    format!("{MAIL_PREFIX}/{folder}/{STATE_FILE}")
}

/// `stale/<folder>/<uidvalidity>`
pub fn stale_prefix(folder: &str, uidvalidity: u32) -> String {
    format!("{STALE_PREFIX}/{folder}/{uidvalidity}")
}

/// Writes `bytes` to `path` whole or not at all: a temporary dot file next to it, then a rename
/// over it. Creates the folder. `durable` also flushes the file to the disk first (for the index
/// and state files; a message whose write was lost is fetched again, its size tells).
pub fn write_atomic(path: &Path, bytes: &[u8], durable: bool) -> std::io::Result<()> {
    use std::io::Write;

    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // A dot name that no reader takes for a message, an index or a state file.
    let temp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let written = std::fs::File::create(&temp).and_then(|mut file| {
        file.write_all(bytes)?;
        if durable {
            file.sync_all()?;
        }
        Ok(())
    });
    if let Err(e) = written.and_then(|()| std::fs::rename(&temp, path)) {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    Ok(())
}

/// The synced files of one account, under one folder on this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalFolder {
    root: PathBuf,
}

impl LocalFolder {
    pub fn new(root: PathBuf) -> LocalFolder {
        LocalFolder { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The file a key names; an error for a key that is not valid.
    pub fn path_of(&self, key: &str) -> std::io::Result<PathBuf> {
        if !is_valid_key(key) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("{key:?} is not a key AzMail writes"),
            ));
        }
        Ok(key
            .split('/')
            .fold(self.root.clone(), |path, segment| path.join(segment)))
    }

    /// Writes the object `key` whole (see [`write_atomic`]).
    pub fn put(&self, key: &str, bytes: &[u8], durable: bool) -> std::io::Result<()> {
        write_atomic(&self.path_of(key)?, bytes, durable)
    }

    /// Reads the object `key`.
    pub fn get(&self, key: &str) -> std::io::Result<Vec<u8>> {
        std::fs::read(self.path_of(key)?)
    }

    /// Removes the object `key` (on an object store: DeleteObject). A missing one is no error.
    pub fn delete(&self, key: &str) -> std::io::Result<()> {
        match std::fs::remove_file(self.path_of(key)?) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }

    /// The size of the object `key`, or `None` when there is none.
    pub fn size_of(&self, key: &str) -> Option<u64> {
        let meta = std::fs::metadata(self.path_of(key).ok()?).ok()?;
        meta.is_file().then(|| meta.len())
    }

    /// Moves every object under `from` to the same place under `to` (on an object store: copy,
    /// then delete). A missing `from` moves nothing.
    pub fn move_prefix(&self, from: &str, to: &str) -> std::io::Result<()> {
        let from = self.path_of(from)?;
        let to = self.path_of(to)?;
        if !from.exists() {
            return Ok(());
        }
        if to.exists() {
            // Moved aside before under the same UIDVALIDITY: keep that, add a new one next to it.
            let mut n = 2;
            let base = to.clone();
            let mut target = to;
            while target.exists() {
                target = PathBuf::from(format!("{}-{n}", base.display()));
                n += 1;
            }
            return std::fs::rename(&from, &target);
        }
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&from, &to)
    }

    /// The synced folders: every `mail/<folder>` holding a state file, by name.
    pub fn folders(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(self.root.join(MAIL_PREFIX)) else {
            return Vec::new();
        };
        let mut folders: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().join(STATE_FILE).is_file())
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .collect();
        folders.sort();
        folders
    }
}

/// One message in a folder's index.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexEntry {
    pub uid: u32,
    #[serde(default)]
    pub message_id: String,
    /// When it was written (its Date header), else when the server received it; RFC 3339, UTC.
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    #[serde(default)]
    pub subject: String,
    /// The IMAP flags when it was synced (`\Seen`, `\Flagged`, ...).
    #[serde(default)]
    pub flags: Vec<String>,
    /// Bytes in the `.eml` file.
    #[serde(default)]
    pub size: u64,
    /// The `.eml` file's key (`mail/<folder>/<yyyy>/<mm>/<uid>.eml`).
    pub path: String,
}

/// The index file: one JSON object per line, by UID, one line per UID (the later entry wins).
pub fn index_to_jsonl(entries: &[IndexEntry]) -> String {
    let mut by_uid: std::collections::BTreeMap<u32, &IndexEntry> =
        std::collections::BTreeMap::new();
    for entry in entries {
        by_uid.insert(entry.uid, entry);
    }
    let mut text = String::new();
    for entry in by_uid.values() {
        // Strings and numbers only: serializing cannot fail.
        if let Ok(line) = serde_json::to_string(entry) {
            text.push_str(&line);
            text.push('\n');
        }
    }
    text
}

/// Reads an index file. Lines that are not an entry (a line cut short by a crash, an empty line)
/// are left out.
pub fn index_from_jsonl(text: &str) -> Vec<IndexEntry> {
    let mut by_uid: std::collections::BTreeMap<u32, IndexEntry> = std::collections::BTreeMap::new();
    for line in text.lines() {
        if let Ok(entry) = serde_json::from_str::<IndexEntry>(line.trim()) {
            by_uid.insert(entry.uid, entry);
        }
    }
    by_uid.into_values().collect()
}

/// What AzMail knows of a folder it synced.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderState {
    pub format: String,
    pub version: u64,
    /// The server's name for the folder (modified UTF-7), for `SELECT`.
    pub server_name: String,
    /// The name the sidebar shows.
    #[serde(default)]
    pub display: String,
    /// The server's UIDVALIDITY: the UIDs below mean these messages only while it is unchanged.
    pub uidvalidity: u32,
    /// Every message up to this UID is in the index (or was gone when it was synced).
    pub last_uid: u32,
    /// Lines in the index.
    #[serde(default)]
    pub messages: u64,
    /// When the folder was last synced; RFC 3339, UTC.
    #[serde(default)]
    pub synced_at: String,
}

impl FolderState {
    pub fn create(server_name: &str, display: &str, uidvalidity: u32) -> FolderState {
        FolderState {
            format: STATE_FORMAT.to_string(),
            version: STATE_VERSION,
            server_name: server_name.to_string(),
            display: display.to_string(),
            uidvalidity,
            last_uid: 0,
            messages: 0,
            synced_at: String::new(),
        }
    }

    /// The state file's contents (pretty JSON, ending in a newline).
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).unwrap_or_default();
        text.push('\n');
        text
    }

    /// Reads a state file; `None` for anything that is not one this AzMail reads.
    pub fn from_json(text: &str) -> Option<FolderState> {
        let state: FolderState = serde_json::from_str(text).ok()?;
        (state.format == STATE_FORMAT && (1..=STATE_VERSION).contains(&state.version))
            .then_some(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn entry(uid: u32) -> IndexEntry {
        IndexEntry {
            uid,
            message_id: format!("<m{uid}@example.org>"),
            date: String::from("2026-09-30T08:42:00Z"),
            from: String::from("Ada <ada@example.org>"),
            to: String::from("ben@example.org"),
            subject: format!("Message {uid}"),
            flags: vec![String::from("\\Seen")],
            size: 1234,
            path: message_key("inbox", 2026, 9, uid),
        }
    }

    #[test]
    fn keys_follow_the_object_store_layout() {
        assert_eq!(
            message_key("inbox", 2026, 9, 42),
            "mail/inbox/2026/09/42.eml"
        );
        assert_eq!(message_key("spam", 1999, 12, 7), "mail/spam/1999/12/7.eml");
        assert_eq!(message_key("inbox", 0, 0, 1), "mail/inbox/0000/00/1.eml");
        assert_eq!(folder_prefix("Work.Projects"), "mail/Work.Projects");
        assert_eq!(index_key("inbox"), "mail/inbox/index.jsonl");
        assert_eq!(state_key("inbox"), "mail/inbox/state.json");
        assert_eq!(stale_prefix("inbox", 7), "stale/inbox/7");
    }

    #[test]
    fn only_relative_paths_inside_the_folder_are_keys() {
        for good in ["mail/inbox/2026/09/1.eml", "a", "mail/Entwürfe/index.jsonl"] {
            assert!(is_valid_key(good), "{good}");
        }
        for bad in [
            "",
            "/etc/passwd",
            "mail//x",
            "mail/../x",
            "./x",
            "mail/x/",
            "a\\b",
            "a\0b",
            "..",
        ] {
            assert!(!is_valid_key(bad), "{bad:?}");
        }
        let store = LocalFolder::new(PathBuf::from("/data"));
        assert!(store.path_of("../x").is_err());
        assert_eq!(
            store.path_of("mail/inbox/state.json").unwrap(),
            Path::new("/data")
                .join("mail")
                .join("inbox")
                .join("state.json")
        );
    }

    #[test]
    fn an_object_is_written_whole_and_leaves_no_temporary_file() {
        let dir = TempDir::new("store");
        let store = LocalFolder::new(dir.0.clone());
        let key = message_key("inbox", 2026, 9, 1);
        assert_eq!(store.size_of(&key), None);
        store.put(&key, b"From: a\r\n\r\nhi\r\n", false).unwrap();
        assert_eq!(store.get(&key).unwrap(), b"From: a\r\n\r\nhi\r\n");
        assert_eq!(store.size_of(&key), Some(15));
        store.put(&key, b"again", true).unwrap();
        assert_eq!(store.get(&key).unwrap(), b"again");
        let names: Vec<String> = std::fs::read_dir(dir.0.join("mail/inbox/2026/09"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec![String::from("1.eml")]);
        assert!(store.put("../escape", b"x", false).is_err());
        assert!(!dir.0.parent().unwrap().join("escape").exists());
    }

    #[test]
    fn a_folder_moves_aside_whole() {
        let dir = TempDir::new("store");
        let store = LocalFolder::new(dir.0.clone());
        store
            .put(&message_key("inbox", 2026, 9, 1), b"one", false)
            .unwrap();
        store.put(&index_key("inbox"), b"{}\n", false).unwrap();
        store
            .move_prefix(&folder_prefix("inbox"), &stale_prefix("inbox", 7))
            .unwrap();
        assert_eq!(store.size_of(&index_key("inbox")), None);
        assert_eq!(store.get("stale/inbox/7/index.jsonl").unwrap(), b"{}\n");
        assert_eq!(store.get("stale/inbox/7/2026/09/1.eml").unwrap(), b"one");
        // Nothing to move is not an error.
        store
            .move_prefix(&folder_prefix("nothing"), &stale_prefix("nothing", 1))
            .unwrap();
    }

    #[test]
    fn the_synced_folders_are_those_with_a_state_file() {
        let dir = TempDir::new("store");
        let store = LocalFolder::new(dir.0.clone());
        assert!(store.folders().is_empty());
        let state = FolderState::create("INBOX", "Inbox", 1);
        store
            .put(&state_key("spam"), state.to_json().as_bytes(), false)
            .unwrap();
        store
            .put(&state_key("inbox"), state.to_json().as_bytes(), false)
            .unwrap();
        store
            .put("mail/half-done/2026/09/1.eml", b"x", false)
            .unwrap();
        assert_eq!(
            store.folders(),
            vec![String::from("inbox"), String::from("spam")]
        );
    }

    #[test]
    fn the_index_is_one_line_per_uid_in_uid_order() {
        let text = index_to_jsonl(&[entry(3), entry(1), entry(2)]);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(text.ends_with('\n'));
        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["uid"], 1);
        assert_eq!(first["message_id"], "<m1@example.org>");
        assert_eq!(first["date"], "2026-09-30T08:42:00Z");
        assert_eq!(first["from"], "Ada <ada@example.org>");
        assert_eq!(first["to"], "ben@example.org");
        assert_eq!(first["subject"], "Message 1");
        assert_eq!(first["flags"][0], "\\Seen");
        assert_eq!(first["size"], 1234);
        assert_eq!(first["path"], "mail/inbox/2026/09/1.eml");
        assert_eq!(index_from_jsonl(&text), vec![entry(1), entry(2), entry(3)]);
        let changed = IndexEntry {
            subject: String::from("again"),
            ..entry(2)
        };
        let deduped = index_to_jsonl(&[entry(2), changed.clone()]);
        assert_eq!(index_from_jsonl(&deduped), vec![changed]);
        assert_eq!(index_to_jsonl(&[]), "");
    }

    #[test]
    fn a_line_cut_short_by_a_crash_is_left_out() {
        let mut text = index_to_jsonl(&[entry(1), entry(2)]);
        text.push_str("{\"uid\": 3, \"subj");
        text.push_str("\n\n");
        assert_eq!(index_from_jsonl(&text), vec![entry(1), entry(2)]);
    }

    #[test]
    fn a_state_file_round_trips_and_a_newer_one_is_not_read() {
        let mut state = FolderState::create("[Gmail]/Spam", "Spam", 600_123);
        state.last_uid = 42;
        state.messages = 40;
        state.synced_at = String::from("2026-09-30T08:42:00Z");
        let text = state.to_json();
        assert!(text.ends_with('\n'));
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["format"], "azmail.folder");
        assert_eq!(json["version"], 1);
        assert_eq!(json["uidvalidity"], 600_123);
        assert_eq!(json["last_uid"], 42);
        assert_eq!(FolderState::from_json(&text), Some(state.clone()));
        assert_eq!(
            FolderState::from_json(&text.replace("\"version\": 1", "\"version\": 2")),
            None
        );
        assert_eq!(FolderState::from_json("{}"), None);
        assert_eq!(FolderState::from_json("not json"), None);
    }
}
