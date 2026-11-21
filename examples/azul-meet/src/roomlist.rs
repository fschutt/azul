//! The rooms this device is in, kept in the data tree as `meet/rooms.json` (azul-appkit's data
//! root, the same key the user's drive has): what brings a chat room back after a restart - its
//! id, code, kind, meeting server and times, the invite secret sealed with this device's local key
//! (`Identity::seal_local`, so the file opens on this device only), the newest message read (the
//! unread count), the departures this device saw (CRYPTO.md section 7) - and the devices the user
//! verified by their safety code. Pure: no azul types; read before the window opens, written on
//! the save thread (`store::save`).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The file in AzMeet's folder.
pub const INDEX_FILE: &str = "rooms.json";
/// The format this build writes.
const VERSION: u32 = 1;

/// One room of this device.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomEntry {
    pub room: String,
    #[serde(default)]
    pub code: String,
    /// `meeting` or `chat` (`chatroom::RoomKind`).
    #[serde(default)]
    pub kind: String,
    /// The meet Worker the room lives on.
    #[serde(default)]
    pub server: String,
    /// The meeting's times, seconds since 1970.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<u64>,
    /// The device (`Identity::device`) the entry belongs to: another device's entry is not this
    /// device's room.
    #[serde(default)]
    pub device: String,
    /// The invite secret sealed with the device's local key; `None` for a room joined by its code
    /// and not let in yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invite: Option<String>,
    /// Whether this device is a member (else it knocked).
    #[serde(default)]
    pub member: bool,
    /// The newest message read.
    #[serde(default)]
    pub read_seq: u64,
    /// The departures this device saw: device -> the time of its last record accepted here.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub departed: BTreeMap<String, u64>,
    /// When this device joined, seconds since 1970 (the list's order: the newest first).
    #[serde(default)]
    pub joined: u64,
}

/// `meet/rooms.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomIndex {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub rooms: Vec<RoomEntry>,
    /// The devices the user verified by their safety code: device -> the name it had then.
    #[serde(default)]
    pub verified: BTreeMap<String, String>,
}

impl RoomIndex {
    /// The index a file holds; an empty one for a file that does not read (and why, for a log line).
    #[must_use]
    pub fn parse(text: &str) -> (RoomIndex, Option<String>) {
        if text.trim().is_empty() {
            return (RoomIndex::default(), None);
        }
        match serde_json::from_str::<RoomIndex>(text) {
            Ok(index) if index.version <= VERSION => (index, None),
            Ok(index) => (
                RoomIndex::default(),
                Some(format!(
                    "written by a newer AzMeet (version {})",
                    index.version
                )),
            ),
            Err(e) => (RoomIndex::default(), Some(format!("not a room list: {e}"))),
        }
    }

