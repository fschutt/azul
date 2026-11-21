//! The ACTION JOURNAL: a bounded breadcrumb trail of what the user just did.
//!
//! Every event callback the engine dispatches leaves one entry — when it
//! fired, which node it hit, and the resolved handler name (the same
//! `cb:`-style resolution the probe uses: `dladdr`, then `addr2line`, then a
//! module-relative offset). A problem report or crash dump can then answer
//! "what happened right before this" without the app instrumenting anything.
//!
//! Bounded by construction: a fixed-capacity ring, oldest dropped first, so
//! a long session costs the same as a short one. Recording is OFF until
//! something enables it — the report dialog and the crash hook do, and an
//! app can via [`set_enabled`] — because an app that never files reports
//! should not pay even this much.
//!
//! It records HANDLER NAMES AND NODES, never user data: no text, no field
//! contents, no clipboard. What the user typed is their business; that a
//! `submit_form` handler ran on `#login` is the diagnostic.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use azul_core::dom::DomNodeId;

/// Entries kept before the oldest is dropped.
pub const DEFAULT_CAPACITY: usize = 64;

/// One dispatched callback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionEntry {
    /// Unix milliseconds when the callback was dispatched.
    pub unix_millis: u64,
    /// `dom.node` the event hit (`root.-` when the event had no node).
    pub node: String,
    /// Resolved handler name, or an address when no symbol was available.
    pub callback: String,
}

/// A journal: the ring of entries, whether it records, and how many entries it keeps.
///
/// An object, so that a test (or anything that wants a trail of its own) owns one and
/// never shares its entries with another; the process has ONE, reached through the free
/// functions of this module ([`process`]).
#[derive(Debug)]
pub struct ActionJournal {
    enabled: AtomicBool,
    capacity: AtomicUsize,
    ring: Mutex<Vec<ActionEntry>>,
}

impl ActionJournal {
    /// A journal that records nothing yet and keeps [`DEFAULT_CAPACITY`] entries.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            capacity: AtomicUsize::new(DEFAULT_CAPACITY),
            ring: Mutex::new(Vec::new()),
        }
    }

    /// Turns recording on or off. Off (the default) makes [`Self::record`] a single
    /// relaxed atomic load.
    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
        if !on {
            self.clear();
        }
    }

    /// Whether the journal is recording.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Sets how many entries are retained (minimum 1).
    pub fn set_capacity(&self, entries: usize) {
        self.capacity.store(entries.max(1), Ordering::Relaxed);
    }

    /// Drops every recorded entry.
    pub fn clear(&self) {
        if let Ok(mut ring) = self.ring.lock() {
            ring.clear();
        }
    }

    /// Records one dispatched callback. Cheap and non-blocking when disabled;
    /// a poisoned/contended lock drops the entry rather than stalling the UI
    /// thread — a breadcrumb is never worth a frame.
    pub fn record(&self, node: DomNodeId, callback_ptr: usize) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        let unix_millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
        let node_str = match node.node.into_crate_internal() {
            Some(id) => alloc::format!("{}.{}", node.dom.inner, id.index()),
            None => alloc::format!("{}.-", node.dom.inner),
        };
        let entry = ActionEntry {
            unix_millis,
            node: node_str,
            callback: crate::probe::callback_name(callback_ptr).to_string(),
        };
        let Ok(mut ring) = self.ring.try_lock() else {
            return;
        };
        let cap = self.capacity.load(Ordering::Relaxed).max(1);
        if ring.len() >= cap {
            let overflow = ring.len() - cap + 1;
            ring.drain(..overflow);
        }
        ring.push(entry);
    }

    /// The most recent entries, oldest first, at most `max` of them.
    #[must_use]
    pub fn recent(&self, max: usize) -> Vec<ActionEntry> {
        let Ok(ring) = self.ring.lock() else {
            return Vec::new();
        };
        let start = ring.len().saturating_sub(max);
        ring[start..].to_vec()
    }

    /// The most recent entries as a JSON array — what a report attaches.
    #[must_use]
    pub fn recent_json(&self, max: usize) -> String {
        let entries = self.recent(max);
        use core::fmt::Write as _;
        let mut out = String::from("[");
        for (i, e) in entries.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                r#"{{"unix_millis":{},"node":"{}","callback":"{}"}}"#,
                e.unix_millis,
                escape(&e.node),
                escape(&e.callback),
            );
        }
        out.push(']');
        out
    }
}

