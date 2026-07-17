//! Text Input Manager
//!
//! Centralizes all text editing logic for contenteditable nodes.
//!
//! This manager handles text input from multiple sources:
//!
//! - Keyboard input (character insertion, backspace, etc.)
//! - IME composition (multi-character input for Asian languages)
//! - Accessibility actions (screen readers, voice control)
//! - Programmatic edits (from callbacks)
//!
//! ## Architecture
//!
//! The text input system uses a two-phase approach:
//!
//! 1. **Record Phase**: When text input occurs, record what changed (`old_text` + `inserted_text`)
//!
//!    - Store in `pending_changeset`
//!    - Do NOT modify any caches yet
//!    - Return affected nodes so callbacks can be invoked
//!
//! 2. **Apply Phase**: After callbacks, if preventDefault was not set:
//!
//!    - Compute new text using `text3::edit`
//!    - Update cursor position
//!    - Update text cache
//!    - Mark nodes dirty for re-layout
//!
//! This separation allows:
//!
//! - User callbacks to inspect the changeset before it's applied
//! - preventDefault to cancel the edit
//! - Consistent behavior across keyboard/IME/A11y sources

use azul_core::{
    dom::DomNodeId,
    events::{
        EventData, EventProvider, EventSource as CoreEventSource, EventType, SyntheticEvent,
        TextInputEventData,
    },
    task::Instant,
};
use azul_css::corety::AzString;

/// Information about a pending text edit that hasn't been applied yet
#[derive(Debug, Clone)]
#[repr(C)]
pub struct PendingTextEdit {
    /// The node that was edited
    pub node: DomNodeId,
    /// The text that was inserted
    pub inserted_text: AzString,
    /// The old text before the edit (plain text extracted from `InlineContent`)
    pub old_text: AzString,
}

impl PendingTextEdit {
    /// Preview the resulting text by appending `inserted_text` to `old_text`.
    ///
    /// NOTE: Actual cursor-based insertion is handled by `apply_text_changeset()`
    /// in window.rs via `text3::edit::insert_text()`.
    #[must_use] pub fn resulting_text(&self) -> AzString {
        let mut result = self.old_text.as_str().to_string();
        result.push_str(self.inserted_text.as_str());
        result.into()
    }
}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
/// C-compatible Option type for `PendingTextEdit`
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum OptionPendingTextEdit {
    None,
    Some(PendingTextEdit),
}

impl OptionPendingTextEdit {
    #[must_use] pub fn into_option(self) -> Option<PendingTextEdit> {
        match self {
            Self::None => None,
            Self::Some(t) => Some(t),
        }
    }
}

impl From<Option<PendingTextEdit>> for OptionPendingTextEdit {
    fn from(o: Option<PendingTextEdit>) -> Self {
        o.map_or_else(|| Self::None, Self::Some)
    }
}

impl<'a> From<Option<&'a PendingTextEdit>> for OptionPendingTextEdit {
    fn from(o: Option<&'a PendingTextEdit>) -> Self {
        o.map_or_else(|| Self::None, |v| Self::Some(v.clone()))
    }
}

/// Source of a text input event
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputSource {
    /// Regular keyboard input
    Keyboard,
    /// IME composition (multi-character input)
    Ime,
    /// Accessibility action from assistive technology
    Accessibility,
    /// Programmatic edit from user callback
    Programmatic,
}

/// Text Input Manager
///
/// Centralizes all text editing logic. This is the single source of truth
/// for text input state.
#[derive(Debug)]
pub struct TextInputManager {
    /// The pending text changeset that hasn't been applied yet.
    /// This is set during the "record" phase and cleared after the "apply" phase.
    pub pending_changeset: Option<PendingTextEdit>,
    /// Source of the current text input
    pub input_source: Option<TextInputSource>,
}

