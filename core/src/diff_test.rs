#[allow(unused_imports)]
pub use super::*;
#[cfg(test)]
mod audit_tests {
    use super::*;
    use crate::dom::NodeData;
    use crate::styled_dom::NodeHierarchyItem;

    // Build a NodeHierarchyItem from optional 0-based indices (encoded 1-based).
    fn hitem(
        parent: Option<usize>,
        prev: Option<usize>,
        next: Option<usize>,
        last_child: Option<usize>,
    ) -> NodeHierarchyItem {
        NodeHierarchyItem {
            parent: parent.map_or(0, |p| p + 1),
            previous_sibling: prev.map_or(0, |p| p + 1),
            next_sibling: next.map_or(0, |p| p + 1),
            last_child: last_child.map_or(0, |p| p + 1),
        }
    }

    // A deep parent chain that would overflow the stack with the old recursion.
    #[test]
    fn reconciliation_key_deep_chain_no_overflow() {
        let build = |n: usize| -> (Vec<NodeData>, Vec<NodeHierarchyItem>) {
            let node_data = (0..n).map(|_| NodeData::create_div()).collect();
            let hierarchy = (0..n)
                .map(|i| {
                    hitem(
                        if i == 0 { None } else { Some(i - 1) },
                        None,
                        None,
                        if i + 1 < n { Some(i + 1) } else { None },
                    )
                })
                .collect();
            (node_data, hierarchy)
        };

        // A very deep linear chain: the OLD recursion overflowed the stack here.
        // A single-node key walk is O(depth) and must complete without recursing.
        let n = 100_000usize;
        let (node_data, hierarchy) = build(n);
        let _ = calculate_reconciliation_key(&node_data, &hierarchy, NodeId::new(n - 1));
        let _ = calculate_contenteditable_key(&node_data, &hierarchy, NodeId::new(n - 1));

        // Whole-DOM precompute calls the per-node walk once per node, so over a
        // *linear* chain it is O(n²) — that only bites a pathological 100k-deep
        // DOM (never a real tree). Exercise the whole-DOM path over a modest
        // chain; correctness is covered by `reconciliation_key_single_node` and
        // `reconciliation_key_distinguishes_siblings`.
        let m = 2_000usize;
        let (nd, hi) = build(m);
        let keys = precompute_reconciliation_keys(&nd, &hi);
        assert_eq!(keys.len(), m);
    }

    // A cyclic (corrupt) hierarchy must terminate, not hang.
    #[test]
    fn reconciliation_key_cycle_terminates() {
        let node_data = vec![
            NodeData::create_div(),
            NodeData::create_div(),
            NodeData::create_div(),
        ];
        // node1.parent = 2, node2.parent = 1 — a cycle not involving root 0.
        let hierarchy = vec![
            hitem(None, None, None, None),
            hitem(Some(2), None, None, None),
            hitem(Some(1), None, None, None),
        ];
        let _ = calculate_reconciliation_key(&node_data, &hierarchy, NodeId::new(1));
        let _ = calculate_contenteditable_key(&node_data, &hierarchy, NodeId::new(1));
    }

    #[test]
    fn reconciliation_key_single_node() {
        let node_data = vec![NodeData::create_div()];
        let hierarchy = vec![hitem(None, None, None, None)];
        let direct = calculate_reconciliation_key(&node_data, &hierarchy, NodeId::new(0));
        let pre = precompute_reconciliation_keys(&node_data, &hierarchy)[0];
        assert_eq!(direct, pre);
    }

    #[test]
    fn reconciliation_key_distinguishes_siblings() {
        // root 0 with two div children 1 and 2 — nth-of-type must differ.
        let node_data = vec![NodeData::create_div(); 3];
        let hierarchy = vec![
            hitem(None, None, None, Some(2)), // root: first_child=1, last_child=2
            hitem(Some(0), None, Some(2), None), // child 1
            hitem(Some(0), Some(1), None, None), // child 2
        ];
        let k1 = calculate_reconciliation_key(&node_data, &hierarchy, NodeId::new(1));
        let k2 = calculate_reconciliation_key(&node_data, &hierarchy, NodeId::new(2));
        assert_ne!(k1, k2);
    }

    #[test]
    fn cursor_offsets_are_always_char_boundaries() {
        // "héllo": h=0, é=1..3 (2 bytes), l=3, l=4, o=5 (len 6).
        let old = "héllo";
        let new = "héllo wörld"; // ö is multibyte too
        for c in 0..=old.len() {
            let r = reconcile_cursor_position(old, new, c);
            assert!(
                new.is_char_boundary(r),
                "cursor {c} mapped to non-char-boundary offset {r} in {new:?}",
            );
            assert!(r <= new.len());
        }
        // Deletion inside a multibyte suffix must not split a codepoint.
        let r = reconcile_cursor_position("aömega", "bömega", 3);
        assert!("bömega".is_char_boundary(r));
    }

    #[test]
    fn cursor_prefix_unchanged_stays_put() {
        assert_eq!(reconcile_cursor_position("Hello", "Hello World", 5), 5);
    }

    #[test]
    fn cursor_empty_cases() {
        assert_eq!(reconcile_cursor_position("", "abc", 0), 3);
        assert_eq!(reconcile_cursor_position("abc", "", 2), 0);
        assert_eq!(reconcile_cursor_position("abc", "abc", 2), 2);
    }
}

// ============================================================================
// Autotest: adversarial unit tests
// ============================================================================
//
// Generated against the autotest task spec for `core/src/diff.rs`. Strategy per
// category:
//
//   * numeric      -> 0 / MIN / MAX / overflow / NaN / saturation
//   * "parser"-ish -> malformed, huge, boundary and unicode text input
//                     (`reconcile_cursor_position` is the byte-offset parser here)
//   * round-trip   -> precompute == per-node compute, fingerprint == recompute,
//                     BitOr == BitOrAssign
//   * getters /    -> invariants hold on default, empty and extreme instances
//     predicates
//
// The module is inline (not `core/tests/`) because `has_*_callback`,
// `create_lifecycle_event` and `ChangeAccumulator::classify_change_scope` are
// private to this module.
#[cfg(test)]
mod autotest_generated {
    use super::*;

    use azul_css::{
        css::CssPropertyValue,
        props::{layout::LayoutWidth, property::CssProperty},
    };

    use crate::{
        callbacks::CoreCallback,
        dom::{DatasetMergeCallbackType, TabIndex},
        geom::{LogicalPosition, LogicalSize},
        refany::{OptionRefAny, RefAny},
        resources::{ImageRef, RawImageFormat},
    };

    // ---------------------------------------------------------------- helpers

    // `CoreCallback::cb` is a raw `usize` fn-pointer slot. `reconcile_dom` only
    // ever inspects `CoreCallbackData::event`, never calls through the pointer,
    // so `0` is a safe sentinel (same convention as
    // `core/tests/reconciliation/deep_reconciliation.rs`).
    fn noop_callback() -> CoreCallback {
        CoreCallback {
            cb: 0usize,
            ctx: OptionRefAny::None,
        }
    }

    fn with_cb(mut nd: NodeData, filter: ComponentEventFilter) -> NodeData {
        nd.add_callback(
            EventFilter::Component(filter),
            RefAny::new(0u32),
            noop_callback(),
        );
        nd
    }

    // Build a NodeHierarchyItem from optional 0-based indices (encoded 1-based).
    fn hitem(
        parent: Option<usize>,
        prev: Option<usize>,
        next: Option<usize>,
        last_child: Option<usize>,
    ) -> NodeHierarchyItem {
        NodeHierarchyItem {
            parent: parent.map_or(0, |p| p + 1),
            previous_sibling: prev.map_or(0, |p| p + 1),
            next_sibling: next.map_or(0, |p| p + 1),
            last_child: last_child.map_or(0, |p| p + 1),
        }
    }

