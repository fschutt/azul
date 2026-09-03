#[allow(unused_imports)]
pub use super::*;
#[cfg(test)]
mod audit_tests {
    use super::*;

    fn cursor(byte: u32) -> TextCursor {
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: byte,
            },
            affinity: CursorAffinity::Leading,
        }
    }

    fn state(byte: u32) -> MultiCursorState {
        MultiCursorState::new_with_cursor(cursor(byte), DomNodeId::ROOT, 0)
    }

    #[test]
    fn primary_tracked_by_id_not_vec_position() {
        let mut mc = state(100);
        // Add a cursor at an EARLIER position; after merge_overlapping's sort it
        // becomes the vector's FIRST element, but it is the primary (last added).
        let b = mc.add_cursor(cursor(0));
        assert_eq!(mc.len(), 2);
        // The primary must be the just-added cursor at byte 0, not the
        // position-last cursor at byte 100.
        assert_eq!(mc.get_primary().unwrap().id, b);
        assert_eq!(mc.get_primary_cursor().unwrap(), cursor(0));
    }

    #[test]
    fn merge_preserves_primary() {
        let mut mc = state(5);
        let _b = mc.add_cursor(cursor(5)); // same position -> merges to one
        assert_eq!(mc.len(), 1);
        // primary_id must resolve to the surviving selection.
        let primary = mc.get_primary().unwrap();
        assert_eq!(primary.id, mc.selections[0].id);
    }

    #[test]
    fn removing_primary_repoints_it() {
        let mut mc = state(0);
        let b = mc.add_cursor(cursor(10)); // primary = b
        assert_eq!(mc.get_primary().unwrap().id, b);
        assert!(mc.remove_selection(b));
        // primary must now be a still-existing selection, not a dangling id.
        let p = mc.get_primary().unwrap();
        assert!(mc.selections.iter().any(|s| s.id == p.id));
    }
}

#[cfg(test)]
mod autotest_generated {
    use super::*;
    use crate::geom::LogicalSize;
    use crate::styled_dom::NodeHierarchyItemId;

    // ---------------------------------------------------------------------
    // Fixtures
    // ---------------------------------------------------------------------

