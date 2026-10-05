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
//! [`MailStore`] is the one writer, and it writes through the shared azul-storage `Drive`
//! (`examples/azul-storage`): ONE `LocalDrive` at the data root, the account's folder as its
//! scoped prefix ([`DriveFolder`]), so every file lands in the data tree's one `.azlin/cache`
//! manifest and the later S3 sync swaps the drive, nothing else. The drive writes a file whole or
//! not at all (a temporary file, then a rename). An account synced to a folder outside the data
//! tree gets a drive of its own there that keeps no manifest.

use std::path::{Component, Path, PathBuf};

use azul_storage::{Drive, DriveError, LocalDrive, ScopedDrive};
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

/// A folder of AzMail's files on a drive: the drive's root on this computer, whether that root
/// is the data tree (its drive keeps the `.azlin/cache` manifest) and the folder's key prefix in
/// the drive (empty, or `/`-separated names each ending in `/`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveFolder {
    root: PathBuf,
    data_tree: bool,
    prefix: String,
}

impl DriveFolder {
    /// `path` (a folder on this computer) placed against the data tree at `data_root`: inside
    /// it, a folder of the tree's ONE drive (its files land in the tree's manifest); outside it
    /// (a folder the user chose elsewhere, `AZMAIL_DATA`), the root of a drive of its own that
    /// keeps no manifest.
    pub fn of(path: &Path, data_root: &Path) -> DriveFolder {
        if data_root.as_os_str().is_empty() {
            return DriveFolder::outside(path.to_path_buf());
        }
        let Ok(rest) = path.strip_prefix(data_root) else {
            return DriveFolder::outside(path.to_path_buf());
        };
        let mut prefix = String::new();
        for component in rest.components() {
            match component {
                Component::Normal(name) => match name.to_str() {
                    Some(name) if is_valid_key(name) => {
                        prefix.push_str(name);
                        prefix.push('/');
                    }
                    _ => return DriveFolder::outside(path.to_path_buf()),
                },
                Component::CurDir => {}
                _ => return DriveFolder::outside(path.to_path_buf()),
            }
        }
        DriveFolder {
            root: data_root.to_path_buf(),
            data_tree: true,
            prefix,
        }
    }

    /// `path` as the root of a drive of its own that keeps no manifest (not the data tree).
    pub fn outside(path: PathBuf) -> DriveFolder {
        DriveFolder {
            root: path,
            data_tree: false,
            prefix: String::new(),
        }
    }

    /// `path` placed as this folder's drive places it: on the data tree's drive when this one
    /// is the data tree's and `path` is inside it, else a drive of its own.
    pub fn locate(&self, path: &Path) -> DriveFolder {
        if self.data_tree {
            DriveFolder::of(path, &self.root)
        } else {
            DriveFolder::outside(path.to_path_buf())
        }
    }

    /// The folder `name` (one name, no `/`) in this one.
    pub fn child(&self, name: &str) -> DriveFolder {
        DriveFolder {
            root: self.root.clone(),
            data_tree: self.data_tree,
            prefix: format!("{}{name}/", self.prefix),
        }
    }

    /// Whether the folder is part of the data tree (its drive keeps the manifest).
    pub fn is_data_tree(&self) -> bool {
        self.data_tree
    }

    /// The folder on this computer.
    pub fn path(&self) -> PathBuf {
        self.prefix
            .split('/')
            .filter(|name| !name.is_empty())
            .fold(self.root.clone(), |path, name| path.join(name))
    }

    /// The drive of the folder: the data tree's (with its manifest) or the folder's own,
    /// scoped to the folder.
    fn drive(&self) -> Result<ScopedDrive<LocalDrive>, DriveError> {
        let local = if self.data_tree {
            LocalDrive::new(self.root.clone())
        } else {
            LocalDrive::without_manifest(self.root.clone())
        };
        ScopedDrive::new(local, &self.prefix, true)
    }
}

/// A drive's error as the `std::io::Error` AzMail's callers read (a missing file is `NotFound`).
fn io_error(e: DriveError) -> std::io::Error {
    let kind = match &e {
        DriveError::NotFound { .. } => std::io::ErrorKind::NotFound,
        DriveError::InvalidKey { .. } => std::io::ErrorKind::InvalidInput,
        DriveError::Denied { .. } => std::io::ErrorKind::PermissionDenied,
        _ => std::io::ErrorKind::Other,
    };
    std::io::Error::new(kind, e.to_string())
}

/// The files of one folder of AzMail (an account's synced mail, its own folder, the AzMail
/// folder), read and written through its drive. Blocking: call it from an azul `Thread`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailStore {
    folder: DriveFolder,
}

impl MailStore {
    pub fn new(folder: DriveFolder) -> MailStore {
        MailStore { folder }
    }

    /// Where the files are.
    pub fn folder(&self) -> &DriveFolder {
        &self.folder
    }

    fn drive(&self) -> std::io::Result<ScopedDrive<LocalDrive>> {
        self.folder.drive().map_err(io_error)
    }

