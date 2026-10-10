//! IMAP's UIDs for the store's messages.
//!
//! IMAP wants every message of a mailbox to keep one number (its UID) for as long as the
//! mailbox's UIDVALIDITY stays, and a message that arrives later to get a higher one. The
//! drive's names cannot give that by themselves: a name keeps the stamp of when the message
//! first arrived in the drive, so a message moved into a folder sorts before older ones there.
//! So each mailbox gets a map, kept in the bridge's state folder:
//!
//! - the first time the bridge sees a mailbox, its messages are numbered 1, 2, 3, ... in the
//!   order of their names - for the drive's `<stamp>-<hash>.eml` names that is arrival order,
//!   so the same mailbox gets the same numbers on every device that starts from it;
//! - a name that appears later (new mail, a move or copy in, an APPEND) gets UIDNEXT, and
//!   UIDNEXT goes up by one; a name that is gone is dropped from the map (expunged), and if it
//!   comes back it is a new message with a new UID;
//! - UIDVALIDITY is the time (seconds since 1970) the map was made. A lost or damaged map is
//!   made anew with the time of then: the mail programs see a new UIDVALIDITY and fetch the
//!   mailbox again, rather than trusting UIDs that now mean other messages.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    sync::Mutex,
};

use azul_storage::{sigv4::sha256_hex, time::now_unix};
use serde::{Deserialize, Serialize};

/// The folder of the maps in the bridge's state folder.
pub const UIDS_DIR: &str = "imap-uids";
/// A map file's `format`.
pub const UIDS_FORMAT: &str = "azul-bridge.uids";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MapFile {
    format: String,
    version: u32,
    mailbox: String,
    validity: u32,
    next: u32,
    uids: BTreeMap<String, u32>,
}

#[derive(Debug, Clone)]
struct UidMap {
    validity: u32,
    next: u32,
    by_name: BTreeMap<String, u32>,
}

/// A mailbox's numbers after [`UidMaps::number`]: its UIDVALIDITY, UIDNEXT, and every name
/// with its UID, by UID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Numbered {
    pub validity: u32,
    pub next: u32,
    pub uids: Vec<(String, u32)>,
}

/// The maps of every mailbox, in a folder of their own (or in memory).
pub struct UidMaps {
    dir: Option<PathBuf>,
    clock: Box<dyn Fn() -> u64 + Send + Sync>,
    maps: Mutex<HashMap<String, UidMap>>,
}

impl std::fmt::Debug for UidMaps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UidMaps").field("dir", &self.dir).finish_non_exhaustive()
    }
}

impl UidMaps {
    /// Maps kept as files in `dir` (made when needed).
    #[must_use]
    pub fn in_folder(dir: PathBuf) -> UidMaps {
        UidMaps {
            dir: Some(dir),
            clock: Box::new(now_unix),
            maps: Mutex::new(HashMap::new()),
        }
    }

    /// Maps in memory only (a demo, the tests).
    #[must_use]
    pub fn in_memory() -> UidMaps {
        UidMaps {
            dir: None,
            clock: Box::new(now_unix),
            maps: Mutex::new(HashMap::new()),
        }
    }

    /// New maps take their UIDVALIDITY from `clock` instead of the system's time.
    #[must_use]
    pub fn with_clock(mut self, clock: impl Fn() -> u64 + Send + Sync + 'static) -> UidMaps {
        self.clock = Box::new(clock);
        self
    }

    fn file_of(&self, mailbox: &str) -> Option<PathBuf> {
        self.dir
            .as_ref()
            .map(|dir| dir.join(format!("{}.json", &sha256_hex(mailbox.as_bytes())[..32])))
    }

    fn fresh(&self) -> UidMap {
        let validity = u32::try_from((self.clock)()).unwrap_or(u32::MAX).max(1);
        UidMap {
            validity,
            next: 1,
            by_name: BTreeMap::new(),
        }
    }

    fn load(&self, mailbox: &str) -> Option<UidMap> {
        let path = self.file_of(mailbox)?;
        let file: MapFile = azcloud_kit::state::read_json(&path).ok().flatten()?;
        let sound = file.format == UIDS_FORMAT
            && file.mailbox == mailbox
            && file.validity > 0
            && file.uids.values().all(|uid| *uid > 0 && *uid < file.next);
        sound.then_some(UidMap {
            validity: file.validity,
            next: file.next,
            by_name: file.uids,
        })
    }

    fn save(&self, mailbox: &str, map: &UidMap) {
        let Some(path) = self.file_of(mailbox) else {
            return;
        };
        let file = MapFile {
            format: UIDS_FORMAT.to_string(),
            version: 1,
            mailbox: mailbox.to_string(),
            validity: map.validity,
            next: map.next,
            uids: map.by_name.clone(),
        };
        // A map that cannot be written is made anew next time: a new UIDVALIDITY, never a
        // wrong UID.
        let _ = azcloud_kit::state::write_json(&path, &file, false);
    }