    fn rect(w: f32, h: f32) -> LogicalRect {
        LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(w, h))
    }

    fn layout_of(entries: &[(usize, LogicalRect)]) -> OrderedMap<NodeId, LogicalRect> {
        let mut m = OrderedMap::default();
        for (idx, r) in entries {
            m.insert(NodeId::new(*idx), *r);
        }
        m
    }

    fn no_layout() -> OrderedMap<NodeId, LogicalRect> {
        OrderedMap::default()
    }

    // Flat diff: empty hierarchies exercise the documented "degrade gracefully"
    // path of the structural reconciliation key.
    fn diff_flat(old: &[NodeData], new: &[NodeData]) -> DiffResult {
        reconcile_dom(
            old,
            new,
            &[],
            &[],
            &no_layout(),
            &no_layout(),
            DomId::ROOT_ID,
            Instant::now(),
        )
    }

    fn count_events(r: &DiffResult, t: EventType) -> usize {
        r.events.iter().filter(|e| e.event_type == t).count()
    }

    fn id_node(id: &str) -> NodeData {
        NodeData::create_div().with_ids_and_classes(vec![IdOrClass::Id(id.into())].into())
    }

    fn class_node(class: &str) -> NodeData {
        NodeData::create_div().with_ids_and_classes(vec![IdOrClass::Class(class.into())].into())
    }

    // A representative unicode torture corpus: multi-byte, combining marks, RTL,
    // ZWJ emoji sequences, CJK, and a lone BOM.
    const UNICODE_SAMPLES: &[&str] = &[
        "",
        "a",
        "héllo",
        "e\u{0301}galite\u{0301}", // combining acute accents
        "مرحبا بالعالم",           // RTL Arabic
        "👨‍👩‍👧‍👦 family",               // ZWJ emoji sequence
        "日本語のテキスト",
        "\u{feff}bom-prefixed",
        "🇩🇪🇫🇷🇯🇵", // regional indicator pairs
        "mixed 漢字 and ascii ✅",
    ];

    // ========================================================================
    // NodeChangeSet — constructor / predicates / numeric bit ops
    // ========================================================================

    #[test]
    fn autotest_changeset_empty_is_a_neutral_element() {
        let e = NodeChangeSet::empty();
        assert_eq!(e.bits, 0);
        assert!(e.is_empty());
        assert!(!e.needs_layout());
        assert!(!e.needs_paint());
        assert!(e.is_visually_unchanged());
        // `Default` must agree with `empty()`.
        assert_eq!(NodeChangeSet::default(), e);
        // Neutral under BitOr in both directions.
        let mut some = NodeChangeSet::empty();
        some.insert(NodeChangeSet::TEXT_CONTENT);
        assert_eq!((some | e).bits, some.bits);
        assert_eq!((e | some).bits, some.bits);
    }

    #[test]
    fn autotest_changeset_contains_zero_is_vacuously_true() {
        // `contains` is an ALL-bits test: `(bits & 0) == 0` holds for every
        // value, including the empty set. Pin the semantics so a future rewrite
        // to `(bits & flag) != 0` (an ANY-bits test) is caught.
        assert!(NodeChangeSet::empty().contains(0));
        let mut s = NodeChangeSet::empty();
        s.insert(NodeChangeSet::CALLBACKS);
        assert!(s.contains(0));
        assert!(NodeChangeSet { bits: u32::MAX }.contains(0));
    }

    #[test]
    fn autotest_changeset_intersects_zero_is_always_false() {
        // `intersects` is an ANY-bits test: masking with 0 can never be non-zero.
        assert!(!NodeChangeSet::empty().intersects(0));
        assert!(!NodeChangeSet { bits: u32::MAX }.intersects(0));
    }

    #[test]
    fn autotest_changeset_contains_is_all_bits_intersects_is_any_bits() {
        let mut s = NodeChangeSet::empty();
        s.insert(NodeChangeSet::TEXT_CONTENT);

        let both = NodeChangeSet::TEXT_CONTENT | NodeChangeSet::IMAGE_CHANGED;
        assert!(!s.contains(both), "contains() must require ALL bits");
        assert!(s.intersects(both), "intersects() must require ANY bit");
        assert!(s.contains(NodeChangeSet::TEXT_CONTENT));
    }

    #[test]
    fn autotest_changeset_insert_min_max_and_idempotent() {
        // MIN (0) is a no-op.
        let mut s = NodeChangeSet::empty();
        s.insert(0);
        assert!(s.is_empty());

        // MAX must not panic and must saturate to "all bits set".
        let mut s = NodeChangeSet::empty();
        s.insert(u32::MAX);
        assert_eq!(s.bits, u32::MAX);
        // Inserting again is idempotent (OR, not ADD -> cannot overflow).
        s.insert(u32::MAX);
        assert_eq!(s.bits, u32::MAX);
        s.insert(NodeChangeSet::TEXT_CONTENT);
        assert_eq!(s.bits, u32::MAX);

        // With every bit set, all defined flags are present.
        assert!(s.contains(NodeChangeSet::NODE_TYPE_CHANGED));
        assert!(s.contains(NodeChangeSet::AFFECTS_LAYOUT));
        assert!(s.contains(NodeChangeSet::AFFECTS_PAINT));
        assert!(s.needs_layout());
        assert!(s.needs_paint());
        assert!(!s.is_visually_unchanged());
        assert!(!s.is_empty());
    }

    #[test]
    fn autotest_changeset_undefined_high_bits_trigger_no_work() {
        // Bits outside every defined flag must not be interpreted as layout or
        // paint work — but the set is still non-empty.
        let s = NodeChangeSet {
            bits: 0b1000_0000_0000_0000_0000_0000_0000_0000,
        };
        assert!(!s.is_empty());
        assert!(!s.needs_layout());
        assert!(!s.needs_paint());
        assert!(s.is_visually_unchanged());
    }

    #[test]
    fn autotest_changeset_layout_and_paint_masks_are_disjoint() {
        // A single flag must never mean "relayout AND repaint" — the two
        // composite masks partition the visual flags.
        assert_eq!(
            NodeChangeSet::AFFECTS_LAYOUT & NodeChangeSet::AFFECTS_PAINT,
            0,
            "AFFECTS_LAYOUT and AFFECTS_PAINT must not overlap",
        );

        for flag in [
            NodeChangeSet::NODE_TYPE_CHANGED,
            NodeChangeSet::TEXT_CONTENT,
            NodeChangeSet::IDS_AND_CLASSES,
            NodeChangeSet::INLINE_STYLE_LAYOUT,
            NodeChangeSet::CHILDREN_CHANGED,
            NodeChangeSet::IMAGE_CHANGED,
            NodeChangeSet::CONTENTEDITABLE,
            NodeChangeSet::INLINE_STYLE_PAINT,
            NodeChangeSet::STYLED_STATE,
            NodeChangeSet::CALLBACKS,
            NodeChangeSet::DATASET,
            NodeChangeSet::ACCESSIBILITY,
        ] {
            let mut s = NodeChangeSet::empty();
            s.insert(flag);
            assert!(
                !(s.needs_layout() && s.needs_paint()),
                "flag {flag:#b} is both"
            );
            // `is_visually_unchanged` is exactly "neither layout nor paint".
            assert_eq!(
                s.is_visually_unchanged(),
                !s.needs_layout() && !s.needs_paint(),
                "is_visually_unchanged() disagrees with needs_layout/needs_paint for {flag:#b}",
            );
        }
    }

    #[test]
    fn autotest_changeset_nonvisual_flags_are_visually_unchanged() {
        let mut s = NodeChangeSet::empty();
        s.insert(NodeChangeSet::CALLBACKS);
        s.insert(NodeChangeSet::DATASET);
        s.insert(NodeChangeSet::ACCESSIBILITY);
        s.insert(NodeChangeSet::TAB_INDEX); // TAB_INDEX is in neither mask
        assert!(!s.is_empty());
        assert!(s.is_visually_unchanged());
        assert!(!s.needs_layout());
        assert!(!s.needs_paint());
    }

    #[test]
    fn autotest_changeset_bitor_matches_bitorassign() {
        // Round-trip: the two operators must agree, and BitOr must be
        // commutative + idempotent for arbitrary (including undefined) bits.
        for (a, b) in [
            (0u32, 0u32),
            (0, u32::MAX),
            (u32::MAX, u32::MAX),
            (NodeChangeSet::TEXT_CONTENT, NodeChangeSet::STYLED_STATE),
            (0xDEAD_BEEF, 0x0BAD_F00D),
        ] {
            let (sa, sb) = (NodeChangeSet { bits: a }, NodeChangeSet { bits: b });

            let by_operator = sa | sb;
            assert_eq!(by_operator.bits, a | b);

            let mut by_assign = sa;
            by_assign |= sb;
            assert_eq!(by_assign, by_operator);

            assert_eq!((sb | sa).bits, by_operator.bits, "BitOr must commute");
            assert_eq!((by_operator | by_operator).bits, by_operator.bits);
        }
    }

    // ========================================================================
    // NodeChangeReport — getters / predicates
    // ========================================================================

    #[test]
    fn autotest_change_report_default_is_inert() {
        let r = NodeChangeReport::default();
        assert!(!r.needs_layout());
        assert!(!r.needs_paint());
        assert!(r.is_visually_unchanged());
        assert_eq!(r.relayout_scope, RelayoutScope::None);
        assert!(r.changed_css_properties.is_empty());
        assert!(r.text_change.is_none());
    }

    #[test]
    fn autotest_change_report_scope_alone_forces_layout() {
        // An empty change_set with a non-None scope must still request layout:
        // `needs_layout()` ORs the two sources.
        let r = NodeChangeReport {
            relayout_scope: RelayoutScope::IfcOnly,
            ..Default::default()
        };
        assert!(r.needs_layout());
        assert!(!r.needs_paint());
        assert!(!r.is_visually_unchanged());
    }

    #[test]
    fn autotest_change_report_paint_flag_does_not_force_layout() {
        let mut r = NodeChangeReport::default();
        r.change_set.insert(NodeChangeSet::STYLED_STATE);
        assert!(!r.needs_layout());
        assert!(r.needs_paint());
        assert!(!r.is_visually_unchanged());
    }

    // ========================================================================
    // reconcile_cursor_position — the byte-offset "parser": unicode + boundary
    // ========================================================================

    #[test]
    fn autotest_cursor_result_is_always_a_valid_slice_index() {
        // The core safety invariant: whatever comes back must be <= new.len()
        // AND land on a char boundary, or a later `&new_text[..cursor]` panics.
        // Sweep every in-range cursor over every pair of the unicode corpus.
        for old in UNICODE_SAMPLES {
            for new in UNICODE_SAMPLES {
                for cursor in 0..=old.len() {
                    let r = reconcile_cursor_position(old, new, cursor);
                    assert!(
                        r <= new.len(),
                        "cursor {cursor} in {old:?} -> {r} exceeds len of {new:?}",
                    );
                    assert!(
                        new.is_char_boundary(r),
                        "cursor {cursor} in {old:?} -> {r} splits a codepoint in {new:?}",
                    );
                    // Must be usable as a real slice index.
                    let _ = &new[..r];
                }
            }
        }
    }

    #[test]
    fn autotest_cursor_is_deterministic() {
        // Same inputs must always give the same answer (no hashing / iteration
        // order leaking into the result).
        for old in UNICODE_SAMPLES {
            for new in UNICODE_SAMPLES {
                let a = reconcile_cursor_position(old, new, old.len() / 2);
                let b = reconcile_cursor_position(old, new, old.len() / 2);
                assert_eq!(a, b);
            }
        }
    }

    #[test]
    fn autotest_cursor_zero_stays_zero_when_texts_differ_at_byte_zero() {
        // cursor 0 <= common_prefix (0) -> snap(0) == 0.
        assert_eq!(reconcile_cursor_position("abc", "xyz", 0), 0);
        assert_eq!(reconcile_cursor_position("日本", "中国", 0), 0);
    }

    #[test]
    fn autotest_cursor_identical_text_clamps_to_len() {
        // Equal texts short-circuit to `snap(cursor)`, which clamps to len and
        // snaps down to a char boundary — so even an absurd cursor is safe here.
        assert_eq!(reconcile_cursor_position("abc", "abc", usize::MAX), 3);
        assert_eq!(reconcile_cursor_position("héllo", "héllo", usize::MAX), 6);
        // Snapping down: byte 2 is mid-'é' (bytes 1..3) -> snaps to 1.
        assert_eq!(reconcile_cursor_position("héllo", "héllo", 2), 1);
    }

    #[test]
    fn autotest_cursor_empty_sides_are_documented_constants() {
        // Empty old  -> end of new. Empty new -> 0. Both empty -> 0 (equal-text path).
        for new in UNICODE_SAMPLES {
            assert_eq!(reconcile_cursor_position("", new, 0), new.len());
            assert_eq!(reconcile_cursor_position("", new, usize::MAX), new.len());
        }
        for old in UNICODE_SAMPLES {
            if old.is_empty() {
                continue; // equal-text path, covered above
            }
            assert_eq!(reconcile_cursor_position(old, "", 0), 0);
            assert_eq!(reconcile_cursor_position(old, "", old.len()), 0);
        }
    }

    #[test]
    fn autotest_cursor_appended_text_keeps_prefix_cursor() {
        // Pure append: any cursor inside the common prefix is untouched.
        let old = "Hello";
        let new = "Hello, World";
        for cursor in 0..=old.len() {
            assert_eq!(reconcile_cursor_position(old, new, cursor), cursor);
        }
    }

    #[test]
    fn autotest_cursor_deleted_tail_clamps_into_new_text() {
        // Pure truncation: a cursor past the end of the new text must land at
        // the new end, never beyond it.
        let old = "Hello, World";
        let new = "Hello";
        assert_eq!(reconcile_cursor_position(old, new, old.len()), new.len());
        assert_eq!(reconcile_cursor_position(old, new, 5), 5);
    }

    #[test]
    fn autotest_cursor_multibyte_insert_before_cursor_shifts_by_suffix_rule() {
        // Insert a 2-byte 'ö' at the front; a cursor sitting in the (unchanged)
        // suffix must keep its distance from the END of the string.
        let old = "mega";
        let new = "ömega";
        let r = reconcile_cursor_position(old, new, 4); // end of old
        assert_eq!(r, new.len());
        assert!(new.is_char_boundary(r));
    }

    #[test]
    fn autotest_cursor_huge_inputs_do_not_hang_or_panic() {
        // 200k-byte strings: the prefix/suffix scans are linear, so this must
        // complete quickly and stay in-bounds.
        let old: String = std::iter::repeat_n('a', 200_000).collect();
        let mut new = old.clone();
        new.push_str("tail");

        let r = reconcile_cursor_position(&old, &new, old.len());
        assert!(r <= new.len());
        assert!(new.is_char_boundary(r));

        // Huge multibyte string: every returned offset must still be a boundary.
        let old_u: String = std::iter::repeat_n('é', 50_000).collect();
        let new_u: String = std::iter::repeat_n('é', 49_999).collect();
        let r = reconcile_cursor_position(&old_u, &new_u, old_u.len());
        assert!(r <= new_u.len());
        assert!(new_u.is_char_boundary(r));
    }

    // Regression test for a former underflow: `reconcile_cursor_position` used
    // to compute `old_text.len() - old_cursor_byte` unchecked, which panicked
    // (debug) / wrapped (release) when the caller passed a cursor byte offset
    // PAST the end of `old_text`. Reaching it needs: old != new, both non-empty,
    // cursor > common_prefix and cursor >= old_suffix_start — e.g.
    // ("abc", "abd", usize::MAX). The fix uses `saturating_sub` so an
    // out-of-range cursor clamps to the end of the new text like every other
    // path here.
    #[test]
    fn autotest_cursor_out_of_range_cursor_must_saturate_not_underflow() {
        // Expected: clamp to the end of the new text, exactly like every other path.
        assert_eq!(reconcile_cursor_position("abc", "abd", usize::MAX), 3);
        assert_eq!(reconcile_cursor_position("abc", "abd", 99), 3);
        assert_eq!(reconcile_cursor_position("héllo", "héllx", usize::MAX), 6);
    }

    // ========================================================================
    // get_node_text_content — round-trip
    // ========================================================================

    #[test]
    fn autotest_text_content_round_trips_unicode() {
        for s in UNICODE_SAMPLES {
            let node = NodeData::create_text_do_not_use_without_block_level_wrapper(*s);
            assert_eq!(
                get_node_text_content(&node),
                Some(*s),
                "create_text -> get_node_text_content must round-trip {s:?}",
            );
        }
    }

    #[test]
    fn autotest_text_content_is_none_for_non_text_nodes() {
        assert_eq!(get_node_text_content(&NodeData::create_div()), None);
        assert_eq!(get_node_text_content(&NodeData::create_body()), None);
        assert_eq!(get_node_text_content(&NodeData::create_br()), None);
        let img = NodeData::create_image(ImageRef::null_image(
            1,
            1,
            RawImageFormat::RGBA8,
            Vec::new(),
        ));
        assert_eq!(get_node_text_content(&img), None);
    }

    // ========================================================================
    // has_*_callback predicates
    // ========================================================================

    #[test]
    fn autotest_callback_predicates_all_false_without_callbacks() {
        let n = NodeData::create_div();
        assert!(!has_mount_callback(&n));
        assert!(!has_unmount_callback(&n));
        assert!(!has_resize_callback(&n));
        assert!(!has_update_callback(&n));
    }

    #[test]
    fn autotest_callback_predicates_are_mutually_exclusive_per_filter() {
        // Each predicate must recognise exactly its own ComponentEventFilter.
        let cases = [
            (
                ComponentEventFilter::AfterMount,
                [true, false, false, false],
            ),
            (
                ComponentEventFilter::BeforeUnmount,
                [false, true, false, false],
            ),
            (
                ComponentEventFilter::NodeResized,
                [false, false, true, false],
            ),
            (ComponentEventFilter::Updated, [false, false, false, true]),
            // A Component filter that none of the four predicates handle.
            (ComponentEventFilter::Selected, [false, false, false, false]),
            (
                ComponentEventFilter::DefaultAction,
                [false, false, false, false],
            ),
        ];

        for (filter, expected) in cases {
            let n = with_cb(NodeData::create_div(), filter);
            let got = [
                has_mount_callback(&n),
                has_unmount_callback(&n),
                has_resize_callback(&n),
                has_update_callback(&n),
            ];
            assert_eq!(got, expected, "predicate mismatch for {filter:?}");
        }
    }

    #[test]
    fn autotest_callback_predicates_find_target_among_many() {
        // The target callback is last of several — `any()` must still find it.
        let mut n = NodeData::create_div();
        for f in [
            ComponentEventFilter::Selected,
            ComponentEventFilter::DefaultAction,
            ComponentEventFilter::NodeResized,
        ] {
            n = with_cb(n, f);
        }
        assert!(has_resize_callback(&n));
        assert!(!has_mount_callback(&n));
    }

    // ========================================================================
    // create_lifecycle_event (private)
    // ========================================================================

    #[test]
    fn autotest_lifecycle_event_fields_are_wired_consistently() {
        let ts = Instant::now();
        let ev = create_lifecycle_event(
            EventType::Mount,
            NodeId::new(1_000_000),
            DomId::ROOT_ID,
            &ts,
            LifecycleEventData {
                reason: LifecycleReason::InitialMount,
                previous_bounds: None,
                current_bounds: rect(1.0, 2.0),
            },
        );

        assert_eq!(ev.event_type, EventType::Mount);
        assert_eq!(ev.source, EventSource::Lifecycle);
        assert_eq!(ev.phase, EventPhase::Target);
        // A lifecycle event is delivered at its target, so the two must agree.
        assert_eq!(ev.target, ev.current_target);
        assert_eq!(
            ev.target.node.into_crate_internal(),
            Some(NodeId::new(1_000_000)),
            "NodeId must survive the 1-based NodeHierarchyItemId encoding",
        );
        assert!(!ev.stopped);
        assert!(!ev.stopped_immediate);
        assert!(!ev.prevented_default);

        let EventData::Lifecycle(data) = &ev.data else {
            panic!("expected EventData::Lifecycle, got {:?}", ev.data);
        };
        assert!(data.previous_bounds.is_none());
        assert_eq!(data.current_bounds, rect(1.0, 2.0));
    }

    // ========================================================================
    // compute_node_changes
    // ========================================================================

    #[test]
    fn autotest_compute_changes_identical_nodes_report_nothing() {
        let a = NodeData::create_text_do_not_use_without_block_level_wrapper("same");
        let b = NodeData::create_text_do_not_use_without_block_level_wrapper("same");
        let changes = compute_node_changes(&a, &b, None, None);
        assert!(
            changes.is_empty(),
            "identical nodes must produce no change flags, got {:#b}",
            changes.bits,
        );
        assert!(changes.is_visually_unchanged());
    }

    #[test]
    fn autotest_compute_changes_node_type_change_short_circuits_everything() {
        // The documented early-return: when the discriminant changes, NOTHING
        // else is inspected — even though these two nodes ALSO differ in
        // classes, callbacks, inline CSS, tab index and contenteditable, and
        // sit in different styled states.
        let old = NodeData::create_div();
        let new = with_cb(
            NodeData::create_text_do_not_use_without_block_level_wrapper("now a text node")
                .with_ids_and_classes(vec![IdOrClass::Class("brand-new".into())].into())
                .with_css("width: 10px")
                .with_tab_index(TabIndex::NoKeyboardFocus)
                .with_contenteditable(true),
            ComponentEventFilter::AfterMount,
        );

        let hovered = StyledNodeState {
            hover: true,
            ..StyledNodeState::default()
        };
        let changes = compute_node_changes(
            &old,
            &new,
            Some(&StyledNodeState::default()),
            Some(&hovered),
        );
        assert_eq!(
            changes.bits,
            NodeChangeSet::NODE_TYPE_CHANGED,
            "a node-type change must be reported alone (early return)",
        );
    }

    #[test]
    fn autotest_compute_changes_text_content_unicode() {
        for (i, s) in UNICODE_SAMPLES.iter().enumerate() {
            let old = NodeData::create_text_do_not_use_without_block_level_wrapper(*s);

            // Same text -> no TEXT_CONTENT flag.
            let same = NodeData::create_text_do_not_use_without_block_level_wrapper(*s);
            assert!(
                !compute_node_changes(&old, &same, None, None)
                    .contains(NodeChangeSet::TEXT_CONTENT),
                "identical text {s:?} must not report TEXT_CONTENT",
            );

            // Different text -> TEXT_CONTENT flag.
            let other = UNICODE_SAMPLES[(i + 1) % UNICODE_SAMPLES.len()];
            if other == *s {
                continue;
            }
            let changed = NodeData::create_text_do_not_use_without_block_level_wrapper(other);
            assert!(
                compute_node_changes(&old, &changed, None, None)
                    .contains(NodeChangeSet::TEXT_CONTENT),
                "{s:?} -> {other:?} must report TEXT_CONTENT",
            );
        }
    }

    #[test]
    fn autotest_compute_changes_paint_only_css_never_sets_layout() {
        // `color` is RelayoutScope::None -> paint bucket only.
        let old = NodeData::create_div().with_css("color: red");
        let new = NodeData::create_div().with_css("color: blue");
        let changes = compute_node_changes(&old, &new, None, None);

        assert!(changes.contains(NodeChangeSet::INLINE_STYLE_PAINT));
        assert!(
            !changes.contains(NodeChangeSet::INLINE_STYLE_LAYOUT),
            "a paint-only property must not request relayout",
        );
        assert!(changes.needs_paint());
        assert!(!changes.needs_layout());
    }

    #[test]
    fn autotest_compute_changes_sizing_css_sets_layout_not_paint() {
        // `width` is RelayoutScope::SizingOnly -> layout bucket.
        let old = NodeData::create_div().with_css("width: 10px");
        let new = NodeData::create_div().with_css("width: 20px");
        let changes = compute_node_changes(&old, &new, None, None);

        assert!(changes.contains(NodeChangeSet::INLINE_STYLE_LAYOUT));
        assert!(!changes.contains(NodeChangeSet::INLINE_STYLE_PAINT));
        assert!(changes.needs_layout());
    }

    #[test]
    fn autotest_compute_changes_detects_removed_property() {
        // Regression guard for the AUDIT note at diff.rs:270 — a property that
        // exists only on the OLD node (i.e. was removed) must still be marked.
        let old = NodeData::create_div().with_css("color: red");
        let new = NodeData::create_div();
        let changes = compute_node_changes(&old, &new, None, None);
        assert!(
            changes.contains(NodeChangeSet::INLINE_STYLE_PAINT),
            "removing an inline property must be reported, got {:#b}",
            changes.bits,
        );
    }

    #[test]
    fn autotest_compute_changes_detects_added_property() {
        let old = NodeData::create_div();
        let new = NodeData::create_div().with_css("width: 5px");
        let changes = compute_node_changes(&old, &new, None, None);
        assert!(changes.contains(NodeChangeSet::INLINE_STYLE_LAYOUT));
    }

    #[test]
    fn autotest_compute_changes_ids_and_classes() {
        let changes = compute_node_changes(&class_node("a"), &class_node("b"), None, None);
        assert!(changes.contains(NodeChangeSet::IDS_AND_CLASSES));

        // Same classes -> no flag.
        let changes = compute_node_changes(&class_node("a"), &class_node("a"), None, None);
        assert!(!changes.contains(NodeChangeSet::IDS_AND_CLASSES));
    }

    #[test]
    fn autotest_compute_changes_styled_state() {
        let n = NodeData::create_div();
        let calm = StyledNodeState::default();
        let hovered = StyledNodeState {
            hover: true,
            ..StyledNodeState::default()
        };

        let changes = compute_node_changes(&n, &n, Some(&calm), Some(&hovered));
        assert!(changes.contains(NodeChangeSet::STYLED_STATE));
        assert!(changes.needs_paint());
        assert!(!changes.needs_layout());

        // Same state -> no flag; and None/None -> no flag.
        assert!(!compute_node_changes(&n, &n, Some(&calm), Some(&calm))
            .contains(NodeChangeSet::STYLED_STATE));
        assert!(!compute_node_changes(&n, &n, None, None).contains(NodeChangeSet::STYLED_STATE));

        // None vs Some(default) are *different* inputs and must be reported.
        assert!(
            compute_node_changes(&n, &n, None, Some(&calm)).contains(NodeChangeSet::STYLED_STATE)
        );
    }

    #[test]
    fn autotest_compute_changes_tab_index_and_contenteditable() {
        let plain = NodeData::create_div();

        let editable = NodeData::create_div().with_contenteditable(true);
        let changes = compute_node_changes(&plain, &editable, None, None);
        assert!(changes.contains(NodeChangeSet::CONTENTEDITABLE));
        assert!(
            changes.needs_layout(),
            "CONTENTEDITABLE is in AFFECTS_LAYOUT"
        );

        let tabbed = NodeData::create_div().with_tab_index(TabIndex::OverrideInParent(3));
        let changes = compute_node_changes(&plain, &tabbed, None, None);
        assert!(changes.contains(NodeChangeSet::TAB_INDEX));
        // TAB_INDEX is in neither composite mask -> no visual work.
        assert!(changes.is_visually_unchanged());
    }

    #[test]
    fn autotest_compute_changes_callbacks_count_and_identity() {
        let plain = NodeData::create_div();
        let one = with_cb(NodeData::create_div(), ComponentEventFilter::AfterMount);

        // Different callback counts.
        let changes = compute_node_changes(&plain, &one, None, None);
        assert!(changes.contains(NodeChangeSet::CALLBACKS));
        assert!(
            changes.is_visually_unchanged(),
            "callbacks are not a visual change"
        );

        // Same count, different event filter.
        let other = with_cb(NodeData::create_div(), ComponentEventFilter::BeforeUnmount);
        let changes = compute_node_changes(&one, &other, None, None);
        assert!(changes.contains(NodeChangeSet::CALLBACKS));

        // Same count, same filter -> no flag (cb pointer 0 == 0).
        let same = with_cb(NodeData::create_div(), ComponentEventFilter::AfterMount);
        let changes = compute_node_changes(&one, &same, None, None);
        assert!(!changes.contains(NodeChangeSet::CALLBACKS));
    }

    #[test]
    fn autotest_compute_changes_image_identity_is_by_image_id() {
        // `ImageRef` hashes its process-unique `id`: shallow clones share it,
        // every fresh `null_image()` gets a new one.
        let img = ImageRef::null_image(4, 4, RawImageFormat::RGBA8, Vec::new());
        let same = NodeData::create_image(img.clone());
        let also_same = NodeData::create_image(img.clone());
        assert!(
            !compute_node_changes(&same, &also_same, None, None)
                .contains(NodeChangeSet::IMAGE_CHANGED),
            "two nodes holding clones of the SAME ImageRef must not report a change",
        );

        // A distinct allocation, even with identical pixels/dimensions, is a
        // different image as far as reconciliation is concerned.
        let other = NodeData::create_image(ImageRef::null_image(
            4,
            4,
            RawImageFormat::RGBA8,
            Vec::new(),
        ));
        let changes = compute_node_changes(&same, &other, None, None);
        assert!(changes.contains(NodeChangeSet::IMAGE_CHANGED));
        assert!(changes.needs_layout(), "IMAGE_CHANGED is in AFFECTS_LAYOUT");
    }

    // ========================================================================
    // calculate_reconciliation_key / precompute_reconciliation_keys
    // ========================================================================

    #[test]
    fn autotest_rec_key_empty_node_data_is_safe() {
        assert!(precompute_reconciliation_keys(&[], &[]).is_empty());
    }

    #[test]
    fn autotest_rec_key_precompute_matches_per_node_calculation() {
        // Round-trip: the O(1)-lookup table must agree with the direct call for
        // every node — the whole point of precomputing.
        let node_data = vec![
            NodeData::create_div(),
            class_node("row"),
            NodeData::create_text_do_not_use_without_block_level_wrapper("leaf"),
            id_node("footer"),
        ];
        let hierarchy = vec![
            hitem(None, None, None, Some(3)),
            hitem(Some(0), None, Some(2), None),
            hitem(Some(0), Some(1), Some(3), None),
            hitem(Some(0), Some(2), None, None),
        ];

        let keys = precompute_reconciliation_keys(&node_data, &hierarchy);
        assert_eq!(keys.len(), node_data.len());
        for (i, k) in keys.iter().enumerate() {
            assert_eq!(
                *k,
                calculate_reconciliation_key(&node_data, &hierarchy, NodeId::new(i)),
                "precomputed key for node {i} disagrees with the direct call",
            );
        }
    }

    #[test]
    fn autotest_rec_key_explicit_key_beats_css_id_and_node_type() {
        // Priority 1 is absolute: it ignores the CSS ID, the classes, the node
        // type and the position in the tree.
        let bare = NodeData::create_div().with_key(7u32);
        let decorated =
            NodeData::create_text_do_not_use_without_block_level_wrapper("totally different")
                .with_key(7u32)
                .with_ids_and_classes(
                    vec![IdOrClass::Id("hero".into()), IdOrClass::Class("x".into())].into(),
                );

        let a = calculate_reconciliation_key(&[bare], &[], NodeId::new(0));
        let b = calculate_reconciliation_key(&[decorated], &[], NodeId::new(0));
        assert_eq!(
            a, b,
            "an explicit .with_key() must dominate every other input"
        );
    }

    #[test]
    fn autotest_rec_key_css_id_used_when_no_explicit_key() {
        let same_a = calculate_reconciliation_key(&[id_node("hero")], &[], NodeId::new(0));
        let same_b = calculate_reconciliation_key(&[id_node("hero")], &[], NodeId::new(0));
        let other = calculate_reconciliation_key(&[id_node("footer")], &[], NodeId::new(0));

        assert_eq!(same_a, same_b, "the CSS-ID key must be stable");
        assert_ne!(
            same_a, other,
            "different CSS IDs must produce different keys"
        );
    }

    #[test]
    fn autotest_rec_key_classes_participate_in_the_structural_key() {
        let a = calculate_reconciliation_key(&[class_node("alpha")], &[], NodeId::new(0));
        let b = calculate_reconciliation_key(&[class_node("beta")], &[], NodeId::new(0));
        assert_ne!(a, b, "classes must feed the structural key");
    }

    #[test]
    fn autotest_rec_key_node_type_participates_in_the_structural_key() {
        let div = calculate_reconciliation_key(&[NodeData::create_div()], &[], NodeId::new(0));
        let txt = calculate_reconciliation_key(
            &[NodeData::create_text_do_not_use_without_block_level_wrapper("x")],
            &[],
            NodeId::new(0),
        );
        assert_ne!(
            div, txt,
            "the node-type discriminant must feed the structural key"
        );
    }

    #[test]
    fn autotest_rec_key_hierarchy_shorter_than_node_data_is_safe() {
        // A truncated / absent hierarchy must degrade to the documented
        // "discriminant + classes" key instead of panicking.
        let node_data = vec![NodeData::create_div(), class_node("a"), id_node("b")];

        let with_none = precompute_reconciliation_keys(&node_data, &[]);
        let with_short =
            precompute_reconciliation_keys(&node_data, &[hitem(None, None, None, None)]);

        assert_eq!(with_none.len(), 3);
        assert_eq!(with_short.len(), 3);
        // Node 0 is a root either way, so both spellings must agree on it.
        assert_eq!(with_none[0], with_short[0]);
    }

    #[test]
    fn autotest_rec_key_identical_leaves_under_different_parents_differ() {
        // The parent chain must be folded in, otherwise keyless nodes under
        // unrelated parents would collide and migrate state across subtrees.
        //
        //   0 root
        //   ├── 1 (#left)   ── 3 div
        //   └── 2 (#right)  ── 4 div
        let node_data = vec![
            NodeData::create_div(),
            id_node("left"),
            id_node("right"),
            NodeData::create_div(),
            NodeData::create_div(),
        ];
        let hierarchy = vec![
            hitem(None, None, None, Some(2)),       // 0: children 1,2
            hitem(Some(0), None, Some(2), Some(3)), // 1: child 3
            hitem(Some(0), Some(1), None, Some(4)), // 2: child 4
            hitem(Some(1), None, None, None),       // 3
            hitem(Some(2), None, None, None),       // 4
        ];

        let k3 = calculate_reconciliation_key(&node_data, &hierarchy, NodeId::new(3));
        let k4 = calculate_reconciliation_key(&node_data, &hierarchy, NodeId::new(4));
        assert_ne!(
            k3, k4,
            "identical leaves under different parents must not share a key"
        );
    }

    // ========================================================================
    // calculate_contenteditable_key
    // ========================================================================

    #[test]
    fn autotest_contenteditable_key_is_deterministic_and_honours_explicit_keys() {
        let node_data = vec![NodeData::create_div().with_key(99u64)];
        let a = calculate_contenteditable_key(&node_data, &[], NodeId::new(0));
        let b = calculate_contenteditable_key(&node_data, &[], NodeId::new(0));
        assert_eq!(a, b, "must be deterministic");

        // Priority 1 is shared with the reconciliation key: for an explicitly
        // keyed node both functions return the SAME value.
        assert_eq!(
            a,
            calculate_reconciliation_key(&node_data, &[], NodeId::new(0)),
            "explicit keys must be identical across both key functions",
        );
    }

    #[test]
    fn autotest_contenteditable_key_distinguishes_nth_of_type() {
        // <div><p>A</p><p contenteditable>B</p></div> — the two same-type
        // siblings must not collide (nth-of-type is folded in).
        let node_data = vec![
            NodeData::create_div(),
            NodeData::create_text_do_not_use_without_block_level_wrapper("A"),
            NodeData::create_text_do_not_use_without_block_level_wrapper("B"),
        ];
        let hierarchy = vec![
            hitem(None, None, None, Some(2)),
            hitem(Some(0), None, Some(2), None),
            hitem(Some(0), Some(1), None, None),
        ];

        let k1 = calculate_contenteditable_key(&node_data, &hierarchy, NodeId::new(1));
        let k2 = calculate_contenteditable_key(&node_data, &hierarchy, NodeId::new(2));
        assert_ne!(k1, k2, "same-type siblings must differ by nth-of-type");
    }

    #[test]
    fn autotest_contenteditable_key_empty_hierarchy_is_safe() {
        let node_data = vec![NodeData::create_div(), class_node("editor")];
        for i in 0..node_data.len() {
            let k = calculate_contenteditable_key(&node_data, &[], NodeId::new(i));
            assert_eq!(
                k,
                calculate_contenteditable_key(&node_data, &[], NodeId::new(i))
            );
        }
    }

    // ========================================================================
    // reconcile_dom
    // ========================================================================

    #[test]
    fn autotest_reconcile_empty_to_empty_is_a_no_op() {
        let r = diff_flat(&[], &[]);
        assert!(r.events.is_empty());
        assert!(r.node_moves.is_empty());
    }

    #[test]
    fn autotest_reconcile_mount_and_unmount_need_a_callback_to_fire() {
        // Without an AfterMount callback the node still mounts — it just fires
        // no event. Same for unmount. The events are opt-in.
        let silent_new = vec![NodeData::create_div()];
        let r = diff_flat(&[], &silent_new);
        assert!(r.events.is_empty(), "no callback -> no event");
        assert!(r.node_moves.is_empty());

        let loud_new = vec![with_cb(
            NodeData::create_div(),
            ComponentEventFilter::AfterMount,
        )];
        let r = diff_flat(&[], &loud_new);
        assert_eq!(count_events(&r, EventType::Mount), 1);

        let loud_old = vec![with_cb(
            NodeData::create_div(),
            ComponentEventFilter::BeforeUnmount,
        )];
        let r = diff_flat(&loud_old, &[]);
        assert_eq!(count_events(&r, EventType::Unmount), 1);
        assert!(r.node_moves.is_empty());
    }

    #[test]
    fn autotest_reconcile_node_moves_are_a_bijection() {
        // 50 indistinguishable divs on both sides: every old node must be
        // claimed exactly once and every new node must claim at most one old
        // node. A queue bug (double-consume) would break this immediately.
        let old: Vec<NodeData> = (0..50).map(|_| NodeData::create_div()).collect();
        let new: Vec<NodeData> = (0..50).map(|_| NodeData::create_div()).collect();

        let r = diff_flat(&old, &new);
        assert_eq!(r.node_moves.len(), 50);

        let mut seen_old = [false; 50];
        let mut seen_new = [false; 50];
        for m in &r.node_moves {
            assert!(!seen_old[m.old_node_id.index()], "old node claimed twice");
            assert!(!seen_new[m.new_node_id.index()], "new node matched twice");
            seen_old[m.old_node_id.index()] = true;
            seen_new[m.new_node_id.index()] = true;
        }
        assert!(
            seen_old.iter().all(|b| *b),
            "every old node must be claimed"
        );
        assert!(
            seen_new.iter().all(|b| *b),
            "every new node must be matched"
        );
        assert!(r.events.is_empty(), "no lifecycle callbacks -> no events");
    }

    #[test]
    fn autotest_reconcile_surplus_new_nodes_mount_and_surplus_old_unmount() {
        // 50 old, 60 new -> 50 matches + 10 mounts, no unmounts.
        let old: Vec<NodeData> = (0..50).map(|_| NodeData::create_div()).collect();
        let new: Vec<NodeData> = (0..60)
            .map(|_| with_cb(NodeData::create_div(), ComponentEventFilter::AfterMount))
            .collect();

        let r = diff_flat(&old, &new);
        assert_eq!(r.node_moves.len(), 50);
        assert_eq!(count_events(&r, EventType::Mount), 10);
        assert_eq!(count_events(&r, EventType::Unmount), 0);

        // 50 old, 40 new -> 40 matches + 10 unmounts.
        let old: Vec<NodeData> = (0..50)
            .map(|_| with_cb(NodeData::create_div(), ComponentEventFilter::BeforeUnmount))
            .collect();
        let new: Vec<NodeData> = (0..40).map(|_| NodeData::create_div()).collect();

        let r = diff_flat(&old, &new);
        assert_eq!(r.node_moves.len(), 40);
        assert_eq!(count_events(&r, EventType::Unmount), 10);
        assert_eq!(count_events(&r, EventType::Mount), 0);
    }

    #[test]
    fn autotest_reconcile_explicit_key_mismatch_mounts_instead_of_guessing() {
        // The documented rule: an explicit `.with_key()` that finds no partner
        // must NOT fall through to the content/structural tiers, even though
        // the two nodes are otherwise byte-identical.
        let old = vec![with_cb(
            NodeData::create_text_do_not_use_without_block_level_wrapper("same content")
                .with_key(1u32),
            ComponentEventFilter::BeforeUnmount,
        )];
        let new = vec![with_cb(
            NodeData::create_text_do_not_use_without_block_level_wrapper("same content")
                .with_key(2u32),
            ComponentEventFilter::AfterMount,
        )];

        let r = diff_flat(&old, &new);
        assert!(
            r.node_moves.is_empty(),
            "keys 1 and 2 must not match, got {:?}",
            r.node_moves,
        );
        assert_eq!(count_events(&r, EventType::Mount), 1);
        assert_eq!(count_events(&r, EventType::Unmount), 1);
    }

    #[test]
    fn autotest_reconcile_update_fires_only_on_rec_key_match_with_changed_content() {
        // Same key, changed text, Updated callback present -> Update event.
        let old =
            vec![NodeData::create_text_do_not_use_without_block_level_wrapper("v1").with_key(1u32)];
        let new = vec![with_cb(
            NodeData::create_text_do_not_use_without_block_level_wrapper("v2").with_key(1u32),
            ComponentEventFilter::Updated,
        )];

        let r = diff_flat(&old, &new);
        assert_eq!(r.node_moves.len(), 1, "the key must match across frames");
        assert_eq!(count_events(&r, EventType::Update), 1);

        // Same key, SAME content -> no Update. Both frames must be byte-identical
        // for this: `NodeData::hash` folds in the callback events too (dom.rs:1579),
        // so the Updated handler has to be present on BOTH sides — otherwise the
        // hashes differ for the callback alone and we'd be testing nothing.
        let stable = with_cb(
            NodeData::create_text_do_not_use_without_block_level_wrapper("v1").with_key(1u32),
            ComponentEventFilter::Updated,
        );
        let old = vec![stable.clone()];
        let new = vec![stable];

        let r = diff_flat(&old, &new);
        assert_eq!(r.node_moves.len(), 1);
        assert_eq!(
            count_events(&r, EventType::Update),
            0,
            "unchanged content must not fire Update",
        );
    }

    #[test]
    fn autotest_reconcile_update_requires_the_callback() {
        // Content changed under a stable key, but no Updated callback -> silent.
        let old =
            vec![NodeData::create_text_do_not_use_without_block_level_wrapper("v1").with_key(1u32)];
        let new =
            vec![NodeData::create_text_do_not_use_without_block_level_wrapper("v2").with_key(1u32)];
        let r = diff_flat(&old, &new);
        assert_eq!(r.node_moves.len(), 1);
        assert!(r.events.is_empty());
    }

    #[test]
    fn autotest_reconcile_missing_layout_entries_default_to_zero_rect() {
        // Neither side has layout data: `unwrap_or(LogicalRect::zero())` means
        // the sizes compare equal, so no Resize fires and nothing panics.
        let old = vec![NodeData::create_div()];
        let new = vec![with_cb(
            NodeData::create_div(),
            ComponentEventFilter::NodeResized,
        )];

        let r = diff_flat(&old, &new);
        assert_eq!(r.node_moves.len(), 1);
        assert_eq!(
            count_events(&r, EventType::Resize),
            0,
            "zero-vs-zero bounds must not be treated as a resize",
        );
    }

    #[test]
    fn autotest_reconcile_resize_fires_with_previous_and_current_bounds() {
        let old = vec![NodeData::create_div()];
        let new = vec![with_cb(
            NodeData::create_div(),
            ComponentEventFilter::NodeResized,
        )];

        let r = reconcile_dom(
            &old,
            &new,
            &[],
            &[],
            &layout_of(&[(0, rect(100.0, 50.0))]),
            &layout_of(&[(0, rect(100.0, 80.0))]),
            DomId::ROOT_ID,
            Instant::now(),
        );

        assert_eq!(count_events(&r, EventType::Resize), 1);
        let EventData::Lifecycle(data) = &r.events[0].data else {
            panic!("resize event must carry EventData::Lifecycle");
        };
        assert_eq!(data.reason, LifecycleReason::Resize);
        assert_eq!(data.previous_bounds, Some(rect(100.0, 50.0)));
        assert_eq!(data.current_bounds, rect(100.0, 80.0));
    }

    #[test]
    fn autotest_reconcile_resize_ignores_pure_translation() {
        // Only `size` is compared — moving a node must not fire Resize.
        let old = vec![NodeData::create_div()];
        let new = vec![with_cb(
            NodeData::create_div(),
            ComponentEventFilter::NodeResized,
        )];

        let moved = LogicalRect::new(
            LogicalPosition::new(999.0, 999.0),
            LogicalSize::new(10.0, 10.0),
        );
        let r = reconcile_dom(
            &old,
            &new,
            &[],
            &[],
            &layout_of(&[(0, rect(10.0, 10.0))]),
            &layout_of(&[(0, moved)]),
            DomId::ROOT_ID,
            Instant::now(),
        );
        assert_eq!(count_events(&r, EventType::Resize), 0);
    }

    #[test]
    fn autotest_reconcile_nan_bounds_do_not_fire_a_resize_every_frame() {
        // NUMERIC EDGE — the sharpest one in this file.
        //
        // The Resize check is `old_rect.size != new_rect.size`. With a DERIVED
        // f32 `PartialEq` this would be catastrophic: `NaN != NaN` is true, so a
        // node whose layout solved to NaN would be reported as "resized" on
        // EVERY frame forever, firing an endless Resize-callback storm on a
        // completely static layout.
        //
        // `LogicalSize` dodges that with a hand-written `PartialEq` that runs
        // both operands through `geom::quantize()`, which maps every NaN to the
        // single sentinel `i64::MIN` (geom.rs:218) — so all NaNs compare EQUAL.
        // This test pins that: revert `LogicalSize` to `#[derive(PartialEq)]`
        // and it goes red.
        let old = vec![NodeData::create_div()];
        let new = vec![with_cb(
            NodeData::create_div(),
            ComponentEventFilter::NodeResized,
        )];

        let nan = rect(f32::NAN, f32::NAN);
        let r = reconcile_dom(
            &old,
            &new,
            &[],
            &[],
            &layout_of(&[(0, nan)]),
            &layout_of(&[(0, nan)]),
            DomId::ROOT_ID,
            Instant::now(),
        );
        assert_eq!(r.node_moves.len(), 1);
        assert_eq!(
            count_events(&r, EventType::Resize),
            0,
            "an unchanged NaN size must not be reported as a resize",
        );

        // Infinities are likewise stable against themselves (they saturate to
        // i64::MAX / i64::MIN under quantize()).
        let inf = rect(f32::INFINITY, f32::NEG_INFINITY);
        let r = reconcile_dom(
            &old,
            &new,
            &[],
            &[],
            &layout_of(&[(0, inf)]),
            &layout_of(&[(0, inf)]),
            DomId::ROOT_ID,
            Instant::now(),
        );
        assert_eq!(
            count_events(&r, EventType::Resize),
            0,
            "infinite-but-equal bounds must not be treated as a resize",
        );

        // But a NaN -> real transition IS a genuine resize, and must still fire.
        let r = reconcile_dom(
            &old,
            &new,
            &[],
            &[],
            &layout_of(&[(0, nan)]),
            &layout_of(&[(0, rect(10.0, 20.0))]),
            DomId::ROOT_ID,
            Instant::now(),
        );
        assert_eq!(
            count_events(&r, EventType::Resize),
            1,
            "NaN -> a real size is a real resize",
        );
    }

    #[test]
    fn autotest_reconcile_extreme_bounds_do_not_panic() {
        // f32 MIN/MAX/subnormal bounds must flow through the Resize comparison
        // without arithmetic surprises (the code only compares, never subtracts).
        let old = vec![NodeData::create_div()];
        let new = vec![with_cb(
            NodeData::create_div(),
            ComponentEventFilter::NodeResized,
        )];

        for (a, b) in [
            (rect(f32::MIN, f32::MAX), rect(f32::MAX, f32::MIN)),
            (rect(f32::MIN_POSITIVE, 0.0), rect(0.0, f32::MIN_POSITIVE)),
            (rect(-0.0, 0.0), rect(0.0, -0.0)), // IEEE: -0.0 == 0.0
        ] {
            let r = reconcile_dom(
                &old,
                &new,
                &[],
                &[],
                &layout_of(&[(0, a)]),
                &layout_of(&[(0, b)]),
                DomId::ROOT_ID,
                Instant::now(),
            );
            assert_eq!(r.node_moves.len(), 1);
        }
    }

    #[test]
    fn autotest_reconcile_keyless_tiers_respect_the_parent_key_gate() {
        // Regression guard for the AUDIT note at diff.rs:601. Two structurally
        // identical leaves live under DIFFERENT parents. The content-hash and
        // structural-hash tiers must not match them across parents, or focus /
        // scroll / dataset state migrates into an unrelated subtree.
        //
        // old:  0 root ── 1 (#left)  ── 2 "leaf"
        // new:  0 root ── 1 (#right) ── 2 "leaf"
        let old_nd = vec![
            NodeData::create_div(),
            id_node("left"),
            NodeData::create_text_do_not_use_without_block_level_wrapper("leaf"),
        ];
        let old_hier = vec![
            hitem(None, None, None, Some(1)),
            hitem(Some(0), None, None, Some(2)),
            hitem(Some(1), None, None, None),
        ];

        let new_nd = vec![
            NodeData::create_div(),
            id_node("right"),
            NodeData::create_text_do_not_use_without_block_level_wrapper("leaf"),
        ];
        let new_hier = old_hier.clone();

        let r = reconcile_dom(
            &old_nd,
            &new_nd,
            &old_hier,
            &new_hier,
            &no_layout(),
            &no_layout(),
            DomId::ROOT_ID,
            Instant::now(),
        );

        // The leaf (index 2) must NOT be matched: its parent's reconciliation
        // key differs (#left vs #right), so both keyless tiers are gated off.
        let leaf_matched = r
            .node_moves
            .iter()
            .any(|m| m.new_node_id.index() == 2 && m.old_node_id.index() == 2);
        assert!(
            !leaf_matched,
            "a leaf must not migrate across parents; moves = {:?}",
            r.node_moves,
        );
    }

    // ========================================================================
    // create_migration_map
    // ========================================================================

    #[test]
    fn autotest_migration_map_empty_and_large() {
        assert!(create_migration_map(&[]).is_empty());

        let moves: Vec<NodeMove> = (0..1000)
            .map(|i| NodeMove {
                old_node_id: NodeId::new(i),
                new_node_id: NodeId::new(i * 2),
            })
            .collect();
        let map = create_migration_map(&moves);
        assert_eq!(map.len(), 1000);
        assert_eq!(map.get(&NodeId::new(999)), Some(&NodeId::new(1998)));
    }

    #[test]
    fn autotest_migration_map_duplicate_old_id_keeps_the_last_write() {
        // The map is a BTreeMap, so a repeated old id overwrites. Pin it: a
        // silent "first wins" flip would strand focus on a stale node.
        let moves = vec![
            NodeMove {
                old_node_id: NodeId::new(0),
                new_node_id: NodeId::new(5),
            },
            NodeMove {
                old_node_id: NodeId::new(0),
                new_node_id: NodeId::new(9),
            },
        ];
        let map = create_migration_map(&moves);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(&NodeId::new(0)), Some(&NodeId::new(9)));
    }

    #[test]
    fn autotest_migration_map_round_trips_a_real_diff() {
        let old: Vec<NodeData> = (0..8).map(|_| NodeData::create_div()).collect();
        let new: Vec<NodeData> = (0..8).map(|_| NodeData::create_div()).collect();
        let r = diff_flat(&old, &new);

        let map = create_migration_map(&r.node_moves);
        assert_eq!(map.len(), r.node_moves.len());
        for m in &r.node_moves {
            assert_eq!(map.get(&m.old_node_id), Some(&m.new_node_id));
        }
    }

    // ========================================================================
    // transfer_states
    // ========================================================================

    #[allow(dead_code)]
    struct TestState(u32);

    // Keeps the PERSISTENT (old) allocation, discarding the fresh one — the
    // real-world case (MapWidget's tile cache is written by background threads).
    extern "C" fn merge_keep_old(_new_data: RefAny, old_data: RefAny) -> RefAny {
        old_data
    }

    /// The framework must NOTICE an image node re-initialising every frame,
    /// with no cooperation from user code.
    ///
    /// The case this exists for: a video node built without a dataset. Each
    /// rebuild hands back a placeholder, the live frame is discarded, and the
    /// node flickers. Nothing in either DOM shows it — the old build has a
    /// frame, the new one has a placeholder — so only the reconciler, which
    /// sees the pair, can detect it.
    #[test]
    fn autotest_the_reconciler_counts_an_image_node_that_reinitialises() {
        use crate::resources::{ImageRef, RawImage, RawImageData, RawImageFormat};

        let real = || {
            ImageRef::new_rawimage(RawImage {
                pixels: RawImageData::U8(vec![9, 9, 9, 9].into()),
                width: 1,
                height: 1,
                premultiplied_alpha: false,
                data_format: RawImageFormat::RGBA8,
                tag: b"frame".to_vec().into(),
            })
            .expect("raw image")
        };
        let placeholder = || ImageRef::null_image(1, 1, RawImageFormat::BGRA8, b"ph".to_vec());

        // A node index this test owns exclusively, so a parallel test cannot
        // move the counter under it.
        const NODE: usize = 4242;
        let before = image_churn_count(NODE);

        for _ in 0..4 {
            let mut old = vec![NodeData::create_div(); NODE + 1];
            old[NODE] = NodeData::create_image(real());
            let mut new = vec![NodeData::create_div(); NODE + 1];
            new[NODE] = NodeData::create_image(placeholder());
            // NO dataset and NO merge callback — exactly the "forgot the
            // dataset on the video node" mistake.
            transfer_states(
                &mut old,
                &mut new,
                &[NodeMove {
                    old_node_id: NodeId::new(NODE),
                    new_node_id: NodeId::new(NODE),
                }],
            );
        }

        assert!(
            image_churn_count(NODE) > before,
            "the reconciler did not notice an image node reverting to a \
             placeholder while the previous build held a real frame — the \
             flicker this lint exists to name would go unreported"
        );
    }

    #[test]
    fn autotest_transfer_states_out_of_range_moves_are_skipped() {
        // The bounds guard must swallow a corrupt NodeMove instead of indexing
        // out of bounds.
        let mut old = vec![NodeData::create_div()];
        let mut new = vec![NodeData::create_div()];

        let moves = vec![
            NodeMove {
                old_node_id: NodeId::new(5), // out of range
                new_node_id: NodeId::new(0),
            },
            NodeMove {
                old_node_id: NodeId::new(0),
                new_node_id: NodeId::new(7), // out of range
            },
            NodeMove {
                old_node_id: NodeId::new(usize::MAX),
                new_node_id: NodeId::new(usize::MAX),
            },
        ];

        transfer_states(&mut old, &mut new, &moves); // must not panic
        assert!(new[0].get_dataset().is_none());
    }

    /// A live capture frame must survive a DOM rebuild.
    ///
    /// Capture widgets rebuild their node with `ImageRef::null_image(...)`
    /// every time — the fresh widget struct holds no frame; frames arrive later
    /// by writeback. Without carrying the previous image across the merge, every
    /// rebuild reverted the node to the placeholder until the next frame landed
    /// ~16-33ms later. That is the flash seen when resizing a window while
    /// screensharing, which looks exactly like the capture re-initialising.
    #[test]
    fn autotest_a_live_image_survives_a_rebuild_that_merges_state() {
        use crate::resources::{image_ref_get_hash, ImageRef};

        // A DELIVERED frame is a raw image — capture_common::present_frame builds
        // it with `ImageRef::new_rawimage`. The placeholder the widget rebuilds
        // its node with is a `null_image`. That difference is exactly what the
        // carry-forward keys on, so the fixture must use both constructors; an
        // earlier version of this test used null_image for BOTH and failed,
        // correctly, because there was then nothing to distinguish.
        let real = ImageRef::new_rawimage(crate::resources::RawImage {
            pixels: crate::resources::RawImageData::U8(vec![1, 2, 3, 4].into()),
            width: 1,
            height: 1,
            premultiplied_alpha: false,
            data_format: crate::resources::RawImageFormat::RGBA8,
            tag: b"frame".to_vec().into(),
        })
        .expect("raw image");
        let mut old = vec![NodeData::create_image(real.clone())];
        old[0].set_dataset(OptionRefAny::Some(RefAny::new(TestState(1))));

        let mut new = vec![NodeData::create_image(ImageRef::null_image(
            1,
            1,
            crate::resources::RawImageFormat::BGRA8,
            b"placeholder".to_vec(),
        ))];
        new[0].set_dataset(OptionRefAny::Some(RefAny::new(TestState(2))));
        new[0].set_merge_callback(crate::dom::DatasetMergeCallback::from_ptr(merge_keep_old));

        transfer_states(
            &mut old,
            &mut new,
            &[NodeMove {
                old_node_id: NodeId::new(0),
                new_node_id: NodeId::new(0),
            }],
        );

        // Both are null images here (null_image is how the widget builds BOTH
        // its placeholder and, in this harness, its frame), so assert on the
        // payload that distinguishes them rather than on the flag.
        let carried = new[0].get_image_ref_cloned().expect("still an image node");
        assert_eq!(
            image_ref_get_hash(&carried),
            image_ref_get_hash(&real),
            "the rebuilt node did not inherit the previous frame — it will show \
             the placeholder until the next writeback, which is the flicker"
        );
    }

    /// The pre-cascade skip path: fresh callbacks were installed on the
    /// retained node, then the fresh dataset arrives. With a merge callback
    /// the retained state wins (the widget's rule), and the fresh callbacks
    /// must end up on the SAME allocation as the node's dataset — the
    /// fragmentation this guards against is a callback mutating one
    /// allocation while the next merge reads another.
    #[test]
    fn autotest_merge_fresh_dataset_unifies_fresh_callbacks_with_the_retained_state() {
        use crate::callbacks::{CoreCallback, CoreCallbackData};
        use crate::dom::{EventFilter, HoverEventFilter};

        // Never invoked here; `CoreCallback::cb` is a type-erased fn address.
        fn noop_cb() {}

        let mut nodes = vec![NodeData::create_div()];
        let retained = RefAny::new(TestState(7));
        let retained_ptr = retained.sharing_info.ptr as usize;
        nodes[0].set_dataset(OptionRefAny::Some(retained));
        nodes[0].set_merge_callback(merge_keep_old as DatasetMergeCallbackType);

        // The fresh build: a new dataset, and callbacks cloned from it — which
        // the skip path installs on the retained node before merging.
        let fresh = RefAny::new(TestState(0));
        let fresh_ptr = fresh.sharing_info.ptr as usize;
        nodes[0].callbacks = vec![CoreCallbackData {
            event: EventFilter::Hover(HoverEventFilter::MouseDown),
            callback: CoreCallback {
                cb: noop_cb as usize,
                ctx: OptionRefAny::None,
            },
            refany: fresh.clone(),
        }]
        .into();
        assert_ne!(retained_ptr, fresh_ptr);

        merge_fresh_dataset(&mut nodes, 0, fresh);

        let ds_ptr = nodes[0].get_dataset().unwrap().sharing_info.ptr as usize;
        assert_eq!(
            ds_ptr, retained_ptr,
            "merge_keep_old keeps the retained allocation"
        );
        let cb_ptr = nodes[0].callbacks.as_ref()[0].refany.sharing_info.ptr as usize;
        assert_eq!(
            cb_ptr, ds_ptr,
            "the fresh callback must be re-pointed at the merged dataset, or the widget \
             mutates one allocation and the next merge reads another"
        );

        // Without a merge callback the fresh dataset wins (same as transfer_states'
        // skip), and the callbacks already point at it.
        let mut plain = vec![NodeData::create_div()];
        plain[0].set_dataset(OptionRefAny::Some(RefAny::new(TestState(1))));
        let fresh2 = RefAny::new(TestState(2));
        let fresh2_ptr = fresh2.sharing_info.ptr as usize;
        merge_fresh_dataset(&mut plain, 0, fresh2);
        assert_eq!(
            plain[0].get_dataset().unwrap().sharing_info.ptr as usize,
            fresh2_ptr
        );

        // An index past the arena is a no-op, not a panic.
        merge_fresh_dataset(&mut plain, 99, RefAny::new(TestState(3)));
    }

    #[test]
    fn autotest_transfer_states_without_merge_callback_leaves_datasets_intact() {
        let mut old = vec![NodeData::create_div()];
        old[0].set_dataset(OptionRefAny::Some(RefAny::new(TestState(1))));

        let mut new = vec![NodeData::create_div()];
        new[0].set_dataset(OptionRefAny::Some(RefAny::new(TestState(2))));

        let old_ptr = old[0].get_dataset().unwrap().sharing_info.ptr as usize;
        let new_ptr = new[0].get_dataset().unwrap().sharing_info.ptr as usize;

        transfer_states(
            &mut old,
            &mut new,
            &[NodeMove {
                old_node_id: NodeId::new(0),
                new_node_id: NodeId::new(0),
            }],
        );

        // No merge callback -> early `continue`, both datasets stay where they were.
        assert_eq!(
            old[0].get_dataset().unwrap().sharing_info.ptr as usize,
            old_ptr,
        );
        assert_eq!(
            new[0].get_dataset().unwrap().sharing_info.ptr as usize,
            new_ptr,
        );
    }

    #[test]
    fn autotest_transfer_states_with_one_missing_dataset_restores_both_sides() {
        // Merge callback present, but the OLD node has no dataset -> the
        // `(new_ds, old_ds)` arm must put the taken dataset back.
        let mut old = vec![NodeData::create_div()];
        let mut new = vec![NodeData::create_div()];
        new[0].set_merge_callback(merge_keep_old as DatasetMergeCallbackType);
        new[0].set_dataset(OptionRefAny::Some(RefAny::new(TestState(2))));

        let new_ptr = new[0].get_dataset().unwrap().sharing_info.ptr as usize;

        transfer_states(
            &mut old,
            &mut new,
            &[NodeMove {
                old_node_id: NodeId::new(0),
                new_node_id: NodeId::new(0),
            }],
        );

        assert!(old[0].get_dataset().is_none());
        assert_eq!(
            new[0].get_dataset().unwrap().sharing_info.ptr as usize,
            new_ptr,
            "the fresh dataset must be restored, not dropped",
        );
    }

    #[test]
    fn autotest_transfer_states_repoints_orphaned_callback_refanys() {
        // The unification rule (diff.rs:909): a widget builds its dataset AND
        // its callback refanys from clones of ONE RefAny. When the merge keeps
        // the OLD allocation, every clone of the FRESH one is orphaned and must
        // be re-pointed at the merged result — otherwise the widget fragments
        // across two caches (the MapWidget grey-tile bug).
        let fresh = RefAny::new(TestState(1));
        let fresh_ptr = fresh.sharing_info.ptr as usize;

        let mut new0 = NodeData::create_div();
        new0.set_merge_callback(merge_keep_old as DatasetMergeCallbackType);
        new0.set_dataset(OptionRefAny::Some(fresh.clone()));
        // A callback on the SAME node, holding a clone of the fresh allocation.
        new0.add_callback(
            EventFilter::Component(ComponentEventFilter::Selected),
            fresh.clone(),
            noop_callback(),
        );

        // A *sibling* node whose callback also clones the fresh allocation —
        // the generalised sweep must reach it too, not just the merge node.
        let mut new1 = NodeData::create_div();
        new1.add_callback(
            EventFilter::Component(ComponentEventFilter::Selected),
            fresh.clone(),
            noop_callback(),
        );

        let persistent = RefAny::new(TestState(2));
        let persistent_ptr = persistent.sharing_info.ptr as usize;
        assert_ne!(
            fresh_ptr, persistent_ptr,
            "test setup: allocations must differ"
        );

        let mut old = vec![NodeData::create_div()];
        old[0].set_dataset(OptionRefAny::Some(persistent));

        let mut new = vec![new0, new1];

        transfer_states(
            &mut old,
            &mut new,
            &[NodeMove {
                old_node_id: NodeId::new(0),
                new_node_id: NodeId::new(0),
            }],
        );

        // The merged dataset is the PERSISTENT allocation.
        assert_eq!(
            new[0].get_dataset().unwrap().sharing_info.ptr as usize,
            persistent_ptr,
            "the merge must keep the persistent allocation",
        );
        // The old node's dataset was moved into the merge result.
        assert!(old[0].get_dataset().is_none());

        // Both orphaned callback refanys — on the merge node AND on the sibling
        // — must now point at the merged allocation.
        for (i, nd) in new.iter().enumerate() {
            for cb in nd.callbacks.as_ref() {
                assert_eq!(
                    cb.refany.sharing_info.ptr as usize, persistent_ptr,
                    "node {i}: an orphaned callback refany was not re-pointed",
                );
            }
        }
    }

    // ========================================================================
    // ChangeAccumulator
    // ========================================================================

    #[test]
    fn autotest_accumulator_new_is_empty_and_inert() {
        let a = ChangeAccumulator::new();
        assert!(a.is_empty());
        assert!(!a.needs_layout());
        assert!(!a.needs_paint_only());
        assert!(a.is_visually_unchanged());
        assert_eq!(a.max_scope, RelayoutScope::None);
        // `new()` and `default()` must agree.
        let d = ChangeAccumulator::default();
        assert_eq!(a.is_empty(), d.is_empty());
        assert_eq!(a.max_scope, d.max_scope);
    }

    #[test]
    fn autotest_accumulator_mount_forces_layout_unmount_does_not() {
        let mut a = ChangeAccumulator::new();
        a.add_mount(NodeId::new(0));
        assert!(!a.is_empty());
        assert!(a.needs_layout(), "a mounted node always needs layout");
        assert!(!a.needs_paint_only());
        assert!(!a.is_visually_unchanged());

        // An unmount alone is NOT layout work here (the node is gone); it only
        // breaks `is_visually_unchanged`. Pin the asymmetry.
        let mut a = ChangeAccumulator::new();
        a.add_unmount(NodeId::new(0));
        assert!(!a.is_empty());
        assert!(!a.needs_layout());
        assert!(!a.is_visually_unchanged());
    }

    #[test]
    fn autotest_accumulator_css_change_routes_paint_vs_layout_by_scope() {
        // scope == None -> paint bucket.
        let mut a = ChangeAccumulator::new();
        a.add_css_change(
            NodeId::new(0),
            CssPropertyType::TextColor,
            RelayoutScope::None,
        );
        assert!(!a.needs_layout());
        assert!(a.needs_paint_only());
        assert!(!a.is_visually_unchanged());
        assert_eq!(a.max_scope, RelayoutScope::None);
        assert!(a.per_node[&NodeId::new(0)]
            .change_set
            .contains(NodeChangeSet::INLINE_STYLE_PAINT));

        // scope > None -> layout bucket.
        let mut a = ChangeAccumulator::new();
        a.add_css_change(
            NodeId::new(0),
            CssPropertyType::Width,
            RelayoutScope::SizingOnly,
        );
        assert!(a.needs_layout());
        assert!(!a.needs_paint_only(), "layout work subsumes paint-only");
        assert_eq!(a.max_scope, RelayoutScope::SizingOnly);
        assert!(a.per_node[&NodeId::new(0)]
            .change_set
            .contains(NodeChangeSet::INLINE_STYLE_LAYOUT));
    }

    #[test]
    fn autotest_accumulator_max_scope_is_monotone() {
        // Once escalated, the scope must never be lowered by a later, weaker
        // change — otherwise a Full relayout gets silently downgraded.
        let mut a = ChangeAccumulator::new();
        a.add_css_change(
            NodeId::new(0),
            CssPropertyType::Display,
            RelayoutScope::Full,
        );
        assert_eq!(a.max_scope, RelayoutScope::Full);

        a.add_css_change(
            NodeId::new(0),
            CssPropertyType::TextColor,
            RelayoutScope::None,
        );
        assert_eq!(
            a.max_scope,
            RelayoutScope::Full,
            "max_scope must not regress"
        );
        assert_eq!(
            a.per_node[&NodeId::new(0)].relayout_scope,
            RelayoutScope::Full,
            "per-node scope must not regress either",
        );

        a.add_css_change(
            NodeId::new(1),
            CssPropertyType::Width,
            RelayoutScope::SizingOnly,
        );
        assert_eq!(a.max_scope, RelayoutScope::Full);
        assert_eq!(
            a.per_node[&NodeId::new(1)].relayout_scope,
            RelayoutScope::SizingOnly,
            "a different node keeps its own, lower scope",
        );
    }

    #[test]
    fn autotest_accumulator_text_change_is_ifc_scoped_and_unicode_safe() {
        for s in UNICODE_SAMPLES {
            let mut a = ChangeAccumulator::new();
            a.add_text_change(NodeId::new(0), String::new(), (*s).to_string());

            let report = &a.per_node[&NodeId::new(0)];
            assert!(report.change_set.contains(NodeChangeSet::TEXT_CONTENT));
            assert_eq!(report.relayout_scope, RelayoutScope::IfcOnly);
            assert_eq!(
                report.text_change,
                Some(TextChange {
                    old_text: String::new(),
                    new_text: (*s).to_string(),
                }),
            );
            assert!(a.needs_layout());
            assert_eq!(a.max_scope, RelayoutScope::IfcOnly);
        }
    }

    #[test]
    fn autotest_accumulator_add_dom_change_accumulates_and_never_clears_text() {
        let node = NodeId::new(0);
        let mut a = ChangeAccumulator::new();

        a.add_dom_change(
            node,
            NodeChangeSet {
                bits: NodeChangeSet::TEXT_CONTENT,
            },
            RelayoutScope::IfcOnly,
            Some(TextChange {
                old_text: "a".to_string(),
                new_text: "b".to_string(),
            }),
            vec![CssPropertyType::Width],
        );

        // A second call with text_change == None must NOT wipe the first one.
        a.add_dom_change(
            node,
            NodeChangeSet {
                bits: NodeChangeSet::STYLED_STATE,
            },
            RelayoutScope::None,
            None,
            vec![CssPropertyType::TextColor],
        );

        let report = &a.per_node[&node];
        assert!(report.change_set.contains(NodeChangeSet::TEXT_CONTENT));
        assert!(
            report.change_set.contains(NodeChangeSet::STYLED_STATE),
            "flags must be OR-accumulated across calls",
        );
        assert_eq!(report.relayout_scope, RelayoutScope::IfcOnly);
        assert!(
            report.text_change.is_some(),
            "a None text_change must not erase a previously recorded one",
        );
        assert_eq!(
            report.changed_css_properties,
            vec![CssPropertyType::Width, CssPropertyType::TextColor],
            "changed properties must be appended, not replaced",
        );
    }

    #[test]
    fn autotest_accumulator_image_change() {
        let mut a = ChangeAccumulator::new();
        a.add_image_change(NodeId::new(0), RelayoutScope::SizingOnly);
        assert!(a.per_node[&NodeId::new(0)]
            .change_set
            .contains(NodeChangeSet::IMAGE_CHANGED));
        assert!(a.needs_layout());
        assert_eq!(a.max_scope, RelayoutScope::SizingOnly);
    }

    #[test]
    fn autotest_accumulator_merge_empty_restyle_result_is_a_no_op() {
        let mut a = ChangeAccumulator::new();
        a.merge_restyle_result(&RestyleResult::default());
        assert!(a.is_empty());
        assert!(a.is_visually_unchanged());
    }

    #[test]
    fn autotest_accumulator_merge_restyle_result_classifies_by_property() {
        let prop = CssProperty::Width(CssPropertyValue::Exact(LayoutWidth::const_px(100)));
        let changed = ChangedCssProperty {
            previous_state: StyledNodeState::default(),
            previous_prop: prop.clone(),
            current_state: StyledNodeState::default(),
            current_prop: prop,
        };

        let mut restyle = RestyleResult::default();
        restyle.changed_nodes.insert(NodeId::new(3), vec![changed]);

        let mut a = ChangeAccumulator::new();
        a.merge_restyle_result(&restyle);

        // `width` -> SizingOnly -> layout bucket.
        assert!(!a.is_empty());
        assert!(a.needs_layout());
        assert_eq!(a.max_scope, RelayoutScope::SizingOnly);
        let report = &a.per_node[&NodeId::new(3)];
        assert!(report
            .change_set
            .contains(NodeChangeSet::INLINE_STYLE_LAYOUT));
        assert_eq!(report.changed_css_properties, vec![CssPropertyType::Width]);
    }

    #[test]
    fn autotest_accumulator_merge_extended_diff_counts_mounts_and_unmounts() {
        // No node_moves at all -> every new node mounted, every old node unmounted.
        let old_nd = vec![NodeData::create_div(), NodeData::create_div()];
        let new_nd = vec![
            NodeData::create_div(),
            NodeData::create_div(),
            NodeData::create_div(),
        ];

        let mut a = ChangeAccumulator::new();
        a.merge_extended_diff(&ExtendedDiffResult::default(), &old_nd, &new_nd);

        assert_eq!(a.mounted_nodes.len(), 3);
        assert_eq!(a.unmounted_nodes.len(), 2);
        assert!(!a.is_empty());
        assert!(a.needs_layout(), "mounted nodes always need layout");
        assert!(!a.is_visually_unchanged());
    }

    #[test]
    fn autotest_accumulator_merge_extended_diff_on_empty_doms_is_empty() {
        let mut a = ChangeAccumulator::new();
        a.merge_extended_diff(&ExtendedDiffResult::default(), &[], &[]);
        assert!(a.is_empty());
    }

    #[test]
    fn autotest_accumulator_merge_extended_diff_skips_empty_change_sets() {
        // A matched node with NO changes must not create a per_node entry.
        let old_nd = vec![NodeData::create_div()];
        let new_nd = vec![NodeData::create_div()];

        let extended = ExtendedDiffResult {
            diff: DiffResult {
                events: Vec::new(),
                node_moves: vec![NodeMove {
                    old_node_id: NodeId::new(0),
                    new_node_id: NodeId::new(0),
                }],
            },
            node_changes: vec![(NodeId::new(0), NodeId::new(0), NodeChangeSet::empty())],
        };

        let mut a = ChangeAccumulator::new();
        a.merge_extended_diff(&extended, &old_nd, &new_nd);

        assert!(a.per_node.is_empty(), "an empty change set must be skipped");
        assert!(a.mounted_nodes.is_empty());
        assert!(a.unmounted_nodes.is_empty());
        assert!(a.is_empty());
    }

    #[test]
    fn autotest_accumulator_merge_extended_diff_extracts_text_change() {
        let old_nd = vec![NodeData::create_text_do_not_use_without_block_level_wrapper("héllo")];
        let new_nd =
            vec![NodeData::create_text_do_not_use_without_block_level_wrapper("héllo wörld")];

        let extended = ExtendedDiffResult {
            diff: DiffResult {
                events: Vec::new(),
                node_moves: vec![NodeMove {
                    old_node_id: NodeId::new(0),
                    new_node_id: NodeId::new(0),
                }],
            },
            node_changes: vec![(
                NodeId::new(0),
                NodeId::new(0),
                NodeChangeSet {
                    bits: NodeChangeSet::TEXT_CONTENT,
                },
            )],
        };

        let mut a = ChangeAccumulator::new();
        a.merge_extended_diff(&extended, &old_nd, &new_nd);

        let report = &a.per_node[&NodeId::new(0)];
        assert_eq!(
            report.text_change,
            Some(TextChange {
                old_text: "héllo".to_string(),
                new_text: "héllo wörld".to_string(),
            }),
            "TEXT_CONTENT must carry the old/new text for cursor reconciliation",
        );
        assert_eq!(report.relayout_scope, RelayoutScope::IfcOnly);
    }

    // ========================================================================
    // ChangeAccumulator::classify_change_scope (private)
    // ========================================================================

    #[test]
    fn autotest_classify_scope_maps_each_flag_to_its_documented_scope() {
        let nodes = vec![NodeData::create_div()];
        let id = NodeId::new(0);

        let classify = |bits: u32| {
            ChangeAccumulator::classify_change_scope(NodeChangeSet { bits }, &nodes, id)
        };

        assert_eq!(classify(0), RelayoutScope::None, "empty -> no work");
        assert_eq!(
            classify(NodeChangeSet::NODE_TYPE_CHANGED),
            RelayoutScope::Full
        );
        assert_eq!(
            classify(NodeChangeSet::CHILDREN_CHANGED),
            RelayoutScope::Full
        );
        assert_eq!(
            classify(NodeChangeSet::IDS_AND_CLASSES),
            RelayoutScope::Full
        );
        assert_eq!(
            classify(NodeChangeSet::TEXT_CONTENT),
            RelayoutScope::IfcOnly
        );
        assert_eq!(
            classify(NodeChangeSet::IMAGE_CHANGED),
            RelayoutScope::SizingOnly
        );
        assert_eq!(
            classify(NodeChangeSet::CONTENTEDITABLE),
            RelayoutScope::SizingOnly
        );
        assert_eq!(classify(NodeChangeSet::STYLED_STATE), RelayoutScope::None);
        assert_eq!(
            classify(NodeChangeSet::INLINE_STYLE_PAINT),
            RelayoutScope::None
        );
        // Non-visual flags -> no work.
        assert_eq!(classify(NodeChangeSet::CALLBACKS), RelayoutScope::None);
        assert_eq!(classify(NodeChangeSet::DATASET), RelayoutScope::None);
        assert_eq!(classify(NodeChangeSet::TAB_INDEX), RelayoutScope::None);
    }

    #[test]
    fn autotest_classify_scope_precedence_is_widest_first() {
        let nodes = vec![NodeData::create_div()];
        let id = NodeId::new(0);

        // NODE_TYPE_CHANGED wins over everything below it.
        let bits = NodeChangeSet::NODE_TYPE_CHANGED
            | NodeChangeSet::TEXT_CONTENT
            | NodeChangeSet::IMAGE_CHANGED
            | NodeChangeSet::STYLED_STATE;
        assert_eq!(
            ChangeAccumulator::classify_change_scope(NodeChangeSet { bits }, &nodes, id),
            RelayoutScope::Full,
        );

        // TEXT_CONTENT (IfcOnly) wins over IMAGE_CHANGED (SizingOnly) — pinning
        // the documented order, even though IfcOnly < SizingOnly.
        let bits = NodeChangeSet::TEXT_CONTENT | NodeChangeSet::IMAGE_CHANGED;
        assert_eq!(
            ChangeAccumulator::classify_change_scope(NodeChangeSet { bits }, &nodes, id),
            RelayoutScope::IfcOnly,
        );
    }

    #[test]
    fn autotest_classify_scope_inline_layout_walks_the_nodes_own_css() {
        // With a sizing property on the node, the scope comes from the property.
        let nodes = vec![NodeData::create_div().with_css("width: 100px")];
        assert_eq!(
            ChangeAccumulator::classify_change_scope(
                NodeChangeSet {
                    bits: NodeChangeSet::INLINE_STYLE_LAYOUT,
                },
                &nodes,
                NodeId::new(0),
            ),
            RelayoutScope::SizingOnly,
        );

        // A `display` change is a full relayout.
        let nodes = vec![NodeData::create_div().with_css("display: flex")];
        assert_eq!(
            ChangeAccumulator::classify_change_scope(
                NodeChangeSet {
                    bits: NodeChangeSet::INLINE_STYLE_LAYOUT,
                },
                &nodes,
                NodeId::new(0),
            ),
            RelayoutScope::Full,
        );

        // No inline CSS at all (the property was REMOVED, so the new node has
        // nothing to walk): the conservative SizingOnly fallback must kick in
        // rather than silently reporting "no layout work".
        let nodes = vec![NodeData::create_div()];
        assert_eq!(
            ChangeAccumulator::classify_change_scope(
                NodeChangeSet {
                    bits: NodeChangeSet::INLINE_STYLE_LAYOUT,
                },
                &nodes,
                NodeId::new(0),
            ),
            RelayoutScope::SizingOnly,
            "an INLINE_STYLE_LAYOUT change must never classify as 'no layout'",
        );
    }

    // ========================================================================
    // reconcile_dom_with_changes
    // ========================================================================

    #[test]
    fn autotest_reconcile_with_changes_on_empty_doms() {
        let r = reconcile_dom_with_changes(
            &[],
            &[],
            &[],
            &[],
            None,
            None,
            &no_layout(),
            &no_layout(),
            DomId::ROOT_ID,
            Instant::now(),
        );
        assert!(r.diff.events.is_empty());
        assert!(r.diff.node_moves.is_empty());
        assert!(r.node_changes.is_empty());
    }

    #[test]
    fn autotest_reconcile_with_changes_reports_one_entry_per_move() {
        let old =
            vec![NodeData::create_text_do_not_use_without_block_level_wrapper("v1").with_key(1u32)];
        let new =
            vec![NodeData::create_text_do_not_use_without_block_level_wrapper("v2").with_key(1u32)];

        let r = reconcile_dom_with_changes(
            &old,
            &new,
            &[],
            &[],
            None,
            None,
            &no_layout(),
            &no_layout(),
            DomId::ROOT_ID,
            Instant::now(),
        );

        assert_eq!(r.diff.node_moves.len(), 1);
        assert_eq!(
            r.node_changes.len(),
            r.diff.node_moves.len(),
            "there must be exactly one change entry per matched pair",
        );

        let (old_id, new_id, changes) = &r.node_changes[0];
        assert_eq!(*old_id, NodeId::new(0));
        assert_eq!(*new_id, NodeId::new(0));
        assert!(changes.contains(NodeChangeSet::TEXT_CONTENT));
    }

    #[test]
    fn autotest_reconcile_with_changes_tolerates_short_styled_state_slices() {
        // `old_styled_nodes` / `new_styled_nodes` are indexed with `.get()`, so a
        // slice shorter than the DOM must degrade to `None`, not panic.
        let old = vec![NodeData::create_div(), NodeData::create_div()];
        let new = vec![NodeData::create_div(), NodeData::create_div()];
        let short = [StyledNodeState::default()]; // 1 entry for 2 nodes

        let r = reconcile_dom_with_changes(
            &old,
            &new,
            &[],
            &[],
            Some(&short[..]),
            Some(&short[..]),
            &no_layout(),
            &no_layout(),
            DomId::ROOT_ID,
            Instant::now(),
        );
        assert_eq!(r.node_changes.len(), 2);
        // Both sides see the same (present-or-absent) state, so no STYLED_STATE.
        for (_, _, changes) in &r.node_changes {
            assert!(!changes.contains(NodeChangeSet::STYLED_STATE));
        }
    }

    #[test]
    fn autotest_reconcile_with_changes_feeds_the_accumulator() {
        // End-to-end: reconcile -> ExtendedDiffResult -> ChangeAccumulator.
        let old = vec![
            NodeData::create_text_do_not_use_without_block_level_wrapper("before").with_key(1u32),
        ];
        let new = vec![
            NodeData::create_text_do_not_use_without_block_level_wrapper("after").with_key(1u32),
        ];

        let extended = reconcile_dom_with_changes(
            &old,
            &new,
            &[],
            &[],
            None,
            None,
            &no_layout(),
            &no_layout(),
            DomId::ROOT_ID,
            Instant::now(),
        );

        let mut acc = ChangeAccumulator::new();
        acc.merge_extended_diff(&extended, &old, &new);

        assert!(!acc.is_empty());
        assert!(acc.needs_layout(), "a text edit needs (IFC) layout");
        assert!(!acc.is_visually_unchanged());
        assert!(acc.mounted_nodes.is_empty());
        assert!(acc.unmounted_nodes.is_empty());

        let report = &acc.per_node[&NodeId::new(0)];
        assert_eq!(report.relayout_scope, RelayoutScope::IfcOnly);
        assert_eq!(
            report.text_change,
            Some(TextChange {
                old_text: "before".to_string(),
                new_text: "after".to_string(),
            }),
        );
    }

    // ========================================================================
    // NodeDataFingerprint
    // ========================================================================

    #[test]
    fn autotest_fingerprint_default_and_self_comparison_are_inert() {
        let d = NodeDataFingerprint::default();
        assert!(d.is_identical(&d));
        assert!(d.diff(&d).is_empty());
        assert!(!d.might_affect_layout(&d));
        assert!(!d.might_affect_visuals(&d));
        assert_eq!(d, NodeDataFingerprint::default());
    }

    #[test]
    fn autotest_fingerprint_is_a_pure_function_of_its_inputs() {
        // Round-trip / determinism: recomputing from equal inputs must give an
        // identical fingerprint (no address or allocation identity leaking in).
        let state = StyledNodeState::default();
        for s in UNICODE_SAMPLES {
            let a = NodeDataFingerprint::compute(
                &NodeData::create_text_do_not_use_without_block_level_wrapper(*s),
                Some(&state),
            );
            let b = NodeDataFingerprint::compute(
                &NodeData::create_text_do_not_use_without_block_level_wrapper(*s),
                Some(&state),
            );
            assert_eq!(a, b, "fingerprint of {s:?} is not deterministic");
            assert!(a.is_identical(&b));
            assert!(a.diff(&b).is_empty());
        }
    }

    #[test]
    fn a_fresh_dataset_allocation_is_not_a_layout_change() {
        // REPORTED (TextArea bleeding into the Slider): every `with_dataset`
        // widget allocates a fresh RefAny per build, RefAny hashes by pointer,
        // and the dataset sat in `attrs_hash` → CONTENTEDITABLE → layout
        // dirty on every RefreshDom. The dataset is state: same type, new
        // allocation, IDENTICAL fingerprint.
        let a = NodeDataFingerprint::compute(
            &NodeData::create_div()
                .with_dataset(crate::refany::OptionRefAny::Some(RefAny::new(42u32))),
            None,
        );
        let b = NodeDataFingerprint::compute(
            &NodeData::create_div()
                .with_dataset(crate::refany::OptionRefAny::Some(RefAny::new(43u32))),
            None,
        );
        assert!(
            a.is_identical(&b),
            "a new allocation of the same dataset type is not a change"
        );
        assert!(a.diff(&b).is_empty());
        assert!(!a.might_affect_layout(&b));

        // A dataset of another TYPE (or none) is a DATASET change — and
        // still not a layout one.
        let c = NodeDataFingerprint::compute(
            &NodeData::create_div().with_dataset(crate::refany::OptionRefAny::Some(RefAny::new(
                String::from("x"),
            ))),
            None,
        );
        let changes = a.diff(&c);
        assert!(changes.contains(NodeChangeSet::DATASET));
        assert!(
            !changes.needs_layout(),
            "a dataset change must never relayout: {changes:?}"
        );
        assert!(!a.might_affect_layout(&c));
        let d = NodeDataFingerprint::compute(&NodeData::create_div(), None);
        assert!(a.diff(&d).contains(NodeChangeSet::DATASET));
        assert!(!a.diff(&d).needs_layout());
    }

    #[test]
    fn autotest_fingerprint_diff_is_symmetric() {
        let a = NodeDataFingerprint::compute(
            &NodeData::create_text_do_not_use_without_block_level_wrapper("a"),
            None,
        );
        let b = NodeDataFingerprint::compute(&class_node("x").with_css("width: 1px"), None);

        assert_eq!(a.diff(&b), b.diff(&a), "diff must be symmetric");
        assert_eq!(
            a.might_affect_layout(&b),
            b.might_affect_layout(&a),
            "might_affect_layout must be symmetric",
        );
        assert_eq!(a.might_affect_visuals(&b), b.might_affect_visuals(&a));
    }

    #[test]
    fn autotest_fingerprint_text_change_is_layout_and_visual() {
        let a = NodeDataFingerprint::compute(
            &NodeData::create_text_do_not_use_without_block_level_wrapper("one"),
            None,
        );
        let b = NodeDataFingerprint::compute(
            &NodeData::create_text_do_not_use_without_block_level_wrapper("two"),
            None,
        );

        assert!(!a.is_identical(&b));
        let changes = a.diff(&b);
        // Conservative by design: content_hash cannot tell text from image.
        assert!(changes.contains(NodeChangeSet::TEXT_CONTENT));
        assert!(changes.contains(NodeChangeSet::IMAGE_CHANGED));
        assert!(a.might_affect_layout(&b));
        assert!(a.might_affect_visuals(&b));
    }

    #[test]
    fn autotest_fingerprint_styled_state_is_visual_but_not_layout() {
        // The sharpest invariant of the fast path: a :hover flip must never be
        // able to trigger relayout.
        let node = NodeData::create_div();
        let calm = StyledNodeState::default();
        let hovered = StyledNodeState {
            hover: true,
            ..StyledNodeState::default()
        };

        let a = NodeDataFingerprint::compute(&node, Some(&calm));
        let b = NodeDataFingerprint::compute(&node, Some(&hovered));

        assert!(!a.is_identical(&b));
        assert!(a.diff(&b).contains(NodeChangeSet::STYLED_STATE));
        assert!(
            !a.might_affect_layout(&b),
            "a styled-state change must not be able to request layout",
        );
        assert!(a.might_affect_visuals(&b));
    }

    #[test]
    fn autotest_fingerprint_callback_change_is_neither_layout_nor_visual() {
        let plain = NodeData::create_div();
        let with_handler = with_cb(NodeData::create_div(), ComponentEventFilter::AfterMount);

        let a = NodeDataFingerprint::compute(&plain, None);
        let b = NodeDataFingerprint::compute(&with_handler, None);

        assert!(
            !a.is_identical(&b),
            "the callback list must be fingerprinted"
        );
        assert!(a.diff(&b).contains(NodeChangeSet::CALLBACKS));
        assert!(
            !a.might_affect_layout(&b),
            "swapping an event handler must not trigger relayout",
        );
        assert!(
            !a.might_affect_visuals(&b),
            "swapping an event handler must not trigger a repaint",
        );
    }

    #[test]
    fn autotest_fingerprint_ids_classes_and_inline_css_are_layout_relevant() {
        let base = NodeDataFingerprint::compute(&NodeData::create_div(), None);

        let classes = NodeDataFingerprint::compute(&class_node("banner"), None);
        assert!(base.diff(&classes).contains(NodeChangeSet::IDS_AND_CLASSES));
        assert!(base.might_affect_layout(&classes));
        assert!(base.might_affect_visuals(&classes));

        let styled =
            NodeDataFingerprint::compute(&NodeData::create_div().with_css("width: 3px"), None);
        assert!(base
            .diff(&styled)
            .contains(NodeChangeSet::INLINE_STYLE_LAYOUT));
        assert!(base.might_affect_layout(&styled));
        assert!(base.might_affect_visuals(&styled));
    }

    #[test]
    fn autotest_fingerprint_attrs_change_flags_tab_index_and_contenteditable() {
        let base = NodeDataFingerprint::compute(&NodeData::create_div(), None);
        let editable =
            NodeDataFingerprint::compute(&NodeData::create_div().with_contenteditable(true), None);

        let changes = base.diff(&editable);
        assert!(changes.contains(NodeChangeSet::TAB_INDEX));
        assert!(changes.contains(NodeChangeSet::CONTENTEDITABLE));
        assert!(
            base.might_affect_layout(&editable),
            "attrs_hash feeds might_affect_layout",
        );
        assert!(
            !base.might_affect_visuals(&editable),
            "attrs_hash is deliberately NOT part of might_affect_visuals",
        );
    }

    #[test]
    fn autotest_fingerprint_agrees_with_compute_node_changes_on_unchanged_nodes() {
        // Tier 1 (fingerprint) must never claim "changed" where Tier 2
        // (compute_node_changes) says "unchanged" — that would defeat the whole
        // two-tier fast path.
        let state = StyledNodeState::default();
        let samples = vec![
            NodeData::create_div(),
            NodeData::create_text_do_not_use_without_block_level_wrapper("hello 🌍"),
            class_node("row"),
            id_node("main"),
            NodeData::create_div().with_css("color: red"),
            NodeData::create_div().with_contenteditable(true),
            with_cb(NodeData::create_div(), ComponentEventFilter::AfterMount),
        ];

        for node in &samples {
            let clone = node.clone();

            let fp_a = NodeDataFingerprint::compute(node, Some(&state));
            let fp_b = NodeDataFingerprint::compute(&clone, Some(&state));
            assert!(
                fp_a.is_identical(&fp_b),
                "a cloned node must fingerprint identically",
            );
            assert!(fp_a.diff(&fp_b).is_empty());

            let tier2 = compute_node_changes(node, &clone, Some(&state), Some(&state));
            assert!(
                tier2.is_empty(),
                "compute_node_changes must agree that a clone is unchanged, got {:#b}",
                tier2.bits,
            );
        }
    }
}