    /// Cursor in run 0 at `byte`, Leading affinity.
    fn c(byte: u32) -> TextCursor {
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: byte,
            },
            affinity: CursorAffinity::Leading,
        }
    }

    /// Cursor with explicit run + affinity (for ordering / boundary probes).
    fn c_full(run: u32, byte: u32, affinity: CursorAffinity) -> TextCursor {
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: run,
                start_byte_in_run: byte,
            },
            affinity,
        }
    }

    fn rng(a: u32, b: u32) -> SelectionRange {
        SelectionRange {
            start: c(a),
            end: c(b),
        }
    }

    fn dom_node(index: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
        }
    }

    fn state(byte: u32) -> MultiCursorState {
        MultiCursorState::new_with_cursor(c(byte), DomNodeId::ROOT, 0)
    }

    /// A `MultiCursorState` with zero selections — "should not normally happen",
    /// so every getter must survive it.
    fn empty_state() -> MultiCursorState {
        MultiCursorState {
            selections: Vec::new(),
            primary_id: SelectionId::new(),
            node_id: DomNodeId::ROOT,
            contenteditable_key: 0,
        }
    }

    fn ident(id: SelectionId, sel: Selection) -> IdentifiedSelection {
        IdentifiedSelection {
            id,
            selection: sel,
            owner: SelectionOwner::LOCAL,
        }
    }

    // ---------------------------------------------------------------------
    // Invariant checkers (documented in `MultiCursorState`'s `## Invariants`)
    // ---------------------------------------------------------------------

    /// `selections` is sorted by position and non-overlapping.
    fn assert_sorted_nonoverlapping(mc: &MultiCursorState) {
        for w in mc.selections.windows(2) {
            let prev_end = selection_end_pos(&w[0].selection);
            let next_start = selection_start_pos(&w[1].selection);
            assert!(
                next_start > prev_end,
                "selections must be sorted and non-overlapping after merge: {:?} then {:?}",
                w[0],
                w[1]
            );
        }
    }

    /// `primary_id` must always name a selection that actually exists (or the
    /// state must be empty). A dangling `primary_id` makes `get_primary()` lie.
    fn assert_primary_resolves(mc: &MultiCursorState) {
        if mc.is_empty() {
            assert!(mc.get_primary().is_none());
            assert!(mc.get_primary_cursor().is_none());
        } else {
            let p = mc
                .get_primary()
                .expect("non-empty state must have a primary");
            assert!(
                mc.selections.iter().any(|s| s.id == p.id),
                "get_primary() returned a selection not in the vec"
            );
            assert_eq!(
                mc.primary_id, p.id,
                "primary_id must name an existing selection (not fall back to last)"
            );
        }
    }

    /// All selection IDs must be distinct.
    fn assert_ids_unique(mc: &MultiCursorState) {
        for (i, a) in mc.selections.iter().enumerate() {
            for b in mc.selections.iter().skip(i + 1) {
                assert_ne!(a.id, b.id, "duplicate SelectionId in state");
            }
        }
    }

    // =====================================================================
    // SelectionId::new  (constructor)
    // =====================================================================

    #[test]
    fn selection_id_new_is_unique_and_strictly_increasing() {
        let mut prev = SelectionId::new();
        assert!(prev.inner > 0, "counter starts at 1, never the 0 sentinel");
        for _ in 0..1000 {
            let next = SelectionId::new();
            // Other tests share the global atomic, so ids may skip — but within
            // one thread they must be strictly increasing and never repeat.
            assert!(
                next.inner > prev.inner,
                "SelectionId counter must be strictly monotonic"
            );
            assert_ne!(next, prev);
            prev = next;
        }
    }

    #[test]
    fn selection_id_default_mints_a_fresh_id() {
        // Documented: Default does NOT return a zero/sentinel value.
        let a = SelectionId::default();
        let b = SelectionId::default();
        let d = SelectionId::new();
        assert_ne!(a, b);
        assert_ne!(b, d);
        assert!(a.inner > 0 && b.inner > 0);
    }

    // =====================================================================
    // SelectionState::add
    // =====================================================================

    #[test]
    fn selection_state_add_dedups_identical_cursors() {
        let mut st = SelectionState {
            selections: Vec::<Selection>::new().into(),
            node_id: DomNodeId::ROOT,
        };
        for _ in 0..100 {
            st.add(Selection::Cursor(c(42)));
        }
        assert_eq!(st.selections.as_ref().len(), 1);
        assert_eq!(st.selections.as_ref()[0], Selection::Cursor(c(42)));
    }

    #[test]
    fn selection_state_add_sorts_descending_input_ascending() {
        let mut st = SelectionState {
            selections: Vec::<Selection>::new().into(),
            node_id: DomNodeId::ROOT,
        };
        for byte in [90u32, 10, 50, 0, 70] {
            st.add(Selection::Cursor(c(byte)));
        }
        let got: Vec<Selection> = st.selections.as_ref().to_vec();
        assert_eq!(got.len(), 5);
        let want: Vec<Selection> = [0u32, 10, 50, 70, 90]
            .iter()
            .map(|b| Selection::Cursor(c(*b)))
            .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn selection_state_add_boundary_and_reversed_ranges_do_not_panic() {
        let mut st = SelectionState {
            selections: Vec::<Selection>::new().into(),
            node_id: DomNodeId::ROOT,
        };
        // u32::MAX bytes, max run index, both affinities, and a *reversed* range
        // (start logically after end — explicitly allowed by SelectionRange docs).
        st.add(Selection::Cursor(c_full(
            u32::MAX,
            u32::MAX,
            CursorAffinity::Trailing,
        )));
        st.add(Selection::Cursor(c_full(0, 0, CursorAffinity::Leading)));
        st.add(Selection::Range(SelectionRange {
            start: c_full(u32::MAX, u32::MAX, CursorAffinity::Trailing),
            end: c_full(0, 0, CursorAffinity::Leading),
        }));
        st.add(Selection::Range(rng(0, u32::MAX)));
        // add() only sorts + dedups; it does not normalize or merge, so all 4 stay.
        assert_eq!(st.selections.as_ref().len(), 4);
        // ... and the result is sorted.
        let got: Vec<Selection> = st.selections.as_ref().to_vec();
        let mut sorted = got.clone();
        sorted.sort_unstable();
        assert_eq!(got, sorted);
    }

    #[test]
    fn selection_state_add_cursor_and_range_at_same_pos_are_distinct() {
        let mut st = SelectionState {
            selections: Vec::<Selection>::new().into(),
            node_id: DomNodeId::ROOT,
        };
        st.add(Selection::Range(rng(5, 5)));
        st.add(Selection::Cursor(c(5)));
        // A zero-width Range and a Cursor are different `Selection` variants,
        // so dedup() cannot collapse them.
        assert_eq!(st.selections.as_ref().len(), 2);
        // Cursor variant sorts before Range variant.
        assert_eq!(st.selections.as_ref()[0], Selection::Cursor(c(5)));
    }

    // =====================================================================
    // MultiCursorState::new_with_cursor  (constructor)
    // =====================================================================

    #[test]
    fn new_with_cursor_invariants_hold() {
        let node = dom_node(7);
        let mc = MultiCursorState::new_with_cursor(c(3), node, 0xDEAD_BEEF);
        assert_eq!(mc.len(), 1);
        assert!(!mc.is_empty());
        assert_eq!(mc.selections.len(), mc.len());
        assert_eq!(mc.primary_id, mc.selections[0].id);
        assert_eq!(mc.node_id, node);
        assert_eq!(mc.contenteditable_key, 0xDEAD_BEEF);
        assert_eq!(mc.get_primary_cursor(), Some(c(3)));
        assert_eq!(mc.to_selections(), vec![Selection::Cursor(c(3))]);
        assert_primary_resolves(&mc);
    }

    #[test]
    fn new_with_cursor_extreme_args_do_not_panic() {
        let mc = MultiCursorState::new_with_cursor(
            c_full(u32::MAX, u32::MAX, CursorAffinity::Trailing),
            dom_node(usize::MAX / 4),
            u64::MAX,
        );
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.contenteditable_key, u64::MAX);
        assert_eq!(
            mc.get_primary_cursor(),
            Some(c_full(u32::MAX, u32::MAX, CursorAffinity::Trailing))
        );
        assert_primary_resolves(&mc);
    }

    #[test]
    fn two_states_get_distinct_ids() {
        let a = state(0);
        let b = state(0);
        assert_ne!(a.primary_id, b.primary_id);
    }

    // =====================================================================
    // add_cursor / add_selection
    // =====================================================================

    #[test]
    fn add_cursor_at_same_position_merges_to_one() {
        let mut mc = state(5);
        let b = mc.add_cursor(c(5));
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(5)));
        assert_eq!(mc.selections[0].id, b, "merge keeps the newer id");
        assert_primary_resolves(&mc);
        assert_ids_unique(&mc);
    }

    #[test]
    fn add_cursor_distinct_positions_stay_separate_and_sorted() {
        let mut mc = state(30);
        let _ = mc.add_cursor(c(10));
        let last = mc.add_cursor(c(20));
        assert_eq!(mc.len(), 3);
        // Sorted by position, NOT by insertion order.
        assert_eq!(
            mc.to_selections(),
            vec![
                Selection::Cursor(c(10)),
                Selection::Cursor(c(20)),
                Selection::Cursor(c(30)),
            ]
        );
        // Primary is the most recently added (byte 20), which is the *middle*
        // element — proving primary is tracked by id, not vec position.
        assert_eq!(mc.get_primary().unwrap().id, last);
        assert_eq!(mc.get_primary_cursor(), Some(c(20)));
        assert_sorted_nonoverlapping(&mc);
        assert_primary_resolves(&mc);
        assert_ids_unique(&mc);
    }

    #[test]
    fn add_cursor_same_byte_different_affinity_does_not_merge() {
        // Leading < Trailing, so cur_start(Trailing) > last_end(Leading) and the
        // merge condition (`cur_start <= last_end`) is false. Two carets survive
        // at the same byte offset.
        let mut mc = MultiCursorState::new_with_cursor(
            c_full(0, 4, CursorAffinity::Leading),
            DomNodeId::ROOT,
            0,
        );
        let _ = mc.add_cursor(c_full(0, 4, CursorAffinity::Trailing));
        assert_eq!(mc.len(), 2);
        assert_sorted_nonoverlapping(&mc);
        assert_primary_resolves(&mc);
    }

    #[test]
    fn add_selection_overlapping_ranges_merge_into_union() {
        let mut mc = empty_state();
        let _ = mc.add_selection(rng(0, 10));
        let _ = mc.add_selection(rng(5, 20));
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Range(rng(0, 20)));
        assert_primary_resolves(&mc);
    }

    #[test]
    fn add_selection_touching_ranges_merge() {
        // Adjacent (end == start) counts as overlapping: `cur_start <= last_end`.
        let mut mc = empty_state();
        let _ = mc.add_selection(rng(0, 10));
        let _ = mc.add_selection(rng(10, 20));
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Range(rng(0, 20)));
    }

    #[test]
    fn add_selection_disjoint_ranges_stay_separate() {
        let mut mc = empty_state();
        let _ = mc.add_selection(rng(0, 10));
        let _ = mc.add_selection(rng(11, 20));
        assert_eq!(mc.len(), 2);
        assert_sorted_nonoverlapping(&mc);
        assert_primary_resolves(&mc);
    }

    #[test]
    fn add_selection_reversed_range_is_normalized_for_merging() {
        // Backwards selection: start (20) is logically after end (5).
        let mut mc = empty_state();
        let _ = mc.add_selection(SelectionRange {
            start: c(20),
            end: c(5),
        });
        // A cursor *inside* the backwards range must merge with it.
        let _ = mc.add_cursor(c(10));
        assert_eq!(mc.len(), 1);
        // The merged result is normalized to a forwards range.
        assert_eq!(mc.selections[0].selection, Selection::Range(rng(5, 20)));
        assert_primary_resolves(&mc);
    }

    #[test]
    fn add_selection_at_u32_max_boundary_does_not_overflow() {
        let mut mc = empty_state();
        let _ = mc.add_selection(rng(u32::MAX - 1, u32::MAX));
        let _ = mc.add_cursor(c(u32::MAX));
        assert_eq!(mc.len(), 1);
        assert_eq!(
            mc.selections[0].selection,
            Selection::Range(rng(u32::MAX - 1, u32::MAX))
        );
        assert_primary_resolves(&mc);
    }

    #[test]
    fn add_cursor_stress_500_distinct_positions() {
        let mut mc = state(0);
        for i in 1..=500u32 {
            let _ = mc.add_cursor(c(i * 2));
        }
        assert_eq!(mc.len(), 501);
        assert_sorted_nonoverlapping(&mc);
        assert_primary_resolves(&mc);
        assert_ids_unique(&mc);
    }

    #[test]
    fn add_cursor_stress_same_position_never_grows() {
        let mut mc = state(9);
        for _ in 0..300 {
            let _ = mc.add_cursor(c(9));
        }
        assert_eq!(mc.len(), 1, "identical cursors must always collapse");
        assert_primary_resolves(&mc);
    }

    // =====================================================================
    // remove_selection
    // =====================================================================

    #[test]
    fn remove_selection_unknown_id_returns_false_and_changes_nothing() {
        let mut mc = state(1);
        let before = mc.clone();
        let ghost = SelectionId::new(); // never inserted anywhere
        assert!(!mc.remove_selection(ghost));
        assert_eq!(mc, before);
    }

    #[test]
    fn remove_selection_twice_second_call_returns_false() {
        let mut mc = state(0);
        let b = mc.add_cursor(c(10));
        assert!(mc.remove_selection(b));
        assert!(!mc.remove_selection(b));
        assert_eq!(mc.len(), 1);
        assert_primary_resolves(&mc);
    }

    #[test]
    fn remove_all_selections_leaves_a_safe_empty_state() {
        let mut mc = state(0);
        let b = mc.add_cursor(c(10));
        let a = mc.selections.iter().find(|s| s.id != b).unwrap().id;
        assert!(mc.remove_selection(a));
        assert!(mc.remove_selection(b));
        assert!(mc.is_empty());
        assert_eq!(mc.len(), 0);
        assert!(mc.get_primary().is_none());
        assert!(mc.get_primary_cursor().is_none());
        assert!(mc.to_selections().is_empty());
        // Further mutation of the empty state must not panic.
        mc.merge_overlapping();
        mc.move_all_cursors(true, |cur| *cur);
        assert!(mc.is_empty());
    }

    #[test]
    fn remove_primary_from_three_repoints_to_a_survivor() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        let p = mc.add_cursor(c(20)); // primary
        assert_eq!(mc.get_primary().unwrap().id, p);
        assert!(mc.remove_selection(p));
        assert_eq!(mc.len(), 2);
        assert_primary_resolves(&mc);
    }

    // =====================================================================
    // get_primary / get_primary_mut / get_primary_cursor / to_selections / len
    // =====================================================================

    #[test]
    fn empty_state_getters_return_none_without_panicking() {
        let mut mc = empty_state();
        assert!(mc.is_empty());
        assert_eq!(mc.len(), 0);
        assert!(mc.get_primary().is_none());
        assert!(mc.get_primary_mut().is_none());
        assert!(mc.get_primary_cursor().is_none());
        assert!(mc.to_selections().is_empty());
        mc.merge_overlapping(); // early-returns on len <= 1
        mc.ensure_primary_valid(); // private: must not panic on empty vec
        assert!(mc.is_empty());
    }

    #[test]
    fn get_primary_falls_back_to_last_when_primary_id_is_dangling() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        mc.primary_id = SelectionId::new(); // dangling: names nothing in the vec
        let p = mc.get_primary().expect("must fall back, not return None");
        assert_eq!(p.id, mc.selections.last().unwrap().id);
        assert_eq!(mc.get_primary_cursor(), Some(c(10)));
    }

    #[test]
    fn ensure_primary_valid_adopts_last_id_when_dangling() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        let dangling = SelectionId::new();
        mc.primary_id = dangling;
        mc.ensure_primary_valid();
        assert_ne!(mc.primary_id, dangling);
        assert_eq!(mc.primary_id, mc.selections.last().unwrap().id);
        // Idempotent: a second call on a now-valid id is a no-op.
        let fixed = mc.primary_id;
        mc.ensure_primary_valid();
        assert_eq!(mc.primary_id, fixed);
    }

    #[test]
    fn ensure_primary_valid_on_empty_leaves_id_untouched() {
        let mut mc = empty_state();
        let before = mc.primary_id;
        mc.ensure_primary_valid();
        assert_eq!(
            mc.primary_id, before,
            "nothing to adopt — id must not change"
        );
        assert!(mc.get_primary().is_none());
    }

    #[test]
    fn get_primary_cursor_of_a_range_is_its_end_field() {
        let mut mc = empty_state();
        mc.set_single_range(rng(3, 9));
        assert_eq!(mc.get_primary_cursor(), Some(c(9)));

        // Backwards range: the raw `end` field is returned (the *focus*), even
        // though it is the lower position. This is deliberate — the caret sits
        // at the focus, not at the max boundary.
        let mut back = empty_state();
        back.set_single_range(SelectionRange {
            start: c(9),
            end: c(3),
        });
        assert_eq!(back.get_primary_cursor(), Some(c(3)));
    }

    #[test]
    fn get_primary_mut_mutation_is_visible_through_get_primary() {
        let mut mc = state(0);
        let p = mc.add_cursor(c(50));
        {
            let prim = mc.get_primary_mut().expect("primary exists");
            assert_eq!(prim.id, p);
            prim.selection = Selection::Range(rng(50, 60));
        }
        assert_eq!(
            mc.get_primary().unwrap().selection,
            Selection::Range(rng(50, 60))
        );
        assert_eq!(mc.get_primary_cursor(), Some(c(60)));
    }

    #[test]
    fn get_primary_mut_falls_back_to_last_when_dangling() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        mc.primary_id = SelectionId::new();
        let last_id = mc.selections.last().unwrap().id;
        let prim = mc.get_primary_mut().expect("fallback to last");
        assert_eq!(prim.id, last_id);
    }

    #[test]
    fn to_selections_matches_the_internal_order_and_len() {
        let mut mc = state(30);
        let _ = mc.add_cursor(c(10));
        let _ = mc.add_selection(rng(15, 20));
        let sels = mc.to_selections();
        assert_eq!(sels.len(), mc.len());
        let inner: Vec<Selection> = mc.selections.iter().map(|s| s.selection).collect();
        assert_eq!(sels, inner);
    }

    #[test]
    fn len_and_is_empty_always_agree() {
        let mut mc = empty_state();
        assert!(mc.is_empty() && mc.is_empty());
        let _ = mc.add_cursor(c(1));
        assert!(!mc.is_empty() && mc.len() == 1);
        for i in 2..20u32 {
            let _ = mc.add_cursor(c(i * 3));
        }
        assert_eq!(mc.len(), mc.selections.len());
        assert_eq!(mc.is_empty(), mc.is_empty());
        assert!(!mc.is_empty());
    }

    // =====================================================================
    // update_from_edit_result
    // =====================================================================

    #[test]
    fn update_from_edit_result_with_empty_slice_clears_everything() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        mc.update_from_edit_result(&[]);
        assert!(mc.is_empty());
        assert!(mc.get_primary().is_none());
        assert!(mc.get_primary_cursor().is_none());
    }

    #[test]
    fn update_from_edit_result_preserves_ids_by_index_and_mints_extras() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        let old: Vec<SelectionId> = mc.selections.iter().map(|s| s.id).collect();
        assert_eq!(old.len(), 2);

        mc.update_from_edit_result(&[
            Selection::Cursor(c(1)),
            Selection::Cursor(c(2)),
            Selection::Cursor(c(3)),
            Selection::Range(rng(4, 8)),
        ]);
        assert_eq!(mc.len(), 4);
        assert_eq!(mc.selections[0].id, old[0], "id preserved by index");
        assert_eq!(mc.selections[1].id, old[1], "id preserved by index");
        assert_ids_unique(&mc);
        assert_primary_resolves(&mc);
        assert_eq!(mc.selections[3].selection, Selection::Range(rng(4, 8)));
    }

    #[test]
    fn update_from_edit_result_shrinking_keeps_primary_resolvable() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        let _ = mc.add_cursor(c(20)); // primary = the byte-20 cursor
        mc.update_from_edit_result(&[Selection::Cursor(c(99))]);
        assert_eq!(mc.len(), 1);
        // The primary's id is gone (only index 0's id survived), so
        // ensure_primary_valid must have re-pointed it at the survivor.
        assert_primary_resolves(&mc);
        assert_eq!(mc.get_primary_cursor(), Some(c(99)));
    }

    #[test]
    fn update_from_edit_result_does_not_merge_overlaps() {
        // Documented: "Don't merge here — edit_text already returns correct positions"
        let mut mc = state(0);
        mc.update_from_edit_result(&[Selection::Range(rng(0, 10)), Selection::Range(rng(5, 15))]);
        assert_eq!(mc.len(), 2, "update must NOT merge");
        // ... but an explicit merge afterwards collapses them.
        mc.merge_overlapping();
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Range(rng(0, 15)));
        assert_primary_resolves(&mc);
    }

    #[test]
    fn update_from_edit_result_with_1000_selections() {
        let mut mc = state(0);
        let big: Vec<Selection> = (0..1000u32).map(|i| Selection::Cursor(c(i * 4))).collect();
        mc.update_from_edit_result(&big);
        assert_eq!(mc.len(), 1000);
        assert_ids_unique(&mc);
        assert_primary_resolves(&mc);
        assert_eq!(mc.to_selections(), big);
    }

    // =====================================================================
    // set_single_cursor / set_single_range
    // =====================================================================

    #[test]
    fn set_single_cursor_collapses_all_selections() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        let _ = mc.add_selection(rng(20, 30));
        mc.set_single_cursor(c(7));
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(7)));
        assert_primary_resolves(&mc);
        assert_eq!(mc.get_primary_cursor(), Some(c(7)));
    }

    #[test]
    fn set_single_range_collapses_all_selections() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        mc.set_single_range(rng(u32::MAX - 2, u32::MAX));
        assert_eq!(mc.len(), 1);
        assert_eq!(
            mc.selections[0].selection,
            Selection::Range(rng(u32::MAX - 2, u32::MAX))
        );
        assert_primary_resolves(&mc);
    }

    #[test]
    fn set_single_cursor_on_empty_state_mints_a_fresh_id() {
        let mut mc = empty_state();
        let stale = mc.primary_id;
        mc.set_single_cursor(c(1));
        assert_eq!(mc.len(), 1);
        assert_ne!(
            mc.primary_id, stale,
            "no last element -> a new id is minted"
        );
        assert_primary_resolves(&mc);
    }

    #[test]
    fn set_single_range_on_empty_state_mints_a_fresh_id() {
        let mut mc = empty_state();
        mc.set_single_range(rng(0, 0));
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Range(rng(0, 0)));
        assert_primary_resolves(&mc);
    }

    #[test]
    fn set_single_cursor_is_idempotent() {
        let mut mc = state(0);
        mc.set_single_cursor(c(5));
        let first = mc.clone();
        mc.set_single_cursor(c(5));
        assert_eq!(mc, first, "re-setting the same cursor must reuse the id");
    }

    // =====================================================================
    // merge_overlapping
    // =====================================================================

    #[test]
    fn merge_overlapping_on_empty_and_single_is_a_noop() {
        let mut e = empty_state();
        e.merge_overlapping();
        assert!(e.is_empty());

        let mut one = state(3);
        let before = one.clone();
        one.merge_overlapping();
        assert_eq!(one, before);
    }

    #[test]
    fn merge_overlapping_collapses_a_whole_chain() {
        let mut mc = empty_state();
        let ids: Vec<SelectionId> = (0..4).map(|_| SelectionId::new()).collect();
        mc.selections = vec![
            ident(ids[0], Selection::Range(rng(25, 40))),
            ident(ids[1], Selection::Range(rng(0, 10))),
            ident(ids[2], Selection::Range(rng(12, 30))),
            ident(ids[3], Selection::Range(rng(5, 15))),
        ];
        mc.primary_id = ids[3];
        mc.merge_overlapping();
        // 0..10 ∪ 5..15 ∪ 12..30 ∪ 25..40 = one contiguous 0..40
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Range(rng(0, 40)));
        assert_sorted_nonoverlapping(&mc);
        assert_primary_resolves(&mc);
    }

    #[test]
    fn merge_overlapping_keeps_disjoint_selections_and_sorts_them() {
        let mut mc = empty_state();
        let ids: Vec<SelectionId> = (0..3).map(|_| SelectionId::new()).collect();
        mc.selections = vec![
            ident(ids[0], Selection::Cursor(c(100))),
            ident(ids[1], Selection::Range(rng(0, 5))),
            ident(ids[2], Selection::Cursor(c(50))),
        ];
        mc.primary_id = ids[0];
        mc.merge_overlapping();
        assert_eq!(mc.len(), 3);
        assert_eq!(
            mc.to_selections(),
            vec![
                Selection::Range(rng(0, 5)),
                Selection::Cursor(c(50)),
                Selection::Cursor(c(100)),
            ]
        );
        assert_sorted_nonoverlapping(&mc);
        // The primary (byte 100) survived the sort untouched.
        assert_eq!(mc.primary_id, ids[0]);
        assert_eq!(mc.get_primary_cursor(), Some(c(100)));
    }

    #[test]
    fn merge_overlapping_zero_width_merge_yields_a_cursor_not_a_range() {
        let mut mc = empty_state();
        let ids: Vec<SelectionId> = (0..2).map(|_| SelectionId::new()).collect();
        mc.selections = vec![
            ident(ids[0], Selection::Cursor(c(8))),
            ident(ids[1], Selection::Range(rng(8, 8))),
        ];
        mc.primary_id = ids[1];
        mc.merge_overlapping();
        assert_eq!(mc.len(), 1);
        // new_start == new_end -> collapses back to a Cursor.
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(8)));
        assert_primary_resolves(&mc);
    }

    #[test]
    fn merge_overlapping_is_idempotent() {
        let mut mc = empty_state();
        let ids: Vec<SelectionId> = (0..5).map(|_| SelectionId::new()).collect();
        mc.selections = vec![
            ident(ids[0], Selection::Range(rng(0, 10))),
            ident(ids[1], Selection::Cursor(c(5))),
            ident(ids[2], Selection::Range(rng(30, 20))), // reversed
            ident(ids[3], Selection::Cursor(c(100))),
            ident(ids[4], Selection::Range(rng(99, 101))),
        ];
        mc.primary_id = ids[2];
        mc.merge_overlapping();
        let once = mc.clone();
        mc.merge_overlapping();
        assert_eq!(mc, once, "merge_overlapping must be a fixed point");
        assert_sorted_nonoverlapping(&mc);
        assert_primary_resolves(&mc);
    }

    #[test]
    fn merge_overlapping_adversarial_200_selections_keeps_invariants() {
        let mut mc = empty_state();
        let mut seed: u32 = 0x1234_5678;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            seed
        };
        let mut sels = Vec::new();
        for i in 0..200u32 {
            let a = next() % 1000;
            let b = next() % 1000;
            let sel = match i % 4 {
                0 => Selection::Cursor(c(a)),
                1 => Selection::Range(SelectionRange {
                    start: c(a),
                    end: c(b),
                }), // may be reversed
                2 => Selection::Range(rng(a.min(b), a.max(b))),
                _ => Selection::Cursor(c_full(
                    0,
                    a,
                    if b % 2 == 0 {
                        CursorAffinity::Leading
                    } else {
                        CursorAffinity::Trailing
                    },
                )),
            };
            sels.push(ident(SelectionId::new(), sel));
        }
        // Throw in the absolute boundaries too.
        sels.push(ident(SelectionId::new(), Selection::Cursor(c(0))));
        sels.push(ident(SelectionId::new(), Selection::Cursor(c(u32::MAX))));
        sels.push(ident(
            SelectionId::new(),
            Selection::Range(rng(u32::MAX - 1, u32::MAX)),
        ));
        mc.primary_id = sels[7].id;
        mc.selections = sels;

        mc.merge_overlapping();

        assert!(!mc.is_empty());
        assert!(mc.len() <= 203);
        assert_sorted_nonoverlapping(&mc);
        assert_primary_resolves(&mc);
        assert_ids_unique(&mc);
    }

    #[test]
    fn merge_overlapping_primary_inside_a_chain_still_resolves() {
        // Three cursors that all collapse into one, plus a far-away cursor.
        // The primary is the *first* link of the merge chain.
        let mut mc = empty_state();
        let ids: Vec<SelectionId> = (0..4).map(|_| SelectionId::new()).collect();
        mc.selections = vec![
            ident(ids[0], Selection::Cursor(c(0))),
            ident(ids[1], Selection::Cursor(c(0))),
            ident(ids[2], Selection::Cursor(c(0))),
            ident(ids[3], Selection::Cursor(c(100))),
        ];
        mc.primary_id = ids[0];
        mc.merge_overlapping();

        assert_eq!(mc.len(), 2);
        // Whatever the merge does with ids, `primary_id` must never dangle.
        assert!(
            mc.selections.iter().any(|s| s.id == mc.primary_id),
            "primary_id must name a surviving selection"
        );
        assert!(mc.get_primary().is_some());
    }

    #[test]
    fn merge_overlapping_primary_should_follow_its_merge_chain() {
        // Same setup as above. `merge_overlapping` records `new_primary = sel.id`
        // when the chain's head is the primary, but the head's id is then
        // overwritten by the *next* merge, so `new_primary` points at an id that
        // no longer exists. ensure_primary_valid() then silently adopts the
        // vector's LAST element — the unrelated cursor at byte 100.
        //
        // Expected: the primary follows the merged selection it was part of (byte 0).
        let mut mc = empty_state();
        let ids: Vec<SelectionId> = (0..4).map(|_| SelectionId::new()).collect();
        mc.selections = vec![
            ident(ids[0], Selection::Cursor(c(0))),
            ident(ids[1], Selection::Cursor(c(0))),
            ident(ids[2], Selection::Cursor(c(0))),
            ident(ids[3], Selection::Cursor(c(100))),
        ];
        mc.primary_id = ids[0];
        mc.merge_overlapping();
        assert_eq!(
            mc.get_primary_cursor(),
            Some(c(0)),
            "primary jumped to an unrelated selection after the merge"
        );
    }

    // =====================================================================
    // move_all_cursors
    // =====================================================================

    #[test]
    fn move_all_cursors_identity_leaves_positions_unchanged() {
        let mut mc = state(0);
        let _ = mc.add_cursor(c(10));
        let _ = mc.add_cursor(c(20));
        let before = mc.to_selections();
        mc.move_all_cursors(false, |cur| *cur);
        assert_eq!(mc.to_selections(), before);
        assert_eq!(mc.len(), 3);
        assert_primary_resolves(&mc);
    }

    #[test]
    fn move_all_cursors_extend_with_no_movement_keeps_a_cursor() {
        // `*c != new_cursor` is false -> the selection must stay a Cursor,
        // not degenerate into a zero-width Range.
        let mut mc = state(4);
        mc.move_all_cursors(true, |cur| *cur);
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(4)));
    }

    #[test]
    fn move_all_cursors_extend_turns_a_cursor_into_a_range() {
        let mut mc = state(10);
        mc.move_all_cursors(true, |cur| c(cur.cluster_id.start_byte_in_run + 5));
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Range(rng(10, 15)));
        // The anchor stayed at 10, only the focus moved.
        assert_eq!(mc.get_primary_cursor(), Some(c(15)));
    }

    #[test]
    fn move_all_cursors_bare_forward_arrow_collapses_range_to_max_boundary() {
        let mut mc = empty_state();
        mc.set_single_range(rng(3, 9));
        mc.move_all_cursors(false, |cur| c(cur.cluster_id.start_byte_in_run + 1));
        assert_eq!(mc.len(), 1);
        // Collapses to the boundary WITHOUT stepping past it (not byte 10).
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(9)));
    }

    #[test]
    fn move_all_cursors_bare_backward_arrow_collapses_range_to_min_boundary() {
        let mut mc = empty_state();
        mc.set_single_range(rng(3, 9));
        mc.move_all_cursors(false, |cur| {
            c(cur.cluster_id.start_byte_in_run.saturating_sub(1))
        });
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(3)));
    }

    #[test]
    fn move_all_cursors_collapses_a_backwards_range_by_direction_not_field_order() {
        // Backwards range (focus at 3, anchor at 9): a forward arrow must still
        // collapse to the max boundary (9), not to the `end` field (3).
        let mut mc = empty_state();
        mc.set_single_range(SelectionRange {
            start: c(9),
            end: c(3),
        });
        mc.move_all_cursors(false, |cur| c(cur.cluster_id.start_byte_in_run + 1));
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(9)));

        let mut back = empty_state();
        back.set_single_range(SelectionRange {
            start: c(9),
            end: c(3),
        });
        back.move_all_cursors(false, |cur| {
            c(cur.cluster_id.start_byte_in_run.saturating_sub(1))
        });
        assert_eq!(back.selections[0].selection, Selection::Cursor(c(3)));
    }

    #[test]
    fn move_all_cursors_without_boundary_collapse_performs_the_step_from_the_focus() {
        // Home over an active range: the caret goes where the step points, not
        // to the range's near edge. `move_fn` here is "go to byte 0".
        let mut mc = empty_state();
        mc.set_single_range(rng(3, 9));
        mc.move_all_cursors_with(false, false, |_| c(0));
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(0)));

        // The same step with the arrow-key rule still answers the boundary.
        let mut arrow = empty_state();
        arrow.set_single_range(rng(3, 9));
        arrow.move_all_cursors_with(false, true, |_| c(0));
        assert_eq!(arrow.selections[0].selection, Selection::Cursor(c(3)));

        // End over the same range: byte 20 is past `hi`, which the boundary
        // rule would have clamped to 9.
        let mut end = empty_state();
        end.set_single_range(rng(3, 9));
        end.move_all_cursors_with(false, false, |_| c(20));
        assert_eq!(end.selections[0].selection, Selection::Cursor(c(20)));

        // A bare cursor is unaffected by the flag either way.
        for collapse in [false, true] {
            let mut bare = state(5);
            bare.move_all_cursors_with(false, collapse, |_| c(0));
            assert_eq!(bare.selections[0].selection, Selection::Cursor(c(0)));
        }
    }

    #[test]
    fn move_all_cursors_extend_back_onto_the_anchor_collapses_to_a_cursor() {
        let mut mc = empty_state();
        mc.set_single_range(rng(3, 4));
        // Shrink the focus back onto the anchor: r.start == new_end.
        mc.move_all_cursors(true, |_| c(3));
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(3)));
    }

    #[test]
    fn move_all_cursors_constant_move_fn_merges_everything_into_one() {
        let mut mc = state(0);
        for i in 1..5u32 {
            let _ = mc.add_cursor(c(i * 10));
        }
        assert_eq!(mc.len(), 5);
        mc.move_all_cursors(false, |_| c(7));
        assert_eq!(mc.len(), 1, "colliding cursors must be merged afterwards");
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(7)));
        assert_primary_resolves(&mc);
        assert_sorted_nonoverlapping(&mc);
    }

    #[test]
    fn move_all_cursors_saturating_at_u32_max_does_not_overflow() {
        let mut mc = empty_state();
        let ids: Vec<SelectionId> = (0..2).map(|_| SelectionId::new()).collect();
        mc.selections = vec![
            ident(ids[0], Selection::Cursor(c(u32::MAX - 1))),
            ident(ids[1], Selection::Cursor(c(u32::MAX))),
        ];
        mc.primary_id = ids[1];
        mc.move_all_cursors(false, |cur| {
            c(cur.cluster_id.start_byte_in_run.saturating_add(1))
        });
        // Both saturate to u32::MAX and merge.
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.selections[0].selection, Selection::Cursor(c(u32::MAX)));
        assert_primary_resolves(&mc);
    }

    #[test]
    fn move_all_cursors_on_empty_state_does_not_panic() {
        let mut mc = empty_state();
        mc.move_all_cursors(false, |cur| *cur);
        mc.move_all_cursors(true, |_| c(u32::MAX));
        assert!(mc.is_empty());
    }

    #[test]
    fn move_all_cursors_stress_keeps_invariants() {
        let mut mc = state(0);
        for i in 1..100u32 {
            let _ = mc.add_cursor(c(i * 5));
        }
        for _ in 0..10 {
            mc.move_all_cursors(false, |cur| {
                // Fold every cursor into a small window -> heavy merging.
                c(cur.cluster_id.start_byte_in_run % 7)
            });
            assert_sorted_nonoverlapping(&mc);
            assert_primary_resolves(&mc);
            assert_ids_unique(&mc);
        }
        assert!(mc.len() <= 7);
    }

    // =====================================================================
    // remap_node_ids
    // =====================================================================

    #[test]
    fn remap_node_ids_for_a_different_dom_is_a_noop() {
        let mut mc = MultiCursorState::new_with_cursor(c(1), dom_node(5), 0);
        mc.node_id.dom = DomId { inner: 7 };
        let before = mc.clone();
        let mut map = BTreeMap::new();
        map.insert(NodeId::new(5), NodeId::new(9));
        mc.remap_node_ids(DomId::ROOT_ID, &map);
        assert_eq!(mc, before, "a foreign DomId must not touch this state");
    }

    #[test]
    fn remap_node_ids_rewrites_a_surviving_node() {
        let mut mc = MultiCursorState::new_with_cursor(c(1), dom_node(5), 0);
        let mut map = BTreeMap::new();
        map.insert(NodeId::new(5), NodeId::new(9));
        mc.remap_node_ids(DomId::ROOT_ID, &map);
        assert_eq!(mc.node_id.node.into_crate_internal(), Some(NodeId::new(9)));
        assert_eq!(mc.len(), 1, "selections survive a successful remap");
        assert_primary_resolves(&mc);
    }

    #[test]
    fn remap_node_ids_clears_selections_when_the_node_was_removed() {
        let mut mc = MultiCursorState::new_with_cursor(c(1), dom_node(5), 0);
        let _ = mc.add_cursor(c(20));
        let map: BTreeMap<NodeId, NodeId> = BTreeMap::new(); // node 5 is gone
        mc.remap_node_ids(DomId::ROOT_ID, &map);
        assert!(mc.is_empty(), "a removed node must drop its selections");
        assert!(mc.get_primary().is_none());
        // node_id itself is left alone (only selections are cleared).
        assert_eq!(mc.node_id.node.into_crate_internal(), Some(NodeId::new(5)));
    }

    #[test]
    fn remap_node_ids_with_a_none_node_is_a_noop() {
        // DomNodeId::ROOT carries NodeHierarchyItemId::NONE -> into_crate_internal()
        // is None, so neither branch runs and the selections must survive.
        let mut mc = state(3);
        let map: BTreeMap<NodeId, NodeId> = BTreeMap::new();
        mc.remap_node_ids(DomId::ROOT_ID, &map);
        assert_eq!(mc.len(), 1);
        assert_eq!(mc.node_id.node, NodeHierarchyItemId::NONE);
        assert_primary_resolves(&mc);
    }

    #[test]
    fn remap_node_ids_handles_large_node_indices() {
        let big = 1_000_000usize;
        let mut mc = MultiCursorState::new_with_cursor(c(1), dom_node(big), 0);
        let mut map = BTreeMap::new();
        map.insert(NodeId::new(big), NodeId::new(big * 2));
        mc.remap_node_ids(DomId::ROOT_ID, &map);
        assert_eq!(
            mc.node_id.node.into_crate_internal(),
            Some(NodeId::new(big * 2))
        );
    }

    #[test]
    fn remap_node_ids_twice_is_stable() {
        let mut mc = MultiCursorState::new_with_cursor(c(1), dom_node(5), 0);
        let mut map = BTreeMap::new();
        map.insert(NodeId::new(5), NodeId::new(9));
        map.insert(NodeId::new(9), NodeId::new(9)); // identity for the new id
        mc.remap_node_ids(DomId::ROOT_ID, &map);
        mc.remap_node_ids(DomId::ROOT_ID, &map);
        assert_eq!(mc.node_id.node.into_crate_internal(), Some(NodeId::new(9)));
        assert_eq!(mc.len(), 1);
    }

    // =====================================================================
    // selection_start_pos / selection_end_pos  (private helpers)
    // =====================================================================

    #[test]
    fn selection_pos_helpers_normalize_reversed_ranges() {
        let forward = Selection::Range(rng(3, 9));
        assert_eq!(selection_start_pos(&forward), c(3));
        assert_eq!(selection_end_pos(&forward), c(9));

        let backward = Selection::Range(SelectionRange {
            start: c(9),
            end: c(3),
        });
        assert_eq!(selection_start_pos(&backward), c(3));
        assert_eq!(selection_end_pos(&backward), c(9));

        let cursor = Selection::Cursor(c(5));
        assert_eq!(selection_start_pos(&cursor), c(5));
        assert_eq!(selection_end_pos(&cursor), c(5));
    }

    #[test]
    fn selection_pos_helpers_start_never_exceeds_end() {
        let mut seed: u32 = 0xACE1_BEEF;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            seed
        };
        let extremes = [0u32, 1, u32::MAX - 1, u32::MAX];
        let mut cases: Vec<Selection> = Vec::new();
        for a in extremes {
            for b in extremes {
                cases.push(Selection::Range(SelectionRange {
                    start: c_full(a, b, CursorAffinity::Trailing),
                    end: c_full(b, a, CursorAffinity::Leading),
                }));
                cases.push(Selection::Cursor(c_full(a, b, CursorAffinity::Leading)));
            }
        }
        for _ in 0..200 {
            cases.push(Selection::Range(SelectionRange {
                start: c(next()),
                end: c(next()),
            }));
        }
        for sel in &cases {
            assert!(
                selection_start_pos(sel) <= selection_end_pos(sel),
                "start must never sort after end: {sel:?}"
            );
        }
    }

    #[test]
    fn selection_pos_helpers_respect_affinity_ordering() {
        // Same byte, different affinity: Leading < Trailing.
        let sel = Selection::Range(SelectionRange {
            start: c_full(0, 4, CursorAffinity::Trailing),
            end: c_full(0, 4, CursorAffinity::Leading),
        });
        assert_eq!(
            selection_start_pos(&sel),
            c_full(0, 4, CursorAffinity::Leading)
        );
        assert_eq!(
            selection_end_pos(&sel),
            c_full(0, 4, CursorAffinity::Trailing)
        );
    }

    // =====================================================================
    // TextSelection
    // =====================================================================

    fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
    }

    #[test]
    fn new_collapsed_invariants_hold() {
        let node = NodeId::new(3);
        let sel = TextSelection::new_collapsed(
            DomId::ROOT_ID,
            node,
            c(7),
            rect(1.0, 2.0, 3.0, 4.0),
            LogicalPosition::new(5.0, 6.0),
        );
        assert!(sel.is_collapsed());
        assert!(sel.is_forward);
        assert_eq!(sel.dom_id, DomId::ROOT_ID);
        assert_eq!(sel.anchor.ifc_root_node_id, node);
        assert_eq!(sel.focus.ifc_root_node_id, node);
        assert_eq!(sel.anchor.cursor, c(7));
        assert_eq!(sel.focus.cursor, c(7));
        assert_eq!(sel.affected_nodes.len(), 1);
        // The collapsed node maps to a zero-width range at the cursor.
        assert_eq!(
            sel.get_range_for_node(&node),
            Some(&SelectionRange {
                start: c(7),
                end: c(7),
            })
        );
    }

    #[test]
    fn new_collapsed_with_non_finite_geometry_does_not_panic() {
        let node = NodeId::new(0);
        let sel = TextSelection::new_collapsed(
            DomId::ROOT_ID,
            node,
            c_full(u32::MAX, u32::MAX, CursorAffinity::Trailing),
            rect(f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX),
            LogicalPosition::new(f32::NAN, f32::NEG_INFINITY),
        );
        // Geometry is carried verbatim; only the cursors decide collapsedness.
        assert!(sel.is_collapsed());
        assert!(sel.get_range_for_node(&node).is_some());
        assert!(sel.anchor.char_bounds.origin.x.is_nan());
    }

    #[test]
    fn get_range_for_node_returns_none_for_an_unaffected_node() {
        let sel = TextSelection::new_collapsed(
            DomId::ROOT_ID,
            NodeId::new(3),
            c(0),
            rect(0.0, 0.0, 0.0, 0.0),
            LogicalPosition::new(0.0, 0.0),
        );
        assert!(sel.get_range_for_node(&NodeId::new(4)).is_none());
        assert!(sel.get_range_for_node(&NodeId::new(0)).is_none());
        assert!(sel.get_range_for_node(&NodeId::new(usize::MAX)).is_none());
    }

    #[test]
    fn get_range_for_node_on_an_empty_map_returns_none() {
        let node = NodeId::new(3);
        let mut sel = TextSelection::new_collapsed(
            DomId::ROOT_ID,
            node,
            c(0),
            rect(0.0, 0.0, 0.0, 0.0),
            LogicalPosition::new(0.0, 0.0),
        );
        sel.affected_nodes.clear();
        assert!(sel.get_range_for_node(&node).is_none());
        assert!(
            sel.is_collapsed(),
            "collapsedness does not depend on the map"
        );
    }

    #[test]
    fn ranges_for_node_returns_every_range_the_node_carries() {
        // A Ctrl+D session puts all of its occurrences on ONE node, so the
        // carrier has to be a list — the map used to hold a single range and
        // every occurrence but one was unexpressible.
        let node = NodeId::new(3);
        let mut sel = TextSelection::new_collapsed(
            DomId::ROOT_ID,
            node,
            c(0),
            rect(0.0, 0.0, 0.0, 0.0),
            LogicalPosition::new(0.0, 0.0),
        );
        let first = SelectionRange {
            start: c(0),
            end: c(2),
        };
        let second = SelectionRange {
            start: c(5),
            end: c(7),
        };
        sel.affected_nodes.insert(node, vec![first, second]);

        assert_eq!(sel.ranges_for_node(&node), &[first, second]);
        assert_eq!(
            sel.get_range_for_node(&node),
            Some(&first),
            "the single-range accessor answers with the FIRST range"
        );
        assert!(sel.ranges_for_node(&NodeId::new(4)).is_empty());

        sel.affected_nodes.insert(node, Vec::new());
        assert!(sel.ranges_for_node(&node).is_empty());
        assert!(
            sel.get_range_for_node(&node).is_none(),
            "an empty list is not a range"
        );
    }

    #[test]
    fn is_collapsed_is_false_when_the_focus_cursor_moves() {
        let node = NodeId::new(3);
        let mut sel = TextSelection::new_collapsed(
            DomId::ROOT_ID,
            node,
            c(7),
            rect(0.0, 0.0, 1.0, 1.0),
            LogicalPosition::new(0.0, 0.0),
        );
        assert!(sel.is_collapsed());
        sel.focus.cursor = c(8);
        assert!(!sel.is_collapsed());
    }

    #[test]
    fn is_collapsed_is_false_when_the_focus_crosses_into_another_ifc() {
        let mut sel = TextSelection::new_collapsed(
            DomId::ROOT_ID,
            NodeId::new(3),
            c(7),
            rect(0.0, 0.0, 1.0, 1.0),
            LogicalPosition::new(0.0, 0.0),
        );
        sel.focus.ifc_root_node_id = NodeId::new(4); // same cursor, different node
        assert!(
            !sel.is_collapsed(),
            "same cursor offset in a different IFC is not a collapsed selection"
        );
    }

    #[test]
    fn is_collapsed_only_looks_at_cursors_not_at_mouse_position() {
        let node = NodeId::new(1);
        let mut sel = TextSelection::new_collapsed(
            DomId::ROOT_ID,
            node,
            c(2),
            rect(0.0, 0.0, 1.0, 1.0),
            LogicalPosition::new(0.0, 0.0),
        );
        sel.focus.mouse_position = LogicalPosition::new(999.0, -999.0);
        assert!(sel.is_collapsed());
    }
}


