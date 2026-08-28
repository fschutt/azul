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
use std::sync::{Mutex, OnceLock};

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

static ENABLED: AtomicBool = AtomicBool::new(false);
static CAPACITY: AtomicUsize = AtomicUsize::new(DEFAULT_CAPACITY);

fn ring() -> &'static Mutex<Vec<ActionEntry>> {
    static RING: OnceLock<Mutex<Vec<ActionEntry>>> = OnceLock::new();
    RING.get_or_init(|| Mutex::new(Vec::new()))
}

/// Turns recording on or off. Off (the default) makes [`record`] a single
/// relaxed atomic load.
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
    if !on {
        clear();
    }
}

/// Whether the journal is recording.
#[must_use]
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Sets how many entries are retained (minimum 1).
pub fn set_capacity(entries: usize) {
    CAPACITY.store(entries.max(1), Ordering::Relaxed);
}

/// Drops every recorded entry.
pub fn clear() {
    if let Ok(mut ring) = ring().lock() {
        ring.clear();
    }
}

/// Records one dispatched callback. Cheap and non-blocking when disabled;
/// a poisoned/contended lock drops the entry rather than stalling the UI
/// thread — a breadcrumb is never worth a frame.
pub fn record(node: DomNodeId, callback_ptr: usize) {
    if !ENABLED.load(Ordering::Relaxed) {
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
    let Ok(mut ring) = ring().try_lock() else {
        return;
    };
    let cap = CAPACITY.load(Ordering::Relaxed).max(1);
    if ring.len() >= cap {
        let overflow = ring.len() - cap + 1;
        ring.drain(..overflow);
    }
    ring.push(entry);
}

/// The most recent entries, oldest first, at most `max` of them.
#[must_use]
pub fn recent(max: usize) -> Vec<ActionEntry> {
    let Ok(ring) = ring().lock() else {
        return Vec::new();
    };
    let start = ring.len().saturating_sub(max);
    ring[start..].to_vec()
}

/// The most recent entries as a JSON array — what a report attaches.
#[must_use]
pub fn recent_json(max: usize) -> String {
    let entries = recent(max);
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
    use super::*;
    use azul_core::id::NodeId;
    use azul_core::{
        dom::{DomId, DomNodeId},
        styled_dom::NodeHierarchyItemId,
    };

    /// The journal is process-global state, so its tests must not run
    /// concurrently with each other — without this they interleave
    /// enable/clear/record and fail at random.
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

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
        let _serial = serial();
        set_enabled(false);
        record(node(1), 0);
        assert!(
            recent(10).is_empty(),
            "a disabled journal must record nothing"
        );
        assert_eq!(recent_json(10), "[]");
    }

    /// LAW: the ring is BOUNDED — a long session costs the same as a short
    /// one, and the entries kept are the most recent ones.
    #[test]
    fn the_ring_drops_the_oldest_and_keeps_the_newest() {
        let _serial = serial();
        set_enabled(true);
        set_capacity(4);
        clear();
        for i in 0..10 {
            record(node(i), 0);
        }
        let kept = recent(100);
        assert_eq!(kept.len(), 4, "capacity must bound the ring");
        assert_eq!(kept[0].node, "0.6", "oldest kept is entry 6 of 0..10");
        assert_eq!(kept[3].node, "0.9", "newest kept is the last recorded");
        set_enabled(false);
        set_capacity(DEFAULT_CAPACITY);
    }

    #[test]
    fn json_is_well_formed_and_escapes() {
        let _serial = serial();
        set_enabled(true);
        set_capacity(8);
        clear();
        record(node(3), 0);
        let json = recent_json(8);
        let parsed: serde_json::Value =
            serde_json::from_str(&json).expect("the journal must emit parseable JSON");
        let arr = parsed.as_array().expect("a JSON array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["node"], "0.3");
        assert_eq!(escape(r#"a"b\c"#), r#"a\"b\\c"#);
        set_enabled(false);
        set_capacity(DEFAULT_CAPACITY);
    }
}
