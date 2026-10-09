//! The mail the IMAP and SMTP servers serve, behind [`MailStore`]: mailboxes by their path in
//! the store (`Inbox`, `Work/Projects`), messages by their name in a mailbox, the three marks
//! every device shares (read, flagged, answered) by the message's id. The protocol servers
//! know nothing else about where mail lives.
//!
//! [`DriveMailStore`] is the Azlin drive's mailbox exactly as AzMail keeps it (`AZLIN_MAIL.md`,
//! through azul-mail-core's `azlin` module): `mail/<Folder>/<stamp>-<hash>.eml`, the markers
//! `mail/.state/<id>/seen|flagged|answered`, the folders and their roles by AzMail's folder
//! rules (a missing well-known folder is listed all the same and made by its first message).
//! An encrypted store (AZL1 objects and an encrypted index) is another implementation of the
//! same trait later; the servers do not change.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use azmail_core::{
    azlin,
    folders::{self, Role},
};
use azul_storage::{ops, time::now_unix, ByteRange, Drive, DriveError};

/// A mailbox: its path in the store (`/` between levels, UTF-8) and what it is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MailboxInfo {
    pub path: String,
    pub role: Role,
}

/// A message of a mailbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessage {
    /// What its marks are filed under (the name without `.eml`).
    pub id: String,
    /// Its name in the mailbox (`20261008T091500Z-3f2a9c1e5b7d4a60.eml`).
    pub name: String,
    /// Bytes.
    pub size: u64,
    /// When it arrived, in seconds since 1970 (IMAP's INTERNALDATE).
    pub arrived: u64,
}

/// The marks every device shares.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Marks {
    pub seen: bool,
    pub flagged: bool,
    pub answered: bool,
}

/// One of the [`Marks`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Seen,
    Flagged,
    Answered,
}

impl Mark {
    /// The marker's name in the drive (`seen`, `flagged`, `answered`).
    #[must_use]
    pub fn marker(self) -> &'static str {
        match self {
            Mark::Seen => azlin::SEEN,
            Mark::Flagged => azlin::FLAGGED,
            Mark::Answered => azlin::ANSWERED,
        }
    }
}

/// Why a store call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// No such mailbox or message.
    NotFound(String),
    /// A name the store cannot take, or a change it does not allow.
    Invalid(String),
    /// Something has this name already.
    Exists(String),
    /// The drive could not help (no connection, a refusal).
    Failed(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::NotFound(what) => write!(f, "{what} does not exist"),
            StoreError::Invalid(why) | StoreError::Failed(why) => write!(f, "{why}"),
            StoreError::Exists(what) => write!(f, "{what} exists already"),
        }
    }
}

fn drive_error(e: DriveError) -> StoreError {
    match e {
        DriveError::NotFound { key } => StoreError::NotFound(key),
        DriveError::InvalidKey { key, reason } => {
            StoreError::Invalid(format!("\"{key}\" is not a valid name: {reason}"))
        }
        other => StoreError::Failed(other.to_string()),
    }
}

/// The mail the servers serve. Every call blocks (the drive is a bucket on the network).
pub trait MailStore: Send + Sync {
    /// Every mailbox, the well-known ones included.
    ///
    /// # Errors
    ///
    /// The store's.
    fn mailboxes(&self) -> Result<Vec<MailboxInfo>, StoreError>;

    /// The messages of the mailbox `path`, by name (that is arrival order for the drive's
    /// names); a well-known mailbox that holds nothing yet is empty.
    ///
    /// # Errors
    ///
    /// The store's.
    fn messages(&self, path: &str) -> Result<Vec<StoredMessage>, StoreError>;

    /// Every message's marks, by id (a message without any is not there).
    ///
    /// # Errors
    ///
    /// The store's.
    fn marks(&self) -> Result<HashMap<String, Marks>, StoreError>;

    /// The message `name` of the mailbox `path`, whole.
    ///
    /// # Errors
    ///
    /// Not found, or the store's.
    fn read(&self, path: &str, name: &str) -> Result<Vec<u8>, StoreError>;

