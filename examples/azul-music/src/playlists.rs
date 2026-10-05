//! Playlists: a name and track ids, each playlist ONE file in the data tree,
//! `music/playlists/<id>.json` (the S3 split). Plain Rust, tested without a window.

use serde::{Deserialize, Serialize};

/// The folder of the playlists, in the app's folder of the data tree.
pub const PLAYLISTS_DIR: &str = "playlists";

/// One playlist.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Playlist {
    /// A stable id (a UUID): the file's name.
    pub id: String,
    pub name: String,
    /// Track ids (`library::Track::id`), in play order; a track may appear more than once.
    pub tracks: Vec<String>,
}

impl Playlist {
    /// An empty playlist called `name`.
    #[must_use]
    pub fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            tracks: Vec::new(),
        }
    }

    /// The playlist's file name in [`PLAYLISTS_DIR`]: `<id>.json`.
    #[must_use]
    pub fn file_name(&self) -> String {
        format!("{}.json", self.id)
    }

    /// The playlist from its file's text, or why not. The file is a JSON object with an id
    /// (serde would also read a struct from an array, so `[]` would be an empty, nameless
    /// playlist in the list).
    pub fn from_json(text: &str) -> Result<Playlist, String> {
        let value: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| format!("the playlist file does not read: {e}"))?;
        if !value.is_object() {
            return Err("the playlist file is not a playlist (a JSON object)".to_string());
        }
        let playlist: Playlist = serde_json::from_value(value)
            .map_err(|e| format!("the playlist file does not read: {e}"))?;
        if playlist.id.trim().is_empty() {
            return Err("the playlist file has no id".to_string());
        }
        Ok(playlist)
    }

    /// The playlist as its file's text.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Appends tracks.
    pub fn add(&mut self, ids: &[String]) {
        self.tracks.extend_from_slice(ids);
    }

    /// Removes the entry at `index` (not every entry of that track).
    pub fn remove_at(&mut self, index: usize) {
        if index < self.tracks.len() {
            self.tracks.remove(index);
        }
    }

    /// Moves the entry at `from` so it ends up at `to` (a drag in the list).
    pub fn move_entry(&mut self, from: usize, to: usize) {
        if from >= self.tracks.len() {
            return;
        }
        let entry = self.tracks.remove(from);
        let to = to.min(self.tracks.len());
        self.tracks.insert(to, entry);
    }

    /// Drops the entries whose track is no longer in the library (`known`). Returns how many.
    pub fn retain_known(&mut self, known: impl Fn(&str) -> bool) -> usize {
        let before = self.tracks.len();
        self.tracks.retain(|id| known(id));
        before - self.tracks.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_playlist_is_one_file_named_by_its_id_and_round_trips() {
        let mut p = Playlist::new("0f3a".into(), "Focus".into());
        assert_eq!(p.name, "Focus");
        assert!(p.tracks.is_empty());
        assert_eq!(p.file_name(), "0f3a.json");
        p.add(&ids(&["a", "b"]));
        assert_eq!(Playlist::from_json(&p.to_json()).expect("reads"), p);
        assert!(Playlist::from_json("[]").is_err());
    }

    #[test]
    fn entries_are_added_removed_and_moved_by_position() {
        let mut p = Playlist::new("x".into(), "Mix".into());
        p.add(&ids(&["a", "b", "a", "c"]));
        p.remove_at(2);
        assert_eq!(p.tracks, ids(&["a", "b", "c"]), "only that entry of a");
        p.move_entry(0, 2);
        assert_eq!(p.tracks, ids(&["b", "c", "a"]));
        p.move_entry(2, 0);
        assert_eq!(p.tracks, ids(&["a", "b", "c"]));
        p.move_entry(1, 99);
        assert_eq!(p.tracks, ids(&["a", "c", "b"]), "past the end is the end");
        p.remove_at(99);
        assert_eq!(p.tracks.len(), 3, "out of range removes nothing");
    }

    #[test]
    fn tracks_gone_from_the_library_are_dropped() {
        let mut p = Playlist::new("x".into(), "Mix".into());
        p.add(&ids(&["a", "gone", "b", "gone"]));
        assert_eq!(p.retain_known(|id| id != "gone"), 2);
        assert_eq!(p.tracks, ids(&["a", "b"]));
    }
}