#[cfg(test)]
mod dom_fingerprint_tests {
    use super::*;
    use crate::dom::{Dom, NodeType};

    fn sample_dom() -> Dom {
        Dom::create_node(NodeType::Div)
            .with_class("page".into())
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                "hello",
            ))
            .with_child(Dom::create_node(NodeType::Div).with_child(
                Dom::create_text_do_not_use_without_block_level_wrapper("world"),
            ))
    }

    #[test]
    fn identical_independently_built_doms_fingerprint_equal_on_both_tiers() {
        let (a, _) = fingerprint_dom(&sample_dom());
        let (b, _) = fingerprint_dom(&sample_dom());
        assert_eq!(a.structure_root, b.structure_root);
        assert_eq!(a.style_root, b.style_root);
        assert_eq!(a.structure, b.structure);
        assert_eq!(a.style, b.style);
        // preorder: root, text, div, text = 4 nodes
        assert_eq!(a.structure.len(), 4);
    }

    #[test]
    fn text_change_moves_exactly_one_structure_hash_and_no_style_hash() {
        let (a, _) = fingerprint_dom(&sample_dom());
        let changed = Dom::create_node(NodeType::Div)
            .with_class("page".into())
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                "hellX",
            ))
            .with_child(Dom::create_node(NodeType::Div).with_child(
                Dom::create_text_do_not_use_without_block_level_wrapper("world"),
            ));
        let (b, _) = fingerprint_dom(&changed);
        assert_ne!(a.structure_root, b.structure_root, "text is structure");
        assert_eq!(
            a.style_root, b.style_root,
            "text change must not touch the style tier"
        );
        let diffs: Vec<usize> = (0..a.structure.len())
            .filter(|&i| a.structure[i] != b.structure[i])
            .collect();
        // "hello" is the root's first child → preorder index 1
        assert_eq!(
            diffs,
            alloc::vec![1],
            "exactly the edited text node differs"
        );
    }

    #[test]
    fn with_css_sheet_change_is_style_tier_only() {
        let base = || sample_dom();
        let (a, _) = fingerprint_dom(&base().with_css("div { color: red; }"));
        let (b, _) = fingerprint_dom(&base().with_css("div { color: blue; }"));
        assert_eq!(
            a.structure_root, b.structure_root,
            "css is EXCLUDED from structure"
        );
        assert_ne!(
            a.style_root, b.style_root,
            "sheet content is style identity"
        );
        // The sheet hangs on the root → style diff localizes to preorder 0.
        let diffs: Vec<usize> = (0..a.style.len())
            .filter(|&i| a.style[i] != b.style[i])
            .collect();
        assert_eq!(diffs, alloc::vec![0]);
    }

    #[test]
    fn inline_css_change_is_style_tier_only() {
        let (a, _) = fingerprint_dom(&sample_dom());
        let mut changed = sample_dom();
        changed.root.set_css("background: red;");
        let (b, _) = fingerprint_dom(&changed);
        assert_eq!(a.structure_root, b.structure_root);
        assert_ne!(a.style_root, b.style_root);
    }

    #[test]
    fn class_change_is_structural() {
        let (a, _) = fingerprint_dom(&sample_dom());
        let changed = Dom::create_node(NodeType::Div)
            .with_class("pages".into())
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                "hello",
            ))
            .with_child(Dom::create_node(NodeType::Div).with_child(
                Dom::create_text_do_not_use_without_block_level_wrapper("world"),
            ));
        let (b, _) = fingerprint_dom(&changed);
        assert_ne!(
            a.structure_root, b.structure_root,
            "a class changes which sheet rules match — structural identity"
        );
    }

    #[test]
    fn added_child_changes_the_parent_and_the_shape() {
        let (a, _) = fingerprint_dom(&sample_dom());
        let (b, _) = fingerprint_dom(&sample_dom().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("extra"),
        ));
        assert_ne!(a.structure_root, b.structure_root);
        assert_ne!(a.structure.len(), b.structure.len());
        // The parent's own hash moved too (child count is folded in), so a
        // same-length reshuffle can never alias.
        assert_ne!(a.structure[0], b.structure[0]);
    }

    #[test]
    fn transfers_collect_callback_nodes_at_their_preorder_indices() {
        let (_, transfers) = fingerprint_dom(&sample_dom());
        assert!(transfers.image_callbacks.is_empty());
        assert!(transfers.callbacks.is_empty());
    }
}