    /// Its first `max` bytes at most (a header fetch of a big message).
    ///
    /// # Errors
    ///
    /// Not found, or the store's.
    fn read_head(&self, path: &str, name: &str, max: u64) -> Result<Vec<u8>, StoreError> {
        let mut bytes = self.read(path, name)?;
        bytes.truncate(usize::try_from(max).unwrap_or(usize::MAX));
        Ok(bytes)
    }

    /// Sets (`on`) or clears the mark of the message `id`.
    ///
    /// # Errors
    ///
    /// The store's.
    fn set_mark(&self, id: &str, mark: Mark, on: bool) -> Result<(), StoreError>;

    /// Files `bytes` in the mailbox `path` as having arrived at `arrived`, with `marks`.
    ///
    /// # Errors
    ///
    /// A mailbox path the store cannot take, or the store's.
    fn append(
        &self,
        path: &str,
        bytes: &[u8],
        arrived: u64,
        marks: Marks,
    ) -> Result<StoredMessage, StoreError>;

    /// Copies the message `name` of `from` into `to` (same name and id: the marks are
    /// shared, as AzMail's moves keep them).
    ///
    /// # Errors
    ///
    /// Not found, a mailbox path the store cannot take, or the store's.
    fn copy(&self, from: &str, name: &str, to: &str) -> Result<(), StoreError>;

    /// Moves the message `name` of `from` into `to`; its marks stay.
    ///
    /// # Errors
    ///
    /// As [`MailStore::copy`].
    fn move_to(&self, from: &str, name: &str, to: &str) -> Result<(), StoreError>;

    /// Removes the messages `names` of the mailbox `path` for good, and the marks of every one
    /// that no other mailbox holds.
    ///
    /// # Errors
    ///
    /// The store's.
    fn expunge(&self, path: &str, names: &[String]) -> Result<(), StoreError>;

    /// Makes the (empty) mailbox `path`.
    ///
    /// # Errors
    ///
    /// A path the store cannot take, one that exists, or the store's.
    fn create_mailbox(&self, path: &str) -> Result<(), StoreError>;

    /// Removes the mailbox `path` with its messages (not the inbox, not one with mailboxes
    /// under it).
    ///
    /// # Errors
    ///
    /// As said, or the store's.
    fn delete_mailbox(&self, path: &str) -> Result<(), StoreError>;

    /// Renames the mailbox `from` (with what is under it) to `to` (not the inbox).
    ///
    /// # Errors
    ///
    /// As said, or the store's.
    fn rename_mailbox(&self, from: &str, to: &str) -> Result<(), StoreError>;
}

/// Mailbox paths nest at most this deep.
pub const MAX_DEPTH: usize = 16;
/// A mailbox path is at most this many bytes.
pub const MAX_PATH_BYTES: usize = 255;

/// Whether `path` can be a mailbox: levels separated by `/`, each a name a file system and an
/// object key take as it is (AzMail's `safe_segment` leaves it unchanged: no `\ : * ? " < > |`,
/// no control character, no surrounding blanks, no leading `.`), at most [`MAX_DEPTH`] deep.
///
/// # Errors
///
/// [`StoreError::Invalid`] saying why.
pub fn check_mailbox_path(path: &str) -> Result<(), StoreError> {
    let invalid = |why: &str| -> Result<(), StoreError> {
        Err(StoreError::Invalid(format!(
            "\"{path}\" cannot be a mailbox: {why}"
        )))
    };
    if path.is_empty() {
        return invalid("it is empty");
    }
    if path.len() > MAX_PATH_BYTES {
        return invalid("it is too long");
    }
    let levels: Vec<&str> = path.split('/').collect();
    if levels.len() > MAX_DEPTH {
        return invalid("it is nested too deep");
    }
    for level in levels {
        if level.is_empty() {
            return invalid("it has an empty level");
        }
        if level == "." || level == ".." || level.starts_with('.') {
            return invalid("a level starts with a dot");
        }
        if folders::safe_segment(level) != level {
            return invalid("it has a character a folder name cannot have");
        }
    }
    Ok(())
}

/// How long a listing of the mailboxes is reused (a mail program asks for every mailbox's
/// STATUS in a row).
const MAILBOX_CACHE: Duration = Duration::from_secs(5);