impl TextInputManager {
    /// Create a new `TextInputManager`
    #[must_use] pub const fn new() -> Self {
        Self {
            pending_changeset: None,
            input_source: None,
        }
    }

    /// Record a text input event (Phase 1)
    ///
    /// This ONLY records what text was inserted. It does NOT apply the changes yet.
    /// The changes are applied later in `apply_changeset()` if preventDefault is not set.
    ///
    /// # Arguments
    ///
    /// - `node` - The DOM node being edited
    /// - `inserted_text` - The text being inserted
    /// - `old_text` - The current text before the edit
    /// - `source` - Where the input came from (keyboard, IME, A11y, etc.)
    ///
    /// Returns the affected node for event generation.
    pub fn record_input(
        &mut self,
        node: DomNodeId,
        inserted_text: String,
        old_text: String,
        source: TextInputSource,
    ) -> DomNodeId {
        self.pending_changeset = Some(PendingTextEdit {
            node,
            inserted_text: inserted_text.into(),
            old_text: old_text.into(),
        });

        self.input_source = Some(source);

        node
    }

    /// Get the pending changeset (if any)
    #[must_use] pub const fn get_pending_changeset(&self) -> Option<&PendingTextEdit> {
        self.pending_changeset.as_ref()
    }

    /// Clear the pending changeset
    ///
    /// This is called after applying the changeset or if preventDefault was set.
    pub fn clear_changeset(&mut self) {
        self.pending_changeset = None;
        self.input_source = None;
    }
}

impl Default for TextInputManager {
    fn default() -> Self {
        Self::new()
    }
}

impl EventProvider for TextInputManager {
    /// Get pending text input events.
    ///
    /// If there's a pending changeset, returns an Input event for the affected node.
    /// The event data includes the old text and inserted text so callbacks can
    /// query the changeset.
    #[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
    fn get_pending_events(&self, timestamp: Instant) -> Vec<SyntheticEvent> {
        let mut events = Vec::new();

        if let Some(changeset) = &self.pending_changeset {
            let event_source = match self.input_source {
                Some(TextInputSource::Keyboard | TextInputSource::Ime) => {
                    CoreEventSource::User
                }
                Some(TextInputSource::Accessibility) => CoreEventSource::User, /* A11y is still */
                // user input
                Some(TextInputSource::Programmatic) => CoreEventSource::Programmatic,
                None => CoreEventSource::User,
            };

            // Generate Input event (fires on every keystroke).
            // Carry the edit details on the event itself (inserted/old text) so
            // callbacks read them straight off the event — like other event
            // types — without having to query `get_pending_changeset()`. The
            // edited node is available via `SyntheticEvent.target`.
            events.push(SyntheticEvent::new(
                EventType::Input,
                event_source,
                changeset.node,
                timestamp,
                EventData::TextInput(TextInputEventData {
                    inserted_text: changeset.inserted_text.as_str().to_string(),
                    old_text: changeset.old_text.as_str().to_string(),
                }),
            ));

            // Note: We don't generate Change events here - those are generated
            // when focus is lost or Enter is pressed (handled elsewhere)
        }

        events
    }
}

impl crate::managers::NodeIdRemap for TextInputManager {
    /// Remap the pending (recorded, not-yet-applied) text edit.
    ///
    /// If the target node was unmounted between "record" and "apply", the
    /// changeset is dropped: applying it would insert the text into whichever
    /// node inherited the index.
    fn remap_node_ids(&mut self, dom: azul_core::dom::DomId, map: &crate::managers::NodeIdMap) {
        let Some(ref mut pending) = self.pending_changeset else {
            return;
        };
        if let Some(new_id) = map.resolve_dom_node_id(dom, pending.node) { pending.node = new_id } else {
            self.pending_changeset = None;
            self.input_source = None;
        }
    }
}

#[cfg(test)]
mod autotest_generated {
    use azul_core::{
        dom::{DomId, DomNodeId, NodeId},
        styled_dom::NodeHierarchyItemId,
        task::SystemTick,
    };

    use super::*;
    use crate::managers::{NodeIdMap, NodeIdRemap};

