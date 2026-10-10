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
//! - CONDSTORE / QRESYNC (RFC 7162): [`UidMaps::track`] numbers the messages with their state
//!   (their flags as IMAP shows them) and gives every change the bridge sees - a new message,
//!   other flags, an expunge - a mod-sequence higher than any before (HIGHESTMODSEQ); the
//!   newest [`MAX_VANISHED`] expunged UIDs are kept with theirs, for VANISHED. Changes made on
//!   another device are seen when the bridge next looks at the mailbox.

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
/// Expunged UIDs kept per mailbox for QRESYNC's VANISHED (the oldest are forgotten).
pub const MAX_VANISHED: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MapFile {
    format: String,
    version: u32,
    mailbox: String,
    validity: u32,
    next: u32,
    uids: BTreeMap<String, u32>,
    /// HIGHESTMODSEQ (a file from before CONDSTORE: 0, then made).
    #[serde(default)]
    highest: u64,
    /// Each message's mod-sequence and the state it was given for, by name.
    #[serde(default)]
    modseqs: BTreeMap<String, (u64, String)>,
    /// The newest expunged UIDs with their mod-sequences.
    #[serde(default)]
    vanished: Vec<(u32, u64)>,
    /// The highest mod-sequence of an expunge no longer in `vanished`.
    #[serde(default)]
    vanished_floor: u64,
}

#[derive(Debug, Clone)]
struct UidMap {
    validity: u32,
    next: u32,
    by_name: BTreeMap<String, u32>,
    highest: u64,
    modseqs: BTreeMap<String, (u64, String)>,
    vanished: Vec<(u32, u64)>,
    vanished_floor: u64,
}