/// The Azlin drive's mailbox (`AZLIN_MAIL.md`) as a [`MailStore`].
pub struct DriveMailStore {
    drive: Arc<dyn Drive>,
    mailboxes: Mutex<Option<(Instant, Vec<MailboxInfo>)>>,
}

impl std::fmt::Debug for DriveMailStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DriveMailStore").finish_non_exhaustive()
    }
}

impl DriveMailStore {
    #[must_use]
    pub fn new(drive: Arc<dyn Drive>) -> DriveMailStore {
        DriveMailStore {
            drive,
            mailboxes: Mutex::new(None),
        }
    }

    /// The drive under it.
    #[must_use]
    pub fn drive(&self) -> &Arc<dyn Drive> {
        &self.drive
    }

    fn forget_mailboxes(&self) {
        *self
            .mailboxes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }

    /// The folders the drive really has (no well-known one added), by path.
    fn folder_paths(&self) -> Result<Vec<String>, StoreError> {
        Ok(azlin::list_mailbox(&*self.drive)
            .map_err(drive_error)?
            .into_iter()
            .map(|folder| folder.path)
            .collect())
    }

    /// The mailbox `path` as the drive has it (exactly, else in another case).
    fn existing(&self, path: &str) -> Result<Option<String>, StoreError> {
        let paths = self.folder_paths()?;
        Ok(paths
            .iter()
            .find(|p| p.as_str() == path)
            .or_else(|| paths.iter().find(|p| p.eq_ignore_ascii_case(path)))
            .cloned())
    }

    fn role_of(&self, path: &str) -> Result<Role, StoreError> {
        let boxes = self.mailboxes()?;
        Ok(boxes
            .iter()
            .find(|m| m.path == path)
            .or_else(|| boxes.iter().find(|m| m.path.eq_ignore_ascii_case(path)))
            .map_or(Role::Other, |m| m.role))
    }
}

impl MailStore for DriveMailStore {
    fn mailboxes(&self) -> Result<Vec<MailboxInfo>, StoreError> {
        {
            let cached = self
                .mailboxes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some((at, boxes)) = cached.as_ref() {
                if at.elapsed() < MAILBOX_CACHE {
                    return Ok(boxes.clone());
                }
            }
        }
        let paths = self.folder_paths()?;
        let boxes: Vec<MailboxInfo> = azlin::local_folders(&paths)
            .into_iter()
            .map(|m| MailboxInfo {
                path: m.server_name,
                role: m.role,
            })
            .collect();
        *self
            .mailboxes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some((Instant::now(), boxes.clone()));
        Ok(boxes)
    }