impl Default for ActionJournal {
    fn default() -> Self {
        Self::new()
    }
}

// ONE journal per process, on purpose: what records into it is the dispatcher of EVERY window,
// and what reads it is the crash hook (`std::panic::set_hook` is process-global) and the report
// dialog of another window - none of which has a window to ask. An app arms it through
// [`set_enabled`].
static PROCESS_JOURNAL: ActionJournal = ActionJournal::new();

/// The process's journal.
#[must_use]
pub const fn process() -> &'static ActionJournal {
    &PROCESS_JOURNAL
}

/// Turns recording of the process's journal on or off ([`ActionJournal::set_enabled`]).
pub fn set_enabled(on: bool) {
    process().set_enabled(on);
}

/// Whether the process's journal is recording.
#[must_use]
pub fn is_enabled() -> bool {
    process().is_enabled()
}

/// Sets how many entries the process's journal retains (minimum 1).
pub fn set_capacity(entries: usize) {
    process().set_capacity(entries);
}

/// Drops every entry of the process's journal.
pub fn clear() {
    process().clear();
}

/// Records one dispatched callback in the process's journal ([`ActionJournal::record`]).
pub fn record(node: DomNodeId, callback_ptr: usize) {
    process().record(node, callback_ptr);
}

/// The most recent entries of the process's journal, oldest first, at most `max` of them.
#[must_use]
pub fn recent(max: usize) -> Vec<ActionEntry> {
    process().recent(max)
}

/// The most recent entries of the process's journal as a JSON array — what a report attaches.
#[must_use]
pub fn recent_json(max: usize) -> String {
    process().recent_json(max)
}

/// Minimal JSON string escaping — journal fields are symbol names and ids,
/// but a symbol name can carry `"` or `\` and must not break the document.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                use core::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use azul_core::{
        dom::{DomId, DomNodeId},
        id::NodeId,
        styled_dom::NodeHierarchyItemId,
    };

    use super::*;

    fn node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    /// LAW: disabled means NOTHING is retained — the journal must not be a
    /// silent always-on recorder of what the user touched.
    #[test]
    fn recording_is_off_until_enabled() {
        let journal = ActionJournal::new();
        journal.record(node(1), 0);
        assert!(
            journal.recent(10).is_empty(),
            "a disabled journal must record nothing"
        );
        assert_eq!(journal.recent_json(10), "[]");
    }

    /// LAW: the ring is BOUNDED — a long session costs the same as a short
    /// one, and the entries kept are the most recent ones.
    #[test]
    fn the_ring_drops_the_oldest_and_keeps_the_newest() {
        let journal = ActionJournal::new();
        journal.set_enabled(true);
        journal.set_capacity(4);
        for i in 0..10 {
            journal.record(node(i), 0);
        }
        let kept = journal.recent(100);
        assert_eq!(kept.len(), 4, "capacity must bound the ring");
        assert_eq!(kept[0].node, "0.6", "oldest kept is entry 6 of 0..10");
        assert_eq!(kept[3].node, "0.9", "newest kept is the last recorded");
    }

    #[test]
    fn json_is_well_formed_and_escapes() {
        let journal = ActionJournal::new();
        journal.set_enabled(true);
        journal.set_capacity(8);
        journal.record(node(3), 0);
        let json = journal.recent_json(8);
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("the journal must emit parseable JSON");
        let arr = parsed.as_array().expect("a JSON array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["node"], "0.3");
        assert_eq!(escape(r#"a"b\c"#), r#"a\"b\\c"#);
    }

    /// Two journals share nothing: what one records the other never sees, so tests that
    /// run side by side each own their trail.
    #[test]
    fn a_journal_keeps_its_entries_to_itself() {
        let (a, b) = (ActionJournal::new(), ActionJournal::new());
        a.set_enabled(true);
        b.set_enabled(true);
        a.record(node(1), 0);
        assert_eq!(a.recent(10).len(), 1);
        assert!(b.recent(10).is_empty());
        a.set_enabled(false);
        assert!(a.recent(10).is_empty(), "turning it off drops what it kept");
        assert!(b.is_enabled(), "and leaves the other one on");
    }
}