    /// A `DomNodeId` for `(dom, node_index)` using the safe 1-based encoder.
    fn dom_node(dom: usize, index: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId { inner: dom },
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
        }
    }

    fn ts() -> Instant {
        Instant::Tick(SystemTick::new(0))
    }

    fn edit(old: &str, inserted: &str) -> PendingTextEdit {
        PendingTextEdit {
            node: dom_node(0, 0),
            inserted_text: inserted.to_string().into(),
            old_text: old.to_string().into(),
        }
    }

    /// Strings chosen to break naive byte/char slicing: combining marks, ZWJ
    /// sequences, regional-indicator flags, bidi overrides, NUL and control
    /// bytes, lone replacement chars.
    fn adversarial_strings() -> Vec<String> {
        vec![
            String::new(),
            "a".to_string(),
            "héllo".to_string(),
            "e\u{0301}\u{0300}\u{0327}".to_string(),
            "👨‍👩‍👧‍👦".to_string(),
            "🇩🇪🇫🇷".to_string(),
            "مرحبا بالعالم".to_string(),
            "\u{202E}override\u{202C}".to_string(),
            "\0nul\0inside\0".to_string(),
            "\r\n\t\u{0b}\u{0c}".to_string(),
            "\u{FFFD}\u{FEFF}".to_string(),
            "𝕥𝕖𝕩𝕥".to_string(),
            "a".repeat(1024),
        ]
    }

    fn text_input_data(ev: &SyntheticEvent) -> &TextInputEventData {
        match &ev.data {
            EventData::TextInput(d) => d,
            other => panic!("expected EventData::TextInput, got {other:?}"),
        }
    }

    // ---------------------------------------------------------------------
    // PendingTextEdit::resulting_text  (getter / round-trip)
    // ---------------------------------------------------------------------

    #[test]
    fn resulting_text_basic_append() {
        assert_eq!(edit("Hello", " World").resulting_text().as_str(), "Hello World");
    }

    #[test]
    fn resulting_text_empty_instance_is_empty() {
        assert_eq!(edit("", "").resulting_text().as_str(), "");
    }

    #[test]
    fn resulting_text_pure_deletion_keeps_old_text() {
        // A pure deletion records an empty `inserted_text`; the preview must be
        // the untouched old text, not an empty string.
        assert_eq!(edit("abc", "").resulting_text().as_str(), "abc");
    }

    #[test]
    fn resulting_text_is_exact_byte_concatenation_for_unicode() {
        for old in adversarial_strings() {
            for inserted in adversarial_strings() {
                let result = edit(&old, &inserted).resulting_text();
                let expected = format!("{old}{inserted}");
                assert_eq!(
                    result.as_str(),
                    expected.as_str(),
                    "concat mismatch for old={old:?} inserted={inserted:?}"
                );
                assert_eq!(
                    result.as_str().len(),
                    old.len() + inserted.len(),
                    "byte length must be additive (no normalization / no truncation)"
                );
            }
        }
    }

    #[test]
    fn resulting_text_preserves_interior_nul_bytes() {
        // AzString is length-prefixed, not NUL-terminated: an embedded NUL must
        // survive the String -> AzString -> &str round-trip untruncated.
        let result = edit("a\0b", "c\0d").resulting_text();
        assert_eq!(result.as_str(), "a\0bc\0d");
        assert_eq!(result.as_str().len(), 6);
        assert_eq!(result.as_str().matches('\0').count(), 2);
    }

    #[test]
    fn resulting_text_round_trips_every_adversarial_string() {
        // encode == decode: String -> AzString -> &str must be the identity.
        for s in adversarial_strings() {
            assert_eq!(edit(&s, "").resulting_text().as_str(), s.as_str());
            assert_eq!(edit("", &s).resulting_text().as_str(), s.as_str());
        }
    }

    #[test]
    fn resulting_text_huge_strings_do_not_panic_or_truncate() {
        let old = "a".repeat(300_000);
        let inserted = "b".repeat(200_000);
        let result = edit(&old, &inserted).resulting_text();
        assert_eq!(result.as_str().len(), 500_000);
        assert!(result.as_str().starts_with("aaaa"));
        assert!(result.as_str().ends_with("bbbb"));
    }

    #[test]
    fn resulting_text_is_pure_and_repeatable() {
        let e = edit("old", "new");
        let first = e.resulting_text();
        let second = e.resulting_text();
        assert_eq!(first.as_str(), second.as_str());
        // The receiver must be untouched by the preview.
        assert_eq!(e.old_text.as_str(), "old");
        assert_eq!(e.inserted_text.as_str(), "new");
    }

    // ---------------------------------------------------------------------
    // OptionPendingTextEdit::into_option  (round-trip)
    // ---------------------------------------------------------------------

    #[test]
    fn option_pending_text_edit_none_round_trip() {
        assert!(OptionPendingTextEdit::None.into_option().is_none());
        // Both `From<Option<T>>` and `From<Option<&T>>` exist — pin the owned one.
        assert!(
            OptionPendingTextEdit::from(None::<PendingTextEdit>)
                .into_option()
                .is_none()
        );
        assert!(
            OptionPendingTextEdit::from(None::<&PendingTextEdit>)
                .into_option()
                .is_none()
        );
    }

    #[test]
    fn option_pending_text_edit_some_round_trip_preserves_fields() {
        for s in adversarial_strings() {
            let original = PendingTextEdit {
                node: dom_node(7, 13),
                inserted_text: s.clone().into(),
                old_text: s.clone().into(),
            };
            let recovered = OptionPendingTextEdit::from(Some(original.clone()))
                .into_option()
                .expect("Some must round-trip to Some");
            assert_eq!(recovered.node, original.node);
            assert_eq!(recovered.inserted_text.as_str(), s.as_str());
            assert_eq!(recovered.old_text.as_str(), s.as_str());
        }
    }

    #[test]
    fn option_pending_text_edit_from_ref_deep_clones() {
        let original = edit("old", "ins");
        let cloned = OptionPendingTextEdit::from(Some(&original))
            .into_option()
            .expect("Some(&T) must map to Some");
        // Deep clone: the borrow is over, and both sides still hold their text.
        assert_eq!(cloned.old_text.as_str(), "old");
        assert_eq!(cloned.inserted_text.as_str(), "ins");
        assert_eq!(original.old_text.as_str(), "old");
    }

    // ---------------------------------------------------------------------
    // TextInputManager::new  (constructor invariants)
    // ---------------------------------------------------------------------

    #[test]
    fn new_starts_with_no_pending_state() {
        let m = TextInputManager::new();
        assert!(m.pending_changeset.is_none());
        assert!(m.input_source.is_none());
        assert!(m.get_pending_changeset().is_none());
        assert!(m.get_pending_events(ts()).is_empty());
    }

    #[test]
    fn default_matches_new() {
        let d = TextInputManager::default();
        assert!(d.pending_changeset.is_none());
        assert!(d.input_source.is_none());
    }

    // ---------------------------------------------------------------------
    // TextInputManager::record_input / get_pending_changeset / clear_changeset
    // ---------------------------------------------------------------------

    #[test]
    fn record_input_returns_the_node_it_was_given_and_stores_it() {
        let mut m = TextInputManager::new();
        let node = dom_node(3, 42);
        let returned = m.record_input(
            node,
            "abc".to_string(),
            "xyz".to_string(),
            TextInputSource::Keyboard,
        );
        assert_eq!(returned, node);

        let pending = m.get_pending_changeset().expect("changeset must be recorded");
        assert_eq!(pending.node, node);
        assert_eq!(pending.inserted_text.as_str(), "abc");
        assert_eq!(pending.old_text.as_str(), "xyz");
        assert_eq!(m.input_source, Some(TextInputSource::Keyboard));
    }

    #[test]
    fn record_input_survives_empty_unicode_and_huge_payloads() {
        let mut m = TextInputManager::new();
        for s in adversarial_strings() {
            let returned = m.record_input(
                dom_node(0, 0),
                s.clone(),
                s.clone(),
                TextInputSource::Ime,
            );
            assert_eq!(returned, dom_node(0, 0));
            let pending = m.get_pending_changeset().expect("recorded");
            assert_eq!(pending.inserted_text.as_str(), s.as_str());
            assert_eq!(pending.old_text.as_str(), s.as_str());
        }

        let huge = "z".repeat(1_000_000);
        m.record_input(
            dom_node(0, 0),
            huge.clone(),
            String::new(),
            TextInputSource::Programmatic,
        );
        assert_eq!(
            m.get_pending_changeset().expect("recorded").inserted_text.as_str().len(),
            1_000_000
        );
    }

    #[test]
    fn record_input_accepts_extreme_node_ids() {
        let mut m = TextInputManager::new();

        // usize::MAX DomId + the sentinel "no node" hierarchy id (DomNodeId::ROOT's).
        let none_node = DomNodeId {
            dom: DomId { inner: usize::MAX },
            node: NodeHierarchyItemId::NONE,
        };
        assert_eq!(m.record_input(none_node, "a".into(), "b".into(), TextInputSource::Keyboard), none_node);
        assert_eq!(m.get_pending_changeset().expect("recorded").node, none_node);

        // The largest representable (1-based encoded) node index.
        let max_node = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_raw(usize::MAX),
        };
        assert_eq!(m.record_input(max_node, "a".into(), "b".into(), TextInputSource::Keyboard), max_node);
        assert_eq!(m.get_pending_changeset().expect("recorded").node, max_node);

        assert_eq!(DomNodeId::ROOT.node.into_crate_internal(), None);
    }

    #[test]
    fn record_input_is_last_write_wins() {
        let mut m = TextInputManager::new();
        m.record_input(dom_node(0, 1), "first".into(), "old1".into(), TextInputSource::Keyboard);
        m.record_input(dom_node(1, 2), "second".into(), "old2".into(), TextInputSource::Accessibility);

        let pending = m.get_pending_changeset().expect("recorded");
        assert_eq!(pending.node, dom_node(1, 2));
        assert_eq!(pending.inserted_text.as_str(), "second");
        assert_eq!(pending.old_text.as_str(), "old2");
        assert_eq!(m.input_source, Some(TextInputSource::Accessibility));

        // Only ever ONE pending edit -> exactly one event, for the newest node.
        let events = m.get_pending_events(ts());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].target, dom_node(1, 2));
    }

    #[test]
    fn clear_changeset_is_idempotent_on_a_fresh_manager() {
        let mut m = TextInputManager::new();
        m.clear_changeset();
        m.clear_changeset();
        m.clear_changeset();
        assert!(m.get_pending_changeset().is_none());
        assert!(m.input_source.is_none());
    }

    #[test]
    fn clear_changeset_clears_both_changeset_and_source() {
        let mut m = TextInputManager::new();
        m.record_input(dom_node(0, 5), "x".into(), "y".into(), TextInputSource::Programmatic);
        assert!(m.get_pending_changeset().is_some());

        m.clear_changeset();
        assert!(m.get_pending_changeset().is_none());
        // A stale input_source would mislabel the NEXT event's EventSource.
        assert!(m.input_source.is_none());
        assert!(m.get_pending_events(ts()).is_empty());

        // Clearing twice must stay clean, not resurrect anything.
        m.clear_changeset();
        assert!(m.get_pending_changeset().is_none());
    }

    // ---------------------------------------------------------------------
    // EventProvider::get_pending_events  (invariants)
    // ---------------------------------------------------------------------

    #[test]
    fn no_events_without_a_pending_changeset() {
        assert!(TextInputManager::new().get_pending_events(ts()).is_empty());
    }

    #[test]
    fn pending_event_carries_text_verbatim() {
        for s in adversarial_strings() {
            let mut m = TextInputManager::new();
            m.record_input(
                dom_node(2, 9),
                s.clone(),
                format!("old-{s}"),
                TextInputSource::Keyboard,
            );

            let events = m.get_pending_events(ts());
            assert_eq!(events.len(), 1, "exactly one Input event per changeset");
            assert_eq!(events[0].event_type, EventType::Input);
            assert_eq!(events[0].target, dom_node(2, 9));

            let data = text_input_data(&events[0]);
            assert_eq!(data.inserted_text, s);
            assert_eq!(data.old_text, format!("old-{s}"));
        }
    }

    #[test]
    fn event_source_mapping_is_stable_for_every_input_source() {
        let cases = [
            (TextInputSource::Keyboard, CoreEventSource::User),
            (TextInputSource::Ime, CoreEventSource::User),
            (TextInputSource::Accessibility, CoreEventSource::User),
            (TextInputSource::Programmatic, CoreEventSource::Programmatic),
        ];
        for (input_source, expected) in cases {
            let mut m = TextInputManager::new();
            m.record_input(dom_node(0, 0), "a".into(), String::new(), input_source);
            let events = m.get_pending_events(ts());
            assert_eq!(events.len(), 1);
            assert_eq!(
                events[0].source, expected,
                "{input_source:?} must map to {expected:?}"
            );
        }
    }

    #[test]
    fn pending_changeset_without_a_source_defaults_to_user() {
        // Torn state: the fields are public, so a changeset can exist with no
        // recorded source. It must still produce a well-formed event.
        let m = TextInputManager {
            pending_changeset: Some(edit("old", "ins")),
            input_source: None,
        };
        let events = m.get_pending_events(ts());
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].source, CoreEventSource::User);
        assert_eq!(text_input_data(&events[0]).inserted_text, "ins");
    }

    #[test]
    fn get_pending_events_does_not_consume_the_changeset() {
        let mut m = TextInputManager::new();
        m.record_input(dom_node(0, 4), "a".into(), "b".into(), TextInputSource::Keyboard);
        assert_eq!(m.get_pending_events(ts()).len(), 1);
        // Reading events is a pure query — only clear_changeset() drains it.
        assert_eq!(m.get_pending_events(ts()).len(), 1);
        assert!(m.get_pending_changeset().is_some());
    }

    // ---------------------------------------------------------------------
    // NodeIdRemap  (stale-NodeId invariants)
    // ---------------------------------------------------------------------

    #[test]
    fn remap_rewrites_a_surviving_node() {
        let mut m = TextInputManager::new();
        m.record_input(dom_node(0, 3), "a".into(), "b".into(), TextInputSource::Keyboard);

        let map = NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(1))]);
        m.remap_node_ids(DomId::ROOT_ID, &map);

        let pending = m.get_pending_changeset().expect("mapped node must survive");
        assert_eq!(pending.node, dom_node(0, 1));
        assert_eq!(pending.inserted_text.as_str(), "a");
        assert_eq!(m.input_source, Some(TextInputSource::Keyboard));
    }

    #[test]
    fn remap_drops_changeset_and_source_when_the_node_is_unmounted() {
        let mut m = TextInputManager::new();
        m.record_input(dom_node(0, 3), "a".into(), "b".into(), TextInputSource::Ime);

        // Node 3 is absent from the map => it was unmounted. Applying the edit
        // would write into whichever node inherited index 3.
        let map = NodeIdMap::from_pairs([(NodeId::new(4), NodeId::new(3))]);
        m.remap_node_ids(DomId::ROOT_ID, &map);

        assert!(m.get_pending_changeset().is_none());
        assert!(m.input_source.is_none(), "source must be cleared with the changeset");
        assert!(m.get_pending_events(ts()).is_empty());
    }

    #[test]
    fn remap_with_an_empty_map_drops_everything() {
        let mut m = TextInputManager::new();
        m.record_input(dom_node(0, 0), "a".into(), "b".into(), TextInputSource::Keyboard);

        let map = NodeIdMap::from_pairs(Vec::<(NodeId, NodeId)>::new());
        assert!(map.is_empty());
        m.remap_node_ids(DomId::ROOT_ID, &map);

        assert!(m.get_pending_changeset().is_none());
        assert!(m.input_source.is_none());
    }

    #[test]
    fn remap_leaves_other_doms_untouched() {
        let mut m = TextInputManager::new();
        m.record_input(dom_node(1, 3), "a".into(), "b".into(), TextInputSource::Keyboard);

        // Reconciliation of DOM 0 says nothing about DOM 1's node ids.
        let map = NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(9))]);
        m.remap_node_ids(DomId::ROOT_ID, &map);

        let pending = m.get_pending_changeset().expect("other-DOM state must survive");
        assert_eq!(pending.node, dom_node(1, 3), "node id must NOT be rewritten");
    }

    #[test]
    fn remap_drops_a_changeset_recorded_on_the_none_node_sentinel() {
        // DomNodeId::ROOT carries NodeHierarchyItemId::NONE, which decodes to
        // `None` — it is unresolvable, so the edit must be dropped rather than
        // silently retargeted.
        let mut m = TextInputManager::new();
        m.record_input(DomNodeId::ROOT, "a".into(), "b".into(), TextInputSource::Keyboard);

        let map = NodeIdMap::from_pairs([(NodeId::new(0), NodeId::new(0))]);
        m.remap_node_ids(DomId::ROOT_ID, &map);

        assert!(m.get_pending_changeset().is_none());
        assert!(m.input_source.is_none());
    }

    #[test]
    fn remap_handles_extreme_node_indices() {
        let mut m = TextInputManager::new();
        let huge = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_raw(usize::MAX),
        };
        m.record_input(huge, "a".into(), "b".into(), TextInputSource::Keyboard);

        // from_raw(usize::MAX) decodes to NodeId(usize::MAX - 1); remapping it
        // down to a small index must not over/underflow the 1-based encoding.
        let map = NodeIdMap::from_pairs([(NodeId::new(usize::MAX - 1), NodeId::new(2))]);
        m.remap_node_ids(DomId::ROOT_ID, &map);

        assert_eq!(m.get_pending_changeset().expect("mapped").node, dom_node(0, 2));
    }

    #[test]
    fn remap_on_an_empty_manager_is_a_noop() {
        let mut m = TextInputManager::new();
        let map = NodeIdMap::from_pairs([(NodeId::new(0), NodeId::new(1))]);
        m.remap_node_ids(DomId::ROOT_ID, &map);
        assert!(m.get_pending_changeset().is_none());
        assert!(m.input_source.is_none());
    }

    #[test]
    fn remap_is_idempotent_when_ids_are_stable() {
        let mut m = TextInputManager::new();
        m.record_input(dom_node(0, 5), "a".into(), "b".into(), TextInputSource::Keyboard);

        let map = NodeIdMap::from_pairs([(NodeId::new(5), NodeId::new(5))]);
        m.remap_node_ids(DomId::ROOT_ID, &map);
        m.remap_node_ids(DomId::ROOT_ID, &map);

        assert_eq!(m.get_pending_changeset().expect("stable").node, dom_node(0, 5));
    }
}