    /// The error for a key AzMail never writes.
    fn check(key: &str) -> std::io::Result<()> {
        if is_valid_key(key) {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("{key:?} is not a key AzMail writes"),
            ))
        }
    }

    /// Writes the object `key` whole (the drive writes a temporary file and renames it).
    pub fn put(&self, key: &str, bytes: &[u8]) -> std::io::Result<()> {
        Self::check(key)?;
        self.drive()?.put(key, bytes).map_err(io_error)
    }

    /// Reads the object `key`.
    pub fn get(&self, key: &str) -> std::io::Result<Vec<u8>> {
        Self::check(key)?;
        self.drive()?.get(key).map_err(io_error)
    }

    /// Removes the object `key` (on an object store: DeleteObject). A missing one is no error.
    pub fn delete(&self, key: &str) -> std::io::Result<()> {
        Self::check(key)?;
        match self.drive()?.delete(key) {
            Ok(()) | Err(DriveError::NotFound { .. }) => Ok(()),
            Err(e) => Err(io_error(e)),
        }
    }

    /// The size of the object `key`, or `None` when there is none.
    pub fn size_of(&self, key: &str) -> Option<u64> {
        Self::check(key).ok()?;
        self.drive().ok()?.head(key).ok().map(|info| info.size)
    }

    /// Every key under the folder `prefix` (`outbox`, `mail/inbox`; at any depth), in key order.
    pub fn keys(&self, prefix: &str) -> Vec<String> {
        let Ok(drive) = self.drive() else {
            return Vec::new();
        };
        let folder = format!("{}/", prefix.trim_end_matches('/'));
        let mut keys: Vec<String> = azul_storage::ops::list_all(&drive, &folder)
            .map(|objects| objects.into_iter().map(|o| o.key).collect())
            .unwrap_or_default();
        keys.sort();
        keys
    }

    /// The names of the folders directly in the folder `prefix` (empty: this folder's own).
    pub fn subfolders(&self, prefix: &str) -> Vec<String> {
        let Ok(drive) = self.drive() else {
            return Vec::new();
        };
        let folder = match prefix.trim_end_matches('/') {
            "" => String::new(),
            p => format!("{p}/"),
        };
        let level = azul_storage::ops::list_folder_all(&drive, &folder).unwrap_or_default();
        let mut names: Vec<String> = level
            .folders
            .iter()
            .filter_map(|f| {
                let name = f.strip_prefix(folder.as_str())?.trim_end_matches('/');
                (!name.is_empty()).then(|| name.to_string())
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Moves every object under `from` to the same place under `to` (on an object store: copy,
    /// then delete). A missing `from` moves nothing; when `to` is taken (moved aside before
    /// under the same UIDVALIDITY), the move goes next to it as `<to>-2`, `<to>-3`, ...
    pub fn move_prefix(&self, from: &str, to: &str) -> std::io::Result<()> {
        Self::check(from)?;
        Self::check(to)?;
        if self.keys(from).is_empty() {
            return Ok(());
        }
        // Taken: objects under it, or (a folder on disk) an empty folder of that name.
        let taken = |name: &str| !self.keys(name).is_empty() || self.folder.path().join(name).exists();
        let mut target = to.to_string();
        let mut n = 2;
        while taken(target.as_str()) {
            target = format!("{to}-{n}");
            n += 1;
        }
        self.drive()?
            .rename(&format!("{from}/"), &format!("{target}/"))
            .map_err(io_error)
    }

    /// The synced folders: every `mail/<folder>` holding a state file, by name.
    pub fn folders(&self) -> Vec<String> {
        self.subfolders(MAIL_PREFIX)
            .into_iter()
            .filter(|name| self.size_of(&state_key(name)).is_some())
            .collect()
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

    /// The data tree's manifest (`<root>/.azlin/cache`), as text.
    fn manifest_of(root: &Path) -> String {
        std::fs::read_to_string(
            root.join(azul_storage::manifest::MANIFEST_DIR)
                .join(azul_storage::manifest::CACHE_FILE),
        )
        .unwrap_or_default()
    }

    #[test]
    fn an_accounts_folder_in_the_data_tree_is_a_folder_of_the_trees_one_drive() {
        let dir = TempDir::new("store-tree");
        let account = dir.0.join("mail").join("ada@example.org");
        let folder = DriveFolder::of(&account, &dir.0);
        assert!(folder.is_data_tree());
        assert_eq!(folder.path(), account);
        assert_eq!(folder.child("outbox").path(), account.join("outbox"));
        let store = MailStore::new(folder);
        let key = message_key("inbox", 2026, 9, 1);
        store.put(&key, b"From: a\r\n\r\nhi\r\n").unwrap();
        assert!(account.join("mail/inbox/2026/09/1.eml").is_file());
        // ONE manifest, the data tree's: the write is recorded under the account's prefix,
        // and the account's folder holds no second one.
        let manifest = manifest_of(&dir.0);
        assert!(
            manifest.contains("mail/ada%40example.org/mail/inbox/2026/09/1.eml")
                || manifest.contains("mail/ada@example.org/mail/inbox/2026/09/1.eml"),
            "the data tree's manifest does not record the message: {manifest}"
        );
        assert!(!account.join(azul_storage::manifest::MANIFEST_DIR).exists());
    }

    #[test]
    fn a_folder_outside_the_data_tree_is_a_drive_of_its_own_without_a_manifest() {
        let tree = TempDir::new("store-tree");
        let elsewhere = TempDir::new("store-elsewhere");
        let folder = DriveFolder::of(&elsewhere.0, &tree.0);
        assert!(!folder.is_data_tree());
        assert_eq!(folder.path(), elsewhere.0);
        MailStore::new(folder)
            .put(&state_key("inbox"), b"{}")
            .unwrap();
        assert!(elsewhere.0.join("mail/inbox/state.json").is_file());
        assert!(!elsewhere.0.join(azul_storage::manifest::MANIFEST_DIR).exists());
        assert!(!tree.0.join(azul_storage::manifest::MANIFEST_DIR).exists());
        // Seen from the data tree's AzMail folder, a path is placed the same way.
        let azmail = DriveFolder::of(&tree.0.join("mail"), &tree.0);
        assert!(azmail.locate(&tree.0.join("mail").join("x")).is_data_tree());
        assert!(!azmail.locate(&elsewhere.0).is_data_tree());
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
        let store = MailStore::new(DriveFolder::outside(PathBuf::from("/data")));
        assert!(store.get("../x").is_err());
        assert_eq!(
            DriveFolder::of(Path::new("/data/mail/ada"), Path::new("/data"))
                .child("inbox")
                .path(),
            Path::new("/data").join("mail").join("ada").join("inbox")
        );
    }

    #[test]
    fn a_folder_lists_its_keys_and_its_subfolders() {
        let dir = TempDir::new("store");
        let store = MailStore::new(DriveFolder::outside(dir.0.clone()));
        store.put("outbox/b.json", b"{}").unwrap();
        store.put("outbox/a.json", b"{}").unwrap();
        store.put("outbox/a.eml", b"x").unwrap();
        store.put("ada/account.json", b"{}").unwrap();
        assert_eq!(
            store.keys("outbox"),
            vec!["outbox/a.eml", "outbox/a.json", "outbox/b.json"]
        );
        assert!(store.keys("nothing").is_empty());
        assert_eq!(store.subfolders(""), vec!["ada", "outbox"]);
        store.delete("outbox/a.eml").unwrap();
        store.delete("outbox/a.eml").unwrap();
        assert_eq!(store.size_of("outbox/a.eml"), None);
    }

    #[test]
    fn an_object_is_written_whole_and_leaves_no_temporary_file() {
        let dir = TempDir::new("store");
        let store = MailStore::new(DriveFolder::outside(dir.0.clone()));
        let key = message_key("inbox", 2026, 9, 1);
        assert_eq!(store.size_of(&key), None);
        store.put(&key, b"From: a\r\n\r\nhi\r\n").unwrap();
        assert_eq!(store.get(&key).unwrap(), b"From: a\r\n\r\nhi\r\n");
        assert_eq!(store.size_of(&key), Some(15));
        store.put(&key, b"again").unwrap();
        assert_eq!(store.get(&key).unwrap(), b"again");
        let names: Vec<String> = std::fs::read_dir(dir.0.join("mail/inbox/2026/09"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec![String::from("1.eml")]);
        assert!(store.put("../escape", b"x").is_err());
        assert!(!dir.0.parent().unwrap().join("escape").exists());
    }

    #[test]
    fn a_folder_moves_aside_whole() {
        let dir = TempDir::new("store");
        let store = MailStore::new(DriveFolder::outside(dir.0.clone()));
        store
            .put(&message_key("inbox", 2026, 9, 1), b"one")
            .unwrap();
        store.put(&index_key("inbox"), b"{}\n").unwrap();
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
        // Moved aside again under the same UIDVALIDITY: next to the first.
        store.put(&index_key("inbox"), b"again\n").unwrap();
        store
            .move_prefix(&folder_prefix("inbox"), &stale_prefix("inbox", 7))
            .unwrap();
        assert_eq!(store.get("stale/inbox/7/index.jsonl").unwrap(), b"{}\n");
        assert_eq!(store.get("stale/inbox/7-2/index.jsonl").unwrap(), b"again\n");
    }

    #[test]
    fn the_synced_folders_are_those_with_a_state_file() {
        let dir = TempDir::new("store");
        let store = MailStore::new(DriveFolder::outside(dir.0.clone()));
        assert!(store.folders().is_empty());
        let state = FolderState::create("INBOX", "Inbox", 1);
        store
            .put(&state_key("spam"), state.to_json().as_bytes())
            .unwrap();
        store
            .put(&state_key("inbox"), state.to_json().as_bytes())
            .unwrap();
        store
            .put("mail/half-done/2026/09/1.eml", b"x")
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