/// A mailbox's numbers after [`UidMaps::number`] / [`UidMaps::track`]: its UIDVALIDITY,
/// UIDNEXT, every name with its UID (by UID), and CONDSTORE's mod-sequences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Numbered {
    pub validity: u32,
    pub next: u32,
    pub uids: Vec<(String, u32)>,
    /// HIGHESTMODSEQ: no change of the mailbox has a higher mod-sequence.
    pub highest_modseq: u64,
    /// Each message's mod-sequence, by name.
    pub modseqs: HashMap<String, u64>,
    /// The newest expunged UIDs with the mod-sequences of their expunges...
    pub vanished: Vec<(u32, u64)>,
    /// ...and the highest mod-sequence of one forgotten (a client that knew less than this one
    /// is told every UID that is not there).
    pub vanished_floor: u64,
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
            highest: 1,
            modseqs: BTreeMap::new(),
            vanished: Vec::new(),
            vanished_floor: 0,
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
            highest: file.highest.max(1),
            modseqs: file.modseqs,
            vanished: file.vanished,
            vanished_floor: file.vanished_floor,
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
            highest: map.highest,
            modseqs: map.modseqs.clone(),
            vanished: map.vanished.clone(),
            vanished_floor: map.vanished_floor,
        };
        // A map that cannot be written is made anew next time: a new UIDVALIDITY, never a
        // wrong UID.
        let _ = azcloud_kit::state::write_json(&path, &file, false);
    }

    /// Numbers the messages `names` of `mailbox` (any order; the store's is name order): the
    /// known keep their UIDs, the gone are dropped, the new get the next UIDs in name order.
    /// The mod-sequences of the known stay; a new message and an expunge get one.
    pub fn number(&self, mailbox: &str, names: &[String]) -> Numbered {
        self.number_with(mailbox, names, None)
    }

    /// [`UidMaps::number`] with each message's state (its flags as IMAP shows them): a message
    /// whose state is not the one its mod-sequence was given for gets a new one.
    pub fn track(&self, mailbox: &str, names: &[String], states: &HashMap<String, String>) -> Numbered {
        self.number_with(mailbox, names, Some(states))
    }

    /// The message `name` of `mailbox` is in `state` now (a STORE): its new mod-sequence, or
    /// the one it has when that was its state already; `None` for a message not numbered.
    pub fn record(&self, mailbox: &str, name: &str, state: &str) -> Option<u64> {
        let mut maps = self
            .maps
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let map = maps.get_mut(mailbox)?;
        if !map.by_name.contains_key(name) {
            return None;
        }
        if let Some((modseq, known)) = map.modseqs.get(name) {
            if known == state {
                return Some(*modseq);
            }
        }
        map.highest += 1;
        let modseq = map.highest;
        map.modseqs.insert(name.to_string(), (modseq, state.to_string()));
        let snapshot = map.clone();
        drop(maps);
        self.save(mailbox, &snapshot);
        Some(modseq)
    }

    fn number_with(&self, mailbox: &str, names: &[String], states: Option<&HashMap<String, String>>) -> Numbered {
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
        // The expunged: remembered with the mod-sequence of their going.
        let gone: Vec<(String, u32)> = map
            .by_name
            .iter()
            .filter(|(name, _)| !present.contains(name.as_str()))
            .map(|(name, uid)| (name.clone(), *uid))
            .collect();
        for (name, uid) in &gone {
            map.highest += 1;
            map.vanished.push((*uid, map.highest));
            map.modseqs.remove(name);
        }
        if map.vanished.len() > MAX_VANISHED {
            let forgotten: Vec<(u32, u64)> = map.vanished.drain(..map.vanished.len() - MAX_VANISHED).collect();
            map.vanished_floor = forgotten
                .iter()
                .map(|(_, modseq)| *modseq)
                .fold(map.vanished_floor, u64::max);
        }
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
        let state_of = |name: &str| -> String {
            states
                .and_then(|states| states.get(name))
                .cloned()
                .unwrap_or_default()
        };
        for name in new {
            map.by_name.insert(name.clone(), map.next);
            map.next += 1;
            map.highest += 1;
            map.modseqs.insert(name.clone(), (map.highest, state_of(name)));
            changed = true;
        }
        // A message of a map from before CONDSTORE gets its first mod-sequence; a known one whose
        // state changed, a new one.
        let names_now: Vec<String> = map.by_name.keys().cloned().collect();
        for name in names_now {
            let state = states.and_then(|states| states.get(&name));
            match map.modseqs.get(&name) {
                None => {
                    map.highest += 1;
                    map.modseqs.insert(name.clone(), (map.highest, state_of(&name)));
                    changed = true;
                }
                Some((_, known)) if state.is_some_and(|state| state != known) => {
                    map.highest += 1;
                    map.modseqs.insert(name.clone(), (map.highest, state_of(&name)));
                    changed = true;
                }
                Some(_) => {}
            }
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
            highest_modseq: map.highest,
            modseqs: map
                .modseqs
                .iter()
                .map(|(name, (modseq, _))| (name.clone(), *modseq))
                .collect(),
            vanished: map.vanished.clone(),
            vanished_floor: map.vanished_floor,
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

    fn states(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(name, state)| (name.to_string(), state.to_string()))
            .collect()
    }

    /// CONDSTORE (RFC 7162): every change the bridge sees of a message - new, its flags - gets a
    /// mod-sequence higher than any before; an expunge too, remembered with the UID for QRESYNC's
    /// VANISHED; a STORE's change is recorded at once and not counted again.
    #[test]
    fn every_change_of_a_message_gets_a_higher_mod_sequence_and_an_expunge_is_remembered() {
        let maps = UidMaps::in_memory().with_clock(|| 7);
        let both = names(&["a.eml", "b.eml"]);
        let first = maps.track("Inbox", &both, &states(&[("a.eml", ""), ("b.eml", "")]));
        let (a, b) = (first.modseqs["a.eml"], first.modseqs["b.eml"]);
        assert!(a > 0 && b > a, "{first:?}");
        assert_eq!(first.highest_modseq, b);
        let same = maps.track("Inbox", &both, &states(&[("a.eml", ""), ("b.eml", "")]));
        assert_eq!(same.highest_modseq, first.highest_modseq, "nothing changed, nothing moves");

        let flagged = maps.track("Inbox", &both, &states(&[("a.eml", ""), ("b.eml", "\\Flagged")]));
        assert_eq!(flagged.modseqs["a.eml"], a);
        assert!(flagged.modseqs["b.eml"] > first.highest_modseq);
        assert_eq!(flagged.highest_modseq, flagged.modseqs["b.eml"]);

        let gone = maps.track("Inbox", &names(&["b.eml"]), &states(&[("b.eml", "\\Flagged")]));
        assert_eq!(gone.vanished.len(), 1, "{gone:?}");
        assert_eq!(gone.vanished[0].0, 1, "a's UID");
        assert!(gone.vanished[0].1 > flagged.highest_modseq);
        assert_eq!(gone.highest_modseq, gone.vanished[0].1);

        let stored = maps.record("Inbox", "b.eml", "\\Flagged \\Seen").expect("a numbered message");
        assert!(stored > gone.highest_modseq);
        let after = maps.track("Inbox", &names(&["b.eml"]), &states(&[("b.eml", "\\Flagged \\Seen")]));
        assert_eq!(after.modseqs["b.eml"], stored, "the same state is no second change");
        assert_eq!(after.highest_modseq, stored);
        // Plain numbering (an APPEND's UID) leaves the mod-sequences as they are.
        let numbered = maps.number("Inbox", &names(&["b.eml"]));
        assert_eq!(numbered.highest_modseq, stored);
    }

    #[test]
    fn the_mod_sequences_and_the_vanished_outlive_the_process() {
        let dir = TempDir::new("uids-modseq");
        let maps = UidMaps::in_folder(dir.0.join(UIDS_DIR)).with_clock(|| 100);
        maps.track("Inbox", &names(&["a.eml", "b.eml"]), &states(&[("a.eml", ""), ("b.eml", "")]));
        let before = maps.track("Inbox", &names(&["b.eml"]), &states(&[("b.eml", "\\Seen")]));
        let again = UidMaps::in_folder(dir.0.join(UIDS_DIR)).with_clock(|| 200);
        let after = again.track("Inbox", &names(&["b.eml"]), &states(&[("b.eml", "\\Seen")]));
        assert_eq!(after.highest_modseq, before.highest_modseq);
        assert_eq!(after.modseqs, before.modseqs);
        assert_eq!(after.vanished, before.vanished);
    }
}