    /// Numbers the messages `names` of `mailbox` (any order; the store's is name order): the
    /// known keep their UIDs, the gone are dropped, the new get the next UIDs in name order.
    pub fn number(&self, mailbox: &str, names: &[String]) -> Numbered {
        let mut maps = self
            .maps
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !maps.contains_key(mailbox) {
            let map = self.load(mailbox).unwrap_or_else(|| self.fresh());
            maps.insert(mailbox.to_string(), map);
        }
        let Some(map) = maps.get_mut(mailbox) else {
            unreachable!("inserted above");
        };
        let present: HashSet<&str> = names.iter().map(String::as_str).collect();
        let before = map.by_name.len();
        map.by_name.retain(|name, _| present.contains(name.as_str()));
        let mut changed = map.by_name.len() != before;
        let mut new: Vec<&String> = names
            .iter()
            .filter(|name| !map.by_name.contains_key(name.as_str()))
            .collect();
        new.sort();
        new.dedup();
        if map.next.checked_add(new.len() as u32).is_none() {
            // UIDs ran out (four billion arrivals): a new map, a new UIDVALIDITY.
            *map = self.fresh();
            map.by_name.clear();
            new = names.iter().collect();
            new.sort();
            new.dedup();
        }
        for name in new {
            map.by_name.insert(name.clone(), map.next);
            map.next += 1;
            changed = true;
        }
        let mut uids: Vec<(String, u32)> = map
            .by_name
            .iter()
            .map(|(name, uid)| (name.clone(), *uid))
            .collect();
        uids.sort_by_key(|(_, uid)| *uid);
        let numbered = Numbered {
            validity: map.validity,
            next: map.next,
            uids,
        };
        if changed {
            let snapshot = map.clone();
            drop(maps);
            self.save(mailbox, &snapshot);
        }
        numbered
    }
}

#[cfg(test)]
mod tests {
    use azul_storage::testing::TempDir;

    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|n| n.to_string()).collect()
    }

    fn uids_of(numbered: &Numbered) -> Vec<(&str, u32)> {
        numbered.uids.iter().map(|(n, u)| (n.as_str(), *u)).collect()
    }

    #[test]
    fn a_new_mailbox_is_numbered_in_name_order_and_new_mail_gets_the_next_uids() {
        let maps = UidMaps::in_memory().with_clock(|| 1_790_843_400);
        let first = maps.number("Inbox", &names(&["20261001-b.eml", "20261001-a.eml"]));
        assert_eq!(first.validity, 1_790_843_400);
        assert_eq!(uids_of(&first), vec![("20261001-a.eml", 1), ("20261001-b.eml", 2)]);
        assert_eq!(first.next, 3);
        // An older message moved in sorts first by name but is new: the next UID, at the end.
        let second = maps.number(
            "Inbox",
            &names(&["20200101-old.eml", "20261001-a.eml", "20261001-b.eml"]),
        );
        assert_eq!(
            uids_of(&second),
            vec![("20261001-a.eml", 1), ("20261001-b.eml", 2), ("20200101-old.eml", 3)]
        );
        assert_eq!((second.validity, second.next), (1_790_843_400, 4));
    }

    #[test]
    fn a_gone_message_takes_its_uid_with_it_and_coming_back_is_a_new_message() {
        let maps = UidMaps::in_memory().with_clock(|| 7);
        maps.number("Inbox", &names(&["a.eml", "b.eml"]));
        let without = maps.number("Inbox", &names(&["b.eml"]));
        assert_eq!(uids_of(&without), vec![("b.eml", 2)]);
        let back = maps.number("Inbox", &names(&["a.eml", "b.eml"]));
        assert_eq!(uids_of(&back), vec![("b.eml", 2), ("a.eml", 3)]);
        // Mailboxes have maps of their own.
        let other = maps.number("Archive", &names(&["a.eml"]));
        assert_eq!(uids_of(&other), vec![("a.eml", 1)]);
    }

    #[test]
    fn the_maps_outlive_the_process_and_a_damaged_one_gets_a_new_uidvalidity() {
        let dir = TempDir::new("uids");
        let maps = UidMaps::in_folder(dir.0.join(UIDS_DIR)).with_clock(|| 100);
        maps.number("Work/Projects", &names(&["a.eml", "b.eml"]));
        maps.number("Work/Projects", &names(&["b.eml", "c.eml"]));
        let again = UidMaps::in_folder(dir.0.join(UIDS_DIR)).with_clock(|| 200);
        let numbered = again.number("Work/Projects", &names(&["b.eml", "c.eml"]));
        assert_eq!(numbered.validity, 100);
        assert_eq!(uids_of(&numbered), vec![("b.eml", 2), ("c.eml", 3)]);
        assert_eq!(numbered.next, 4);
        // Damage the file: the next process starts over with its own time.
        for entry in std::fs::read_dir(dir.0.join(UIDS_DIR)).unwrap().flatten() {
            std::fs::write(entry.path(), b"{ not json").unwrap();
        }
        let fresh = UidMaps::in_folder(dir.0.join(UIDS_DIR)).with_clock(|| 300);
        let numbered = fresh.number("Work/Projects", &names(&["b.eml", "c.eml"]));
        assert_eq!(numbered.validity, 300);
        assert_eq!(uids_of(&numbered), vec![("b.eml", 1), ("c.eml", 2)]);
    }
}