    fn messages(&self, path: &str) -> Result<Vec<StoredMessage>, StoreError> {
        let level = ops::list_folder_all(&*self.drive, &azlin::folder_prefix(path))
            .map_err(drive_error)?;
        let mut out: Vec<StoredMessage> = level
            .objects
            .into_iter()
            .filter_map(|object| {
                let id = azlin::message_id(&object.key)?.to_string();
                let name = object.key.rsplit('/').next()?.to_string();
                let arrived = azlin::stamp_of(&id).or(object.modified).unwrap_or(0);
                Some(StoredMessage {
                    id,
                    name,
                    size: object.size,
                    arrived,
                })
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn marks(&self) -> Result<HashMap<String, Marks>, StoreError> {
        Ok(azlin::list_states(&*self.drive)
            .map_err(drive_error)?
            .into_iter()
            .map(|(id, state)| {
                (
                    id,
                    Marks {
                        seen: state.seen,
                        flagged: state.flagged,
                        answered: state.answered,
                    },
                )
            })
            .collect())
    }

    fn read(&self, path: &str, name: &str) -> Result<Vec<u8>, StoreError> {
        self.drive
            .get(&azlin::message_key(path, name))
            .map_err(drive_error)
    }

    fn read_head(&self, path: &str, name: &str, max: u64) -> Result<Vec<u8>, StoreError> {
        if max == 0 {
            return Ok(Vec::new());
        }
        match self
            .drive
            .get_range(&azlin::message_key(path, name), ByteRange::new(0, Some(max - 1)))
        {
            Ok(bytes) => Ok(bytes),
            // An empty message has no first byte.
            Err(DriveError::InvalidRange { .. }) => Ok(Vec::new()),
            Err(e) => Err(drive_error(e)),
        }
    }

    fn set_mark(&self, id: &str, mark: Mark, on: bool) -> Result<(), StoreError> {
        let key = azlin::marker_key(id, mark.marker());
        if on {
            self.drive.put(&key, &[]).map_err(drive_error)
        } else {
            self.drive.delete(&key).map_err(drive_error)
        }
    }

    fn append(
        &self,
        path: &str,
        bytes: &[u8],
        arrived: u64,
        marks: Marks,
    ) -> Result<StoredMessage, StoreError> {
        check_mailbox_path(path)?;
        let name = azlin::object_name(bytes, arrived);
        self.drive
            .put(&azlin::message_key(path, &name), bytes)
            .map_err(drive_error)?;
        let id = name
            .strip_suffix(azlin::EML)
            .unwrap_or(name.as_str())
            .to_string();
        for (mark, on) in [
            (Mark::Seen, marks.seen),
            (Mark::Flagged, marks.flagged),
            (Mark::Answered, marks.answered),
        ] {
            if on {
                self.set_mark(&id, mark, true)?;
            }
        }
        // A first message makes a well-known folder real.
        self.forget_mailboxes();
        Ok(StoredMessage {
            id,
            name,
            size: bytes.len() as u64,
            arrived,
        })
    }

    fn copy(&self, from: &str, name: &str, to: &str) -> Result<(), StoreError> {
        check_mailbox_path(to)?;
        self.drive
            .copy(&azlin::message_key(from, name), &azlin::message_key(to, name))
            .map_err(drive_error)?;
        self.forget_mailboxes();
        Ok(())
    }

    fn move_to(&self, from: &str, name: &str, to: &str) -> Result<(), StoreError> {
        self.copy(from, name, to)?;
        self.drive
            .delete(&azlin::message_key(from, name))
            .map_err(drive_error)
    }

    fn expunge(&self, path: &str, names: &[String]) -> Result<(), StoreError> {
        if names.is_empty() {
            return Ok(());
        }
        let mut ids = Vec::new();
        for name in names {
            let key = azlin::message_key(path, name);
            self.drive.delete(&key).map_err(drive_error)?;
            if let Some(id) = azlin::message_id(&key) {
                ids.push(id.to_string());
            }
        }
        // The marks go with the last copy: a message another mailbox still holds keeps them.
        let elsewhere: HashSet<String> = azlin::list_mailbox(&*self.drive)
            .map_err(drive_error)?
            .into_iter()
            .flat_map(|folder| folder.messages)
            .filter_map(|object| azlin::message_id(&object.key).map(str::to_string))
            .collect();
        for id in ids.iter().filter(|id| !elsewhere.contains(*id)) {
            for marker in ops::list_all(&*self.drive, &azlin::state_prefix(id)).map_err(drive_error)? {
                self.drive.delete(&marker.key).map_err(drive_error)?;
            }
        }
        Ok(())
    }

    fn create_mailbox(&self, path: &str) -> Result<(), StoreError> {
        check_mailbox_path(path)?;
        if self.existing(path)?.is_some() {
            return Err(StoreError::Exists(format!("the mailbox \"{path}\"")));
        }
        self.drive
            .create_folder(&azlin::folder_prefix(path))
            .map_err(drive_error)?;
        self.forget_mailboxes();
        Ok(())
    }

    fn delete_mailbox(&self, path: &str) -> Result<(), StoreError> {
        check_mailbox_path(path)?;
        if self.role_of(path)? == Role::Inbox {
            return Err(StoreError::Invalid(String::from("the inbox cannot be deleted")));
        }
        let Some(real) = self.existing(path)? else {
            return Err(StoreError::NotFound(format!("the mailbox \"{path}\"")));
        };
        let below = format!("{real}/");
        if self.folder_paths()?.iter().any(|p| p.starts_with(&below)) {
            return Err(StoreError::Invalid(format!(
                "\"{path}\" has mailboxes in it: delete them first"
            )));
        }
        self.drive
            .delete_folder(&azlin::folder_prefix(&real))
            .map_err(drive_error)?;
        self.forget_mailboxes();
        Ok(())
    }

    fn rename_mailbox(&self, from: &str, to: &str) -> Result<(), StoreError> {
        check_mailbox_path(from)?;
        check_mailbox_path(to)?;
        if self.role_of(from)? == Role::Inbox {
            return Err(StoreError::Invalid(String::from("the inbox cannot be renamed")));
        }
        let Some(real) = self.existing(from)? else {
            return Err(StoreError::NotFound(format!("the mailbox \"{from}\"")));
        };
        if self.existing(to)?.is_some() {
            return Err(StoreError::Exists(format!("the mailbox \"{to}\"")));
        }
        self.drive
            .rename(&azlin::folder_prefix(&real), &azlin::folder_prefix(to))
            .map_err(drive_error)?;
        self.forget_mailboxes();
        Ok(())
    }
}

/// Now, in seconds since 1970 (what a message filed without a date arrived at).
#[must_use]
pub fn now() -> u64 {
    now_unix()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MemoryDrive;

    /// 2026-10-01T08:30:00Z
    const OCT_1: u64 = 1_790_843_400;

    fn mail(subject: &str) -> Vec<u8> {
        format!("From: ada@example.org\r\nSubject: {subject}\r\n\r\nHello.\r\n").into_bytes()
    }

    fn store() -> (Arc<MemoryDrive>, DriveMailStore) {
        let drive = Arc::new(MemoryDrive::new());
        (drive.clone(), DriveMailStore::new(drive))
    }

    #[test]
    fn the_well_known_mailboxes_are_there_before_any_mail_and_junk_is_spam() {
        let (drive, store) = store();
        drive.put("mail/Junk/x.eml", &mail("spam")).unwrap();
        drive.put("mail/Work/Projects/y.eml", &mail("kickoff")).unwrap();
        drive.put("mail/.state/x/seen", b"").unwrap();
        let boxes = store.mailboxes().unwrap();
        let got: Vec<(&str, Role)> = boxes.iter().map(|b| (b.path.as_str(), b.role)).collect();
        assert!(got.contains(&("Inbox", Role::Inbox)), "{got:?}");
        assert!(got.contains(&("Junk", Role::Spam)), "{got:?}");
        assert!(got.contains(&("Sent", Role::Sent)), "{got:?}");
        assert!(got.contains(&("Work/Projects", Role::Other)), "{got:?}");
        assert!(!got.iter().any(|(p, _)| p.starts_with('.')), "{got:?}");
        assert_eq!(got.iter().filter(|(_, r)| *r == Role::Spam).count(), 1, "{got:?}");
    }

    #[test]
    fn appended_mail_gets_azlin_names_and_markers_and_lists_in_arrival_order() {
        let (drive, store) = store();
        let first = store
            .append("Inbox", &mail("one"), OCT_1, Marks { seen: true, ..Marks::default() })
            .unwrap();
        let second = store.append("Inbox", &mail("two"), OCT_1 + 60, Marks::default()).unwrap();
        assert_eq!(first.name, azlin::object_name(&mail("one"), OCT_1));
        assert!(first.name.starts_with("20261001T083000Z-") && first.name.ends_with(".eml"));
        assert_eq!(format!("{}.eml", first.id), first.name);
        assert!(drive.keys().contains(&format!("mail/.state/{}/seen", first.id)));
        let listed = store.messages("Inbox").unwrap();
        assert_eq!(listed, vec![first.clone(), second.clone()]);
        assert_eq!(listed[0].arrived, OCT_1);
        let marks = store.marks().unwrap();
        assert!(marks[&first.id].seen);
        assert!(!marks.contains_key(&second.id));
        assert_eq!(store.read("Inbox", &first.name).unwrap(), mail("one"));
        assert_eq!(store.read_head("Inbox", &first.name, 4).unwrap(), b"From");
        assert!(matches!(store.read("Inbox", "nope.eml"), Err(StoreError::NotFound(_))));
    }

    #[test]
    fn a_hand_placed_file_is_a_message_dated_by_the_drive() {
        let (drive, store) = store();
        drive.set_now(OCT_1);
        drive.put("mail/Inbox/invoice.EML", &mail("invoice")).unwrap();
        drive.put("mail/Inbox/notes.txt", b"not mail").unwrap();
        let listed = store.messages("Inbox").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!((listed[0].id.as_str(), listed[0].name.as_str()), ("invoice", "invoice.EML"));
        assert_eq!(listed[0].arrived, OCT_1);
    }

    #[test]
    fn marks_are_set_and_cleared_as_empty_marker_objects() {
        let (drive, store) = store();
        store.set_mark("m1", Mark::Flagged, true).unwrap();
        store.set_mark("m1", Mark::Answered, true).unwrap();
        assert_eq!(
            store.marks().unwrap()["m1"],
            Marks { seen: false, flagged: true, answered: true }
        );
        store.set_mark("m1", Mark::Flagged, false).unwrap();
        assert_eq!(drive.keys(), vec!["mail/.state/m1/answered"]);
    }

    #[test]
    fn a_move_keeps_the_name_and_the_marks_and_expunging_the_last_copy_drops_them() {
        let (drive, store) = store();
        let m = store
            .append("Inbox", &mail("move me"), OCT_1, Marks { seen: true, ..Marks::default() })
            .unwrap();
        store.copy("Inbox", &m.name, "Archive").unwrap();
        store.move_to("Inbox", &m.name, "Trash").unwrap();
        assert!(store.messages("Inbox").unwrap().is_empty());
        assert_eq!(store.messages("Trash").unwrap()[0].name, m.name);
        assert_eq!(store.messages("Archive").unwrap()[0].name, m.name);
        store.expunge("Trash", &[m.name.clone()]).unwrap();
        assert!(store.marks().unwrap()[&m.id].seen, "Archive still holds it");
        store.expunge("Archive", &[m.name.clone()]).unwrap();
        assert!(store.marks().unwrap().is_empty());
        assert!(drive.keys().iter().all(|k| !k.contains(&m.id)), "{:?}", drive.keys());
    }

    #[test]
    fn mailboxes_are_made_renamed_and_deleted_but_never_the_inbox() {
        let (_drive, store) = store();
        store.create_mailbox("Receipts").unwrap();
        assert!(matches!(store.create_mailbox("Receipts"), Err(StoreError::Exists(_))));
        assert!(store.mailboxes().unwrap().iter().any(|m| m.path == "Receipts"));
        store.append("Receipts", &mail("r"), OCT_1, Marks::default()).unwrap();
        store.rename_mailbox("Receipts", "Old/Receipts").unwrap();
        assert_eq!(store.messages("Old/Receipts").unwrap().len(), 1);
        assert!(matches!(store.delete_mailbox("Old"), Err(StoreError::Invalid(_))));
        store.delete_mailbox("Old/Receipts").unwrap();
        assert!(store.messages("Old/Receipts").unwrap().is_empty());
        assert!(matches!(store.delete_mailbox("Inbox"), Err(StoreError::Invalid(_))));
        assert!(matches!(store.delete_mailbox("INBOX"), Err(StoreError::Invalid(_))));
        assert!(matches!(store.rename_mailbox("Inbox", "X"), Err(StoreError::Invalid(_))));
        assert!(matches!(store.delete_mailbox("Nothing"), Err(StoreError::NotFound(_))));
    }

    #[test]
    fn a_mailbox_path_cannot_climb_hide_or_carry_odd_characters() {
        for good in ["Inbox", "Work/Projects", "Entwürfe", "R&D", "[Gmail]"] {
            assert!(check_mailbox_path(good).is_ok(), "{good}");
        }
        for bad in [
            "", "..", "a/../b", ".state", "a/.hidden", "a//b", "/abs", "a/", " lead", "trail.",
            "a\\b", "a:b", "nul\0", "tab\there", "x?",
        ] {
            assert!(check_mailbox_path(bad).is_err(), "{bad:?}");
        }
        assert!(check_mailbox_path(&"a/".repeat(20)[..39]).is_err());
        assert!(check_mailbox_path(&"x".repeat(300)).is_err());
    }
}