#[cfg(test)]
mod owner_tests {
    use alloc::{vec, vec::Vec};

    use crate::{
        dom::{DomId, DomNodeId, NodeId},
        selection::{
            CursorAffinity, GraphemeClusterId, MultiCursorState, Selection, SelectionOwner,
            SelectionRange, TextCursor,
        },
        styled_dom::NodeHierarchyItemId,
    };

    fn cursor(byte: u32) -> TextCursor {
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: byte,
            },
            affinity: CursorAffinity::Leading,
        }
    }

    fn node() -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
        }
    }

    fn state() -> MultiCursorState {
        MultiCursorState::new_with_cursor(cursor(0), node(), 0)
    }

    // ------------------------------------------------------------------
    // U3: a selection is `(owner, id)`; the engine acts on LOCAL only
    // ------------------------------------------------------------------

    fn with_peer(mut mc: MultiCursorState, peer: SelectionOwner, at: u32) -> MultiCursorState {
        assert!(mc.set_owner_selections(peer, &[Selection::Cursor(cursor(at))]));
        mc
    }

    #[test]
    fn typing_never_lands_on_a_peers_caret() {
        // THE DEFECT: `to_selections` handed every owner's caret to
        // `edit_text`, which inserts at each one - so a local keystroke was
        // typed at the peer's caret too. And `update_from_edit_result` then
        // rebuilt the list as LOCAL, absorbing the peer after one keystroke.
        let bob = SelectionOwner::new(2, 2);
        let mut mc = with_peer(state(), bob, 7);
        let edit_set = mc.to_selections();
        assert_eq!(edit_set, vec![Selection::Cursor(cursor(0))], "local only");

        let bob_id = mc.selections.iter().find(|s| s.owner == bob).unwrap().id;
        let local_id = mc.primary_id;
        // The edit moved the local caret to 1; Bob is not in the result.
        mc.update_from_edit_result(&[Selection::Cursor(cursor(1))]);
        assert_eq!(mc.len(), 2, "Bob is still in the session");
        let bob_after = mc.selections.iter().find(|s| s.owner == bob).unwrap();
        assert_eq!(bob_after.id, bob_id, "and keeps his id");
        assert_eq!(bob_after.selection, Selection::Cursor(cursor(7)), "and his place");
        assert_eq!(mc.get_primary().map(|p| p.id), Some(local_id));
        assert_eq!(mc.get_primary_cursor(), Some(cursor(1)));
    }

    #[test]
    fn a_plain_click_keeps_the_peers_in_view() {
        let alice = SelectionOwner::new(1, 1);
        let mut mc = with_peer(state(), alice, 3);
        let _ = mc.add_cursor(cursor(5)); // local multi-cursor
        assert_eq!(mc.local_len(), 2);

        mc.set_single_cursor(cursor(9));
        assert_eq!(mc.local_len(), 1, "the local set collapsed");
        assert!(mc.owners().contains(&alice), "the click did not erase Alice");
        assert!(mc.get_primary().unwrap().owner.is_local());

        mc.set_single_range(SelectionRange {
            start: cursor(1),
            end: cursor(4),
        });
        assert_eq!(mc.local_len(), 1);
        assert!(mc.owners().contains(&alice));
    }

    #[test]
    fn the_primary_is_never_a_peer() {
        // The list is owner-sorted, so a `last()` fallback WAS the peer
        // whenever one existed: removing the local primary would have made
        // Bob "the selection" - for the IME, the platform selection, copy.
        let bob = SelectionOwner::new(2, 2);
        let mut mc = with_peer(state(), bob, 7);
        let local_id = mc.primary_id;
        assert!(mc.remove_selection(local_id));
        assert!(mc.get_primary().is_none(), "no local selection: no primary");
        assert!(mc.get_primary_cursor().is_none());
        assert_eq!(mc.local_len(), 0);
        assert_eq!(mc.len(), 1, "Bob is still painted");

        // With a second local caret, the primary falls back onto IT.
        let mut mc = with_peer(state(), bob, 7);
        let second = mc.add_cursor(cursor(4));
        let first_local = mc.local_selections().map(|s| s.id).find(|id| *id != second).unwrap();
        assert!(mc.remove_selection(second));
        assert_eq!(mc.get_primary().map(|p| p.id), Some(first_local));
        assert!(mc.get_primary().unwrap().owner.is_local());
    }

    #[test]
    fn arrow_keys_move_only_the_local_carets() {
        let alice = SelectionOwner::new(1, 1);
        let mut mc = with_peer(state(), alice, 3);
        mc.move_all_cursors(false, |c| cursor(c.cluster_id.start_byte_in_run + 1));
        assert_eq!(mc.get_primary_cursor(), Some(cursor(1)));
        let alice_sel = mc.selections.iter().find(|s| s.owner == alice).unwrap();
        assert_eq!(alice_sel.selection, Selection::Cursor(cursor(3)), "Alice did not move");
    }

    #[test]
    fn local_len_counts_the_carets_typed_into() {
        let alice = SelectionOwner::new(1, 1);
        let mut mc = with_peer(state(), alice, 3);
        let _ = mc.add_cursor(cursor(5));
        assert_eq!(mc.len(), 3, "painted");
        assert_eq!(mc.local_len(), 2, "typed into");
    }

    #[test]
    fn the_engines_own_selections_are_local() {
        let mc = state();
        assert!(mc.selections[0].owner.is_local());
        assert_eq!(SelectionOwner::LOCAL, SelectionOwner::default());
        assert_eq!(mc.owners(), vec![SelectionOwner::LOCAL]);
    }

    /// THE POINT OF THE WHOLE FIELD. Two participants' cursors at the same
    /// place must stay two cursors: merging them would silently delete someone
    /// from the session - their caret absorbed into another person's and
    /// repainted in that person's colour.
    #[test]
    fn two_owners_at_the_same_position_do_not_merge() {
        let mut mc = state();
        let alice = SelectionOwner::new(1, 1);
        let bob = SelectionOwner::new(2, 2);
        assert!(mc.set_owner_selections(alice, &[Selection::Cursor(cursor(0))]));
        assert!(mc.set_owner_selections(bob, &[Selection::Cursor(cursor(0))]));

        mc.merge_overlapping();

        assert_eq!(mc.selections.len(), 3, "local + two peers, all at offset 0");
        assert_eq!(mc.owners().len(), 3);
    }

    /// ...but one owner's own overlapping selections still collapse, which is
    /// what `merge_overlapping` is for.
    #[test]
    fn one_owners_overlapping_selections_still_merge() {
        let mut mc = state();
        let alice = SelectionOwner::new(1, 1);
        mc.set_owner_selections(
            alice,
            &[
                Selection::Range(SelectionRange {
                    start: cursor(0),
                    end: cursor(5),
                }),
                Selection::Range(SelectionRange {
                    start: cursor(3),
                    end: cursor(9),
                }),
            ],
        );
        mc.merge_overlapping();
        let alice_count = mc.selections.iter().filter(|s| s.owner == alice).count();
        assert_eq!(alice_count, 1, "one owner's overlaps must still collapse");
    }

    /// A remote participant's state is a SNAPSHOT: replacing is what stops a
    /// missed message leaving a stale caret behind forever.
    #[test]
    fn injecting_replaces_that_owner_and_leaves_the_others_alone() {
        let mut mc = state();
        let alice = SelectionOwner::new(1, 1);
        let bob = SelectionOwner::new(2, 2);
        mc.set_owner_selections(alice, &[Selection::Cursor(cursor(0)), Selection::Cursor(cursor(4))]);
        mc.set_owner_selections(bob, &[Selection::Cursor(cursor(8))]);
        assert_eq!(mc.selections.iter().filter(|s| s.owner == alice).count(), 2);

        mc.set_owner_selections(alice, &[Selection::Cursor(cursor(2))]);
        assert_eq!(
            mc.selections.iter().filter(|s| s.owner == alice).count(),
            1,
            "a snapshot replaces, it does not accumulate"
        );
        assert_eq!(mc.selections.iter().filter(|s| s.owner == bob).count(), 1);
        assert!(mc.selections.iter().any(|s| s.owner.is_local()));
    }

    #[test]
    fn a_participant_who_leaves_takes_only_their_own_carets() {
        let mut mc = state();
        let alice = SelectionOwner::new(1, 1);
        let bob = SelectionOwner::new(2, 2);
        mc.set_owner_selections(alice, &[Selection::Cursor(cursor(0))]);
        mc.set_owner_selections(bob, &[Selection::Cursor(cursor(4))]);

        assert_eq!(mc.remove_owner(alice), 1);
        assert!(!mc.owners().contains(&alice));
        assert!(mc.owners().contains(&bob));
        assert!(mc.selections.iter().any(|s| s.owner.is_local()));
    }

    /// THE LOCAL CARET IS THE ENGINE'S. Letting an app overwrite or delete it
    /// through the remote door would make every text-editing invariant the
    /// engine maintains someone else's problem - and removing it would leave
    /// the document with no caret at all.
    #[test]
    fn the_local_owner_cannot_be_injected_or_removed() {
        let mut mc = state();
        assert!(!mc.set_owner_selections(SelectionOwner::LOCAL, &[Selection::Cursor(cursor(9))]));
        assert_eq!(mc.remove_owner(SelectionOwner::LOCAL), 0);
        assert_eq!(mc.selections.len(), 1);
        assert!(mc.selections[0].owner.is_local());
    }
}