    /// The file's text: pretty JSON with a final newline.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = self.clone();
        out.version = VERSION;
        let mut text = serde_json::to_string_pretty(&out).unwrap_or_else(|_| String::from("{}"));
        text.push('\n');
        text
    }

    /// The rooms of `device`, the newest first.
    #[must_use]
    pub fn rooms_of(&self, device: &str) -> Vec<&RoomEntry> {
        let mut rooms: Vec<&RoomEntry> = self.rooms.iter().filter(|r| r.device == device).collect();
        rooms.sort_by(|a, b| b.joined.cmp(&a.joined).then_with(|| a.room.cmp(&b.room)));
        rooms
    }

    /// The entry of `room` for `device`.
    #[must_use]
    pub fn get(&self, device: &str, room: &str) -> Option<&RoomEntry> {
        self.rooms
            .iter()
            .find(|r| r.device == device && r.room == room)
    }

    /// Keeps `entry`, replacing the one of the same room and device. True when that changed the
    /// index.
    pub fn upsert(&mut self, entry: RoomEntry) -> bool {
        match self
            .rooms
            .iter_mut()
            .find(|r| r.device == entry.device && r.room == entry.room)
        {
            Some(existing) if *existing == entry => false,
            Some(existing) => {
                *existing = entry;
                true
            }
            None => {
                self.rooms.push(entry);
                true
            }
        }
    }

    /// Forgets `room` of `device` (it left). True when it was there.
    pub fn remove(&mut self, device: &str, room: &str) -> bool {
        let before = self.rooms.len();
        self.rooms
            .retain(|r| !(r.device == device && r.room == room));
        self.rooms.len() != before
    }

    /// The user compared `device`'s safety code and marked it verified.
    pub fn verify(&mut self, device: &str, name: &str) -> bool {
        self.verified
            .insert(device.to_string(), name.to_string())
            .as_deref()
            != Some(name)
    }

    /// Whether the user verified `device`.
    #[must_use]
    pub fn is_verified(&self, device: &str) -> bool {
        self.verified.contains_key(device)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(device: &str, room: &str, joined: u64) -> RoomEntry {
        RoomEntry {
            room: room.to_string(),
            code: String::from("xq4-8kd-2nm"),
            kind: String::from("chat"),
            server: String::from("http://127.0.0.1:8790"),
            device: device.to_string(),
            invite: Some(String::from("c2VhbGVk")),
            member: true,
            joined,
            ..RoomEntry::default()
        }
    }

    #[test]
    fn the_list_round_trips_through_its_file_and_keeps_the_departures_and_the_verified() {
        let mut index = RoomIndex::default();
        let mut room = entry("dev-a", "room-1", 10);
        room.read_seq = 42;
        room.starts_at = Some(1_760_000_000);
        room.departed
            .insert(String::from("dev-c"), 1_759_999_000_000);
        assert!(index.upsert(room.clone()));
        assert!(
            !index.upsert(room.clone()),
            "the same entry again changes nothing"
        );
        assert!(index.verify("dev-b", "Ben"));
        assert!(!index.verify("dev-b", "Ben"));
        let text = index.to_json();
        assert!(text.contains("\"version\": 1"), "{text}");
        assert!(
            !text.contains("ends_at"),
            "an empty time is left out: {text}"
        );
        let (back, problem) = RoomIndex::parse(&text);
        assert_eq!(problem, None);
        assert_eq!(back.get("dev-a", "room-1"), Some(&room));
        assert!(back.is_verified("dev-b"));
        assert!(!back.is_verified("dev-c"));
    }

    #[test]
    fn a_device_sees_its_own_rooms_newest_first_and_leaving_forgets_one() {
        let mut index = RoomIndex::default();
        index.upsert(entry("dev-a", "old", 10));
        index.upsert(entry("dev-a", "new", 20));
        index.upsert(entry("dev-b", "theirs", 30));
        let mine: Vec<&str> = index
            .rooms_of("dev-a")
            .iter()
            .map(|r| r.room.as_str())
            .collect();
        assert_eq!(mine, vec!["new", "old"]);
        assert!(
            index.get("dev-a", "theirs").is_none(),
            "another device's room"
        );
        let mut moved = entry("dev-a", "old", 10);
        moved.read_seq = 7;
        assert!(index.upsert(moved));
        assert_eq!(index.get("dev-a", "old").map(|r| r.read_seq), Some(7));
        assert!(index.remove("dev-a", "old"));
        assert!(!index.remove("dev-a", "old"));
        assert_eq!(index.rooms_of("dev-a").len(), 1);
    }

    #[test]
    fn a_file_that_does_not_read_or_is_newer_is_an_empty_list_with_the_reason() {
        assert_eq!(RoomIndex::parse(""), (RoomIndex::default(), None));
        let (index, problem) = RoomIndex::parse("not json");
        assert_eq!(index, RoomIndex::default());
        assert!(problem.unwrap().contains("not a room list"));
        let (index, problem) = RoomIndex::parse(r#"{"version": 9, "rooms": []}"#);
        assert_eq!(index, RoomIndex::default());
        assert!(problem.unwrap().contains("newer"));
        let (index, _) = RoomIndex::parse(r#"{"rooms": [{"room": "r"}]}"#);
        assert_eq!(index.rooms.len(), 1, "every field but the room is optional");
    }
}
