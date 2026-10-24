//! The loop detector: a file whose state here keeps returning to one it had within the hour
//! (deleted and restored, version A, B, A) is frozen and the user is asked, instead of the
//! sync flipping it for ever. Two devices that undo each other, a peer that deletes and puts
//! back, a tool that rewrites what the sync just wrote: each is a loop.
//!
//! A pass asks [`LoopDetector::frozen`] before it changes a file here, and tells
//! [`LoopDetector::applied`] once it did. A file whose state returns [`REPEATS`] times within
//! [`WINDOW_SECS`] is frozen. A growing file (a log) never repeats a state, so it never trips.
//! The memory lives in the pairing's state folder (`loops.json`).

use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};

use crate::{
    error::CloudResult,
    state::{read_json, write_json},
};

/// The memory's file in a pairing's state folder.
pub const LOOPS_FILE: &str = "loops.json";
/// How often a state may come back before the file is frozen.
pub const REPEATS: usize = 3;
/// How far back the detector looks (seconds).
pub const WINDOW_SECS: f64 = 3600.0;

/// What a file is here after a step: its content (and executable bit), or `None` for gone.
pub type State = Option<String>;

/// The states each file had here, with when.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LoopDetector {
    /// Per key, the states a pass left it in within the window, oldest first.
    #[serde(default)]
    pub history: BTreeMap<String, Vec<(f64, State)>>,
}

impl LoopDetector {
    /// The memory kept in `state_dir` (empty when there is none or it cannot be read: a lost
    /// memory only makes the next loop take longer to catch).
    #[must_use]
    pub fn load(state_dir: &Path) -> LoopDetector {
        read_json(&state_dir.join(LOOPS_FILE))
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Writes the memory into `state_dir` (0600).
    ///
    /// # Errors
    ///
    /// When it cannot be written.
    pub fn save(&self, state_dir: &Path) -> CloudResult<()> {
        write_json(&state_dir.join(LOOPS_FILE), self, true)
    }

    /// Whether changing `key` here into `next` would repeat a loop: the states within the
    /// window plus `next` hold [`REPEATS`] repeats or more.
    #[must_use]
    pub fn frozen(&self, key: &str, next: &State, now: f64) -> bool {
        let mut states: Vec<&State> = self.recent(key, now).map(|(_, s)| s).collect();
        states.push(next);
        repeats(&states) >= REPEATS
    }

    /// A pass left `key` in `state` at `now`. Older entries than the window are dropped.
    pub fn applied(&mut self, key: &str, state: State, now: f64) {
        let mut kept: Vec<(f64, State)> = self
            .recent(key, now)
            .map(|(t, s)| (t, s.clone()))
            .collect();
        kept.push((now, state));
        self.history.insert(key.to_string(), kept);
    }

    /// The user chose what `key` should be: it is no loop any more, its history starts afresh.
    pub fn release(&mut self, key: &str) {
        self.history.remove(key);
    }

    /// Drops every key with nothing left in the window (the memory stays small).
    pub fn forget_old(&mut self, now: f64) {
        self.history.retain(|_, states| {
            states.retain(|(t, _)| now - *t < WINDOW_SECS);
            !states.is_empty()
        });
    }

    fn recent<'a>(&'a self, key: &str, now: f64) -> impl Iterator<Item = (f64, &'a State)> + 'a {
        self.history
            .get(key)
            .into_iter()
            .flatten()
            .filter(move |(t, _)| now - *t < WINDOW_SECS)
            .map(|(t, s)| (*t, s))
    }
}

/// How many of `states` repeat one before them.
fn repeats(states: &[&State]) -> usize {
    let mut seen: Vec<&State> = Vec::new();
    let mut n = 0;
    for s in states {
        if seen.contains(s) {
            n += 1;
        } else {
            seen.push(s);
        }
    }
    n
}