/// U3-a: a peer's caret moves with the text it is anchored in.
#[cfg(test)]
mod peer_shift_tests {
    use super::*;
    use crate::styled_dom::NodeHierarchyItemId;

    fn cursor(byte: u32) -> TextCursor {
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: byte,
            },
            affinity: CursorAffinity::Leading,
        }
    }

    fn node() -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
        }
    }

    fn with_peer_at(at: Selection) -> (MultiCursorState, SelectionOwner) {
        let bob = SelectionOwner::new(2, 2);
        let mut mc = MultiCursorState::new_with_cursor(cursor(0), node(), 0);
        assert!(mc.set_owner_selections(bob, &[at]));
        (mc, bob)
    }

    fn peer(mc: &MultiCursorState, who: SelectionOwner) -> Selection {
        mc.selections.iter().find(|s| s.owner == who).unwrap().selection
    }

    fn change(start: u32, end: u32, inserted: u32) -> RunTextChange {
        RunTextChange {
            run: 0,
            start,
            end,
            inserted,
        }
    }

    #[test]
    fn the_diff_between_two_texts_is_the_replaced_middle() {
        assert_eq!(RunTextChange::between(0, "hello", "hexllo"), Some(change(2, 2, 1)));
        assert_eq!(RunTextChange::between(0, "abc", "ac"), Some(change(1, 2, 0)));
        assert_eq!(RunTextChange::between(0, "abcd", "aXYd"), Some(change(1, 3, 2)));
        assert_eq!(RunTextChange::between(0, "same", "same"), None);
        assert_eq!(RunTextChange::between(0, "", "new"), Some(change(0, 0, 3)));
        assert_eq!(RunTextChange::between(0, "gone", ""), Some(change(0, 4, 0)));
    }

    /// Inserting a repeated character: the prefix wins, the change lands
    /// AFTER the existing copy, and the arithmetic stays consistent (no
    /// overlap between prefix and suffix).
    #[test]
    fn a_repeated_character_is_placed_after_its_twin() {
        assert_eq!(RunTextChange::between(0, "aa", "aaa"), Some(change(2, 2, 1)));
        assert_eq!(RunTextChange::between(0, "aaa", "aa"), Some(change(2, 3, 0)));
    }

    /// A change never starts or ends inside a multi-byte character.
    #[test]
    fn the_diff_respects_char_boundaries() {
        // 'é' (C3 A9) -> 'è' (C3 A8): the first byte is shared, but the
        // change must cover the whole character.
        assert_eq!(RunTextChange::between(0, "\u{e9}", "\u{e8}"), Some(change(0, 2, 2)));
    }

    #[test]
    fn a_change_before_the_caret_shifts_it_and_one_after_does_not() {
        // "hello world", Bob at 6 (before "world").
        let (mut mc, bob) = with_peer_at(Selection::Cursor(cursor(6)));
        mc.shift_peers_across(&[change(0, 0, 3)]); // insert 3 at the start
        assert_eq!(peer(&mc, bob), Selection::Cursor(cursor(9)));
        mc.shift_peers_across(&[change(2, 4, 0)]); // delete 2 before him
        assert_eq!(peer(&mc, bob), Selection::Cursor(cursor(7)));
        mc.shift_peers_across(&[change(9, 9, 5)]); // insert after him
        assert_eq!(peer(&mc, bob), Selection::Cursor(cursor(7)));
    }

    /// A caret AT a pure insert's position is attached to the character that
    /// follows, so it moves with it.
    #[test]
    fn an_insert_at_the_caret_pushes_it_after_the_new_text() {
        let (mut mc, bob) = with_peer_at(Selection::Cursor(cursor(4)));
        mc.shift_peers_across(&[change(4, 4, 2)]);
        assert_eq!(peer(&mc, bob), Selection::Cursor(cursor(6)));
    }

    #[test]
    fn a_change_spanning_the_caret_collapses_it_to_the_change_start() {
        let (mut mc, bob) = with_peer_at(Selection::Cursor(cursor(5)));
        mc.shift_peers_across(&[change(3, 8, 1)]); // replace 3..8 by one byte
        assert_eq!(peer(&mc, bob), Selection::Cursor(cursor(3)));
    }

    #[test]
    fn a_peer_range_moves_both_ends_and_may_collapse() {
        let range = Selection::Range(SelectionRange {
            start: cursor(4),
            end: cursor(8),
        });
        let (mut mc, bob) = with_peer_at(range);
        mc.shift_peers_across(&[change(0, 0, 2)]);
        assert_eq!(
            peer(&mc, bob),
            Selection::Range(SelectionRange {
                start: cursor(6),
                end: cursor(10),
            })
        );
        // A replace that swallows the whole range collapses it.
        mc.shift_peers_across(&[change(5, 12, 0)]);
        assert_eq!(
            peer(&mc, bob),
            Selection::Range(SelectionRange {
                start: cursor(5),
                end: cursor(5),
            })
        );
    }

    /// The local selection is placed by the edit result, never by this.
    #[test]
    fn local_selections_are_not_shifted() {
        let (mut mc, _bob) = with_peer_at(Selection::Cursor(cursor(6)));
        mc.shift_peers_across(&[change(0, 0, 3)]);
        assert_eq!(mc.get_primary_cursor(), Some(cursor(0)));
    }

    /// U3-b: a change the app's generation brought moves the LOCAL caret too.
    #[test]
    fn shift_all_moves_the_local_caret_as_well() {
        let (mut mc, bob) = with_peer_at(Selection::Cursor(cursor(6)));
        mc.set_single_cursor(cursor(3));
        mc.shift_all_across(&[change(0, 0, 2)]);
        assert_eq!(mc.get_primary_cursor(), Some(cursor(5)));
        assert_eq!(peer(&mc, bob), Selection::Cursor(cursor(8)));
    }

    /// A change on ANOTHER run leaves a caret alone.
    #[test]
    fn only_the_changed_run_is_affected() {
        let (mut mc, bob) = with_peer_at(Selection::Cursor(cursor(6)));
        mc.shift_peers_across(&[RunTextChange {
            run: 1,
            start: 0,
            end: 0,
            inserted: 3,
        }]);
        assert_eq!(peer(&mc, bob), Selection::Cursor(cursor(6)));
    }
}
